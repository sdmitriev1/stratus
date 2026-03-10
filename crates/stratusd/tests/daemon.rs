use std::time::Duration;

use hyper_util::rt::TokioIo;
use stratusd::proto::stratus_service_client::StratusServiceClient;
use stratusd::proto::stratus_service_server::StratusServiceServer;
use stratusd::proto::GetStatusRequest;
use stratusd::server::StratusServer;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tower::service_fn;

/// Start a gRPC server on the given Unix socket path.
/// Returns a shutdown sender — drop it to stop the server.
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

/// Connect a gRPC client to the given Unix socket path.
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
async fn daemon_binds_unix_socket() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    let _shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Socket file should exist
    assert!(sock.exists(), "socket file should exist after bind");

    // Should be able to connect
    let stream = tokio::net::UnixStream::connect(&sock).await;
    assert!(stream.is_ok(), "should connect to socket");
}

#[tokio::test]
async fn daemon_removes_socket_file_allows_rebind() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    // Start and stop first server
    let shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(shutdown);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Stale socket file may still exist — remove it like main.rs does
    if sock.exists() {
        std::fs::remove_file(&sock).unwrap();
    }

    // Second server should bind successfully
    let _shutdown2 = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(sock.exists());
}

#[tokio::test]
async fn get_status_returns_version_and_uptime() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    let _shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;
    let resp = client
        .get_status(GetStatusRequest {})
        .await
        .expect("GetStatus failed")
        .into_inner();

    assert_eq!(resp.version, env!("CARGO_PKG_VERSION"));
    assert!(resp.uptime.ends_with('s'), "uptime should end with 's'");
}

#[tokio::test]
async fn get_status_uptime_increases() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    let _shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp1 = client
        .get_status(GetStatusRequest {})
        .await
        .unwrap()
        .into_inner();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let resp2 = client
        .get_status(GetStatusRequest {})
        .await
        .unwrap()
        .into_inner();

    let parse_secs = |s: &str| -> u64 { s.trim_end_matches('s').parse().unwrap() };
    assert!(parse_secs(&resp2.uptime) >= parse_secs(&resp1.uptime));
}

#[tokio::test]
async fn multiple_concurrent_clients() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    let _shutdown = start_server(&sock);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut handles = vec![];
    for _ in 0..5 {
        let sock_path = sock.clone();
        handles.push(tokio::spawn(async move {
            let mut client = connect(sock_path).await;
            let resp = client
                .get_status(GetStatusRequest {})
                .await
                .unwrap()
                .into_inner();
            assert_eq!(resp.version, env!("CARGO_PKG_VERSION"));
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}
