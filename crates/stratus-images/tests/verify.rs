use stratus_images::ImageError;
use stratus_images::verify::{
    ChecksumSpec, parse_checksum, parse_checksum_line, qemu_img_info, validate_image,
};
use stratus_resources::ImageFormat;

#[test]
fn parse_checksum_valid() {
    let spec = parse_checksum("sha256:abc123").unwrap();
    assert_eq!(spec, ChecksumSpec::Inline { hex: "abc123" });
}

#[test]
fn parse_checksum_unsupported_algo() {
    let err = parse_checksum("md5:abc123").unwrap_err();
    assert!(matches!(err, ImageError::UnsupportedAlgorithm(ref a) if a == "md5"));
}

#[test]
fn parse_checksum_no_colon() {
    let err = parse_checksum("sha256abc").unwrap_err();
    assert!(matches!(err, ImageError::InvalidImage(_)));
}

#[test]
fn parse_checksum_remote_url() {
    let spec = parse_checksum("sha256:https://example.com/SHA256SUMS").unwrap();
    assert_eq!(
        spec,
        ChecksumSpec::Remote {
            url: "https://example.com/SHA256SUMS"
        }
    );
}

#[test]
fn parse_checksum_line_binary_mode() {
    let (hash, filename) = parse_checksum_line("abc123 *cirros-0.6.3-aarch64-disk.img").unwrap();
    assert_eq!(hash, "abc123");
    assert_eq!(filename, "cirros-0.6.3-aarch64-disk.img");
}

#[test]
fn parse_checksum_line_text_mode() {
    let (hash, filename) = parse_checksum_line("abc123  cirros-0.6.3-aarch64-disk.img").unwrap();
    assert_eq!(hash, "abc123");
    assert_eq!(filename, "cirros-0.6.3-aarch64-disk.img");
}

#[test]
fn parse_checksum_line_empty() {
    assert!(parse_checksum_line("").is_none());
    assert!(parse_checksum_line("   ").is_none());
}

#[tokio::test]
#[ignore] // requires qemu-img
async fn qemu_img_info_valid_qcow2() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = dir.path().join("test.qcow2");

    // Create a qcow2 image
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&img_path)
        .arg("1G")
        .status()
        .await
        .expect("failed to run qemu-img create");
    assert!(status.success());

    let info = qemu_img_info(&img_path).await.unwrap();
    assert_eq!(info.format, "qcow2");
    assert_eq!(info.virtual_size, 1024 * 1024 * 1024);
    assert!(info.actual_size > 0);
}

#[tokio::test]
#[ignore] // requires qemu-img
async fn qemu_img_info_rejects_backing_file() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.qcow2");
    let overlay_path = dir.path().join("overlay.qcow2");

    // Create base image
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&base_path)
        .arg("1G")
        .status()
        .await
        .unwrap();
    assert!(status.success());

    // Create overlay with backing file
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2", "-F", "qcow2", "-b"])
        .arg(&base_path)
        .arg(&overlay_path)
        .status()
        .await
        .unwrap();
    assert!(status.success());

    let err = qemu_img_info(&overlay_path).await.unwrap_err();
    assert!(matches!(err, ImageError::BackingFile(_)));
}

#[tokio::test]
#[ignore] // requires qemu-img
async fn validate_image_format_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = dir.path().join("test.raw");

    // Create a raw image
    let status = tokio::process::Command::new("qemu-img")
        .args(["create", "-f", "raw"])
        .arg(&img_path)
        .arg("1M")
        .status()
        .await
        .unwrap();
    assert!(status.success());

    // Validate as qcow2 — should fail
    let err = validate_image(&img_path, ImageFormat::Qcow2)
        .await
        .unwrap_err();
    assert!(matches!(err, ImageError::InvalidImage(_)));
}
