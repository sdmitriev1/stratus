use tonic::{Request, Response, Status};

use crate::proto::{
    stratus_service_server::StratusService, GetStatusRequest, GetStatusResponse,
};

pub struct StratusServer {
    start_time: std::time::Instant,
}

impl StratusServer {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
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
}
