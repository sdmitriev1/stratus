use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use stratus_resources::*;
use stratus_store::{EventType, Store, StoreError, WatchableStore};
use tokio_stream::StreamExt;

fn temp_db_path() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    (dir, path)
}

fn make_network(name: &str) -> Resource {
    Resource::Network(Network {
        name: name.to_string(),
    })
}

fn make_subnet(name: &str) -> Resource {
    Resource::Subnet(Subnet {
        name: name.to_string(),
        network: "test-net".to_string(),
        cidr: "10.0.0.0/24".parse().unwrap(),
        gateway: "10.0.0.1".parse().unwrap(),
        dns: vec![],
        dhcp: true,
        nat: NatMode::None,
        isolated: false,
    })
}

fn make_instance(name: &str) -> Resource {
    Resource::Instance(Instance {
        name: name.to_string(),
        cpus: 2,
        memory_mb: 1024,
        disk_gb: 20,
        image: "ubuntu".to_string(),
        secure_boot: false,
        vtpm: false,
        interfaces: vec![Interface {
            subnet: "test-subnet".to_string(),
            ip: None,
            mac: None,
            security_groups: vec![],
        }],
        user_data: None,
        ssh_authorized_keys: vec![],
    })
}

fn make_security_group(name: &str) -> Resource {
    Resource::SecurityGroup(SecurityGroup {
        name: name.to_string(),
        rules: vec![SecurityGroupRule {
            direction: Direction::Ingress,
            protocol: Protocol::Tcp,
            port: Some(22),
            remote_cidr: Some("0.0.0.0/0".parse().unwrap()),
            remote_sg: None,
        }],
    })
}

fn make_image(name: &str) -> Resource {
    Resource::Image(Image {
        name: name.to_string(),
        source_url: "https://example.com/image.qcow2".to_string(),
        format: ImageFormat::Qcow2,
        architecture: None,
        os_type: None,
        checksum: None,
        min_disk_gb: None,
        min_ram_mb: None,
    })
}

fn make_port_forward(name: &str) -> Resource {
    Resource::PortForward(PortForward {
        name: name.to_string(),
        instance: "test-vm".to_string(),
        host_port: 8080,
        instance_port: 80,
        protocol: PortProtocol::Tcp,
        host_ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    })
}

// --- CRUD Tests ---

#[test]
fn test_open_creates_database() {
    let (_dir, path) = temp_db_path();

    // First open creates the file.
    {
        let _store = Store::open(&path).unwrap();
        assert!(path.exists());
    }

    // Second open (reopen) succeeds.
    let _store2 = Store::open(&path).unwrap();
}

#[test]
fn test_put_and_get_network() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    let net = make_network("prod");
    store.put(&net).unwrap();

    let got = store.get("Network", "prod").unwrap();
    assert_eq!(got, Some(net));
}

#[test]
fn test_put_and_get_all_resource_kinds() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    let resources = vec![
        make_network("net1"),
        make_subnet("sub1"),
        make_instance("vm1"),
        make_security_group("sg1"),
        make_image("img1"),
        make_port_forward("pf1"),
    ];

    for r in &resources {
        store.put(r).unwrap();
    }

    assert_eq!(
        store.get("Network", "net1").unwrap().as_ref(),
        Some(&resources[0])
    );
    assert_eq!(
        store.get("Subnet", "sub1").unwrap().as_ref(),
        Some(&resources[1])
    );
    assert_eq!(
        store.get("Instance", "vm1").unwrap().as_ref(),
        Some(&resources[2])
    );
    assert_eq!(
        store.get("SecurityGroup", "sg1").unwrap().as_ref(),
        Some(&resources[3])
    );
    assert_eq!(
        store.get("Image", "img1").unwrap().as_ref(),
        Some(&resources[4])
    );
    assert_eq!(
        store.get("PortForward", "pf1").unwrap().as_ref(),
        Some(&resources[5])
    );
}

#[test]
fn test_get_nonexistent_returns_none() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    assert_eq!(store.get("Network", "nope").unwrap(), None);
}

#[test]
fn test_put_overwrites_existing() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    let net1 = make_network("prod");
    store.put(&net1).unwrap();

    // Put again — should return old value.
    let net2 = make_network("prod");
    let old = store.put(&net2).unwrap();
    assert_eq!(old, Some(net1));

    let got = store.get("Network", "prod").unwrap();
    assert_eq!(got, Some(net2));
}

#[test]
fn test_delete_existing() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    let net = make_network("prod");
    store.put(&net).unwrap();

    let deleted = store.delete("Network", "prod").unwrap();
    assert_eq!(deleted, Some(net));

    assert_eq!(store.get("Network", "prod").unwrap(), None);
}

#[test]
fn test_delete_nonexistent_returns_none() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    assert_eq!(store.delete("Network", "nope").unwrap(), None);
}

#[test]
fn test_list_empty() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    let list = store.list("Network").unwrap();
    assert!(list.is_empty());
}

#[test]
fn test_list_returns_all() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    let nets: Vec<Resource> = (0..3).map(|i| make_network(&format!("net{i}"))).collect();
    for n in &nets {
        store.put(n).unwrap();
    }

    let mut list = store.list("Network").unwrap();
    list.sort_by(|a, b| a.name().cmp(b.name()));
    assert_eq!(list.len(), 3);
    assert_eq!(list, nets);
}

#[test]
fn test_list_only_matching_kind() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    store.put(&make_network("net1")).unwrap();
    store.put(&make_subnet("sub1")).unwrap();

    let nets = store.list("Network").unwrap();
    assert_eq!(nets.len(), 1);
    assert_eq!(nets[0].kind_str(), "Network");
}

#[test]
fn test_unknown_kind_returns_error() {
    let (_dir, path) = temp_db_path();
    let store = Store::open(&path).unwrap();

    let err = store.get("Bogus", "x").unwrap_err();
    assert!(matches!(err, StoreError::UnknownKind(_)));

    let err = store.list("Bogus").unwrap_err();
    assert!(matches!(err, StoreError::UnknownKind(_)));
}

#[test]
fn test_schema_version_mismatch() {
    let (_dir, path) = temp_db_path();

    // Create a store normally.
    {
        let _store = Store::open(&path).unwrap();
    }

    // Tamper with schema version.
    {
        let db = redb::Database::open(&path).unwrap();
        let txn = db.begin_write().unwrap();
        {
            let mut config = txn.open_table(stratus_store::schema::CONFIG).unwrap();
            let bad_version = serde_json::to_vec(&999u32).unwrap();
            config
                .insert("schema_version", bad_version.as_slice())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    // Reopen should fail with SchemaMismatch.
    let err = Store::open(&path).unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::SchemaMismatch {
                expected: 1,
                found: 999
            }
        ),
        "expected SchemaMismatch, got: {err:?}"
    );
}

// --- WatchableStore Tests ---

#[tokio::test]
async fn test_watchable_put_increments_revision() {
    let (_dir, path) = temp_db_path();
    let store = WatchableStore::open(&path).unwrap();

    let (r1, _) = store.put(&make_network("a")).unwrap();
    let (r2, _) = store.put(&make_network("b")).unwrap();
    let (r3, _) = store.put(&make_network("c")).unwrap();

    assert_eq!(r1, 1);
    assert_eq!(r2, 2);
    assert_eq!(r3, 3);
    assert_eq!(store.revision(), 3);
}

#[tokio::test]
async fn test_revision_persists_across_reopen() {
    let (_dir, path) = temp_db_path();

    {
        let store = WatchableStore::open(&path).unwrap();
        store.put(&make_network("a")).unwrap();
        store.put(&make_network("b")).unwrap();
        assert_eq!(store.revision(), 2);
    }

    let store = WatchableStore::open(&path).unwrap();
    assert_eq!(store.revision(), 2);

    let (rev, _) = store.put(&make_network("c")).unwrap();
    assert_eq!(rev, 3);
}

#[tokio::test]
async fn test_watch_receives_matching_events() {
    let (_dir, path) = temp_db_path();
    let store = Arc::new(WatchableStore::open(&path).unwrap());

    let mut stream = store.watch("Network/", 1).unwrap();

    store.put(&make_network("foo")).unwrap();

    let event = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");

    assert_eq!(event.event_type, EventType::Put);
    assert_eq!(event.key, "Network/foo");
    assert_eq!(event.revision, 1);
    assert!(event.resource.is_some());
}

#[tokio::test]
async fn test_watch_filters_non_matching() {
    let (_dir, path) = temp_db_path();
    let store = Arc::new(WatchableStore::open(&path).unwrap());

    let mut stream = store.watch("Network/", 1).unwrap();

    store.put(&make_subnet("sub1")).unwrap();

    let result = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;
    assert!(result.is_err(), "should timeout — no matching events");
}

#[tokio::test]
async fn test_watch_receives_delete_events() {
    let (_dir, path) = temp_db_path();
    let store = Arc::new(WatchableStore::open(&path).unwrap());

    let mut stream = store.watch("Instance/", 1).unwrap();

    store.put(&make_instance("vm1")).unwrap();
    store.delete("Instance", "vm1").unwrap();

    let put_event = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert_eq!(put_event.event_type, EventType::Put);

    let del_event = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert_eq!(del_event.event_type, EventType::Delete);
    assert!(del_event.resource.is_none());
    assert_eq!(del_event.key, "Instance/vm1");
}

#[tokio::test]
async fn test_watch_replays_from_revision() {
    let (_dir, path) = temp_db_path();
    let store = Arc::new(WatchableStore::open(&path).unwrap());

    // Put 3 networks.
    store.put(&make_network("a")).unwrap();
    store.put(&make_network("b")).unwrap();
    store.put(&make_network("c")).unwrap();

    // Watch from revision 2 — should replay events 2 and 3.
    let mut stream = store.watch("Network/", 2).unwrap();

    let e1 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert_eq!(e1.revision, 2);
    assert_eq!(e1.key, "Network/b");

    let e2 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert_eq!(e2.revision, 3);
    assert_eq!(e2.key, "Network/c");

    // Then a live event.
    store.put(&make_network("d")).unwrap();
    let e3 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert_eq!(e3.revision, 4);
    assert_eq!(e3.key, "Network/d");
}

#[tokio::test]
async fn test_watch_replay_after_reopen() {
    let (_dir, path) = temp_db_path();

    {
        let store = WatchableStore::open(&path).unwrap();
        store.put(&make_network("a")).unwrap();
        store.put(&make_network("b")).unwrap();
    }

    let store = Arc::new(WatchableStore::open(&path).unwrap());
    let mut stream = store.watch("Network/", 1).unwrap();

    let e1 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert_eq!(e1.revision, 1);

    let e2 = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    assert_eq!(e2.revision, 2);
}

#[tokio::test]
async fn test_changelog_compaction() {
    let (_dir, path) = temp_db_path();
    let store = WatchableStore::open(&path).unwrap();

    store.put(&make_network("a")).unwrap();
    store.put(&make_network("b")).unwrap();

    // Compact with zero duration — all entries are older than "now".
    let removed = store.compact(Duration::ZERO).unwrap();
    assert_eq!(removed, 2);

    // Watch from revision 1 should get nothing historical.
    let mut stream = store.watch("Network/", 1).unwrap();
    let result = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;
    assert!(result.is_err(), "should timeout — changelog was compacted");
}

#[tokio::test]
async fn test_concurrent_readers_during_write() {
    let (_dir, path) = temp_db_path();
    let store = Arc::new(WatchableStore::open(&path).unwrap());

    // Spawn a writer.
    let writer_store = Arc::clone(&store);
    let writer = tokio::spawn(async move {
        for i in 0..10 {
            writer_store.put(&make_network(&format!("net{i}"))).unwrap();
        }
    });

    // Spawn multiple readers.
    let mut readers = vec![];
    for _ in 0..5 {
        let reader_store = Arc::clone(&store);
        readers.push(tokio::spawn(async move {
            for _ in 0..10 {
                let _ = reader_store.list("Network").unwrap();
            }
        }));
    }

    writer.await.unwrap();
    for r in readers {
        r.await.unwrap();
    }

    assert_eq!(store.list("Network").unwrap().len(), 10);
}

#[tokio::test]
async fn test_watch_multiple_subscribers() {
    let (_dir, path) = temp_db_path();
    let store = Arc::new(WatchableStore::open(&path).unwrap());

    let mut stream1 = store.watch("Network/", 1).unwrap();
    let mut stream2 = store.watch("Network/", 1).unwrap();

    store.put(&make_network("x")).unwrap();

    let e1 = tokio::time::timeout(Duration::from_secs(1), stream1.next())
        .await
        .expect("timeout")
        .expect("stream ended");
    let e2 = tokio::time::timeout(Duration::from_secs(1), stream2.next())
        .await
        .expect("timeout")
        .expect("stream ended");

    assert_eq!(e1.key, "Network/x");
    assert_eq!(e2.key, "Network/x");
}

#[tokio::test]
async fn test_put_get_roundtrip_preserves_fields() {
    let (_dir, path) = temp_db_path();
    let store = WatchableStore::open(&path).unwrap();

    // Fully-populated Instance with all optional fields.
    let instance = Resource::Instance(Instance {
        name: "full-vm".to_string(),
        cpus: 4,
        memory_mb: 8192,
        disk_gb: 100,
        image: "ubuntu-22.04".to_string(),
        secure_boot: true,
        vtpm: true,
        interfaces: vec![
            Interface {
                subnet: "mgmt".to_string(),
                ip: Some("10.0.0.5".parse().unwrap()),
                mac: Some("02:df:aa:bb:cc:dd".to_string()),
                security_groups: vec!["allow-ssh".to_string(), "allow-http".to_string()],
            },
            Interface {
                subnet: "data".to_string(),
                ip: Some("10.1.0.5".parse().unwrap()),
                mac: None,
                security_groups: vec![],
            },
        ],
        user_data: Some("#!/bin/bash\necho hello".to_string()),
        ssh_authorized_keys: vec![
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITest user@host".to_string(),
        ],
    });

    store.put(&instance).unwrap();
    let got = store.get("Instance", "full-vm").unwrap().unwrap();
    assert_eq!(got, instance);
}
