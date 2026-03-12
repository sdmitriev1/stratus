use stratus_vm::firmware::{FirmwarePaths, find_firmware, prepare_vars};
use stratus_vm::host::Arch;

// --- find_firmware ---

#[test]
fn find_firmware_returns_existing_paths() {
    // This may or may not find firmware depending on the host.
    // If it succeeds, both paths must exist.
    if let Ok((code, vars)) = find_firmware(Arch::Aarch64) {
        assert!(code.exists(), "code path should exist: {}", code.display());
        assert!(vars.exists(), "vars path should exist: {}", vars.display());
    }
}

#[test]
fn find_firmware_x86_returns_existing_paths() {
    if let Ok((code, vars)) = find_firmware(Arch::X86_64) {
        assert!(code.exists());
        assert!(vars.exists());
    }
}

#[test]
fn find_firmware_error_message_includes_arch() {
    // If firmware isn't installed, the error should mention the arch.
    // This test is useful regardless of whether firmware is installed.
    if let Err(e) = find_firmware(Arch::Aarch64) {
        let msg = e.to_string();
        assert!(msg.contains("aarch64"), "error should mention arch: {msg}");
    }
    if let Err(e) = find_firmware(Arch::X86_64) {
        let msg = e.to_string();
        assert!(msg.contains("x86_64"), "error should mention arch: {msg}");
    }
}

// --- prepare_vars ---

#[test]
fn prepare_vars_copies_template() {
    let dir = tempfile::tempdir().unwrap();
    let template = dir.path().join("VARS_template.fd");
    let code = dir.path().join("CODE.fd");
    let instance_dir = dir.path().join("instance");

    std::fs::write(&template, b"firmware vars data").unwrap();
    std::fs::write(&code, b"firmware code data").unwrap();
    std::fs::create_dir(&instance_dir).unwrap();

    let result = prepare_vars(&code, &template, &instance_dir).unwrap();
    assert_eq!(result.code, code);
    assert_eq!(result.vars, instance_dir.join("VARS.fd"));
    assert!(result.vars.exists());

    let content = std::fs::read(&result.vars).unwrap();
    assert_eq!(content, b"firmware vars data");
}

#[test]
fn prepare_vars_skips_if_exists() {
    let dir = tempfile::tempdir().unwrap();
    let template = dir.path().join("VARS_template.fd");
    let code = dir.path().join("CODE.fd");
    let instance_dir = dir.path().join("instance");

    std::fs::write(&template, b"new template data").unwrap();
    std::fs::write(&code, b"firmware code").unwrap();
    std::fs::create_dir(&instance_dir).unwrap();

    // Pre-create the instance VARS file
    let existing_vars = instance_dir.join("VARS.fd");
    std::fs::write(&existing_vars, b"existing vars data").unwrap();

    let result = prepare_vars(&code, &template, &instance_dir).unwrap();

    // Should NOT overwrite
    let content = std::fs::read(&result.vars).unwrap();
    assert_eq!(content, b"existing vars data");
}

#[test]
fn prepare_vars_returns_correct_paths() {
    let dir = tempfile::tempdir().unwrap();
    let template = dir.path().join("template.fd");
    let code = dir.path().join("code.fd");
    let instance_dir = dir.path().join("inst");

    std::fs::write(&template, b"t").unwrap();
    std::fs::write(&code, b"c").unwrap();
    std::fs::create_dir(&instance_dir).unwrap();

    let FirmwarePaths { code: c, vars: v } = prepare_vars(&code, &template, &instance_dir).unwrap();
    assert_eq!(c, code);
    assert_eq!(v, instance_dir.join("VARS.fd"));
}
