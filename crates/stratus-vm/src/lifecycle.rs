use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::libc;
use stratus_resources::{Image, ImageFormat, Instance};
use tracing::{info, warn};

use crate::VmError;
use crate::disk;
use crate::firmware::{self, FirmwarePaths};
use crate::host::HostInfo;
use crate::qemu::{self, QemuConfig};
use crate::qmp::QmpClient;
use crate::tap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceStatus {
    Pending,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

impl std::fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceStatus::Pending => write!(f, "Pending"),
            InstanceStatus::Starting => write!(f, "Starting"),
            InstanceStatus::Running => write!(f, "Running"),
            InstanceStatus::Stopping => write!(f, "Stopping"),
            InstanceStatus::Stopped => write!(f, "Stopped"),
            InstanceStatus::Failed => write!(f, "Failed"),
        }
    }
}

pub struct VmHandle {
    pub name: String,
    pub status: InstanceStatus,
    pub pid: Option<u32>,
    pub instance_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub child: Option<tokio::process::Child>,
    pub tap_names: Vec<String>,
}

/// Prepare an instance: create dirs, firmware, overlay disk.
pub async fn prepare(
    instance: &Instance,
    image: &Image,
    base_image_path: &Path,
    data_dir: &Path,
    host: &HostInfo,
) -> Result<VmHandle, VmError> {
    let instance_dir = data_dir.join("instances").join(&instance.name);
    let runtime_dir = PathBuf::from("/run/stratus").join(&instance.name);

    std::fs::create_dir_all(&instance_dir)?;
    std::fs::create_dir_all(&runtime_dir)?;

    // Find and prepare firmware
    let (code_path, vars_template) = firmware::find_firmware(host.arch)?;
    let _firmware = firmware::prepare_vars(&code_path, &vars_template, &instance_dir)?;

    // Create overlay disk
    let overlay_path = instance_dir.join("disk.qcow2");
    if !overlay_path.exists() {
        let base_format = match image.format {
            ImageFormat::Qcow2 => "qcow2",
            ImageFormat::Raw => "raw",
        };
        disk::create_overlay(&overlay_path, base_image_path, base_format).await?;
    }

    // Resize if needed
    disk::resize_if_needed(&overlay_path, instance.disk_gb).await?;

    info!(name = instance.name, "instance prepared");

    Ok(VmHandle {
        name: instance.name.clone(),
        status: InstanceStatus::Pending,
        pid: None,
        instance_dir,
        runtime_dir,
        child: None,
        tap_names: Vec::new(),
    })
}

/// Start QEMU for a prepared instance.
pub async fn start(
    handle: &mut VmHandle,
    instance: &Instance,
    host: &HostInfo,
) -> Result<(), VmError> {
    if handle.status == InstanceStatus::Running {
        return Err(VmError::AlreadyRunning);
    }

    handle.status = InstanceStatus::Starting;

    // Create tap devices
    let mut tap_fds: Vec<OwnedFd> = Vec::new();
    let mut tap_names: Vec<String> = Vec::new();

    for (i, _iface) in instance.interfaces.iter().enumerate() {
        let name = tap::tap_name(&instance.name, i);
        let fd = tap::create_tap(&name)?;
        tap::set_up(&name).await?;
        tap_names.push(name);
        tap_fds.push(fd);
    }

    // Build firmware paths
    let firmware = FirmwarePaths {
        code: {
            let (code, _) = firmware::find_firmware(host.arch)?;
            code
        },
        vars: handle.instance_dir.join("VARS.fd"),
    };

    let disk_path = handle.instance_dir.join("disk.qcow2");

    let config = QemuConfig {
        instance,
        host,
        firmware: &firmware,
        disk_path: &disk_path,
        runtime_dir: &handle.runtime_dir,
        tap_fds: &tap_fds,
    };

    let (binary, args) = qemu::build_args(&config);

    info!(name = instance.name, binary = binary, "starting QEMU");

    // Build the fds to pass to the child process.
    // We need to keep the tap fds open and pass them through.
    use std::os::fd::AsRawFd;
    let raw_fds: Vec<i32> = tap_fds.iter().map(|fd| fd.as_raw_fd()).collect();

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.args(&args);

    // Pass tap fds through to QEMU by clearing close-on-exec
    unsafe {
        let fds_for_closure = raw_fds.clone();
        cmd.pre_exec(move || {
            for &fd in &fds_for_closure {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags >= 0 {
                    libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
                }
            }
            Ok(())
        });
    }

    let child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| VmError::QemuStart(format!("failed to spawn QEMU: {e}")))?;

    let pid = child.id();
    handle.pid = pid;
    handle.child = Some(child);
    handle.tap_names = tap_names;

    // Drop tap fds in parent — QEMU now owns them
    drop(tap_fds);

    // Wait for QMP to become available
    let qmp_sock = handle.runtime_dir.join("qmp.sock");
    let mut connected = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if QmpClient::connect(&qmp_sock).await.is_ok() {
            connected = true;
            break;
        }
    }

    if connected {
        handle.status = InstanceStatus::Running;
        info!(name = instance.name, pid = ?handle.pid, "QEMU started");
    } else {
        handle.status = InstanceStatus::Failed;
        warn!(name = instance.name, "QEMU started but QMP not reachable");
    }

    Ok(())
}

/// Gracefully stop a VM: ACPI powerdown, then escalate to SIGTERM/SIGKILL.
pub async fn stop(handle: &mut VmHandle, timeout: Duration) -> Result<(), VmError> {
    if handle.status != InstanceStatus::Running && handle.status != InstanceStatus::Starting {
        return Err(VmError::NotRunning);
    }

    handle.status = InstanceStatus::Stopping;

    // Try QMP powerdown
    let qmp_sock = handle.runtime_dir.join("qmp.sock");
    if let Ok(mut qmp) = QmpClient::connect(&qmp_sock).await {
        let _ = qmp.system_powerdown().await;
    }

    // Wait for process to exit
    if let Some(ref mut child) = handle.child {
        let exited = tokio::time::timeout(timeout, child.wait()).await;
        if exited.is_ok() {
            handle.status = InstanceStatus::Stopped;
            handle.pid = None;
            handle.child = None;
            cleanup_taps(&handle.tap_names).await;
            return Ok(());
        }

        // Escalate to SIGTERM
        if let Some(pid) = handle.pid {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGTERM,
            );
        }

        let exited = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        if exited.is_ok() {
            handle.status = InstanceStatus::Stopped;
            handle.pid = None;
            handle.child = None;
            cleanup_taps(&handle.tap_names).await;
            return Ok(());
        }

        // Final escalation: SIGKILL
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    handle.status = InstanceStatus::Stopped;
    handle.pid = None;
    handle.child = None;
    cleanup_taps(&handle.tap_names).await;
    Ok(())
}

/// Kill a VM immediately: SIGTERM then SIGKILL.
pub async fn kill(handle: &mut VmHandle) -> Result<(), VmError> {
    if handle.status != InstanceStatus::Running && handle.status != InstanceStatus::Starting {
        return Err(VmError::NotRunning);
    }

    if let Some(pid) = handle.pid {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGTERM,
        );
    }

    if let Some(ref mut child) = handle.child {
        let exited = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        if exited.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    handle.status = InstanceStatus::Stopped;
    handle.pid = None;
    handle.child = None;
    cleanup_taps(&handle.tap_names).await;
    Ok(())
}

/// Destroy a VM: stop if running, remove instance dir and runtime dir.
pub async fn destroy(handle: &mut VmHandle) -> Result<(), VmError> {
    if handle.status == InstanceStatus::Running || handle.status == InstanceStatus::Starting {
        kill(handle).await?;
    }

    cleanup_taps(&handle.tap_names).await;

    if handle.instance_dir.exists() {
        std::fs::remove_dir_all(&handle.instance_dir).map_err(VmError::Io)?;
    }

    if handle.runtime_dir.exists() {
        let _ = std::fs::remove_dir_all(&handle.runtime_dir);
    }

    Ok(())
}

async fn cleanup_taps(tap_names: &[String]) {
    for name in tap_names {
        if let Err(e) = tap::delete_tap(name).await {
            warn!(tap = name, error = %e, "failed to delete tap");
        }
    }
}
