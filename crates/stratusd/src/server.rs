use std::sync::Arc;

use stratus_store::WatchableStore;
use tonic::{Request, Response, Status};

use crate::proto::{
    DumpStoreRequest, DumpStoreResponse, GetStatusRequest, GetStatusResponse,
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
}
