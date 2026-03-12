use stratus_vm::VmError;

#[test]
fn io_error_display() {
    let err = VmError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    ));
    assert!(err.to_string().contains("file not found"));
}

#[test]
fn qemu_img_error_display() {
    let err = VmError::QemuImg("create failed".into());
    assert_eq!(err.to_string(), "qemu-img failed: create failed");
}

#[test]
fn qemu_start_error_display() {
    let err = VmError::QemuStart("spawn failed".into());
    assert_eq!(err.to_string(), "QEMU start failed: spawn failed");
}

#[test]
fn qmp_error_display() {
    let err = VmError::Qmp("timeout".into());
    assert_eq!(err.to_string(), "QMP error: timeout");
}

#[test]
fn firmware_not_found_display() {
    let err = VmError::FirmwareNotFound("aarch64".into());
    assert_eq!(err.to_string(), "firmware not found: aarch64");
}

#[test]
fn tap_error_display() {
    let err = VmError::Tap("ioctl failed".into());
    assert_eq!(err.to_string(), "tap device error: ioctl failed");
}

#[test]
fn host_error_display() {
    let err = VmError::Host("unsupported arch".into());
    assert_eq!(err.to_string(), "host detection error: unsupported arch");
}

#[test]
fn not_running_display() {
    let err = VmError::NotRunning;
    assert_eq!(err.to_string(), "VM is not running");
}

#[test]
fn already_running_display() {
    let err = VmError::AlreadyRunning;
    assert_eq!(err.to_string(), "VM is already running");
}

#[test]
fn io_error_from_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let vm_err: VmError = io_err.into();
    assert!(matches!(vm_err, VmError::Io(_)));
}
