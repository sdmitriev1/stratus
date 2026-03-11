use anyhow::{Context, Result};
use stratus_resources::{Resource, serialize_yaml_documents};

use crate::connect::connect;
use crate::proto::DumpStoreRequest;
use crate::proto::stratus_service_client::StratusServiceClient;

pub async fn run(socket: &str) -> Result<()> {
    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let response = client
        .dump_store(DumpStoreRequest {})
        .await
        .context("failed to dump store — is stratusd running?")?;

    let dump = response.into_inner();

    if dump.resources.is_empty() {
        println!("# revision: {}", dump.revision);
        println!("# store is empty");
        return Ok(());
    }

    let resources: Vec<Resource> = dump
        .resources
        .iter()
        .map(|json| serde_json::from_str(json))
        .collect::<Result<_, _>>()
        .context("failed to deserialize resources")?;

    println!("# revision: {}", dump.revision);
    println!("# {} resource(s)", resources.len());
    let yaml = serialize_yaml_documents(&resources).context("failed to serialize YAML")?;
    print!("{yaml}");

    Ok(())
}
