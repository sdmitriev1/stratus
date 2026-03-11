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

fn from_json(s: &str) -> Resource {
    serde_json::from_str(s).unwrap()
}

#[tokio::test]
async fn get_list_resources() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net1 = Resource::Network(stratus_resources::Network {
        name: "net-a".to_string(),
    });
    let net2 = Resource::Network(stratus_resources::Network {
        name: "net-b".to_string(),
    });

    client
        .apply(ApplyRequest {
            resources: vec![to_json(&net1), to_json(&net2)],
        })
        .await
        .unwrap();

    let resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.resources.len(), 2);
}

#[tokio::test]
async fn get_single_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "specific-net".to_string(),
    });

    client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap();

    let resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("specific-net".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.resources.len(), 1);
    assert_eq!(from_json(&resp.resources[0]), net);
}

#[tokio::test]
async fn get_nonexistent_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("nonexistent".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.resources.is_empty());
}

#[tokio::test]
async fn get_yaml_output() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "yaml-net".to_string(),
    });

    client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap();

    let resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("yaml-net".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    // Deserialize from JSON, serialize to YAML, parse back
    let resources: Vec<Resource> = resp.resources.iter().map(|json| from_json(json)).collect();

    let yaml = stratus_resources::serialize_yaml_documents(&resources).unwrap();
    let parsed_back = stratus_resources::parse_yaml_documents(&yaml).unwrap();
    assert_eq!(parsed_back.len(), 1);
    assert_eq!(parsed_back[0], net);
}

#[tokio::test]
async fn get_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "json-net".to_string(),
    });

    client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap();

    let resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("json-net".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    // Deserialize from JSON, serialize to pretty JSON, parse back
    let resources: Vec<Resource> = resp.resources.iter().map(|json| from_json(json)).collect();

    let json = serde_json::to_string_pretty(&resources).unwrap();
    let parsed_back: Vec<Resource> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed_back.len(), 1);
    assert_eq!(parsed_back[0], net);
}
