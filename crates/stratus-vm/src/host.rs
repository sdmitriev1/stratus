use std::path::Path;

use crate::VmError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arch::X86_64 => write!(f, "x86_64"),
            Arch::Aarch64 => write!(f, "aarch64"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HostInfo {
    pub arch: Arch,
    pub kvm_available: bool,
    pub qemu_binary: String,
}

/// Detect host architecture, KVM availability, and QEMU binary.
pub fn detect() -> Result<HostInfo, VmError> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => Arch::X86_64,
        "aarch64" => Arch::Aarch64,
        other => return Err(VmError::Host(format!("unsupported architecture: {other}"))),
    };

    let kvm_available = Path::new("/dev/kvm").exists()
        && std::fs::metadata("/dev/kvm")
            .map(|m| {
                use std::os::unix::fs::MetadataExt;
                // Check if readable+writable (character device accessible)
                m.mode() & 0o666 != 0
            })
            .unwrap_or(false);

    let binary_name = match arch {
        Arch::X86_64 => "qemu-system-x86_64",
        Arch::Aarch64 => "qemu-system-aarch64",
    };

    // Verify qemu binary exists on PATH
    let qemu_binary = which(binary_name)
        .ok_or_else(|| VmError::Host(format!("{binary_name} not found in PATH")))?;

    Ok(HostInfo {
        arch,
        kvm_available,
        qemu_binary,
    })
}

/// Simple PATH lookup for a binary.
pub fn which(name: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}
