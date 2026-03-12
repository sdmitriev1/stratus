use stratus_vm::host::{Arch, HostInfo, detect, which};

// --- Arch display ---

#[test]
fn arch_display_x86_64() {
    assert_eq!(Arch::X86_64.to_string(), "x86_64");
}

#[test]
fn arch_display_aarch64() {
    assert_eq!(Arch::Aarch64.to_string(), "aarch64");
}

// --- which() ---

#[test]
fn which_finds_sh() {
    // /bin/sh should always exist on Linux
    assert!(which("sh").is_some());
}

#[test]
fn which_finds_ls() {
    assert!(which("ls").is_some());
}

#[test]
fn which_returns_absolute_path() {
    let path = which("sh").unwrap();
    assert!(path.starts_with('/'), "should be absolute: {path}");
}

#[test]
fn which_missing_binary() {
    assert!(which("definitely_not_a_real_binary_xyz_123").is_none());
}

#[test]
fn which_empty_name() {
    assert!(which("").is_none());
}

// --- detect() ---

#[test]
fn detect_returns_valid_arch() {
    // detect() might fail if qemu isn't installed, which is fine
    if let Ok(info) = detect() {
        // We're running on Linux, so arch should be one of these
        assert!(
            info.arch == Arch::X86_64 || info.arch == Arch::Aarch64,
            "unexpected arch: {:?}",
            info.arch
        );
    }
}

#[test]
fn detect_arch_matches_compile_target() {
    if let Ok(info) = detect() {
        match std::env::consts::ARCH {
            "x86_64" => assert_eq!(info.arch, Arch::X86_64),
            "aarch64" => assert_eq!(info.arch, Arch::Aarch64),
            _ => {} // other arches would error in detect()
        }
    }
}

#[test]
fn detect_qemu_binary_contains_arch() {
    if let Ok(info) = detect() {
        match info.arch {
            Arch::X86_64 => assert!(info.qemu_binary.contains("x86_64")),
            Arch::Aarch64 => assert!(info.qemu_binary.contains("aarch64")),
        }
    }
}

#[test]
fn detect_qemu_binary_is_absolute_path() {
    if let Ok(info) = detect() {
        assert!(
            info.qemu_binary.starts_with('/'),
            "qemu_binary should be absolute: {}",
            info.qemu_binary
        );
    }
}

// --- HostInfo ---

#[test]
fn host_info_clone() {
    let info = HostInfo {
        arch: Arch::Aarch64,
        kvm_available: false,
        qemu_binary: "qemu-system-aarch64".into(),
    };
    let cloned = info.clone();
    assert_eq!(cloned.arch, Arch::Aarch64);
    assert!(!cloned.kvm_available);
    assert_eq!(cloned.qemu_binary, "qemu-system-aarch64");
}

#[test]
fn arch_eq() {
    assert_eq!(Arch::X86_64, Arch::X86_64);
    assert_eq!(Arch::Aarch64, Arch::Aarch64);
    assert_ne!(Arch::X86_64, Arch::Aarch64);
}
