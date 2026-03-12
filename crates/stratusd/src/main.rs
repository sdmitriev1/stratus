use std::sync::Arc;

use anyhow::Result;
use stratus_images::ImageCache;
use stratus_store::WatchableStore;
use stratus_vm::host;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tracing::info;

use stratusd::config::Config;
use stratusd::proto;
use stratusd::server::StratusServer;
use stratusd::vm_manager::VmManager;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::default();

    // Detect host capabilities.
    let host_info = host::detect()?;
    info!(
        arch = %host_info.arch,
        kvm = host_info.kvm_available,
        qemu = host_info.qemu_binary,
        "host detected"
    );

    // Ensure data directory exists and open the store.
    std::fs::create_dir_all(&config.data_dir)?;
    let store = Arc::new(WatchableStore::open(config.db_path())?);
    info!(path = %config.db_path().display(), "store opened");

    // Initialize image cache.
    let image_cache = Arc::new(ImageCache::new(config.images_dir())?);
    info!(path = %config.images_dir().display(), "image cache initialized");

    // Ensure instances directory exists.
    std::fs::create_dir_all(config.instances_dir())?;

    // Create VM manager and recover existing VMs.
    let vm_manager = VmManager::new(host_info, config.data_dir.clone(), config.runtime_dir());
    vm_manager.recover().await;

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

    let stratus_service = proto::stratus_service_server::StratusServiceServer::new(
        StratusServer::new(store, image_cache, vm_manager),
    );

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
