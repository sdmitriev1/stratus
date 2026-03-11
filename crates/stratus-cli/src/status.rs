use anyhow::{Context, Result};

use crate::connect::connect;
use crate::proto::GetStatusRequest;
use crate::proto::stratus_service_client::StratusServiceClient;

pub async fn run(socket: &str) -> Result<()> {
    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let response = client
        .get_status(GetStatusRequest {})
        .await
        .context("failed to get daemon status — is stratusd running?")?;

    let status = response.into_inner();
    println!("stratusd v{}", status.version);
    println!("uptime:  {}", status.uptime);

    Ok(())
}
