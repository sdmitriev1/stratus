use std::sync::Arc;
use std::time::Duration;

use hyper_util::rt::TokioIo;
use stratus_resources::Resource;
use stratus_store::WatchableStore;
use stratusd::proto::stratus_service_client::StratusServiceClient;
use stratusd::proto::stratus_service_server::StratusServiceServer;
use stratusd::proto::{ApplyRequest, GetRequest};
use stratusd::server::StratusServer;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tower::service_fn;

fn temp_store(dir: &std::path::Path) -> Arc<WatchableStore> {
    let db_path = dir.join("test.db");
    Arc::new(WatchableStore::open(&db_path).expect("failed to open store"))
}

fn start_server(
    socket_path: &std::path::Path,
    store: Arc<WatchableStore>,
) -> tokio::sync::oneshot::Sender<()> {
    let listener = UnixListener::bind(socket_path).expect("failed to bind socket");
    let stream = UnixListenerStream::new(listener);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        Server::builder()
            .add_service(StratusServiceServer::new(StratusServer::new(store)))
            .serve_with_incoming_shutdown(stream, async {
                rx.await.ok();
            })
            .await
            .expect("server error");
    });

    tx
}

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

fn to_json(r: &Resource) -> String {
    serde_json::to_string(r).unwrap()
}

#[tokio::test]
async fn apply_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // Write a temp YAML file
    let yaml = "kind: Network\nname: test-net\n";
    let yaml_path = dir.path().join("test.yaml");
    std::fs::write(&yaml_path, yaml).unwrap();

    // Parse and apply via RPC
    let resources = stratus_resources::parse_yaml_documents(yaml).unwrap();
    let json_resources: Vec<String> = resources.iter().map(to_json).collect();

    let resp = client
        .apply(ApplyRequest {
            resources: json_resources,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].kind, "Network");
    assert_eq!(resp.results[0].action, "created");
}

#[tokio::test]
async fn apply_directory() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // Create a temp directory with two YAML files
    let yaml_dir = dir.path().join("yamls");
    std::fs::create_dir(&yaml_dir).unwrap();

    std::fs::write(
        yaml_dir.join("01-network.yaml"),
        "kind: Network\nname: dir-net\n",
    )
    .unwrap();

    std::fs::write(
        yaml_dir.join("02-image.yaml"),
        "kind: Image\nname: dir-img\nsource_url: https://example.com/img.qcow2\nformat: qcow2\n",
    )
    .unwrap();

    // Read and concatenate like the CLI does
    let mut entries: Vec<_> = std::fs::read_dir(&yaml_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".yaml") || name.ends_with(".yml")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut combined = String::new();
    for entry in entries {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        if !combined.is_empty() {
            combined.push_str("\n---\n");
        }
        combined.push_str(&content);
    }

    let resources = stratus_resources::parse_yaml_documents(&combined).unwrap();
    let json_resources: Vec<String> = resources.iter().map(to_json).collect();

    let resp = client
        .apply(ApplyRequest {
            resources: json_resources,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.results.len(), 2);

    // Verify both stored
    let get_net = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("dir-net".to_string()),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(get_net.resources.len(), 1);

    let get_img = client
        .get(GetRequest {
            kind: "Image".to_string(),
            name: Some("dir-img".to_string()),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(get_img.resources.len(), 1);
}

#[tokio::test]
async fn apply_validation_error_message() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // Apply instance referencing nonexistent image
    let instance = Resource::Instance(stratus_resources::Instance {
        name: "bad-vm".to_string(),
        cpus: 2,
        memory_mb: 1024,
        disk_gb: 20,
        image: "no-such-image".to_string(),
        secure_boot: false,
        vtpm: false,
        interfaces: vec![],
        user_data: None,
        ssh_authorized_keys: vec![],
    });

    let resp = client
        .apply(ApplyRequest {
            resources: vec![to_json(&instance)],
        })
        .await;

    assert!(resp.is_err());
    let msg = resp.unwrap_err().message().to_string();
    assert!(
        msg.contains("no-such-image"),
        "error should mention the missing image: {msg}"
    );
}

#[tokio::test]
async fn apply_empty_resources_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .apply(ApplyRequest { resources: vec![] })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.results.is_empty());
}
