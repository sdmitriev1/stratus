use serde::Deserialize;

use crate::types::Resource;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("empty input: no documents found")]
    Empty,
}

/// Parse a YAML string that may contain multiple `---`-separated documents,
/// each representing one Resource.
pub fn parse_yaml_documents(input: &str) -> Result<Vec<Resource>, ParseError> {
    if input.trim().is_empty() {
        return Err(ParseError::Empty);
    }
    let mut resources = Vec::new();
    for document in serde_yaml::Deserializer::from_str(input) {
        let resource = Resource::deserialize(document)?;
        resources.push(resource);
    }
    if resources.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(resources)
}

/// Serialize a slice of Resources into a multi-document YAML string with `---` separators.
pub fn serialize_yaml_documents(resources: &[Resource]) -> Result<String, serde_yaml::Error> {
    let mut output = String::new();
    for (i, resource) in resources.iter().enumerate() {
        if i > 0 {
            output.push_str("---\n");
        }
        let doc = serde_yaml::to_string(resource)?;
        output.push_str(&doc);
    }
    Ok(output)
}
