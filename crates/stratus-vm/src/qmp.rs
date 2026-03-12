use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufStream};
use tokio::net::UnixStream;

use crate::VmError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmStatus {
    Running,
    Paused,
    Shutdown,
    Suspended,
    Unknown,
}

impl std::fmt::Display for VmStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmStatus::Running => write!(f, "running"),
            VmStatus::Paused => write!(f, "paused"),
            VmStatus::Shutdown => write!(f, "shutdown"),
            VmStatus::Suspended => write!(f, "suspended"),
            VmStatus::Unknown => write!(f, "unknown"),
        }
    }
}

pub struct QmpClient {
    stream: BufStream<UnixStream>,
}

impl QmpClient {
    /// Connect to a QMP socket, read the greeting, and negotiate capabilities.
    pub async fn connect(path: &Path) -> Result<Self, VmError> {
        let sock = tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(path))
            .await
            .map_err(|_| VmError::Qmp("connection timed out".into()))?
            .map_err(|e| VmError::Qmp(format!("connect failed: {e}")))?;

        let mut client = QmpClient {
            stream: BufStream::new(sock),
        };

        // Read greeting
        let _greeting = client.read_line().await?;

        // Send qmp_capabilities
        client
            .send_command(r#"{"execute": "qmp_capabilities"}"#)
            .await?;

        // Read response
        let _resp = client.read_line().await?;

        Ok(client)
    }

    /// Query VM status.
    pub async fn query_status(&mut self) -> Result<VmStatus, VmError> {
        self.send_command(r#"{"execute": "query-status"}"#).await?;
        let resp = self.read_line().await?;

        let val: serde_json::Value =
            serde_json::from_str(&resp).map_err(|e| VmError::Qmp(format!("invalid JSON: {e}")))?;

        let status_str = val["return"]["status"].as_str().unwrap_or("unknown");

        Ok(match status_str {
            "running" => VmStatus::Running,
            "paused" => VmStatus::Paused,
            "shutdown" => VmStatus::Shutdown,
            "suspended" => VmStatus::Suspended,
            _ => VmStatus::Unknown,
        })
    }

    /// Request a clean guest shutdown (ACPI power button).
    pub async fn system_powerdown(&mut self) -> Result<(), VmError> {
        self.send_command(r#"{"execute": "system_powerdown"}"#)
            .await?;
        let _resp = self.read_line().await?;
        Ok(())
    }

    /// Reset the guest.
    pub async fn system_reset(&mut self) -> Result<(), VmError> {
        self.send_command(r#"{"execute": "system_reset"}"#).await?;
        let _resp = self.read_line().await?;
        Ok(())
    }

    /// Quit QEMU immediately.
    pub async fn quit(&mut self) -> Result<(), VmError> {
        self.send_command(r#"{"execute": "quit"}"#).await?;
        // QEMU may close the connection immediately, so ignore read errors
        let _ = self.read_line().await;
        Ok(())
    }

    async fn send_command(&mut self, cmd: &str) -> Result<(), VmError> {
        self.stream
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| VmError::Qmp(format!("write failed: {e}")))?;
        self.stream
            .write_all(b"\n")
            .await
            .map_err(|e| VmError::Qmp(format!("write failed: {e}")))?;
        self.stream
            .flush()
            .await
            .map_err(|e| VmError::Qmp(format!("flush failed: {e}")))?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<String, VmError> {
        let mut line = String::new();
        let result =
            tokio::time::timeout(Duration::from_secs(5), self.stream.read_line(&mut line)).await;

        match result {
            Ok(Ok(0)) => Err(VmError::Qmp("connection closed".into())),
            Ok(Ok(_)) => Ok(line),
            Ok(Err(e)) => Err(VmError::Qmp(format!("read failed: {e}"))),
            Err(_) => Err(VmError::Qmp("read timed out".into())),
        }
    }
}
