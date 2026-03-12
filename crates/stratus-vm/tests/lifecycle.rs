use stratus_vm::lifecycle::InstanceStatus;

// --- InstanceStatus display ---

#[test]
fn status_display_pending() {
    assert_eq!(InstanceStatus::Pending.to_string(), "Pending");
}

#[test]
fn status_display_starting() {
    assert_eq!(InstanceStatus::Starting.to_string(), "Starting");
}

#[test]
fn status_display_running() {
    assert_eq!(InstanceStatus::Running.to_string(), "Running");
}

#[test]
fn status_display_stopping() {
    assert_eq!(InstanceStatus::Stopping.to_string(), "Stopping");
}

#[test]
fn status_display_stopped() {
    assert_eq!(InstanceStatus::Stopped.to_string(), "Stopped");
}

#[test]
fn status_display_failed() {
    assert_eq!(InstanceStatus::Failed.to_string(), "Failed");
}

// --- InstanceStatus equality ---

#[test]
fn status_eq() {
    assert_eq!(InstanceStatus::Running, InstanceStatus::Running);
    assert_ne!(InstanceStatus::Running, InstanceStatus::Stopped);
}

#[test]
fn status_copy() {
    let s = InstanceStatus::Pending;
    let s2 = s;
    assert_eq!(s, s2);
}

// --- All variants are distinct ---

#[test]
fn all_variants_distinct() {
    let variants = [
        InstanceStatus::Pending,
        InstanceStatus::Starting,
        InstanceStatus::Running,
        InstanceStatus::Stopping,
        InstanceStatus::Stopped,
        InstanceStatus::Failed,
    ];
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b, "{a} should != {b}");
            }
        }
    }
}

// --- All display values are distinct ---

#[test]
fn all_display_values_distinct() {
    let displays: Vec<String> = [
        InstanceStatus::Pending,
        InstanceStatus::Starting,
        InstanceStatus::Running,
        InstanceStatus::Stopping,
        InstanceStatus::Stopped,
        InstanceStatus::Failed,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    for (i, a) in displays.iter().enumerate() {
        for (j, b) in displays.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}
