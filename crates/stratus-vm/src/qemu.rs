use std::os::fd::AsRawFd;
use std::os::fd::OwnedFd;
use std::path::Path;

use stratus_resources::Instance;

use crate::firmware::FirmwarePaths;
use crate::host::{Arch, HostInfo};

pub struct QemuConfig<'a> {
    pub instance: &'a Instance,
    pub host: &'a HostInfo,
    pub firmware: &'a FirmwarePaths,
    pub disk_path: &'a Path,
    pub runtime_dir: &'a Path,
    pub tap_fds: &'a [OwnedFd],
}

/// Build QEMU command-line arguments.
/// Returns (binary_path, args).
pub fn build_args(config: &QemuConfig<'_>) -> (String, Vec<String>) {
    let inst = config.instance;
    let host = config.host;
    let mut args = Vec::new();

    // -name
    args.push("-name".into());
    args.push(inst.name.clone());

    // -machine
    let machine = match (host.arch, host.kvm_available) {
        (Arch::Aarch64, true) => "virt,accel=kvm,gic-version=3",
        (Arch::Aarch64, false) => "virt,accel=tcg",
        (Arch::X86_64, true) => "q35,accel=kvm",
        (Arch::X86_64, false) => "q35,accel=tcg",
    };
    args.push("-machine".into());
    args.push(machine.into());

    // -cpu
    args.push("-cpu".into());
    if host.kvm_available {
        args.push("host".into());
    } else {
        args.push("max".into());
    }

    // -m and -smp
    args.push("-m".into());
    args.push(format!("{}", inst.memory_mb));
    args.push("-smp".into());
    args.push(format!("{}", inst.cpus));

    // Display/console
    args.push("-nographic".into());
    args.push("-nodefaults".into());

    // Serial console socket
    let serial_sock = config.runtime_dir.join("serial.sock");
    args.push("-chardev".into());
    args.push(format!(
        "socket,id=serial0,path={},server=on,wait=off",
        serial_sock.display()
    ));
    args.push("-serial".into());
    args.push("chardev:serial0".into());

    // QMP socket
    let qmp_sock = config.runtime_dir.join("qmp.sock");
    args.push("-chardev".into());
    args.push(format!(
        "socket,id=qmp0,path={},server=on,wait=off",
        qmp_sock.display()
    ));
    args.push("-mon".into());
    args.push("chardev=qmp0,mode=control".into());

    // VNC socket
    let vnc_sock = config.runtime_dir.join("vnc.sock");
    args.push("-vnc".into());
    args.push(format!("unix:{}", vnc_sock.display()));

    // PID file
    let pid_file = config.runtime_dir.join("qemu.pid");
    args.push("-pidfile".into());
    args.push(pid_file.to_string_lossy().into_owned());

    // Firmware
    args.push("-drive".into());
    args.push(format!(
        "if=pflash,format=raw,readonly=on,file={}",
        config.firmware.code.display()
    ));
    args.push("-drive".into());
    args.push(format!(
        "if=pflash,format=raw,file={}",
        config.firmware.vars.display()
    ));

    // Main disk
    args.push("-drive".into());
    args.push(format!(
        "file={},format=qcow2,if=virtio,cache=writeback,discard=unmap",
        config.disk_path.display()
    ));

    // RNG
    args.push("-device".into());
    args.push("virtio-rng-pci".into());

    // Network interfaces
    for (i, (iface, fd)) in inst.interfaces.iter().zip(config.tap_fds).enumerate() {
        let net_id = format!("net{i}");
        args.push("-netdev".into());
        args.push(format!("tap,id={net_id},fd={}", fd.as_raw_fd()));
        args.push("-device".into());
        let mac = iface.mac.as_deref().unwrap_or("52:54:00:00:00:01");
        args.push(format!("virtio-net-pci,netdev={net_id},mac={mac}"));
    }

    (host.qemu_binary.clone(), args)
}
