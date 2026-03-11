use std::collections::HashSet;
use std::net::IpAddr;

use ipnet::IpNet;
use stratus_resources::*;

#[test]
fn test_allocate_sequential_v4() {
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/24".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    let ip1 = alloc.allocate().unwrap();
    let ip2 = alloc.allocate().unwrap();
    let ip3 = alloc.allocate().unwrap();
    // Should skip .0 (network) and .1 (gateway), start at .2
    assert_eq!(ip1, "10.0.0.2".parse::<IpAddr>().unwrap());
    assert_eq!(ip2, "10.0.0.3".parse::<IpAddr>().unwrap());
    assert_eq!(ip3, "10.0.0.4".parse::<IpAddr>().unwrap());
}

#[test]
fn test_allocate_sequential_v6() {
    let mut alloc =
        SubnetAllocator::new("fd00::/120".parse().unwrap(), "fd00::1".parse().unwrap()).unwrap();
    let ip1 = alloc.allocate().unwrap();
    let ip2 = alloc.allocate().unwrap();
    let ip3 = alloc.allocate().unwrap();
    assert_eq!(ip1, "fd00::2".parse::<IpAddr>().unwrap());
    assert_eq!(ip2, "fd00::3".parse::<IpAddr>().unwrap());
    assert_eq!(ip3, "fd00::4".parse::<IpAddr>().unwrap());
}

#[test]
fn test_allocate_skips_gateway() {
    // Gateway at .2 — allocator should skip it
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/24".parse().unwrap(), "10.0.0.2".parse().unwrap()).unwrap();
    let ip1 = alloc.allocate().unwrap();
    let ip2 = alloc.allocate().unwrap();
    // Should skip .0 (network) and .2 (gateway), give .1 and .3
    assert_eq!(ip1, "10.0.0.1".parse::<IpAddr>().unwrap());
    assert_eq!(ip2, "10.0.0.3".parse::<IpAddr>().unwrap());
}

#[test]
fn test_allocate_skips_network_and_broadcast_v4() {
    // /30 = 4 addresses: .0 (network), .1 (gateway), .2 (usable), .3 (broadcast)
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/30".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    let ip1 = alloc.allocate().unwrap();
    assert_eq!(ip1, "10.0.0.2".parse::<IpAddr>().unwrap());
    // Next should be exhausted (only .2 is usable)
    assert!(alloc.allocate().is_err());
}

#[test]
fn test_allocate_skips_subnet_router_anycast_v6() {
    // For IPv6, the network address (subnet-router anycast) should be skipped
    let mut alloc =
        SubnetAllocator::new("fd00::/126".parse().unwrap(), "fd00::1".parse().unwrap()).unwrap();
    let ip1 = alloc.allocate().unwrap();
    let ip2 = alloc.allocate().unwrap();
    // ::0 is network (skipped), ::1 is gateway (skipped), ::2 and ::3 usable
    assert_eq!(ip1, "fd00::2".parse::<IpAddr>().unwrap());
    assert_eq!(ip2, "fd00::3".parse::<IpAddr>().unwrap());
    // No broadcast concept in IPv6 for /126, but only 4 addrs total
    // After ::2 and ::3, should be exhausted
    assert!(alloc.allocate().is_err());
}

#[test]
fn test_reserve_then_allocate() {
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/24".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    alloc.reserve("10.0.0.2".parse().unwrap()).unwrap();
    let ip1 = alloc.allocate().unwrap();
    // Should skip .2 (reserved) and give .3
    assert_eq!(ip1, "10.0.0.3".parse::<IpAddr>().unwrap());
}

#[test]
fn test_allocate_exhaustion() {
    // /30 = .0 network, .1 gateway, .2 usable, .3 broadcast
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/30".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    alloc.allocate().unwrap(); // .2
    assert!(matches!(alloc.allocate(), Err(AllocError::Exhausted)));
}

#[test]
fn test_allocate_exhaustion_v6() {
    // /126 = 4 addresses: ::0 network, ::1 gateway, ::2 and ::3 usable
    let mut alloc =
        SubnetAllocator::new("fd00::/126".parse().unwrap(), "fd00::1".parse().unwrap()).unwrap();
    alloc.allocate().unwrap(); // ::2
    alloc.allocate().unwrap(); // ::3
    assert!(matches!(alloc.allocate(), Err(AllocError::Exhausted)));
}

#[test]
fn test_reserve_out_of_range() {
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/24".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    let result = alloc.reserve("192.168.1.1".parse().unwrap());
    assert!(matches!(result, Err(AllocError::OutOfRange(_))));
}

#[test]
fn test_reserve_network_address() {
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/24".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    let result = alloc.reserve("10.0.0.0".parse().unwrap());
    assert!(matches!(result, Err(AllocError::ReservedAddress(_))));
}

#[test]
fn test_reserve_broadcast_address_v4() {
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/24".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    let result = alloc.reserve("10.0.0.255".parse().unwrap());
    assert!(matches!(result, Err(AllocError::ReservedAddress(_))));
}

#[test]
fn test_reserve_duplicate() {
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/24".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    alloc.reserve("10.0.0.10".parse().unwrap()).unwrap();
    let result = alloc.reserve("10.0.0.10".parse().unwrap());
    assert!(matches!(result, Err(AllocError::AlreadyAllocated(_))));
}

#[test]
fn test_allocate_full_subnet_v4() {
    // /28 = 16 addrs: .0 network, .15 broadcast, .1 gateway = 13 usable
    let mut alloc =
        SubnetAllocator::new("10.0.0.0/28".parse().unwrap(), "10.0.0.1".parse().unwrap()).unwrap();
    let mut ips = HashSet::new();
    for _ in 0..13 {
        let ip = alloc.allocate().unwrap();
        assert!(ips.insert(ip), "duplicate IP allocated: {ip}");
        // Must be in range
        let net: IpNet = "10.0.0.0/28".parse().unwrap();
        assert!(net.contains(&ip));
    }
    assert_eq!(ips.len(), 13);
    assert!(alloc.allocate().is_err());
}

#[test]
fn test_allocate_full_subnet_v6() {
    // /124 = 16 addrs: ::0 network, ::1 gateway = 14 usable (no broadcast)
    let mut alloc =
        SubnetAllocator::new("fd00::/124".parse().unwrap(), "fd00::1".parse().unwrap()).unwrap();
    let mut ips = HashSet::new();
    for _ in 0..14 {
        let ip = alloc.allocate().unwrap();
        assert!(ips.insert(ip), "duplicate IP allocated: {ip}");
        let net: IpNet = "fd00::/124".parse().unwrap();
        assert!(net.contains(&ip));
    }
    assert_eq!(ips.len(), 14);
    assert!(alloc.allocate().is_err());
}

#[test]
fn test_generate_mac_format() {
    let mac = generate_mac();
    let parts: Vec<&str> = mac.split(':').collect();
    assert_eq!(parts.len(), 6);
    assert_eq!(parts[0], "02");
    assert_eq!(parts[1], "df");
    for part in &parts {
        assert_eq!(part.len(), 2);
        assert!(part.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

#[test]
fn test_generate_mac_uniqueness() {
    let macs: HashSet<String> = (0..1000).map(|_| generate_mac()).collect();
    assert_eq!(macs.len(), 1000);
}

#[test]
fn test_generate_mac_locally_administered() {
    let mac = generate_mac();
    let first_byte = u8::from_str_radix(&mac[0..2], 16).unwrap();
    assert_eq!(first_byte & 0x02, 0x02, "locally administered bit not set");
}

#[test]
fn test_allocate_addresses_full() {
    let mut resources = vec![
        Resource::Network(Network { name: "net".into() }),
        Resource::Subnet(Subnet {
            name: "sub".into(),
            network: "net".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
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
            name: "vm1".into(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "img".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "sub".into(),
                ip: Some("10.0.0.10".parse().unwrap()),
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
        Resource::Instance(Instance {
            name: "vm2".into(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "img".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "sub".into(),
                ip: None,
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
    ];

    allocate_addresses(&mut resources).unwrap();

    // vm1 should keep its static IP
    if let Resource::Instance(inst) = &resources[3] {
        assert_eq!(inst.interfaces[0].ip, Some("10.0.0.10".parse().unwrap()));
        assert!(inst.interfaces[0].mac.is_some());
    }

    // vm2 should get an auto-assigned IP (not .0, .1, .10, or .255)
    if let Resource::Instance(inst) = &resources[4] {
        let ip = inst.interfaces[0].ip.unwrap();
        assert_ne!(ip, "10.0.0.0".parse::<IpAddr>().unwrap());
        assert_ne!(ip, "10.0.0.1".parse::<IpAddr>().unwrap());
        assert_ne!(ip, "10.0.0.10".parse::<IpAddr>().unwrap());
        assert_ne!(ip, "10.0.0.255".parse::<IpAddr>().unwrap());
        assert!(inst.interfaces[0].mac.is_some());
    }
}

#[test]
fn test_allocate_addresses_unknown_subnet() {
    let mut resources = vec![Resource::Instance(Instance {
        name: "vm1".into(),
        cpus: 1,
        memory_mb: 512,
        disk_gb: 10,
        image: "img".into(),
        secure_boot: false,
        vtpm: false,
        interfaces: vec![Interface {
            subnet: "nonexistent".into(),
            ip: Some("10.0.0.5".parse().unwrap()),
            mac: None,
            security_groups: vec![],
        }],
        user_data: None,
        ssh_authorized_keys: vec![],
    })];
    let result = allocate_addresses(&mut resources);
    assert!(matches!(result, Err(AllocError::UnknownSubnet(_))));
}

#[test]
fn test_allocate_addresses_static_conflict() {
    let mut resources = vec![
        Resource::Subnet(Subnet {
            name: "sub".into(),
            network: "net".into(),
            cidr: "10.0.0.0/24".parse().unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            dns: vec![],
            dhcp: true,
            nat: NatMode::None,
            isolated: false,
        }),
        Resource::Instance(Instance {
            name: "vm1".into(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "img".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "sub".into(),
                ip: Some("10.0.0.10".parse().unwrap()),
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
        Resource::Instance(Instance {
            name: "vm2".into(),
            cpus: 1,
            memory_mb: 512,
            disk_gb: 10,
            image: "img".into(),
            secure_boot: false,
            vtpm: false,
            interfaces: vec![Interface {
                subnet: "sub".into(),
                ip: Some("10.0.0.10".parse().unwrap()),
                mac: None,
                security_groups: vec![],
            }],
            user_data: None,
            ssh_authorized_keys: vec![],
        }),
    ];
    let result = allocate_addresses(&mut resources);
    assert!(matches!(result, Err(AllocError::StaticConflict(_))));
}
