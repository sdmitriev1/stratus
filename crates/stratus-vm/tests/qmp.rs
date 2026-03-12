use std::path::PathBuf;

use stratus_vm::qmp::{QmpClient, VmStatus};

// --- VmStatus display ---

#[test]
fn status_display_running() {
    assert_eq!(VmStatus::Running.to_string(), "running");
}

#[test]
fn status_display_paused() {
    assert_eq!(VmStatus::Paused.to_string(), "paused");
}

#[test]
fn status_display_shutdown() {
    assert_eq!(VmStatus::Shutdown.to_string(), "shutdown");
}

#[test]
fn status_display_suspended() {
    assert_eq!(VmStatus::Suspended.to_string(), "suspended");
}

#[test]
fn status_display_unknown() {
    assert_eq!(VmStatus::Unknown.to_string(), "unknown");
}

// --- VmStatus equality ---

#[test]
fn status_eq() {
    assert_eq!(VmStatus::Running, VmStatus::Running);
    assert_ne!(VmStatus::Running, VmStatus::Shutdown);
}

#[test]
fn status_copy() {
    let s = VmStatus::Running;
    let s2 = s; // copy
    assert_eq!(s, s2);
}

// --- QMP JSON parsing ---

#[test]
fn parse_query_status_running() {
    let json = r#"{"return": {"running": true, "status": "running"}}"#;
    let val: serde_json::Value = serde_json::from_str(json).unwrap();
    let status_str = val["return"]["status"].as_str().unwrap();
    assert_eq!(status_str, "running");
}

#[test]
fn parse_query_status_paused() {
    let json = r#"{"return": {"running": false, "status": "paused"}}"#;
    let val: serde_json::Value = serde_json::from_str(json).unwrap();
    let status_str = val["return"]["status"].as_str().unwrap();
    assert_eq!(status_str, "paused");
}

#[test]
fn parse_query_status_shutdown() {
    let json = r#"{"return": {"running": false, "status": "shutdown"}}"#;
    let val: serde_json::Value = serde_json::from_str(json).unwrap();
    let status_str = val["return"]["status"].as_str().unwrap();
    assert_eq!(status_str, "shutdown");
}

#[test]
fn parse_query_status_missing_field() {
    let json = r#"{"return": {}}"#;
    let val: serde_json::Value = serde_json::from_str(json).unwrap();
    let status_str = val["return"]["status"].as_str();
    assert_eq!(status_str, None);
}

#[test]
fn parse_qmp_greeting() {
    let greeting = r#"{"QMP": {"version": {"qemu": {"micro": 0, "minor": 2, "major": 8}, "package": ""}, "capabilities": ["oob"]}}"#;
    let val: serde_json::Value = serde_json::from_str(greeting).unwrap();
    assert!(val["QMP"]["version"]["qemu"]["major"].as_u64().is_some());
}

#[test]
fn parse_qmp_success_response() {
    let resp = r#"{"return": {}}"#;
    let val: serde_json::Value = serde_json::from_str(resp).unwrap();
    assert!(val["return"].is_object());
}

#[test]
fn parse_qmp_error_response() {
    let resp = r#"{"error": {"class": "GenericError", "desc": "command not found"}}"#;
    let val: serde_json::Value = serde_json::from_str(resp).unwrap();
    assert!(val["error"].is_object());
    assert_eq!(val["error"]["desc"].as_str().unwrap(), "command not found");
}

#[test]
fn parse_qmp_event() {
    let event = r#"{"event": "SHUTDOWN", "data": {"guest": true, "reason": "guest-shutdown"}, "timestamp": {"seconds": 1234, "microseconds": 0}}"#;
    let val: serde_json::Value = serde_json::from_str(event).unwrap();
    assert_eq!(val["event"].as_str().unwrap(), "SHUTDOWN");
}

// --- Connection errors ---

#[tokio::test]
async fn connect_nonexistent_socket_fails() {
    let path = PathBuf::from("/tmp/stratus-test-nonexistent-qmp.sock");
    let result = QmpClient::connect(&path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn connect_not_a_socket_fails() {
    let dir = tempfile::tempdir().unwrap();
    let not_socket = dir.path().join("regular-file");
    std::fs::write(&not_socket, "not a socket").unwrap();
    let result = QmpClient::connect(&not_socket).await;
    assert!(result.is_err());
}
