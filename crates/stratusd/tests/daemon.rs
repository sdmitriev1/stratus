use std::sync::Arc;
use std::time::Duration;

use hyper_util::rt::TokioIo;
use stratus_images::ImageCache;
use stratus_resources::Resource;
use stratus_store::WatchableStore;
use stratusd::proto::stratus_service_client::StratusServiceClient;
use stratusd::proto::stratus_service_server::StratusServiceServer;
use stratusd::proto::{
    ApplyRequest, DeleteRequest, DumpStoreRequest, GetRequest, GetStatusRequest,
};
use stratusd::server::StratusServer;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tower::service_fn;

/// Start a gRPC server on the given Unix socket path.
/// Returns a shutdown sender — drop it to stop the server.
fn start_server(
    socket_path: &std::path::Path,
    store: Arc<WatchableStore>,
) -> tokio::sync::oneshot::Sender<()> {
    let images_dir = socket_path.parent().unwrap().join("images");
    let image_cache = Arc::new(ImageCache::new(images_dir).expect("failed to create image cache"));
    start_server_with_cache(socket_path, store, image_cache)
}

fn start_server_with_cache(
    socket_path: &std::path::Path,
    store: Arc<WatchableStore>,
    image_cache: Arc<ImageCache>,
) -> tokio::sync::oneshot::Sender<()> {
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

/// Create a WatchableStore backed by a tempdir.
fn temp_store(dir: &std::path::Path) -> Arc<WatchableStore> {
    let db_path = dir.join("test.db");
    Arc::new(WatchableStore::open(&db_path).expect("failed to open store"))
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

/// Helper: serialize a Resource to JSON string for ApplyRequest.
fn to_json(r: &Resource) -> String {
    serde_json::to_string(r).unwrap()
}

/// Helper: parse a JSON string back to Resource.
fn from_json(s: &str) -> Resource {
    serde_json::from_str(s).unwrap()
}

// ========== Existing tests ==========

#[tokio::test]
async fn daemon_binds_unix_socket() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
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
    let store = temp_store(dir.path());
    let shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(shutdown);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Stale socket file may still exist — remove it like main.rs does
    if sock.exists() {
        std::fs::remove_file(&sock).unwrap();
    }

    // Second server should bind successfully
    let store2 = temp_store(dir.path());
    let _shutdown2 = start_server(&sock, store2);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(sock.exists());
}

#[tokio::test]
async fn get_status_returns_version_and_uptime() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
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
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
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
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
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

#[tokio::test]
async fn dump_store_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;
    let resp = client
        .dump_store(DumpStoreRequest {})
        .await
        .expect("DumpStore failed")
        .into_inner();

    assert!(resp.resources.is_empty());
    assert_eq!(resp.revision, 0);
}

#[tokio::test]
async fn dump_store_returns_resources() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let net = Resource::Network(stratus_resources::Network {
        name: "test-net".to_string(),
    });
    store.put(&net).unwrap();

    let img = Resource::Image(stratus_resources::Image {
        name: "test-img".to_string(),
        source_url: "https://example.com/image.qcow2".to_string(),
        format: stratus_resources::ImageFormat::Qcow2,
        architecture: None,
        os_type: None,
        checksum: None,
        min_disk_gb: None,
        min_ram_mb: None,
    });
    store.put(&img).unwrap();

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;
    let resp = client
        .dump_store(DumpStoreRequest {})
        .await
        .expect("DumpStore failed")
        .into_inner();

    assert_eq!(resp.resources.len(), 2);
    assert_eq!(resp.revision, 2);

    let resources: Vec<Resource> = resp
        .resources
        .iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect();

    assert_eq!(resources[0], net);
    assert_eq!(resources[1], img);
}

// ========== Apply tests ==========

#[tokio::test]
async fn apply_single_network() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "test-net".to_string(),
    });

    let resp = client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].kind, "Network");
    assert_eq!(resp.results[0].name, "test-net");
    assert_eq!(resp.results[0].action, "created");
    assert_eq!(resp.results[0].revision, 1);

    // Get it back
    let get_resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("test-net".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(get_resp.resources.len(), 1);
    let got: Resource = from_json(&get_resp.resources[0]);
    assert_eq!(got, net);
}

#[tokio::test]
async fn apply_updates_existing() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "test-net".to_string(),
    });

    // First apply — created
    let resp1 = client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp1.results[0].action, "created");

    // Second apply (identical) — unchanged
    let resp2 = client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp2.results[0].action, "unchanged");
    assert_eq!(resp2.results[0].revision, resp1.results[0].revision);

    // Third apply (modified) — updated
    let net2 = Resource::Network(stratus_resources::Network {
        name: "test-net-2".to_string(),
    });
    // Apply a different network under same request to show "created" still works,
    // but re-apply original net to show unchanged vs a real update scenario.
    // Instead: modify a resource that has mutable fields.
    let img = Resource::Image(stratus_resources::Image {
        name: "test-img".to_string(),
        source_url: "https://example.com/v1.qcow2".to_string(),
        format: stratus_resources::ImageFormat::Qcow2,
        architecture: None,
        os_type: None,
        checksum: None,
        min_disk_gb: None,
        min_ram_mb: None,
    });
    let resp3 = client
        .apply(ApplyRequest {
            resources: vec![to_json(&img)],
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp3.results[0].action, "created");

    // Now update the image
    let img_updated = Resource::Image(stratus_resources::Image {
        name: "test-img".to_string(),
        source_url: "https://example.com/v2.qcow2".to_string(),
        format: stratus_resources::ImageFormat::Qcow2,
        architecture: None,
        os_type: None,
        checksum: None,
        min_disk_gb: None,
        min_ram_mb: None,
    });
    let resp4 = client
        .apply(ApplyRequest {
            resources: vec![to_json(&img_updated)],
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp4.results[0].action, "updated");
    assert!(resp4.results[0].revision > resp3.results[0].revision);

    let _ = net2; // suppress unused warning
}

#[tokio::test]
async fn apply_multiple_with_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resources = vec![
        to_json(&Resource::Network(stratus_resources::Network {
            name: "net1".to_string(),
        })),
        to_json(&Resource::Subnet(stratus_resources::Subnet {
            name: "sub1".to_string(),
            network: "net1".to_string(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: stratus_resources::NatMode::None,
            isolated: false,
        })),
        to_json(&Resource::Image(stratus_resources::Image {
            name: "img1".to_string(),
            source_url: "https://example.com/image.qcow2".to_string(),
            format: stratus_resources::ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        })),
        to_json(&Resource::SecurityGroup(stratus_resources::SecurityGroup {
            name: "sg1".to_string(),
            rules: vec![stratus_resources::SecurityGroupRule {
                direction: stratus_resources::Direction::Ingress,
                protocol: stratus_resources::Protocol::Tcp,
                port: Some(22),
                remote_cidr: Some("0.0.0.0/0".parse().unwrap()),
                remote_sg: None,
            }],
        })),
        to_json(&Resource::Instance(stratus_resources::Instance {
            name: "vm1".to_string(),
            cpus: 2,
            memory_mb: 1024,
            disk_gb: 20,
            image: "img1".to_string(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![stratus_resources::Interface {
                subnet: "sub1".to_string(),
                ip: None,
                mac: None,
                security_groups: vec!["sg1".to_string()],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        })),
    ];

    let resp = client
        .apply(ApplyRequest { resources })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.results.len(), 5);
    for r in &resp.results {
        assert_eq!(r.action, "created");
    }
}

#[tokio::test]
async fn apply_deduplicates_incoming() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // Same network appears twice — should deduplicate, last wins
    let net1 = Resource::Network(stratus_resources::Network {
        name: "dup-net".to_string(),
    });

    let resp = client
        .apply(ApplyRequest {
            resources: vec![to_json(&net1), to_json(&net1)],
        })
        .await
        .unwrap()
        .into_inner();

    // Only one result after dedup
    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].action, "created");

    // Verify only one stored
    let get_resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(get_resp.resources.len(), 1);
}

#[tokio::test]
async fn apply_validation_error() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // Instance referencing nonexistent image
    let instance = Resource::Instance(stratus_resources::Instance {
        name: "bad-vm".to_string(),
        cpus: 2,
        memory_mb: 1024,
        disk_gb: 20,
        image: "nonexistent-image".to_string(),
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
    let status = resp.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn apply_ip_allocation() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resources = vec![
        to_json(&Resource::Network(stratus_resources::Network {
            name: "net1".to_string(),
        })),
        to_json(&Resource::Subnet(stratus_resources::Subnet {
            name: "sub1".to_string(),
            network: "net1".to_string(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: stratus_resources::NatMode::None,
            isolated: false,
        })),
        to_json(&Resource::Image(stratus_resources::Image {
            name: "img1".to_string(),
            source_url: "https://example.com/image.qcow2".to_string(),
            format: stratus_resources::ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        })),
        to_json(&Resource::Instance(stratus_resources::Instance {
            name: "vm1".to_string(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "img1".to_string(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![stratus_resources::Interface {
                subnet: "sub1".to_string(),
                ip: None,
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        })),
    ];

    client.apply(ApplyRequest { resources }).await.unwrap();

    // Get instance, verify IP/MAC allocated
    let get_resp = client
        .get(GetRequest {
            kind: "Instance".to_string(),
            name: Some("vm1".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    let instance: Resource = from_json(&get_resp.resources[0]);
    if let Resource::Instance(inst) = instance {
        assert!(inst.interfaces[0].ip.is_some(), "IP should be allocated");
        assert!(inst.interfaces[0].mac.is_some(), "MAC should be allocated");
    } else {
        panic!("expected Instance");
    }
}

#[tokio::test]
async fn apply_preserves_existing_allocations() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // First: apply full environment
    let resources = vec![
        to_json(&Resource::Network(stratus_resources::Network {
            name: "net1".to_string(),
        })),
        to_json(&Resource::Subnet(stratus_resources::Subnet {
            name: "sub1".to_string(),
            network: "net1".to_string(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: stratus_resources::NatMode::None,
            isolated: false,
        })),
        to_json(&Resource::Image(stratus_resources::Image {
            name: "img1".to_string(),
            source_url: "https://example.com/image.qcow2".to_string(),
            format: stratus_resources::ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        })),
        to_json(&Resource::Instance(stratus_resources::Instance {
            name: "vm1".to_string(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "img1".to_string(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![stratus_resources::Interface {
                subnet: "sub1".to_string(),
                ip: None,
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        })),
    ];

    client
        .apply(ApplyRequest {
            resources: resources.clone(),
        })
        .await
        .unwrap();

    // Get allocated IP/MAC
    let get1 = client
        .get(GetRequest {
            kind: "Instance".to_string(),
            name: Some("vm1".to_string()),
        })
        .await
        .unwrap()
        .into_inner();
    let inst1: Resource = from_json(&get1.resources[0]);
    let (ip1, mac1) = if let Resource::Instance(ref i) = inst1 {
        (i.interfaces[0].ip, i.interfaces[0].mac.clone())
    } else {
        panic!("expected Instance");
    };

    // Re-apply the same instance (without IP/MAC, simulating user re-applying)
    let re_apply = vec![to_json(&Resource::Instance(stratus_resources::Instance {
        name: "vm1".to_string(),
        cpus: 1,
        memory_mb: 512,
        disk_gb: 10,
        image: "img1".to_string(),
        secure_boot: false,
        vtpm: false,
        interfaces: vec![stratus_resources::Interface {
            subnet: "sub1".to_string(),
            ip: None,
            mac: None,
            security_groups: vec![],
        }],
        user_data: None,
        ssh_authorized_keys: vec![],
    }))];

    let resp2 = client
        .apply(ApplyRequest {
            resources: re_apply,
        })
        .await
        .unwrap()
        .into_inner();

    // Should be unchanged since IP/MAC are carried forward from stored version
    assert_eq!(resp2.results[0].action, "unchanged");

    // Verify IP/MAC are preserved exactly
    let get2 = client
        .get(GetRequest {
            kind: "Instance".to_string(),
            name: Some("vm1".to_string()),
        })
        .await
        .unwrap()
        .into_inner();
    let inst2: Resource = from_json(&get2.resources[0]);
    if let Resource::Instance(ref i) = inst2 {
        assert_eq!(i.interfaces[0].ip, ip1, "IP should be preserved");
        assert_eq!(i.interfaces[0].mac, mac1, "MAC should be preserved");
    } else {
        panic!("expected Instance");
    }
}

#[tokio::test]
async fn apply_from_example_files() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    // Read and parse all example files (path relative to workspace root)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let examples_dir = workspace_root.join("examples/simple");
    let mut entries: Vec<_> = std::fs::read_dir(&examples_dir)
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

    assert_eq!(resp.results.len(), 5);

    // Verify all stored correctly
    for kind in &["Network", "Subnet", "SecurityGroup", "Image", "Instance"] {
        let get_resp = client
            .get(GetRequest {
                kind: kind.to_string(),
                name: None,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(
            !get_resp.resources.is_empty(),
            "should have {kind} resources"
        );
    }

    // Re-apply same files — should succeed (idempotent)
    let json_resources2: Vec<String> = resources.iter().map(to_json).collect();
    let resp2 = client
        .apply(ApplyRequest {
            resources: json_resources2,
        })
        .await
        .expect("second apply of same files should succeed")
        .into_inner();

    assert_eq!(resp2.results.len(), 5);
}

// ========== Get tests ==========

#[tokio::test]
async fn get_single_resource() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "my-net".to_string(),
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
            name: Some("my-net".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.resources.len(), 1);
    assert_eq!(from_json(&resp.resources[0]), net);
}

#[tokio::test]
async fn get_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("nope".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.resources.is_empty());
}

#[tokio::test]
async fn get_list_all_of_kind() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net1 = Resource::Network(stratus_resources::Network {
        name: "net1".to_string(),
    });
    let net2 = Resource::Network(stratus_resources::Network {
        name: "net2".to_string(),
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
async fn get_invalid_kind() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .get(GetRequest {
            kind: "Bogus".to_string(),
            name: None,
        })
        .await;

    assert!(resp.is_err());
    assert_eq!(resp.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn get_list_empty_kind() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.resources.is_empty());
}

// ========== Delete tests ==========

#[tokio::test]
async fn delete_existing() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "to-delete".to_string(),
    });

    client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap();

    let del_resp = client
        .delete(DeleteRequest {
            kind: "Network".to_string(),
            name: "to-delete".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(del_resp.found);

    // Verify it's gone
    let get_resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("to-delete".to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(get_resp.resources.is_empty());
}

#[tokio::test]
async fn delete_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .delete(DeleteRequest {
            kind: "Network".to_string(),
            name: "nope".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(!resp.found);
}

#[tokio::test]
async fn delete_invalid_kind() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let resp = client
        .delete(DeleteRequest {
            kind: "Bogus".to_string(),
            name: "whatever".to_string(),
        })
        .await;

    assert!(resp.is_err());
    assert_eq!(resp.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn apply_then_delete_then_reapply() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let _shutdown = start_server(&sock, store);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let net = Resource::Network(stratus_resources::Network {
        name: "lifecycle".to_string(),
    });

    // Apply
    let resp = client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.results[0].action, "created");

    // Verify exists
    let get_resp = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("lifecycle".to_string()),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(get_resp.resources.len(), 1);

    // Delete
    let del_resp = client
        .delete(DeleteRequest {
            kind: "Network".to_string(),
            name: "lifecycle".to_string(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(del_resp.found);

    // Verify gone
    let get_resp2 = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("lifecycle".to_string()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(get_resp2.resources.is_empty());

    // Reapply
    let resp2 = client
        .apply(ApplyRequest {
            resources: vec![to_json(&net)],
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp2.results[0].action, "created");

    // Verify back
    let get_resp3 = client
        .get(GetRequest {
            kind: "Network".to_string(),
            name: Some("lifecycle".to_string()),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(get_resp3.resources.len(), 1);
}

// ========== Image download tests ==========

#[tokio::test]
async fn apply_image_triggers_download() {
    use sha2::{Digest, Sha256};

    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let images_dir = dir.path().join("images");
    let image_cache =
        Arc::new(ImageCache::new(images_dir.clone()).expect("failed to create image cache"));

    // Start a test HTTP server serving known data
    let data = b"fake qcow2 image data for testing";
    let digest = Sha256::digest(data);
    let hex_digest = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let checksum = format!("sha256:{hex_digest}");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/image");

    let served_data = data.to_vec();
    let http_handle = tokio::spawn(async move {
        use axum::{Router, routing::get};
        let app = Router::new().route(
            "/image",
            get(move || {
                let d = served_data.clone();
                async move { d }
            }),
        );
        axum::serve(listener, app).await.unwrap();
    });

    let _shutdown = start_server_with_cache(&sock, store, image_cache);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let img = Resource::Image(stratus_resources::Image {
        name: "test-img".to_string(),
        source_url: url,
        format: stratus_resources::ImageFormat::Raw,
        architecture: None,
        os_type: None,
        checksum: Some(checksum),
        min_disk_gb: None,
        min_ram_mb: None,
    });

    let resp = client
        .apply(ApplyRequest {
            resources: vec![to_json(&img)],
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].action, "created");

    // Verify image is cached
    let cached_path = images_dir.join("sha256").join(&hex_digest);
    assert!(
        cached_path.exists(),
        "image should be cached at {cached_path:?}"
    );
    let cached_content = std::fs::read(&cached_path).unwrap();
    assert_eq!(cached_content, data);

    // No .partial files should remain
    let downloading_dir = images_dir.join(".downloading");
    let partials: Vec<_> = std::fs::read_dir(&downloading_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(partials.is_empty(), "no partial files should remain");

    http_handle.abort();
}

#[tokio::test]
async fn delete_image_evicts_cache() {
    use sha2::{Digest, Sha256};

    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");
    let store = temp_store(dir.path());

    let images_dir = dir.path().join("images");
    let image_cache =
        Arc::new(ImageCache::new(images_dir.clone()).expect("failed to create image cache"));

    // Start a test HTTP server serving known data
    let data = b"fake image data for eviction test";
    let digest = Sha256::digest(data);
    let hex_digest = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let checksum = format!("sha256:{hex_digest}");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/image");

    let served_data = data.to_vec();
    let http_handle = tokio::spawn(async move {
        use axum::{Router, routing::get};
        let app = Router::new().route(
            "/image",
            get(move || {
                let d = served_data.clone();
                async move { d }
            }),
        );
        axum::serve(listener, app).await.unwrap();
    });

    let _shutdown = start_server_with_cache(&sock, store, image_cache);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = connect(sock).await;

    let img = Resource::Image(stratus_resources::Image {
        name: "evict-img".to_string(),
        source_url: url,
        format: stratus_resources::ImageFormat::Raw,
        architecture: None,
        os_type: None,
        checksum: Some(checksum),
        min_disk_gb: None,
        min_ram_mb: None,
    });

    // Apply triggers download
    client
        .apply(ApplyRequest {
            resources: vec![to_json(&img)],
        })
        .await
        .unwrap();

    // Verify cached file exists
    let cached_path = images_dir.join("sha256").join(&hex_digest);
    assert!(cached_path.exists(), "image should be cached after apply");

    // Delete the image resource
    let del_resp = client
        .delete(DeleteRequest {
            kind: "Image".to_string(),
            name: "evict-img".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(del_resp.found);

    // Cached file should be gone
    assert!(
        !cached_path.exists(),
        "cached image file should be evicted after delete"
    );

    http_handle.abort();
}
