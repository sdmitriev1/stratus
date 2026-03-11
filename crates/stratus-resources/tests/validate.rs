use stratus_resources::*;

fn base_resources() -> Vec<Resource> {
    vec![
        Resource::Network(Network {
            name: "mgmt".into(),
        }),
        Resource::Subnet(Subnet {
            name: "mgmt-sub".into(),
            network: "mgmt".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
        Resource::Image(Image {
            name: "ubuntu".into(),
            source_url: "https://example.com/img".into(),
            format: ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        }),
        Resource::SecurityGroup(SecurityGroup {
            name: "default".into(),
            rules: vec![SecurityGroupRule {
                direction: Direction::Ingress,
                protocol: Protocol::Tcp,
                port: Some(22),
                remote_cidr: Some("0.0.0.0/0".parse().unwrap()),
                remote_sg: None,
            }],
        }),
        Resource::Instance(Instance {
            name: "vm1".into(),
            cpus: 2,
            memory_mb: 1024,
            disk_gb: 20,
            image: "ubuntu".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "mgmt-sub".into(),
                ip: None,
                mac: None,
                security_groups: vec!["default".into()],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
        Resource::PortForward(PortForward {
            name: "ssh-fwd".into(),
            instance: "vm1".into(),
            host_port: 2222,
            instance_port: 22,
            protocol: PortProtocol::Tcp,
            host_ip: "0.0.0.0".parse().unwrap(),
        }),
    ]
}

#[test]
fn test_valid_full_set() {
    validate(&base_resources()).unwrap();
}

#[test]
fn test_valid_full_set_ipv6() {
    let resources = vec![
        Resource::Network(Network {
            name: "v6net".into(),
        }),
        Resource::Subnet(Subnet {
            name: "v6-sub".into(),
            network: "v6net".into(),
            cidr: "fd00::/120".parse().unwrap(),
            gateway: "fd00::1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
        Resource::Image(Image {
            name: "ubuntu".into(),
            source_url: "https://example.com/img".into(),
            format: ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        }),
        Resource::SecurityGroup(SecurityGroup {
            name: "default".into(),
            rules: vec![SecurityGroupRule {
                direction: Direction::Ingress,
                protocol: Protocol::Tcp,
                port: Some(443),
                remote_cidr: Some("::/0".parse().unwrap()),
                remote_sg: None,
            }],
        }),
        Resource::Instance(Instance {
            name: "vm1".into(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "ubuntu".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "v6-sub".into(),
                ip: Some("fd00::10".parse().unwrap()),
                mac: None,
                security_groups: vec!["default".into()],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
    ];
    validate(&resources).unwrap();
}

#[test]
fn test_subnet_missing_network() {
    let resources = vec![Resource::Subnet(Subnet {
        name: "s".into(),
        network: "nonexistent".into(),
        cidr: "10.0.0.0/24".parse().unwrap(),
        gateway: "10.0.0.1".parse().unwrap(),
        dns: vec![],
        dhcp: true,
        nat: NatMode::None,
        isolated: false,
    })];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::MissingReference { to_kind, .. } if to_kind == "Network"
    )));
}

#[test]
fn test_instance_missing_image() {
    let resources = vec![
        Resource::Network(Network { name: "n".into() }),
        Resource::Subnet(Subnet {
            name: "s".into(),
            network: "n".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
        Resource::Instance(Instance {
            name: "i".into(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "nonexistent".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "s".into(),
                ip: None,
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::MissingReference { to_kind, .. } if to_kind == "Image"
    )));
}

#[test]
fn test_instance_missing_subnet() {
    let resources = vec![
        Resource::Image(Image {
            name: "img".into(),
            source_url: "https://example.com/img".into(),
            format: ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        }),
        Resource::Instance(Instance {
            name: "i".into(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "img".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "nonexistent".into(),
                ip: None,
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::MissingReference { to_kind, .. } if to_kind == "Subnet"
    )));
}

#[test]
fn test_instance_missing_security_group() {
    let mut resources = base_resources();
    if let Resource::Instance(ref mut i) = resources[4] {
        i.interfaces[0].security_groups = vec!["nonexistent".into()];
    }
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::MissingReference { to_kind, .. } if to_kind == "SecurityGroup"
    )));
}

#[test]
fn test_portforward_missing_instance() {
    let resources = vec![Resource::PortForward(PortForward {
        name: "pf".into(),
        instance: "nonexistent".into(),
        host_port: 22,
        instance_port: 22,
        protocol: PortProtocol::Tcp,
        host_ip: "0.0.0.0".parse().unwrap(),
    })];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::MissingReference { to_kind, .. } if to_kind == "Instance"
    )));
}

#[test]
fn test_sg_remote_sg_missing() {
    let resources = vec![Resource::SecurityGroup(SecurityGroup {
        name: "sg".into(),
        rules: vec![SecurityGroupRule {
            direction: Direction::Ingress,
            protocol: Protocol::Tcp,
            port: Some(80),
            remote_cidr: None,
            remote_sg: Some("nonexistent".into()),
        }],
    })];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::MissingReference { to_kind, .. } if to_kind == "SecurityGroup"
    )));
}

#[test]
fn test_duplicate_network_names() {
    let resources = vec![
        Resource::Network(Network { name: "n".into() }),
        Resource::Network(Network { name: "n".into() }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::DuplicateName { kind, name } if kind == "Network" && name == "n"
    )));
}

#[test]
fn test_duplicate_instance_names() {
    let mut resources = base_resources();
    let instance_clone = resources[4].clone();
    resources.push(instance_clone);
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::DuplicateName { kind, .. } if kind == "Instance"
    )));
}

#[test]
fn test_duplicate_names_different_kinds() {
    // Same name across different kinds is allowed
    let resources = vec![
        Resource::Network(Network { name: "foo".into() }),
        Resource::Image(Image {
            name: "foo".into(),
            source_url: "https://example.com/img".into(),
            format: ImageFormat::Qcow2,
            architecture: None,
            os_type: None,
            checksum: None,
            min_disk_gb: None,
            min_ram_mb: None,
        }),
    ];
    validate(&resources).unwrap();
}

#[test]
fn test_subnet_gateway_outside_cidr() {
    let resources = vec![
        Resource::Network(Network { name: "n".into() }),
        Resource::Subnet(Subnet {
            name: "s".into(),
            network: "n".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "192.168.1.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("not within CIDR")
    )));
}

#[test]
fn test_subnet_gateway_is_network_address() {
    let resources = vec![
        Resource::Network(Network { name: "n".into() }),
        Resource::Subnet(Subnet {
            name: "s".into(),
            network: "n".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.0".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("network address")
    )));
}

#[test]
fn test_subnet_gateway_is_broadcast() {
    let resources = vec![
        Resource::Network(Network { name: "n".into() }),
        Resource::Subnet(Subnet {
            name: "s".into(),
            network: "n".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.255".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("broadcast")
    )));
}

#[test]
fn test_subnet_gateway_cidr_address_family_mismatch() {
    let resources = vec![
        Resource::Network(Network { name: "n".into() }),
        Resource::Subnet(Subnet {
            name: "s".into(),
            network: "n".into(),
            cidr: "fd00::/120".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("family mismatch")
    )));
}

#[test]
fn test_subnet_ipv6_gateway_is_subnet_router_anycast() {
    // Subnet-router anycast = network address (all-zeros host part)
    let resources = vec![
        Resource::Network(Network { name: "n".into() }),
        Resource::Subnet(Subnet {
            name: "s".into(),
            network: "n".into(),
            cidr: "fd00::/120".parse().unwrap(),
            gateway: "fd00::".parse().unwrap(), // network address = subnet-router anycast
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("network address")
    )));
}

#[test]
fn test_portforward_port_zero() {
    let mut resources = base_resources();
    if let Resource::PortForward(ref mut pf) = resources[5] {
        pf.host_port = 0;
    }
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("host_port")
    )));
}

#[test]
fn test_sg_rule_both_remote_cidr_and_sg() {
    let resources = vec![Resource::SecurityGroup(SecurityGroup {
        name: "sg".into(),
        rules: vec![SecurityGroupRule {
            direction: Direction::Ingress,
            protocol: Protocol::Tcp,
            port: Some(80),
            remote_cidr: Some("0.0.0.0/0".parse().unwrap()),
            remote_sg: Some("sg".into()),
        }],
    })];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("both")
    )));
}

#[test]
fn test_sg_rule_neither_remote() {
    let resources = vec![Resource::SecurityGroup(SecurityGroup {
        name: "sg".into(),
        rules: vec![SecurityGroupRule {
            direction: Direction::Ingress,
            protocol: Protocol::Tcp,
            port: Some(80),
            remote_cidr: None,
            remote_sg: None,
        }],
    })];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("either")
    )));
}

#[test]
fn test_sg_tcp_without_port() {
    let resources = vec![Resource::SecurityGroup(SecurityGroup {
        name: "sg".into(),
        rules: vec![SecurityGroupRule {
            direction: Direction::Ingress,
            protocol: Protocol::Tcp,
            port: None,
            remote_cidr: Some("0.0.0.0/0".parse().unwrap()),
            remote_sg: None,
        }],
    })];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("port")
    )));
}

#[test]
fn test_sg_icmp_without_port() {
    let resources = vec![Resource::SecurityGroup(SecurityGroup {
        name: "sg".into(),
        rules: vec![SecurityGroupRule {
            direction: Direction::Ingress,
            protocol: Protocol::Icmp,
            port: None,
            remote_cidr: Some("0.0.0.0/0".parse().unwrap()),
            remote_sg: None,
        }],
    })];
    validate(&resources).unwrap();
}

#[test]
fn test_multiple_errors_collected() {
    let resources = vec![
        Resource::Subnet(Subnet {
            name: "s".into(),
            network: "missing-net".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
        Resource::Instance(Instance {
            name: "i".into(),
            cpus: 0,
            memory_mb: 512,
            disk_gb: 10,
            image: "missing-img".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "missing-sub".into(),
                ip: None,
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
    ];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.len() >= 3); // at least: missing network, missing image, missing subnet, cpus=0
}

#[test]
fn test_instance_zero_cpus() {
    let mut resources = base_resources();
    if let Resource::Instance(ref mut i) = resources[4] {
        i.cpus = 0;
    }
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("cpus")
    )));
}

#[test]
fn test_mac_format_validation() {
    let mut resources = base_resources();
    if let Resource::Instance(ref mut i) = resources[4] {
        i.interfaces[0].mac = Some("invalid-mac".into());
    }
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("MAC")
    )));
}

#[test]
fn test_image_invalid_source_url_scheme() {
    let resources = vec![Resource::Image(Image {
        name: "img".into(),
        source_url: "ftp://example.com/img".into(),
        format: ImageFormat::Qcow2,
        architecture: None,
        os_type: None,
        checksum: None,
        min_disk_gb: None,
        min_ram_mb: None,
    })];
    let err = validate(&resources).unwrap_err();
    assert!(err.0.iter().any(|e| matches!(e,
        ValidationError::InvalidField { message, .. } if message.contains("source_url")
    )));
}
