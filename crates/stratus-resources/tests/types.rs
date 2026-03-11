use std::net::IpAddr;

use stratus_resources::*;

#[test]
fn test_network_deserialize() {
    let yaml = "kind: Network\nname: mgmt";
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(r.name(), "mgmt");
    assert!(matches!(r, Resource::Network(_)));
}

#[test]
fn test_subnet_defaults() {
    let yaml = r#"
kind: Subnet
name: mgmt-sub
network: mgmt
cidr: 10.0.0.0/24
gateway: 10.0.0.1
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Subnet(s) = r {
        assert!(s.dhcp);
        assert_eq!(s.nat, NatMode::None);
        assert!(!s.isolated);
        assert!(s.dns.is_empty());
    } else {
        panic!("expected Subnet");
    }
}

#[test]
fn test_subnet_all_fields() {
    let yaml = r#"
kind: Subnet
name: mgmt-sub
network: mgmt
cidr: 10.0.0.0/24
gateway: 10.0.0.1
dns:
  - 8.8.8.8
  - 1.1.1.1
dhcp: false
nat: masquerade
isolated: true
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Subnet(s) = r {
        assert!(!s.dhcp);
        assert_eq!(s.nat, NatMode::Masquerade);
        assert!(s.isolated);
        assert_eq!(s.dns.len(), 2);
    } else {
        panic!("expected Subnet");
    }
}

#[test]
fn test_subnet_ipv6() {
    let yaml = r#"
kind: Subnet
name: v6-sub
network: v6net
cidr: fd00::/120
gateway: fd00::1
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Subnet(s) = r {
        assert!(s.cidr.addr().is_ipv6());
        assert!(s.gateway.is_ipv6());
    } else {
        panic!("expected Subnet");
    }
}

#[test]
fn test_instance_defaults() {
    let yaml = r#"
kind: Instance
name: vm1
cpus: 2
memory_mb: 1024
image: ubuntu
interfaces:
  - subnet: mgmt-sub
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Instance(i) = r {
        assert_eq!(i.disk_gb, 20);
        assert!(!i.secure_boot);
        assert!(!i.vtpm);
        assert!(i.user_data.is_none());
        assert!(i.ssh_authorized_keys.is_empty());
    } else {
        panic!("expected Instance");
    }
}

#[test]
fn test_instance_full() {
    let yaml = r#"
kind: Instance
name: vm1
cpus: 4
memory_mb: 4096
disk_gb: 100
image: ubuntu
secure_boot: true
vtpm: true
interfaces:
  - subnet: mgmt-sub
    ip: 10.0.0.10
    mac: "02:df:00:00:00:01"
    security_groups:
      - default
  - subnet: data-sub
user_data: |
  #cloud-config
  packages: [nginx]
ssh_authorized_keys:
  - ssh-ed25519 AAAA...
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Instance(i) = r {
        assert_eq!(i.cpus, 4);
        assert_eq!(i.memory_mb, 4096);
        assert_eq!(i.disk_gb, 100);
        assert!(i.secure_boot);
        assert!(i.vtpm);
        assert_eq!(i.interfaces.len(), 2);
        assert!(i.user_data.is_some());
        assert_eq!(i.ssh_authorized_keys.len(), 1);
    } else {
        panic!("expected Instance");
    }
}

#[test]
fn test_interface_mac_empty_string() {
    let yaml = r#"
subnet: test-sub
mac: ""
"#;
    let iface: Interface = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(iface.mac, None);
}

#[test]
fn test_interface_mac_omitted() {
    let yaml = "subnet: test-sub\n";
    let iface: Interface = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(iface.mac, None);
}

#[test]
fn test_interface_mac_explicit() {
    let yaml = r#"
subnet: test-sub
mac: "02:df:aa:bb:cc:dd"
"#;
    let iface: Interface = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(iface.mac, Some("02:df:aa:bb:cc:dd".to_string()));
}

#[test]
fn test_interface_ipv6() {
    let yaml = r#"
subnet: v6-sub
ip: "fd00::10"
"#;
    let iface: Interface = serde_yaml::from_str(yaml).unwrap();
    assert!(iface.ip.unwrap().is_ipv6());
}

#[test]
fn test_security_group_with_remote_cidr() {
    let yaml = r#"
kind: SecurityGroup
name: web
rules:
  - direction: ingress
    protocol: tcp
    port: 80
    remote_cidr: 0.0.0.0/0
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::SecurityGroup(sg) = r {
        assert_eq!(sg.rules.len(), 1);
        assert!(sg.rules[0].remote_cidr.is_some());
        assert!(sg.rules[0].remote_sg.is_none());
    } else {
        panic!("expected SecurityGroup");
    }
}

#[test]
fn test_security_group_with_remote_sg() {
    let yaml = r#"
kind: SecurityGroup
name: backend
rules:
  - direction: ingress
    protocol: tcp
    port: 8080
    remote_sg: web
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::SecurityGroup(sg) = r {
        assert!(sg.rules[0].remote_sg.is_some());
        assert!(sg.rules[0].remote_cidr.is_none());
    } else {
        panic!("expected SecurityGroup");
    }
}

#[test]
fn test_security_group_with_ipv6_cidr() {
    let yaml = r#"
kind: SecurityGroup
name: v6sg
rules:
  - direction: ingress
    protocol: tcp
    port: 443
    remote_cidr: "::/0"
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::SecurityGroup(sg) = r {
        let cidr = sg.rules[0].remote_cidr.unwrap();
        assert!(cidr.addr().is_ipv6());
    } else {
        panic!("expected SecurityGroup");
    }
}

#[test]
fn test_image_minimal() {
    let yaml = r#"
kind: Image
name: ubuntu
source_url: https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img
format: qcow2
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Image(img) = r {
        assert_eq!(img.name, "ubuntu");
        assert_eq!(img.format, ImageFormat::Qcow2);
        assert!(img.architecture.is_none());
        assert!(img.checksum.is_none());
    } else {
        panic!("expected Image");
    }
}

#[test]
fn test_image_full() {
    let yaml = r#"
kind: Image
name: ubuntu
source_url: https://example.com/image.img
format: qcow2
architecture: amd64
os_type: linux
checksum: "sha256:abcdef1234567890"
min_disk_gb: 10
min_ram_mb: 512
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Image(img) = r {
        assert_eq!(img.architecture, Some("amd64".to_string()));
        assert_eq!(img.os_type, Some("linux".to_string()));
        assert_eq!(img.min_disk_gb, Some(10));
        assert_eq!(img.min_ram_mb, Some(512));
    } else {
        panic!("expected Image");
    }
}

#[test]
fn test_image_file_url() {
    let yaml = r#"
kind: Image
name: local-img
source_url: "file:///var/lib/images/test.qcow2"
format: qcow2
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::Image(img) = r {
        assert!(img.source_url.starts_with("file://"));
    } else {
        panic!("expected Image");
    }
}

#[test]
fn test_portforward_defaults() {
    let yaml = r#"
kind: PortForward
name: ssh-fwd
instance: vm1
host_port: 2222
instance_port: 22
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::PortForward(pf) = r {
        assert_eq!(pf.protocol, PortProtocol::Tcp);
        assert_eq!(pf.host_ip, IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    } else {
        panic!("expected PortForward");
    }
}

#[test]
fn test_portforward_full() {
    let yaml = r#"
kind: PortForward
name: dns-fwd
instance: vm1
host_port: 5353
instance_port: 53
protocol: udp
host_ip: 127.0.0.1
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::PortForward(pf) = r {
        assert_eq!(pf.protocol, PortProtocol::Udp);
        assert_eq!(
            pf.host_ip,
            IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
        );
    } else {
        panic!("expected PortForward");
    }
}

#[test]
fn test_portforward_ipv6() {
    let yaml = r#"
kind: PortForward
name: ssh-fwd
instance: vm1
host_port: 2222
instance_port: 22
host_ip: "::"
"#;
    let r: Resource = serde_yaml::from_str(yaml).unwrap();
    if let Resource::PortForward(pf) = r {
        assert!(pf.host_ip.is_ipv6());
    } else {
        panic!("expected PortForward");
    }
}

#[test]
fn test_all_enum_variants() {
    // NatMode
    for (s, v) in [("none", NatMode::None), ("masquerade", NatMode::Masquerade)] {
        let json = format!("\"{}\"", s);
        let parsed: NatMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
        let ser = serde_json::to_string(&v).unwrap();
        assert_eq!(ser, json);
    }

    // Direction
    for (s, v) in [
        ("ingress", Direction::Ingress),
        ("egress", Direction::Egress),
    ] {
        let json = format!("\"{}\"", s);
        let parsed: Direction = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
    }

    // Protocol
    for (s, v) in [
        ("tcp", Protocol::Tcp),
        ("udp", Protocol::Udp),
        ("icmp", Protocol::Icmp),
        ("any", Protocol::Any),
    ] {
        let json = format!("\"{}\"", s);
        let parsed: Protocol = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
    }

    // ImageFormat
    for (s, v) in [("qcow2", ImageFormat::Qcow2), ("raw", ImageFormat::Raw)] {
        let json = format!("\"{}\"", s);
        let parsed: ImageFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
    }

    // PortProtocol
    for (s, v) in [("tcp", PortProtocol::Tcp), ("udp", PortProtocol::Udp)] {
        let json = format!("\"{}\"", s);
        let parsed: PortProtocol = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
    }
}

#[test]
fn test_resource_name_and_kind() {
    let cases: Vec<(&str, &str, &str)> = vec![
        ("kind: Network\nname: n1", "n1", "Network"),
        (
            "kind: Subnet\nname: s1\nnetwork: n1\ncidr: 10.0.0.0/24\ngateway: 10.0.0.1",
            "s1",
            "Subnet",
        ),
        (
            "kind: Instance\nname: i1\ncpus: 1\nmemory_mb: 512\nimage: img\ninterfaces:\n  - subnet: s1",
            "i1",
            "Instance",
        ),
        (
            "kind: SecurityGroup\nname: sg1\nrules:\n  - direction: ingress\n    protocol: any\n    remote_cidr: 0.0.0.0/0",
            "sg1",
            "SecurityGroup",
        ),
        (
            "kind: Image\nname: img1\nsource_url: https://example.com/img\nformat: qcow2",
            "img1",
            "Image",
        ),
        (
            "kind: PortForward\nname: pf1\ninstance: i1\nhost_port: 22\ninstance_port: 22",
            "pf1",
            "PortForward",
        ),
    ];
    for (yaml, expected_name, expected_kind) in cases {
        let r: Resource = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(r.name(), expected_name);
        assert_eq!(r.kind_str(), expected_kind);
    }
}
