use std::path::Path;

use tracing::info;

use crate::VmError;

/// Create a qcow2 overlay backed by a base image.
pub async fn create_overlay(
    overlay_path: &Path,
    base_image: &Path,
    base_format: &str,
) -> Result<(), VmError> {
    info!(
        overlay = %overlay_path.display(),
        base = %base_image.display(),
        "creating qcow2 overlay"
    );

    let output = tokio::process::Command::new("qemu-img")
        .args([
            "create",
            "-f",
            "qcow2",
            "-b",
            &base_image.to_string_lossy(),
            "-F",
            base_format,
        ])
        .arg(overlay_path)
        .output()
        .await
        .map_err(|e| VmError::QemuImg(format!("failed to run qemu-img: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::QemuImg(format!(
            "qemu-img create failed: {stderr}"
        )));
    }

    Ok(())
}

/// Resize image to at least `size_gb` GB if currently smaller.
pub async fn resize_if_needed(image_path: &Path, size_gb: u32) -> Result<(), VmError> {
    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(image_path)
        .output()
        .await
        .map_err(|e| VmError::QemuImg(format!("failed to run qemu-img info: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::QemuImg(format!("qemu-img info failed: {stderr}")));
    }

    let info: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| VmError::QemuImg(format!("failed to parse qemu-img info: {e}")))?;

    let current_size = info["virtual-size"]
        .as_u64()
        .ok_or_else(|| VmError::QemuImg("missing virtual-size in qemu-img info".into()))?;

    let target_size = u64::from(size_gb) * 1024 * 1024 * 1024;

    if current_size >= target_size {
        return Ok(());
    }

    info!(
        image = %image_path.display(),
        current_gb = current_size / (1024 * 1024 * 1024),
        target_gb = size_gb,
        "resizing disk"
    );

    let output = tokio::process::Command::new("qemu-img")
        .args(["resize"])
        .arg(image_path)
        .arg(format!("{size_gb}G"))
        .output()
        .await
        .map_err(|e| VmError::QemuImg(format!("failed to run qemu-img resize: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::QemuImg(format!(
            "qemu-img resize failed: {stderr}"
        )));
    }

    Ok(())
}
