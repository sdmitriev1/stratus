use stratus_vm::tap::tap_name;

// --- Basic naming ---

#[test]
fn short_name() {
    assert_eq!(tap_name("web", 0), "st-web-0");
}

#[test]
fn single_char_name() {
    assert_eq!(tap_name("a", 0), "st-a-0");
}

#[test]
fn empty_name() {
    assert_eq!(tap_name("", 0), "st--0");
}

// --- Length constraint (max 15 chars, Linux IFNAMSIZ) ---

#[test]
fn truncation_at_15_chars() {
    let name = tap_name("my-very-long-instance-name", 0);
    assert!(
        name.len() <= 15,
        "tap name too long: {name} (len={})",
        name.len()
    );
    assert!(name.starts_with("st-"));
    assert!(name.ends_with("-0"));
}

#[test]
fn exact_boundary_name() {
    // "st-" = 3, "-0" = 2, so max name portion = 10
    let name = tap_name("abcdefghij", 0); // exactly 10 chars
    assert!(name.len() <= 15, "tap name too long: {name}");
    assert_eq!(name, "st-abcdefghij-0");
}

#[test]
fn one_over_boundary() {
    let name = tap_name("abcdefghijk", 0); // 11 chars, should be truncated
    assert!(name.len() <= 15, "tap name too long: {name}");
    assert!(name.starts_with("st-"));
    assert!(name.ends_with("-0"));
}

#[test]
fn long_name_with_double_digit_index() {
    let name = tap_name("very-long-name", 12);
    assert!(
        name.len() <= 15,
        "tap name too long: {name} (len={})",
        name.len()
    );
    assert!(name.ends_with("-12"));
}

#[test]
fn long_name_with_triple_digit_index() {
    let name = tap_name("instance", 100);
    assert!(
        name.len() <= 15,
        "tap name too long: {name} (len={})",
        name.len()
    );
    assert!(name.ends_with("-100"));
}

// --- Multi-index ---

#[test]
fn index_zero() {
    let name = tap_name("dev-vm", 0);
    assert!(name.ends_with("-0"));
}

#[test]
fn index_one() {
    let name = tap_name("dev-vm", 1);
    assert!(name.ends_with("-1"));
}

#[test]
fn index_nine() {
    let name = tap_name("dev-vm", 9);
    assert!(name.ends_with("-9"));
}

#[test]
fn index_ten() {
    let name = tap_name("dev-vm", 10);
    assert!(name.ends_with("-10"));
}

// --- Special character filtering ---

#[test]
fn filters_dots() {
    let name = tap_name("my.vm", 0);
    assert!(!name.contains('.'), "dots should be filtered: {name}");
}

#[test]
fn filters_underscores() {
    let name = tap_name("my_vm", 0);
    assert!(
        !name.contains('_'),
        "underscores should be filtered: {name}"
    );
}

#[test]
fn filters_spaces() {
    let name = tap_name("my vm", 0);
    assert!(!name.contains(' '), "spaces should be filtered: {name}");
}

#[test]
fn filters_slashes() {
    let name = tap_name("my/vm", 0);
    assert!(!name.contains('/'), "slashes should be filtered: {name}");
}

#[test]
fn preserves_hyphens() {
    let name = tap_name("my-vm", 0);
    assert_eq!(name, "st-my-vm-0");
}

#[test]
fn preserves_alphanumerics() {
    let name = tap_name("vm1", 0);
    assert_eq!(name, "st-vm1-0");
}

#[test]
fn all_special_chars_produces_empty_middle() {
    // All filtered out
    let name = tap_name("...__///", 0);
    assert_eq!(name, "st--0");
}

// --- Uniqueness: different instances produce different names ---

#[test]
fn different_instances_different_names() {
    let a = tap_name("alpha", 0);
    let b = tap_name("beta", 0);
    assert_ne!(a, b);
}

#[test]
fn same_instance_different_index() {
    let a = tap_name("vm", 0);
    let b = tap_name("vm", 1);
    assert_ne!(a, b);
}
