use std::time::Duration;

use hyper_util::rt::TokioIo;
use stratusd::proto::GetStatusRequest;
use stratusd::proto::stratus_service_client::StratusServiceClient;
use stratusd::proto::stratus_service_server::StratusServiceServer;
use stratusd::server::StratusServer;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tower::service_fn;

/// Start a stratusd gRPC server on a temp socket.
fn start_server(socket_path: &std::path::Path) -> tokio::sync::oneshot::Sender<()> {
    let listener = UnixListener::bind(socket_path).expect("failed to bind socket");
    let stream = UnixListenerStream::new(listener);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        Server::builder()
            .add_service(StratusServiceServer::new(StratusServer::new()))
            .serve_with_incoming_shutdown(stream, async {
                rx.await.ok();
            })
            .await
            .expect("server error");
    });

    tx
}

/// Connect a gRPC client to a Unix socket.
async fn connect(socket_path: std::path::PathBuf) -> StratusServiceClient<Channel> {
    let channel = Endpoint::from_static("http://[::]:50051")
        .connect_with_connector(service_fn(move |_: Uri| {
            let path = socket_path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .expect("failed to connect");

    StratusServiceClient::new(channel)
}

#[tokio::test]
async fn cli_connects_and_gets_status() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    let _shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;
    let resp = client
        .get_status(GetStatusRequest {})
        .await
        .expect("GetStatus should succeed")
        .into_inner();

    assert!(!resp.version.is_empty(), "version should not be empty");
    assert!(resp.uptime.ends_with('s'), "uptime should end with 's'");
}

#[tokio::test]
async fn cli_error_when_daemon_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("nonexistent.sock");

    // No server started — socket doesn't exist
    let result = Endpoint::from_static("http://[::]:50051")
        .connect_with_connector(service_fn(move |_: Uri| {
            let path = sock.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await;

    assert!(
        result.is_err(),
        "connecting to nonexistent socket should fail"
    );
}

#[tokio::test]
async fn cli_error_when_daemon_shuts_down_mid_session() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    let shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // First call should succeed
    let resp = client.get_status(GetStatusRequest {}).await;
    assert!(resp.is_ok(), "first call should succeed");

    // Shut down the server
    drop(shutdown);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Next call should fail
    let resp = client.get_status(GetStatusRequest {}).await;
    assert!(resp.is_err(), "call after shutdown should fail");
}

#[tokio::test]
async fn cli_gets_correct_version() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    let _shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;
    let resp = client
        .get_status(GetStatusRequest {})
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.version, "0.1.0");
}
