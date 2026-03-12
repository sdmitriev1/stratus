use std::{sync::Arc, time::Duration};

use stratus_images::ImageCache;
use stratus_resources::{Resource, allocate_addresses, validate};
use stratus_store::WatchableStore;
use tonic::{Request, Response, Status};
use tracing::warn;

use crate::proto::{
    ApplyRequest, ApplyResponse, ApplyResult, DeleteRequest, DeleteResponse, DumpStoreRequest,
    DumpStoreResponse, GetRequest, GetResponse, GetStatusRequest, GetStatusResponse,
    InstanceActionRequest, InstanceActionResponse, stratus_service_server::StratusService,
};
use crate::vm_manager::VmManager;

pub fn format_uptime(uptime: Duration) -> String {
    let seconds = uptime.as_secs();
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, secs)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}
pub struct StratusServer {
    start_time: std::time::Instant,
    store: Arc<WatchableStore>,
    image_cache: Arc<ImageCache>,
    vm_manager: Arc<VmManager>,
}

impl StratusServer {
    pub fn new(
        store: Arc<WatchableStore>,
        image_cache: Arc<ImageCache>,
        vm_manager: Arc<VmManager>,
    ) -> Self {
        Self {
            start_time: std::time::Instant::now(),
            store,
            image_cache,
            vm_manager,
        }
    }
}

/// Returns resources that depend on the given (kind, name) pair.
fn find_dependents(resources: &[Resource], kind: &str, name: &str) -> Vec<(String, String)> {
    let mut deps = Vec::new();
    for r in resources {
        let is_dep = match r {
            Resource::Instance(inst) => match kind {
                "Image" => inst.image == name,
                "Subnet" => inst.interfaces.iter().any(|i| i.subnet == name),
                "SecurityGroup" => inst
                    .interfaces
                    .iter()
                    .any(|i| i.security_groups.iter().any(|sg| sg == name)),
                _ => false,
            },
            Resource::Subnet(sub) => kind == "Network" && sub.network == name,
            Resource::SecurityGroup(sg) => {
                kind == "SecurityGroup"
                    && sg.name != name
                    && sg
                        .rules
                        .iter()
                        .any(|r| r.remote_sg.as_deref() == Some(name))
            }
            Resource::PortForward(pf) => kind == "Instance" && pf.instance == name,
            _ => false,
        };
        if is_dep {
            deps.push((r.kind_str().to_string(), r.name().to_string()));
        }
    }
    deps
}

/// Returns a sort priority for resource kinds so dependencies are stored first.
fn kind_priority(kind: &str) -> u8 {
    match kind {
        "Network" => 0,
        "Subnet" | "Image" | "SecurityGroup" => 1,
        "Instance" => 2,
        "PortForward" => 3,
        _ => 4,
    }
}

#[tonic::async_trait]
impl StratusService for StratusServer {
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        let uptime = self.start_time.elapsed();
        let response = GetStatusResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime: format_uptime(uptime),
        };
        Ok(Response::new(response))
    }

    async fn dump_store(
        &self,
        _request: Request<DumpStoreRequest>,
    ) -> Result<Response<DumpStoreResponse>, Status> {
        let resources = self
            .store
            .list_all()
            .map_err(|e| Status::internal(e.to_string()))?;
        let mut json_resources = Vec::with_capacity(resources.len());
        for r in &resources {
            let json = serde_json::to_string(r).map_err(|e| Status::internal(e.to_string()))?;
            json_resources.push(json);
        }
        Ok(Response::new(DumpStoreResponse {
            resources: json_resources,
            revision: self.store.revision(),
        }))
    }

    async fn apply(
        &self,
        request: Request<ApplyRequest>,
    ) -> Result<Response<ApplyResponse>, Status> {
        let req = request.into_inner();

        // 1. Deserialize incoming resources, deduplicating by (kind, name) — last wins
        let mut all_parsed: Vec<Resource> = Vec::with_capacity(req.resources.len());
        for json in &req.resources {
            let resource: Resource = serde_json::from_str(json)
                .map_err(|e| Status::invalid_argument(format!("failed to parse resource: {e}")))?;
            all_parsed.push(resource);
        }
        // Walk in reverse so that last occurrence wins, then reverse back to preserve order
        let mut seen = std::collections::HashSet::new();
        let mut incoming: Vec<Resource> = Vec::with_capacity(all_parsed.len());
        for r in all_parsed.into_iter().rev() {
            let key = (r.kind_str().to_string(), r.name().to_string());
            if seen.insert(key) {
                incoming.push(r);
            }
        }
        incoming.reverse();

        if incoming.is_empty() {
            return Ok(Response::new(ApplyResponse {
                results: Vec::new(),
            }));
        }

        // 2. Load existing resources
        let existing = self
            .store
            .list_all()
            .map_err(|e| Status::internal(e.to_string()))?;

        // 3. Build merged set: existing + incoming (incoming overrides same kind+name)
        //    For incoming instances, carry forward stored IP/MAC allocations so that
        //    re-applying the same YAML doesn't generate new addresses each time.
        let mut merged: Vec<Resource> = Vec::new();
        let incoming_keys: std::collections::HashSet<(String, String)> = incoming
            .iter()
            .map(|r| (r.kind_str().to_string(), r.name().to_string()))
            .collect();

        // Build a map of existing instances for carrying forward allocations
        let existing_instances: std::collections::HashMap<String, &Resource> = existing
            .iter()
            .filter(|r| r.kind_str() == "Instance")
            .map(|r| (r.name().to_string(), r))
            .collect();

        // Add existing resources that are NOT being overridden
        for r in &existing {
            let key = (r.kind_str().to_string(), r.name().to_string());
            if !incoming_keys.contains(&key) {
                merged.push(r.clone());
            }
        }
        // Add incoming, carrying forward stored IP/MAC for instances
        for r in &incoming {
            if let Resource::Instance(inst) = r
                && let Some(Resource::Instance(stored)) =
                    existing_instances.get(inst.name.as_str()).copied()
            {
                let mut patched = inst.clone();
                for (iface, stored_iface) in patched.interfaces.iter_mut().zip(&stored.interfaces) {
                    if iface.subnet == stored_iface.subnet {
                        if iface.ip.is_none() {
                            iface.ip = stored_iface.ip;
                        }
                        if iface.mac.is_none() {
                            iface.mac = stored_iface.mac.clone();
                        }
                    }
                }
                merged.push(Resource::Instance(patched));
                continue;
            }
            merged.push(r.clone());
        }

        // 4. Validate merged set
        validate(&merged).map_err(|e| Status::invalid_argument(e.to_string()))?;

        // 5. Download images with checksums
        for r in &incoming {
            if let Resource::Image(img) = r {
                if let Some(ref checksum) = img.checksum {
                    self.image_cache
                        .ensure(&img.source_url, checksum, img.format)
                        .await
                        .map_err(|e| Status::internal(format!("image download failed: {e}")))?;
                } else {
                    warn!(name = img.name, "image has no checksum, skipping download");
                }
            }
        }

        // 6. Sort incoming by dependency priority
        incoming.sort_by_key(|r| kind_priority(r.kind_str()));

        // 7. Run allocate_addresses on merged set, then extract incoming instances
        //    with their allocations. Already-stored instances keep existing allocations.
        allocate_addresses(&mut merged)
            .map_err(|e| Status::internal(format!("IP allocation failed: {e}")))?;

        // Build a map of allocated instances from merged set
        let allocated_instances: std::collections::HashMap<String, Resource> = merged
            .into_iter()
            .filter(|r| {
                r.kind_str() == "Instance"
                    && incoming_keys.contains(&("Instance".to_string(), r.name().to_string()))
            })
            .map(|r| (r.name().to_string(), r))
            .collect();

        // 8. Store each incoming resource (skip if unchanged)
        let mut results = Vec::with_capacity(incoming.len());
        for r in &incoming {
            // Use the allocated version for instances
            let to_store = if r.kind_str() == "Instance" {
                allocated_instances.get(r.name()).unwrap_or(r)
            } else {
                r
            };

            // Check if the resource already exists and is identical
            let existing = self
                .store
                .get(to_store.kind_str(), to_store.name())
                .map_err(|e| Status::internal(e.to_string()))?;

            if existing.as_ref() == Some(to_store) {
                results.push(ApplyResult {
                    kind: to_store.kind_str().to_string(),
                    name: to_store.name().to_string(),
                    action: "unchanged".to_string(),
                    revision: self.store.revision(),
                });
                continue;
            }

            let (revision, old) = self
                .store
                .put(to_store)
                .map_err(|e| Status::internal(e.to_string()))?;

            results.push(ApplyResult {
                kind: to_store.kind_str().to_string(),
                name: to_store.name().to_string(),
                action: if old.is_some() {
                    "updated".to_string()
                } else {
                    "created".to_string()
                },
                revision,
            });

            // After storing an Instance, start its VM
            if let Resource::Instance(inst) = to_store {
                // Look up the Image resource from the store
                if let Ok(Some(Resource::Image(image))) = self.store.get("Image", &inst.image) {
                    if let Some(ref checksum) = image.checksum {
                        if let Some(base_path) = self
                            .image_cache
                            .lookup(checksum.strip_prefix("sha256:").unwrap_or(checksum))
                        {
                            match self
                                .vm_manager
                                .start_instance(&inst.name, inst, &image, &base_path)
                                .await
                            {
                                Ok(status) => {
                                    tracing::info!(
                                        name = inst.name,
                                        status = %status,
                                        "VM started"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        name = inst.name,
                                        error = %e,
                                        "failed to start VM"
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                name = inst.name,
                                "base image not cached, skipping VM start"
                            );
                        }
                    } else {
                        tracing::warn!(
                            name = inst.name,
                            "image has no checksum, skipping VM start"
                        );
                    }
                }
            }
        }

        Ok(Response::new(ApplyResponse { results }))
    }

    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        let req = request.into_inner();

        let resources = if let Some(ref name) = req.name {
            if !name.is_empty() {
                match self.store.get(&req.kind, name) {
                    Ok(Some(r)) => vec![r],
                    Ok(None) => vec![],
                    Err(stratus_store::StoreError::UnknownKind(k)) => {
                        return Err(Status::invalid_argument(format!(
                            "unknown resource kind: {k}"
                        )));
                    }
                    Err(e) => return Err(Status::internal(e.to_string())),
                }
            } else {
                self.store.list(&req.kind).map_err(|e| match e {
                    stratus_store::StoreError::UnknownKind(k) => {
                        Status::invalid_argument(format!("unknown resource kind: {k}"))
                    }
                    other => Status::internal(other.to_string()),
                })?
            }
        } else {
            self.store.list(&req.kind).map_err(|e| match e {
                stratus_store::StoreError::UnknownKind(k) => {
                    Status::invalid_argument(format!("unknown resource kind: {k}"))
                }
                other => Status::internal(other.to_string()),
            })?
        };

        let mut json_resources = Vec::with_capacity(resources.len());
        for r in &resources {
            let json = serde_json::to_string(r).map_err(|e| Status::internal(e.to_string()))?;
            json_resources.push(json);
        }

        // Populate instance statuses
        let mut instance_statuses = std::collections::HashMap::new();
        if req.kind == "Instance" {
            let statuses = self.vm_manager.statuses().await;
            for (name, status) in statuses {
                instance_statuses.insert(name, status.to_string());
            }
        }

        Ok(Response::new(GetResponse {
            resources: json_resources,
            revision: self.store.revision(),
            instance_statuses,
        }))
    }

    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let req = request.into_inner();

        // Check referential integrity before deleting
        let all = self
            .store
            .list_all()
            .map_err(|e| Status::internal(e.to_string()))?;
        let deps = find_dependents(&all, &req.kind, &req.name);
        if !deps.is_empty() {
            let names: Vec<String> = deps.iter().map(|(k, n)| format!("{k}/{n}")).collect();
            return Err(Status::failed_precondition(format!(
                "cannot delete {}/{}: referenced by {}",
                req.kind,
                req.name,
                names.join(", ")
            )));
        }

        // Destroy VM if deleting an instance
        if req.kind == "Instance"
            && let Err(e) = self.vm_manager.destroy_instance(&req.name).await
        {
            warn!(name = req.name, error = %e, "failed to destroy VM");
        }

        let (revision, old) = self
            .store
            .delete(&req.kind, &req.name)
            .map_err(|e| match e {
                stratus_store::StoreError::UnknownKind(k) => {
                    Status::invalid_argument(format!("unknown resource kind: {k}"))
                }
                other => Status::internal(other.to_string()),
            })?;

        if let Some(Resource::Image(ref img)) = old
            && let Some(ref checksum) = img.checksum
            && let Err(e) = self.image_cache.evict(checksum)
        {
            warn!(name = img.name, error = %e, "failed to evict cached image");
        }

        Ok(Response::new(DeleteResponse {
            found: old.is_some(),
            revision,
        }))
    }

    async fn instance_start(
        &self,
        request: Request<InstanceActionRequest>,
    ) -> Result<Response<InstanceActionResponse>, Status> {
        let req = request.into_inner();

        // Load instance from store
        let instance = match self.store.get("Instance", &req.name) {
            Ok(Some(Resource::Instance(inst))) => inst,
            Ok(_) => {
                return Err(Status::not_found(format!(
                    "instance not found: {}",
                    req.name
                )));
            }
            Err(e) => return Err(Status::internal(e.to_string())),
        };

        // Look up image
        let image = match self.store.get("Image", &instance.image) {
            Ok(Some(Resource::Image(img))) => img,
            Ok(_) => {
                return Err(Status::not_found(format!(
                    "image not found: {}",
                    instance.image
                )));
            }
            Err(e) => return Err(Status::internal(e.to_string())),
        };

        // Get base image path
        let base_path = image
            .checksum
            .as_ref()
            .and_then(|cs| {
                self.image_cache
                    .lookup(cs.strip_prefix("sha256:").unwrap_or(cs))
            })
            .ok_or_else(|| Status::failed_precondition("base image not cached"))?;

        match self
            .vm_manager
            .start_instance(&req.name, &instance, &image, &base_path)
            .await
        {
            Ok(status) => Ok(Response::new(InstanceActionResponse {
                name: req.name,
                status: status.to_string(),
                message: "started".into(),
            })),
            Err(e) => Err(Status::internal(format!("failed to start VM: {e}"))),
        }
    }

    async fn instance_stop(
        &self,
        request: Request<InstanceActionRequest>,
    ) -> Result<Response<InstanceActionResponse>, Status> {
        let req = request.into_inner();

        match self
            .vm_manager
            .stop_instance(&req.name, Duration::from_secs(30))
            .await
        {
            Ok(status) => Ok(Response::new(InstanceActionResponse {
                name: req.name,
                status: status.to_string(),
                message: "stopped".into(),
            })),
            Err(e) => Err(Status::internal(format!("failed to stop VM: {e}"))),
        }
    }

    async fn instance_kill(
        &self,
        request: Request<InstanceActionRequest>,
    ) -> Result<Response<InstanceActionResponse>, Status> {
        let req = request.into_inner();

        match self.vm_manager.kill_instance(&req.name).await {
            Ok(status) => Ok(Response::new(InstanceActionResponse {
                name: req.name,
                status: status.to_string(),
                message: "killed".into(),
            })),
            Err(e) => Err(Status::internal(format!("failed to kill VM: {e}"))),
        }
    }
}
