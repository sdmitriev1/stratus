use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ipnet::IpNet;

use crate::types::*;

#[derive(Debug, thiserror::Error)]
pub enum AllocError {
    #[error("address {0} is out of subnet range")]
    OutOfRange(IpAddr),
    #[error("address {0} is a reserved address (network/broadcast/gateway)")]
    ReservedAddress(IpAddr),
    #[error("address {0} is already allocated")]
    AlreadyAllocated(IpAddr),
    #[error("subnet exhausted: no more addresses available")]
    Exhausted,
    #[error("address family mismatch")]
    AddressFamilyMismatch,
    #[error("unknown subnet: {0}")]
    UnknownSubnet(String),
    #[error("static IP conflict: {0} assigned to multiple interfaces")]
    StaticConflict(IpAddr),
}

/// Allocator for IP addresses within a subnet. Works with both IPv4 and IPv6.
/// Internally uses u128 for unified arithmetic.
pub struct SubnetAllocator {
    /// Network address as u128
    network: u128,
    /// Broadcast address as u128 (for IPv4) or last address (for IPv6)
    last: u128,
    /// Gateway as u128
    gateway: u128,
    /// Whether the subnet is IPv4
    is_v4: bool,
    /// Set of allocated addresses (as u128)
    allocated: HashSet<u128>,
    /// Next candidate offset from network address
    next_candidate: u128,
}

impl SubnetAllocator {
    pub fn new(cidr: IpNet, gateway: IpAddr) -> Result<Self, AllocError> {
        let is_v4 = matches!(cidr, IpNet::V4(_));
        let gw_is_v4 = gateway.is_ipv4();
        if is_v4 != gw_is_v4 {
            return Err(AllocError::AddressFamilyMismatch);
        }

        let network = ip_to_u128(cidr.network());
        let last = ip_to_u128(cidr.broadcast());
        let gw = ip_to_u128(gateway);

        Ok(Self {
            network,
            last,
            gateway: gw,
            is_v4,
            allocated: HashSet::new(),
            next_candidate: 1, // Start after network address
        })
    }

    /// Reserve a specific IP address (e.g., a static IP).
    pub fn reserve(&mut self, ip: IpAddr) -> Result<(), AllocError> {
        let addr = ip_to_u128(ip);

        if addr < self.network || addr > self.last {
            return Err(AllocError::OutOfRange(ip));
        }

        if self.is_reserved(addr) {
            return Err(AllocError::ReservedAddress(ip));
        }

        if !self.allocated.insert(addr) {
            return Err(AllocError::AlreadyAllocated(ip));
        }

        Ok(())
    }

    /// Allocate the next available IP address.
    pub fn allocate(&mut self) -> Result<IpAddr, AllocError> {
        let range_size = self.last - self.network + 1;

        for _ in 0..range_size {
            let candidate = self.network + self.next_candidate;
            self.next_candidate += 1;

            // Wrap around
            if self.network + self.next_candidate > self.last {
                self.next_candidate = 1;
            }

            if candidate > self.last {
                continue;
            }

            if self.is_reserved(candidate) || self.allocated.contains(&candidate) {
                continue;
            }

            self.allocated.insert(candidate);
            return Ok(u128_to_ip(candidate, self.is_v4));
        }

        Err(AllocError::Exhausted)
    }

    fn is_reserved(&self, addr: u128) -> bool {
        // Network address is always reserved
        if addr == self.network {
            return true;
        }
        // Gateway is always reserved
        if addr == self.gateway {
            return true;
        }
        // For IPv4, broadcast is reserved
        if self.is_v4 && addr == self.last {
            return true;
        }
        false
    }
}

fn ip_to_u128(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(v4) => u32::from(v4) as u128,
        IpAddr::V6(v6) => u128::from(v6),
    }
}

fn u128_to_ip(val: u128, is_v4: bool) -> IpAddr {
    if is_v4 {
        IpAddr::V4(Ipv4Addr::from(val as u32))
    } else {
        IpAddr::V6(Ipv6Addr::from(val))
    }
}

/// Generate a locally-administered MAC address using UUID v4 bytes.
/// Format: `02:df:XX:XX:XX:XX`
pub fn generate_mac() -> String {
    let uuid = uuid::Uuid::new_v4();
    let bytes = uuid.as_bytes();
    format!(
        "02:df:{:02x}:{:02x}:{:02x}:{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

/// Convenience function: build allocators from subnets, reserve static IPs, then auto-assign.
pub fn allocate_addresses(resources: &mut [Resource]) -> Result<(), AllocError> {
    // Collect subnet info
    let mut allocators: std::collections::HashMap<String, SubnetAllocator> =
        std::collections::HashMap::new();

    for r in resources.iter() {
        if let Resource::Subnet(s) = r {
            let alloc = SubnetAllocator::new(s.cidr, s.gateway)?;
            allocators.insert(s.name.clone(), alloc);
        }
    }

    // Pass 1: Reserve all static IPs
    // Track (subnet, ip) pairs to detect conflicts
    let mut seen_static: HashSet<(String, IpAddr)> = HashSet::new();

    for r in resources.iter() {
        if let Resource::Instance(inst) = r {
            for iface in &inst.interfaces {
                if let Some(ip) = iface.ip {
                    let alloc = allocators
                        .get_mut(&iface.subnet)
                        .ok_or_else(|| AllocError::UnknownSubnet(iface.subnet.clone()))?;
                    if !seen_static.insert((iface.subnet.clone(), ip)) {
                        return Err(AllocError::StaticConflict(ip));
                    }
                    alloc.reserve(ip)?;
                }
            }
        }
    }

    // Pass 2: Auto-assign IPs and MACs
    for r in resources.iter_mut() {
        if let Resource::Instance(inst) = r {
            for iface in &mut inst.interfaces {
                if iface.ip.is_none() {
                    let alloc = allocators
                        .get_mut(&iface.subnet)
                        .ok_or_else(|| AllocError::UnknownSubnet(iface.subnet.clone()))?;
                    iface.ip = Some(alloc.allocate()?);
                }
                if iface.mac.is_none() {
                    iface.mac = Some(generate_mac());
                }
            }
        }
    }

    Ok(())
}
