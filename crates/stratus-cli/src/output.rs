use std::collections::HashMap;

use anyhow::Result;
use stratus_resources::{Resource, serialize_yaml_documents};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Yaml,
    Json,
}

/// Normalize a user-provided kind string to the canonical form.
pub fn normalize_kind(s: &str) -> Result<String> {
    let canonical = match s.to_lowercase().as_str() {
        "network" | "networks" | "net" | "nets" => "Network",
        "subnet" | "subnets" | "sub" | "subs" => "Subnet",
        "instance" | "instances" | "inst" => "Instance",
        "securitygroup" | "securitygroups" | "sg" | "sgs" => "SecurityGroup",
        "image" | "images" => "Image",
        "portforward" | "portforwards" | "pf" => "PortForward",
        _ => anyhow::bail!("unknown resource kind: {s}"),
    };
    Ok(canonical.to_string())
}

pub fn print_resources(
    resources: &[Resource],
    format: OutputFormat,
    instance_statuses: &HashMap<String, String>,
) -> Result<()> {
    match format {
        OutputFormat::Table => {
            print_table(resources, instance_statuses);
            Ok(())
        }
        OutputFormat::Yaml => {
            let yaml = serialize_yaml_documents(resources)?;
            print!("{yaml}");
            Ok(())
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(resources)?;
            println!("{json}");
            Ok(())
        }
    }
}

fn print_table(resources: &[Resource], instance_statuses: &HashMap<String, String>) {
    if resources.is_empty() {
        println!("No resources found.");
        return;
    }

    let has_statuses = !instance_statuses.is_empty();

    if has_statuses {
        println!("{:<20} {:<30} {:<12} DETAILS", "KIND", "NAME", "STATUS");
    } else {
        println!("{:<20} {:<30} DETAILS", "KIND", "NAME");
    }

    for r in resources {
        let details = resource_details(r);
        if has_statuses {
            let status = if let Resource::Instance(inst) = r {
                instance_statuses
                    .get(&inst.name)
                    .map(|s| s.as_str())
                    .unwrap_or("-")
            } else {
                ""
            };
            println!(
                "{:<20} {:<30} {:<12} {}",
                r.kind_str(),
                r.name(),
                status,
                details
            );
        } else {
            println!("{:<20} {:<30} {}", r.kind_str(), r.name(), details);
        }
    }
}

fn resource_details(r: &Resource) -> String {
    match r {
        Resource::Network(_) => String::new(),
        Resource::Subnet(s) => format!("network={} cidr={}", s.network, s.cidr),
        Resource::Instance(i) => {
            format!("cpus={} mem={}MB image={}", i.cpus, i.memory_mb, i.image)
        }
        Resource::SecurityGroup(sg) => format!("{} rule(s)", sg.rules.len()),
        Resource::Image(img) => format!("{:?} {}", img.format, img.source_url),
        Resource::PortForward(pf) => {
            format!(
                "{}:{} -> {}:{}",
                pf.host_ip, pf.host_port, pf.instance, pf.instance_port
            )
        }
    }
}
