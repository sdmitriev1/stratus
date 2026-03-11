use std::net::IpAddr;

use ipnet::IpNet;
use serde::{Deserialize, Deserializer, Serialize};

// --- Enums ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NatMode {
    None,
    Masquerade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Ingress,
    Egress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Qcow2,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortProtocol {
    Tcp,
    Udp,
}

// --- Resource structs ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subnet {
    pub name: String,
    pub network: String,
    pub cidr: IpNet,
    pub gateway: IpAddr,
    #[serde(default)]
    pub dns: Vec<IpAddr>,
    #[serde(default = "default_true")]
    pub dhcp: bool,
    #[serde(default = "default_nat_mode")]
    pub nat: NatMode,
    #[serde(default)]
    pub isolated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub name: String,
    pub cpus: u32,
    pub memory_mb: u32,
    #[serde(default = "default_disk_gb")]
    pub disk_gb: u32,
    pub image: String,
    #[serde(default)]
    pub secure_boot: bool,
    #[serde(default)]
    pub vtpm: bool,
    pub interfaces: Vec<Interface>,
    #[serde(default)]
    pub user_data: Option<String>,
    #[serde(default)]
    pub ssh_authorized_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interface {
    pub subnet: String,
    #[serde(default)]
    pub ip: Option<IpAddr>,
    #[serde(default, deserialize_with = "deserialize_optional_mac")]
    pub mac: Option<String>,
    #[serde(default)]
    pub security_groups: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityGroup {
    pub name: String,
    pub rules: Vec<SecurityGroupRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityGroupRule {
    pub direction: Direction,
    pub protocol: Protocol,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub remote_cidr: Option<IpNet>,
    #[serde(default)]
    pub remote_sg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub name: String,
    pub source_url: String,
    pub format: ImageFormat,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub os_type: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub min_disk_gb: Option<u32>,
    #[serde(default)]
    pub min_ram_mb: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortForward {
    pub name: String,
    pub instance: String,
    pub host_port: u16,
    pub instance_port: u16,
    #[serde(default = "default_port_protocol")]
    pub protocol: PortProtocol,
    #[serde(default = "default_host_ip")]
    pub host_ip: IpAddr,
}

// --- Tagged enum ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Resource {
    Network(Network),
    Subnet(Subnet),
    Instance(Instance),
    SecurityGroup(SecurityGroup),
    Image(Image),
    PortForward(PortForward),
}

impl Resource {
    pub fn name(&self) -> &str {
        match self {
            Resource::Network(r) => &r.name,
            Resource::Subnet(r) => &r.name,
            Resource::Instance(r) => &r.name,
            Resource::SecurityGroup(r) => &r.name,
            Resource::Image(r) => &r.name,
            Resource::PortForward(r) => &r.name,
        }
    }

    pub fn kind_str(&self) -> &str {
        match self {
            Resource::Network(_) => "Network",
            Resource::Subnet(_) => "Subnet",
            Resource::Instance(_) => "Instance",
            Resource::SecurityGroup(_) => "SecurityGroup",
            Resource::Image(_) => "Image",
            Resource::PortForward(_) => "PortForward",
        }
    }
}

// --- Defaults ---

fn default_true() -> bool {
    true
}

fn default_nat_mode() -> NatMode {
    NatMode::None
}

fn default_disk_gb() -> u32 {
    20
}

fn default_port_protocol() -> PortProtocol {
    PortProtocol::Tcp
}

fn default_host_ip() -> IpAddr {
    IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
}

// --- Custom deserializer for mac: "" → None ---

fn deserialize_optional_mac<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.filter(|s| !s.is_empty()))
}
