use stratus_resources::*;

#[test]
fn test_parse_single_document() {
    let yaml = "kind: Network\nname: mgmt\n";
    let resources = parse_yaml_documents(yaml).unwrap();
    assert_eq!(resources.len(), 1);
    assert!(matches!(&resources[0], Resource::Network(_)));
}

#[test]
fn test_parse_multi_document() {
    let yaml = r#"kind: Network
name: mgmt
---
kind: Image
name: ubuntu
source_url: https://example.com/img
format: qcow2
---
kind: Network
name: data
"#;
    let resources = parse_yaml_documents(yaml).unwrap();
    assert_eq!(resources.len(), 3);
    assert!(matches!(&resources[0], Resource::Network(_)));
    assert!(matches!(&resources[1], Resource::Image(_)));
    assert!(matches!(&resources[2], Resource::Network(_)));
}

#[test]
fn test_parse_full_environment() {
    let yaml = r#"kind: Network
name: mgmt
---
kind: Subnet
name: mgmt-sub
network: mgmt
cidr: 10.0.0.0/24
gateway: 10.0.0.1
nat: masquerade
---
kind: Image
name: ubuntu
source_url: https://example.com/ubuntu.img
format: qcow2
---
kind: SecurityGroup
name: default
rules:
  - direction: ingress
    protocol: tcp
    port: 22
    remote_cidr: 0.0.0.0/0
---
kind: Instance
name: router
cpus: 2
memory_mb: 2048
image: ubuntu
interfaces:
  - subnet: mgmt-sub
    ip: 10.0.0.10
    security_groups:
      - default
---
kind: PortForward
name: ssh-router
instance: router
host_port: 2222
instance_port: 22
"#;
    let resources = parse_yaml_documents(yaml).unwrap();
    assert_eq!(resources.len(), 6);
    assert_eq!(resources[0].kind_str(), "Network");
    assert_eq!(resources[1].kind_str(), "Subnet");
    assert_eq!(resources[2].kind_str(), "Image");
    assert_eq!(resources[3].kind_str(), "SecurityGroup");
    assert_eq!(resources[4].kind_str(), "Instance");
    assert_eq!(resources[5].kind_str(), "PortForward");
}

#[test]
fn test_parse_empty_input() {
    let result = parse_yaml_documents("");
    assert!(matches!(result, Err(ParseError::Empty)));
}

#[test]
fn test_parse_invalid_kind() {
    let yaml = "kind: FooBar\nname: x\n";
    let result = parse_yaml_documents(yaml);
    assert!(result.is_err());
}

#[test]
fn test_parse_missing_required_field() {
    let yaml = "kind: Network\n";
    let result = parse_yaml_documents(yaml);
    assert!(result.is_err());
}

#[test]
fn test_parse_bad_cidr() {
    let yaml = "kind: Subnet\nname: s\nnetwork: n\ncidr: not-a-cidr\ngateway: 10.0.0.1\n";
    let result = parse_yaml_documents(yaml);
    assert!(result.is_err());
}

#[test]
fn test_parse_bad_ip() {
    let yaml = "kind: Subnet\nname: s\nnetwork: n\ncidr: 10.0.0.0/24\ngateway: not-an-ip\n";
    let result = parse_yaml_documents(yaml);
    assert!(result.is_err());
}

#[test]
fn test_roundtrip_all_types() {
    let resources = vec![
        Resource::Network(Network { name: "n1".into() }),
        Resource::Subnet(Subnet {
            name: "s1".into(),
            network: "n1".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
        Resource::Image(Image {
            name: "img1".into(),
            source_url: "https://example.com/img".into(),
            format: ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        }),
        Resource::SecurityGroup(SecurityGroup {
            name: "sg1".into(),
            rules: vec![SecurityGroupRule {
                direction: Direction::Ingress,
                protocol: Protocol::Any,
                port: None,
                remote_cidr: Some("0.0.0.0/0".parse().unwrap()),
                remote_sg: None,
            }],
        }),
        Resource::Instance(Instance {
            name: "i1".into(),
            cpus: 2,
            memory_mb: 1024,
            disk_gb: 20,
            image: "img1".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "s1".into(),
                ip: None,
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
        Resource::PortForward(PortForward {
            name: "pf1".into(),
            instance: "i1".into(),
            host_port: 2222,
            instance_port: 22,
            protocol: PortProtocol::Tcp,
            host_ip: "0.0.0.0".parse().unwrap(),
        }),
    ];

    for resource in &resources {
        let yaml = serde_yaml::to_string(resource).unwrap();
        let parsed: Resource = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(&parsed, resource);
    }
}

#[test]
fn test_roundtrip_multi_document() {
    let resources = vec![
        Resource::Network(Network { name: "n1".into() }),
        Resource::Network(Network { name: "n2".into() }),
        Resource::Image(Image {
            name: "img".into(),
            source_url: "https://example.com/img".into(),
            format: ImageFormat::Raw,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        }),
    ];

    let serialized = serialize_yaml_documents(&resources).unwrap();
    let parsed = parse_yaml_documents(&serialized).unwrap();
    assert_eq!(parsed, resources);
}
