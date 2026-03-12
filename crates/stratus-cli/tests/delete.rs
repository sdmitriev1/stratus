use std::sync::Arc;
use std::time::Duration;

use hyper_util::rt::TokioIo;
use stratus_images::ImageCache;
use stratus_resources::Resource;
use stratus_store::WatchableStore;
use stratusd::proto::stratus_service_client::StratusServiceClient;
use stratusd::proto::stratus_service_server::StratusServiceServer;
use stratusd::proto::{ApplyRequest, DeleteRequest, GetRequest};
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
    let images_dir = socket_path.parent().unwrap().join("images");
    let image_cache = Arc::new(ImageCache::new(images_dir).expect("failed to create image cache"));
    let listener = UnixListener::bind(socket_path).expect("failed to bind socket");
    let stream = UnixListenerStream::new(listener);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        Server::builder()
            .add_service(StratusServiceServer::new(StratusServer::new(
                store,
                image_cache,
            )))
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
async fn delete_existing_resource() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "del-net".to_string(),
    });

    client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap();

    let resp = client
        .delete(DeleteRequest {
            kind: "Network".to_string(),
            name: "del-net".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.found);
}

#[tokio::test]
async fn delete_nonexistent_resource() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .delete(DeleteRequest {
            kind: "Network".to_string(),
            name: "no-such-net".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(!resp.found);
}

#[tokio::test]
async fn delete_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // Apply two networks
    let net1 = Resource::Network(stratus_resources::Network {
        name: "file-net1".to_string(),
    });
    let net2 = Resource::Network(stratus_resources::Network {
        name: "file-net2".to_string(),
    });

    client
        .apply(ApplyRequest {
            resources: vec![to_json(&net1), to_json(&net2)],
        })
        .await
        .unwrap();

    // Write a YAML file with both resources
    let yaml = "kind: Network\nname: file-net1\n---\nkind: Network\nname: file-net2\n";
    let yaml_path = dir.path().join("delete.yaml");
    std::fs::write(&yaml_path, yaml).unwrap();

    // Parse and delete each
    let resources = stratus_resources::parse_yaml_documents(yaml).unwrap();
    for r in &resources {
        let resp = client
            .delete(DeleteRequest {
                kind: r.kind_str().to_string(),
                name: r.name().to_string(),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(resp.found);
    }

    // Verify all removed
    let get_resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(get_resp.resources.is_empty());
}
