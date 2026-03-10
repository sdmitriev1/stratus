use anyhow::{Context, Result};
use hyper_util::rt::TokioIo;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

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

async fn connect(socket: &str) -> Result<Channel> {
    let socket = socket.to_string();

    // tonic requires a URI but ignores it for Unix sockets
    let channel = Endpoint::from_static("http://[::]:50051")
        .connect_with_connector(service_fn(move |_: Uri| {
            let socket = socket.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(socket).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .context("failed to connect to daemon socket")?;

    Ok(channel)
}
