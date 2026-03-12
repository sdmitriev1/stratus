use stratus_vm::disk::{create_overlay, resize_if_needed};

// --- create_overlay ---

#[tokio::test]
#[ignore] // requires qemu-img
async fn create_overlay_produces_qcow2() {
    let dir = tempfile::tempdir().unwrap();

    // Create a base image first
    let base = dir.path().join("base.qcow2");
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&base)
        .arg("1G")
        .status()
        .await
        .unwrap();
    assert!(status.success());

    // Create overlay
    let overlay = dir.path().join("overlay.qcow2");
    create_overlay(&overlay, &base, "qcow2").await.unwrap();

    assert!(overlay.exists(), "overlay should be created");

    // Verify it's a qcow2 with correct backing file
    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(&overlay)
        .output()
        .await
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["format"].as_str().unwrap(), "qcow2");
    assert!(info["backing-filename"].as_str().is_some());
}

#[tokio::test]
#[ignore] // requires qemu-img
async fn create_overlay_with_raw_base() {
    let dir = tempfile::tempdir().unwrap();

    let base = dir.path().join("base.raw");
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "raw"])
        .arg(&base)
        .arg("512M")
        .status()
        .await
        .unwrap();
    assert!(status.success());

    let overlay = dir.path().join("overlay.qcow2");
    create_overlay(&overlay, &base, "raw").await.unwrap();
    assert!(overlay.exists());
}

#[tokio::test]
async fn create_overlay_nonexistent_base_fails() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("nonexistent.qcow2");
    let overlay = dir.path().join("overlay.qcow2");

    let result = create_overlay(&overlay, &base, "qcow2").await;
    assert!(result.is_err());
}

// --- resize_if_needed ---

#[tokio::test]
#[ignore] // requires qemu-img
async fn resize_grows_image() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("disk.qcow2");

    // Create a 1G image
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&img)
        .arg("1G")
        .status()
        .await
        .unwrap();
    assert!(status.success());

    // Resize to 10G
    resize_if_needed(&img, 10).await.unwrap();

    // Verify new size
    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(&img)
        .output()
        .await
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let virtual_size = info["virtual-size"].as_u64().unwrap();
    assert_eq!(virtual_size, 10 * 1024 * 1024 * 1024);
}

#[tokio::test]
#[ignore] // requires qemu-img
async fn resize_noop_when_already_big_enough() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("disk.qcow2");

    // Create a 20G image
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&img)
        .arg("20G")
        .status()
        .await
        .unwrap();
    assert!(status.success());

    // "Resize" to 10G — should be a no-op
    resize_if_needed(&img, 10).await.unwrap();

    // Verify size unchanged
    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(&img)
        .output()
        .await
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let virtual_size = info["virtual-size"].as_u64().unwrap();
    assert_eq!(virtual_size, 20 * 1024 * 1024 * 1024);
}

#[tokio::test]
#[ignore] // requires qemu-img
async fn resize_exact_size_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("disk.qcow2");

    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&img)
        .arg("5G")
        .status()
        .await
        .unwrap();
    assert!(status.success());

    // Same size — no-op
    resize_if_needed(&img, 5).await.unwrap();

    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(&img)
        .output()
        .await
        .unwrap();
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let virtual_size = info["virtual-size"].as_u64().unwrap();
    assert_eq!(virtual_size, 5 * 1024 * 1024 * 1024);
}

#[tokio::test]
async fn resize_nonexistent_image_fails() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("nonexistent.qcow2");

    let result = resize_if_needed(&img, 10).await;
    assert!(result.is_err());
}
