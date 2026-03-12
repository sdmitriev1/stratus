use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use ipnet::IpNet;

use crate::types::*;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ValidationError {
    #[error("duplicate {kind} name: {name}")]
    DuplicateName { kind: String, name: String },
    #[error("{from_kind} '{from_name}' references missing {to_kind} '{to_name}'")]
    MissingReference {
        from_kind: String,
        from_name: String,
        to_kind: String,
        to_name: String,
    },
    #[error("{kind} '{name}': {message}")]
    InvalidField {
        kind: String,
        name: String,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("validation failed with {} error(s):\n{}", .0.len(), format_errors(.0))]
pub struct ValidationErrors(pub Vec<ValidationError>);

fn format_errors(errors: &[ValidationError]) -> String {
    errors
        .iter()
        .map(|e| format!("  - {e}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Validate a set of resources for duplicate names, missing cross-references, and field constraints.
/// Collects all errors rather than failing fast.
pub fn validate(resources: &[Resource]) -> Result<(), ValidationErrors> {
    let mut errors = Vec::new();

    // Pass 1: Build name sets, detect duplicates
    let mut names: HashMap<&str, HashSet<&str>> = HashMap::new();
    for r in resources {
        let kind = r.kind_str();
        let name = r.name();
        if !names.entry(kind).or_default().insert(name) {
            errors.push(ValidationError::DuplicateName {
                kind: kind.to_string(),
                name: name.to_string(),
            });
        }
    }

    let networks = names.get("Network").cloned().unwrap_or_default();
    let subnets = names.get("Subnet").cloned().unwrap_or_default();
    let instances = names.get("Instance").cloned().unwrap_or_default();
    let security_groups = names.get("SecurityGroup").cloned().unwrap_or_default();
    let images = names.get("Image").cloned().unwrap_or_default();

    // Pass 2: Cross-reference checks and field validation
    for r in resources {
        match r {
            Resource::Subnet(s) => {
                validate_subnet(s, &networks, &mut errors);
            }
            Resource::Instance(i) => {
                validate_instance(i, &images, &subnets, &security_groups, &mut errors);
            }
            Resource::SecurityGroup(sg) => {
                validate_security_group(sg, &security_groups, &mut errors);
            }
            Resource::Image(img) => {
                validate_image(img, &mut errors);
            }
            Resource::PortForward(pf) => {
                validate_port_forward(pf, &instances, &mut errors);
            }
            Resource::Network(_) => {}
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

fn validate_subnet(s: &Subnet, networks: &HashSet<&str>, errors: &mut Vec<ValidationError>) {
    if !networks.contains(s.network.as_str()) {
        errors.push(ValidationError::MissingReference {
            from_kind: "Subnet".into(),
            from_name: s.name.clone(),
            to_kind: "Network".into(),
            to_name: s.network.clone(),
        });
    }

    // Gateway and CIDR must be same address family
    let gw_is_v4 = s.gateway.is_ipv4();
    let cidr_is_v4 = matches!(s.cidr, IpNet::V4(_));
    if gw_is_v4 != cidr_is_v4 {
        errors.push(ValidationError::InvalidField {
            kind: "Subnet".into(),
            name: s.name.clone(),
            message: "gateway and CIDR address family mismatch".into(),
        });
        return; // Further checks don't make sense with mismatched families
    }

    // CIDR prefix length check
    match &s.cidr {
        IpNet::V4(net) => {
            if net.prefix_len() > 30 {
                errors.push(ValidationError::InvalidField {
                    kind: "Subnet".into(),
                    name: s.name.clone(),
                    message: "IPv4 CIDR prefix must be /30 or shorter".into(),
                });
            }
        }
        IpNet::V6(net) => {
            if net.prefix_len() > 126 {
                errors.push(ValidationError::InvalidField {
                    kind: "Subnet".into(),
                    name: s.name.clone(),
                    message: "IPv6 CIDR prefix must be /126 or shorter".into(),
                });
            }
        }
    }

    // Gateway must be within CIDR
    if !s.cidr.contains(&s.gateway) {
        errors.push(ValidationError::InvalidField {
            kind: "Subnet".into(),
            name: s.name.clone(),
            message: "gateway is not within CIDR range".into(),
        });
        return;
    }

    // Gateway must not be network address
    if s.gateway == s.cidr.network() {
        errors.push(ValidationError::InvalidField {
            kind: "Subnet".into(),
            name: s.name.clone(),
            message: "gateway cannot be the network address".into(),
        });
    }

    // Gateway must not be broadcast (IPv4) or subnet-router anycast (IPv6)
    match &s.cidr {
        IpNet::V4(net) => {
            if s.gateway == IpAddr::V4(net.broadcast()) {
                errors.push(ValidationError::InvalidField {
                    kind: "Subnet".into(),
                    name: s.name.clone(),
                    message: "gateway cannot be the broadcast address".into(),
                });
            }
        }
        IpNet::V6(_) => {
            // Subnet-router anycast = network address, already checked above
        }
    }
}

fn validate_instance(
    i: &Instance,
    images: &HashSet<&str>,
    subnets: &HashSet<&str>,
    security_groups: &HashSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    if !images.contains(i.image.as_str()) {
        errors.push(ValidationError::MissingReference {
            from_kind: "Instance".into(),
            from_name: i.name.clone(),
            to_kind: "Image".into(),
            to_name: i.image.clone(),
        });
    }

    if i.cpus < 1 {
        errors.push(ValidationError::InvalidField {
            kind: "Instance".into(),
            name: i.name.clone(),
            message: "cpus must be >= 1".into(),
        });
    }
    if i.memory_mb < 1 {
        errors.push(ValidationError::InvalidField {
            kind: "Instance".into(),
            name: i.name.clone(),
            message: "memory_mb must be >= 1".into(),
        });
    }
    if i.disk_gb < 1 {
        errors.push(ValidationError::InvalidField {
            kind: "Instance".into(),
            name: i.name.clone(),
            message: "disk_gb must be >= 1".into(),
        });
    }

    for iface in &i.interfaces {
        if !subnets.contains(iface.subnet.as_str()) {
            errors.push(ValidationError::MissingReference {
                from_kind: "Instance".into(),
                from_name: i.name.clone(),
                to_kind: "Subnet".into(),
                to_name: iface.subnet.clone(),
            });
        }
        for sg_name in &iface.security_groups {
            if !security_groups.contains(sg_name.as_str()) {
                errors.push(ValidationError::MissingReference {
                    from_kind: "Instance".into(),
                    from_name: i.name.clone(),
                    to_kind: "SecurityGroup".into(),
                    to_name: sg_name.clone(),
                });
            }
        }
        if let Some(mac) = &iface.mac {
            validate_mac_format(mac, &i.name, errors);
        }
    }
}

fn validate_mac_format(mac: &str, instance_name: &str, errors: &mut Vec<ValidationError>) {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        errors.push(ValidationError::InvalidField {
            kind: "Instance".into(),
            name: instance_name.to_string(),
            message: format!("invalid MAC address format: {mac}"),
        });
        return;
    }
    for part in &parts {
        if part.len() != 2 || !part.chars().all(|c| c.is_ascii_hexdigit()) {
            errors.push(ValidationError::InvalidField {
                kind: "Instance".into(),
                name: instance_name.to_string(),
                message: format!("invalid MAC address format: {mac}"),
            });
            return;
        }
    }
}

fn validate_security_group(
    sg: &SecurityGroup,
    security_groups: &HashSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    for rule in &sg.rules {
        // remote_cidr and remote_sg are mutually exclusive, at least one required
        match (&rule.remote_cidr, &rule.remote_sg) {
            (Some(_), Some(_)) => {
                errors.push(ValidationError::InvalidField {
                    kind: "SecurityGroup".into(),
                    name: sg.name.clone(),
                    message: "rule has both remote_cidr and remote_sg; only one allowed".into(),
                });
            }
            (None, None) => {
                errors.push(ValidationError::InvalidField {
                    kind: "SecurityGroup".into(),
                    name: sg.name.clone(),
                    message: "rule must have either remote_cidr or remote_sg".into(),
                });
            }
            _ => {}
        }

        // remote_sg cross-reference
        if let Some(ref sg_ref) = rule.remote_sg
            && !security_groups.contains(sg_ref.as_str())
        {
            errors.push(ValidationError::MissingReference {
                from_kind: "SecurityGroup".into(),
                from_name: sg.name.clone(),
                to_kind: "SecurityGroup".into(),
                to_name: sg_ref.clone(),
            });
        }

        // port required for tcp/udp
        match rule.protocol {
            Protocol::Tcp | Protocol::Udp => match rule.port {
                None => {
                    errors.push(ValidationError::InvalidField {
                        kind: "SecurityGroup".into(),
                        name: sg.name.clone(),
                        message: format!("port is required for {:?} protocol", rule.protocol),
                    });
                }
                Some(0) => {
                    errors.push(ValidationError::InvalidField {
                        kind: "SecurityGroup".into(),
                        name: sg.name.clone(),
                        message: "port must not be 0".into(),
                    });
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn validate_image(img: &Image, errors: &mut Vec<ValidationError>) {
    if img.source_url.is_empty() {
        errors.push(ValidationError::InvalidField {
            kind: "Image".into(),
            name: img.name.clone(),
            message: "source_url must not be empty".into(),
        });
    } else if !img.source_url.starts_with("https://")
        && !img.source_url.starts_with("http://")
        && !img.source_url.starts_with("file://")
    {
        errors.push(ValidationError::InvalidField {
            kind: "Image".into(),
            name: img.name.clone(),
            message: "source_url must start with https://, http://, or file://".into(),
        });
    }

    if let Some(ref checksum) = img.checksum {
        if !checksum.contains(':') {
            errors.push(ValidationError::InvalidField {
                kind: "Image".into(),
                name: img.name.clone(),
                message: "checksum must be in format 'algorithm:hex'".into(),
            });
        } else {
            let parts: Vec<&str> = checksum.splitn(2, ':').collect();
            if parts[0].is_empty() || parts[1].is_empty() {
                errors.push(ValidationError::InvalidField {
                    kind: "Image".into(),
                    name: img.name.clone(),
                    message: "checksum must be in format 'algorithm:hex'".into(),
                });
            } else if !parts[1].starts_with("http://")
                && !parts[1].starts_with("https://")
                && !parts[1].chars().all(|c| c.is_ascii_hexdigit())
            {
                errors.push(ValidationError::InvalidField {
                    kind: "Image".into(),
                    name: img.name.clone(),
                    message: "checksum hex portion contains non-hex characters".into(),
                });
            }
        }
    }
}

fn validate_port_forward(
    pf: &PortForward,
    instances: &HashSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    if !instances.contains(pf.instance.as_str()) {
        errors.push(ValidationError::MissingReference {
            from_kind: "PortForward".into(),
            from_name: pf.name.clone(),
            to_kind: "Instance".into(),
            to_name: pf.instance.clone(),
        });
    }

    if pf.host_port == 0 {
        errors.push(ValidationError::InvalidField {
            kind: "PortForward".into(),
            name: pf.name.clone(),
            message: "host_port must not be 0".into(),
        });
    }
    if pf.instance_port == 0 {
        errors.push(ValidationError::InvalidField {
            kind: "PortForward".into(),
            name: pf.name.clone(),
            message: "instance_port must not be 0".into(),
        });
    }
}
