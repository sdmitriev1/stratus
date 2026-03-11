use anyhow::{Context, Result};
use stratus_resources::parse_yaml_documents;

use crate::connect::connect;
use crate::output::normalize_kind;
use crate::proto::DeleteRequest;
use crate::proto::stratus_service_client::StratusServiceClient;

pub async fn run_by_name(socket: &str, kind: &str, name: &str) -> Result<()> {
    let kind = normalize_kind(kind)?;

    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let response = client
        .delete(DeleteRequest {
            kind: kind.clone(),
            name: name.to_string(),
        })
        .await
        .context("delete failed")?;

    let resp = response.into_inner();
    if resp.found {
        println!("{kind}/{name} deleted (revision {})", resp.revision);
    } else {
        println!("{kind}/{name} not found");
    }

    Ok(())
}

pub async fn run_from_file(socket: &str, file: &str) -> Result<()> {
    let yaml = std::fs::read_to_string(file).context("failed to read file")?;
    let resources = parse_yaml_documents(&yaml).context("failed to parse YAML")?;

    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    for r in &resources {
        let response = client
            .delete(DeleteRequest {
                kind: r.kind_str().to_string(),
                name: r.name().to_string(),
            })
            .await
            .context("delete failed")?;

        let resp = response.into_inner();
        if resp.found {
            println!(
                "{}/{} deleted (revision {})",
                r.kind_str(),
                r.name(),
                resp.revision
            );
        } else {
            println!("{}/{} not found", r.kind_str(), r.name());
        }
    }

    Ok(())
}
