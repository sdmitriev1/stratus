use std::sync::Arc;

use anyhow::Result;
use stratus_store::WatchableStore;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tracing::info;

use stratusd::config::Config;
use stratusd::proto;
use stratusd::server::StratusServer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::default();

    // Ensure data directory exists and open the store.
    std::fs::create_dir_all(&config.data_dir)?;
    let store = Arc::new(WatchableStore::open(config.db_path())?);
    info!(path = %config.db_path().display(), "store opened");

    // Ensure socket parent directory exists.
    if let Some(parent) = config.socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove stale socket file.
    if config.socket_path.exists() {
        std::fs::remove_file(&config.socket_path)?;
    }

    let listener = UnixListener::bind(&config.socket_path)?;
    let stream = UnixListenerStream::new(listener);

    info!(socket = %config.socket_path.display(), "stratusd listening");

    let stratus_service =
        proto::stratus_service_server::StratusServiceServer::new(StratusServer::new(store));

    Server::builder()
        .add_service(stratus_service)
        .serve_with_incoming_shutdown(stream, async {
            tokio::signal::ctrl_c().await.ok();
            info!("shutting down");
        })
        .await?;

    // Clean up socket on exit.
    let _ = std::fs::remove_file(&config.socket_path);

    Ok(())
}
