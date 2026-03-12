use std::os::fd::{FromRawFd, OwnedFd};

use nix::libc;

use crate::VmError;

/// Generate a tap device name from instance name and interface index.
/// Format: `st-<truncated>-<idx>`, max 15 chars (Linux IFNAMSIZ limit).
pub fn tap_name(instance_name: &str, iface_index: usize) -> String {
    let suffix = format!("-{iface_index}");
    let prefix = "st-";
    let max_name_len = 15 - prefix.len() - suffix.len();
    let truncated: String = instance_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .take(max_name_len)
        .collect();
    format!("{prefix}{truncated}{suffix}")
}

/// Create a TAP device and return the fd.
/// Uses ioctl TUNSETIFF on /dev/net/tun.
pub fn create_tap(name: &str) -> Result<OwnedFd, VmError> {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/net/tun")
        .map_err(|e| VmError::Tap(format!("failed to open /dev/net/tun: {e}")))?;

    // IFF_TAP | IFF_NO_PI | IFF_VNET_HDR
    const IFF_TAP: libc::c_short = 0x0002;
    const IFF_NO_PI: libc::c_short = 0x1000;
    const IFF_VNET_HDR: libc::c_short = 0x4000;

    #[repr(C)]
    struct Ifreq {
        ifr_name: [u8; libc::IFNAMSIZ],
        ifr_flags: libc::c_short,
        _pad: [u8; 22],
    }

    let mut ifr = Ifreq {
        ifr_name: [0u8; libc::IFNAMSIZ],
        ifr_flags: IFF_TAP | IFF_NO_PI | IFF_VNET_HDR,
        _pad: [0u8; 22],
    };

    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(libc::IFNAMSIZ - 1);
    ifr.ifr_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

    // TUNSETIFF = _IOW('T', 202, int) = 0x400454ca
    const TUNSETIFF: u32 = 0x400454ca;

    let ret = unsafe { libc::ioctl(file.as_raw_fd(), TUNSETIFF as _, &ifr as *const Ifreq) };

    if ret < 0 {
        return Err(VmError::Tap(format!(
            "TUNSETIFF failed for {name}: {}",
            std::io::Error::last_os_error()
        )));
    }

    // Convert to OwnedFd — the file handle must stay open for QEMU
    let raw_fd = file.as_raw_fd();
    std::mem::forget(file); // prevent close
    Ok(unsafe { OwnedFd::from_raw_fd(raw_fd) })
}

/// Delete a tap device.
pub async fn delete_tap(name: &str) -> Result<(), VmError> {
    let output = tokio::process::Command::new("ip")
        .args(["link", "delete", name])
        .output()
        .await
        .map_err(|e| VmError::Tap(format!("failed to run ip link delete: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "not found" — device may already be gone
        if !stderr.contains("Cannot find device") {
            return Err(VmError::Tap(format!(
                "ip link delete {name} failed: {stderr}"
            )));
        }
    }

    Ok(())
}

/// Bring a tap device up.
pub async fn set_up(name: &str) -> Result<(), VmError> {
    let output = tokio::process::Command::new("ip")
        .args(["link", "set", name, "up"])
        .output()
        .await
        .map_err(|e| VmError::Tap(format!("failed to run ip link set up: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::Tap(format!(
            "ip link set {name} up failed: {stderr}"
        )));
    }

    Ok(())
}
