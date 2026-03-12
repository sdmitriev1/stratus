pub mod disk;
pub mod firmware;
pub mod host;
pub mod lifecycle;
pub mod qemu;
pub mod qmp;
pub mod tap;

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("qemu-img failed: {0}")]
    QemuImg(String),
    #[error("QEMU start failed: {0}")]
    QemuStart(String),
    #[error("QMP error: {0}")]
    Qmp(String),
    #[error("firmware not found: {0}")]
    FirmwareNotFound(String),
    #[error("tap device error: {0}")]
    Tap(String),
    #[error("host detection error: {0}")]
    Host(String),
    #[error("VM is not running")]
    NotRunning,
    #[error("VM is already running")]
    AlreadyRunning,
}
