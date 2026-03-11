use std::sync::Arc;

use stratus_resources::{Resource, allocate_addresses, validate};
use stratus_store::WatchableStore;
use tonic::{Request, Response, Status};

use crate::proto::{
    ApplyRequest, ApplyResponse, ApplyResult, DeleteRequest, DeleteResponse, DumpStoreRequest,
    DumpStoreResponse, GetRequest, GetResponse, GetStatusRequest, GetStatusResponse,
    stratus_service_server::StratusService,
};

pub struct StratusServer {
    start_time: std::time::Instant,
    store: Arc<WatchableStore>,
}

impl StratusServer {
    pub fn new(store: Arc<WatchableStore>) -> Self {
        Self {
            start_time: std::time::Instant::now(),
            store,
        }
    }
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
            uptime: format!("{}s", uptime.as_secs()),
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

        // 5. Sort incoming by dependency priority
        incoming.sort_by_key(|r| kind_priority(r.kind_str()));

        // 6. Run allocate_addresses on merged set, then extract incoming instances
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

        // 7. Store each incoming resource (skip if unchanged)
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

        Ok(Response::new(GetResponse {
            resources: json_resources,
            revision: self.store.revision(),
        }))
    }

    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let req = request.into_inner();

        let (revision, old) = self
            .store
            .delete(&req.kind, &req.name)
            .map_err(|e| match e {
                stratus_store::StoreError::UnknownKind(k) => {
                    Status::invalid_argument(format!("unknown resource kind: {k}"))
                }
                other => Status::internal(other.to_string()),
            })?;

        Ok(Response::new(DeleteResponse {
            found: old.is_some(),
            revision,
        }))
    }
}
