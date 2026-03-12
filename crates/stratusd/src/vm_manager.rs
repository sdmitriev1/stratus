use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, warn};

use stratus_resources::{Image, Instance};
use stratus_vm::VmError;
use stratus_vm::host::HostInfo;
use stratus_vm::lifecycle::{self, InstanceStatus, VmHandle};

pub struct VmManager {
    host: HostInfo,
    data_dir: PathBuf,
    #[allow(dead_code)]
    runtime_dir: PathBuf,
    vms: RwLock<HashMap<String, VmHandle>>,
}

impl VmManager {
    pub fn new(host: HostInfo, data_dir: PathBuf, runtime_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            host,
            data_dir,
            runtime_dir,
            vms: RwLock::new(HashMap::new()),
        })
    }

    /// Create a stub VmManager for testing (no real QEMU operations).
    pub fn stub(data_dir: PathBuf) -> Arc<Self> {
        use stratus_vm::host::Arch;
        Arc::new(Self {
            host: HostInfo {
                arch: Arch::Aarch64,
                kvm_available: false,
                qemu_binary: "qemu-system-aarch64".into(),
            },
            data_dir: data_dir.clone(),
            runtime_dir: data_dir.join("run"),
            vms: RwLock::new(HashMap::new()),
        })
    }

    /// Recover running VMs by scanning runtime directory for PID files.
    pub async fn recover(&self) {
        let runtime_base = PathBuf::from("/run/stratus");
        let entries = match std::fs::read_dir(&runtime_base) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Skip non-instance dirs (like the socket dir)
            if name == "stratusd.sock" {
                continue;
            }

            let pid_file = path.join("qemu.pid");
            if !pid_file.exists() {
                continue;
            }

            let pid_str = match std::fs::read_to_string(&pid_file) {
                Ok(s) => s.trim().to_string(),
                Err(_) => continue,
            };

            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => {
                    warn!(name, "invalid PID file, cleaning up");
                    let _ = std::fs::remove_dir_all(&path);
                    continue;
                }
            };

            // Check if process is still running
            let proc_path = format!("/proc/{pid}");
            if Path::new(&proc_path).exists() {
                info!(name, pid, "recovered running VM");
                let handle = VmHandle {
                    name: name.clone(),
                    status: InstanceStatus::Running,
                    pid: Some(pid),
                    instance_dir: self.data_dir.join("instances").join(&name),
                    runtime_dir: path.clone(),
                    child: None,           // Can't recover the Child handle
                    tap_names: Vec::new(), // Can't recover tap names
                };
                self.vms.write().await.insert(name, handle);
            } else {
                info!(name, pid, "cleaning up stale VM runtime");
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    /// Start a new instance VM.
    pub async fn start_instance(
        &self,
        name: &str,
        instance: &Instance,
        image: &Image,
        base_image_path: &Path,
    ) -> Result<InstanceStatus, VmError> {
        // Check if already running
        {
            let vms = self.vms.read().await;
            if let Some(handle) = vms.get(name)
                && handle.status == InstanceStatus::Running
            {
                return Err(VmError::AlreadyRunning);
            }
        }

        let mut handle =
            lifecycle::prepare(instance, image, base_image_path, &self.data_dir, &self.host)
                .await?;

        lifecycle::start(&mut handle, instance, &self.host).await?;

        let status = handle.status;
        self.vms.write().await.insert(name.to_string(), handle);
        Ok(status)
    }

    /// Stop an instance gracefully.
    pub async fn stop_instance(
        &self,
        name: &str,
        timeout: Duration,
    ) -> Result<InstanceStatus, VmError> {
        let mut vms = self.vms.write().await;
        let handle = vms.get_mut(name).ok_or(VmError::NotRunning)?;

        lifecycle::stop(handle, timeout).await?;
        Ok(handle.status)
    }

    /// Kill an instance immediately.
    pub async fn kill_instance(&self, name: &str) -> Result<InstanceStatus, VmError> {
        let mut vms = self.vms.write().await;
        let handle = vms.get_mut(name).ok_or(VmError::NotRunning)?;

        lifecycle::kill(handle).await?;
        Ok(handle.status)
    }

    /// Destroy an instance: stop if running, remove all files.
    pub async fn destroy_instance(&self, name: &str) -> Result<(), VmError> {
        let mut vms = self.vms.write().await;
        if let Some(mut handle) = vms.remove(name) {
            lifecycle::destroy(&mut handle).await?;
        }
        Ok(())
    }

    /// Get status of all tracked VMs.
    pub async fn statuses(&self) -> HashMap<String, InstanceStatus> {
        let vms = self.vms.read().await;
        vms.iter()
            .map(|(name, handle)| (name.clone(), handle.status))
            .collect()
    }

    /// Get status of a single VM.
    pub async fn status(&self, name: &str) -> Option<InstanceStatus> {
        let vms = self.vms.read().await;
        vms.get(name).map(|h| h.status)
    }
}
