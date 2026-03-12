use anyhow::{Context, Result};
use stratus_resources::Resource;

use crate::connect::connect;
use crate::output::{OutputFormat, normalize_kind, print_resources};
use crate::proto::GetRequest;
use crate::proto::stratus_service_client::StratusServiceClient;

pub async fn run(socket: &str, kind: &str, name: Option<&str>, output: OutputFormat) -> Result<()> {
    let kind = normalize_kind(kind)?;

    let channel = connect(socket).await?;
    let mut client = StratusServiceClient::new(channel);

    let response = client
        .get(GetRequest {
            kind,
            name: name.map(|s| s.to_string()),
        })
        .await
        .context("get failed")?;

    let resp = response.into_inner();
    let resources: Vec<Resource> = resp
        .resources
        .iter()
        .map(|json| serde_json::from_str(json))
        .collect::<Result<_, _>>()
        .context("failed to deserialize resources")?;

    print_resources(&resources, output, &resp.instance_statuses)?;

    Ok(())
}
