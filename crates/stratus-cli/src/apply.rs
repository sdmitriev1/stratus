use anyhow::{Context, Result, bail};
use stratus_resources::parse_yaml_documents;

use crate::connect::connect;
use crate::proto::ApplyRequest;
use crate::proto::stratus_service_client::StratusServiceClient;

pub async fn run(socket: &str, file: &str) -> Result<()> {
    let yaml = read_input(file)?;
    let resources = parse_yaml_documents(&yaml).context("failed to parse YAML")?;

    if resources.is_empty() {
        bail!("no resources found in input");
    }

    let mut json_resources = Vec::with_capacity(resources.len());
    for r in &resources {
        json_resources.push(serde_json::to_string(r)?);
    }

    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let response = client
        .apply(ApplyRequest {
            resources: json_resources,
        })
        .await
        .context("apply failed")?;

    let results = response.into_inner().results;

    println!("{:<20} {:<30} {:<10} REVISION", "KIND", "NAME", "ACTION");
    for r in &results {
        println!(
            "{:<20} {:<30} {:<10} {}",
            r.kind, r.name, r.action, r.revision
        );
    }

    Ok(())
}

fn read_input(file: &str) -> Result<String> {
    if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("failed to read stdin")?;
        return Ok(buf);
    }

    let path = std::path::Path::new(file);
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .context("failed to read directory")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.ends_with(".yaml") || name.ends_with(".yml")
            })
            .collect();

        if entries.is_empty() {
            bail!("no YAML files found in directory: {file}");
        }

        entries.sort_by_key(|e| e.file_name());

        let mut combined = String::new();
        for entry in entries {
            let content =
                std::fs::read_to_string(entry.path()).context("failed to read YAML file")?;
            if !combined.is_empty() {
                combined.push_str("\n---\n");
            }
            combined.push_str(&content);
        }
        Ok(combined)
    } else {
        std::fs::read_to_string(path).context("failed to read file")
    }
}
