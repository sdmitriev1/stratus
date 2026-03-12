use anyhow::Result;

use crate::connect::connect;
use crate::proto::{InstanceActionRequest, stratus_service_client::StratusServiceClient};

pub async fn start(socket: &str, name: &str) -> Result<()> {
    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let resp = client
        .instance_start(InstanceActionRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();

    println!(
        "Instance {} — status: {}, {}",
        resp.name, resp.status, resp.message
    );
    Ok(())
}

pub async fn stop(socket: &str, name: &str) -> Result<()> {
    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let resp = client
        .instance_stop(InstanceActionRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();

    println!(
        "Instance {} — status: {}, {}",
        resp.name, resp.status, resp.message
    );
    Ok(())
}

pub async fn kill(socket: &str, name: &str) -> Result<()> {
    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let resp = client
        .instance_kill(InstanceActionRequest {
            name: name.to_string(),
        })
        .await?
        .into_inner();

    println!(
        "Instance {} — status: {}, {}",
        resp.name, resp.status, resp.message
    );
    Ok(())
}
