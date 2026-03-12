use std::path::{Path, PathBuf};

use tracing::info;

use crate::VmError;
use crate::host::Arch;

#[derive(Debug, Clone)]
pub struct FirmwarePaths {
    pub code: PathBuf,
    pub vars: PathBuf,
}

/// Well-known OVMF/AAVMF firmware paths per architecture.
fn firmware_search_paths(arch: Arch) -> Vec<(&'static str, &'static str)> {
    match arch {
        Arch::Aarch64 => vec![
            (
                "/usr/share/AAVMF/AAVMF_CODE.fd",
                "/usr/share/AAVMF/AAVMF_VARS.fd",
            ),
            (
                "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd",
                "/usr/share/qemu-efi-aarch64/QEMU_VARS.fd",
            ),
            (
                "/usr/share/edk2/aarch64/QEMU_CODE.fd",
                "/usr/share/edk2/aarch64/QEMU_VARS.fd",
            ),
        ],
        Arch::X86_64 => vec![
            (
                "/usr/share/OVMF/OVMF_CODE.fd",
                "/usr/share/OVMF/OVMF_VARS.fd",
            ),
            (
                "/usr/share/OVMF/OVMF_CODE_4M.fd",
                "/usr/share/OVMF/OVMF_VARS_4M.fd",
            ),
            (
                "/usr/share/edk2/x64/OVMF_CODE.4m.fd",
                "/usr/share/edk2/x64/OVMF_VARS.4m.fd",
            ),
        ],
    }
}

/// Find firmware CODE and VARS template files on the host.
pub fn find_firmware(arch: Arch) -> Result<(PathBuf, PathBuf), VmError> {
    for (code, vars) in firmware_search_paths(arch) {
        if Path::new(code).exists() && Path::new(vars).exists() {
            return Ok((PathBuf::from(code), PathBuf::from(vars)));
        }
    }

    Err(VmError::FirmwareNotFound(format!(
        "no UEFI firmware found for {arch}"
    )))
}

/// Copy VARS template into instance directory, returning usable FirmwarePaths.
/// Skips copy if the instance-local VARS already exists.
pub fn prepare_vars(
    code_path: &Path,
    vars_template: &Path,
    instance_dir: &Path,
) -> Result<FirmwarePaths, VmError> {
    let instance_vars = instance_dir.join("VARS.fd");

    if !instance_vars.exists() {
        info!(
            src = %vars_template.display(),
            dst = %instance_vars.display(),
            "copying firmware VARS"
        );
        std::fs::copy(vars_template, &instance_vars)?;
    }

    Ok(FirmwarePaths {
        code: code_path.to_path_buf(),
        vars: instance_vars,
    })
}
