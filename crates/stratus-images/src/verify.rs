use std::path::Path;

use serde_json::Value;
use stratus_resources::ImageFormat;

use crate::ImageError;

/// Parse a checksum string of the form "algorithm:hex".
/// Only sha256 is supported.
pub fn parse_checksum(s: &str) -> Result<(&str, &str), ImageError> {
    let (algo, hex) = s
        .split_once(':')
        .ok_or_else(|| ImageError::InvalidImage(format!("invalid checksum format: {s}")))?;
    if algo != "sha256" {
        return Err(ImageError::UnsupportedAlgorithm(algo.to_string()));
    }
    if hex.is_empty() {
        return Err(ImageError::InvalidImage("empty checksum hex".to_string()));
    }
    Ok((algo, hex))
}

#[derive(Debug)]
pub struct QemuImgInfo {
    pub format: String,
    pub virtual_size: u64,
    pub actual_size: u64,
}

/// Run `qemu-img info --output=json` and parse the result.
/// Rejects images with backing file references.
pub async fn qemu_img_info(path: &Path) -> Result<QemuImgInfo, ImageError> {
    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(path)
        .output()
        .await
        .map_err(|e| ImageError::QemuImg(format!("failed to run qemu-img: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ImageError::QemuImg(format!(
            "qemu-img exited with {}: {}",
            output.status, stderr
        )));
    }

    let info: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| ImageError::QemuImg(format!("failed to parse qemu-img output: {e}")))?;

    if let Some(backing) = info.get("backing-filename")
        && let Some(s) = backing.as_str()
    {
        return Err(ImageError::BackingFile(s.to_string()));
    }
    if let Some(backing) = info.get("full-backing-filename")
        && let Some(s) = backing.as_str()
    {
        return Err(ImageError::BackingFile(s.to_string()));
    }

    let format = info["format"]
        .as_str()
        .ok_or_else(|| ImageError::QemuImg("missing format field".to_string()))?
        .to_string();
    let virtual_size = info["virtual-size"]
        .as_u64()
        .ok_or_else(|| ImageError::QemuImg("missing virtual-size field".to_string()))?;
    let actual_size = info["actual-size"]
        .as_u64()
        .ok_or_else(|| ImageError::QemuImg("missing actual-size field".to_string()))?;

    Ok(QemuImgInfo {
        format,
        virtual_size,
        actual_size,
    })
}

/// Validate that an image file matches the expected format.
pub async fn validate_image(path: &Path, expected: ImageFormat) -> Result<QemuImgInfo, ImageError> {
    let info = qemu_img_info(path).await?;

    let expected_str = match expected {
        ImageFormat::Qcow2 => "qcow2",
        ImageFormat::Raw => "raw",
    };

    if info.format != expected_str {
        return Err(ImageError::InvalidImage(format!(
            "expected format {expected_str}, got {}",
            info.format
        )));
    }

    Ok(info)
}
