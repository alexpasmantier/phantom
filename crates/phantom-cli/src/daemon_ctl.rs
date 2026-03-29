use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};

use crate::connection::Connection;

static CUSTOM_SOCKET_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn set_socket_path(path: &str) {
    let _ = CUSTOM_SOCKET_PATH.set(PathBuf::from(path));
}

pub fn socket_path() -> PathBuf {
    if let Some(p) = CUSTOM_SOCKET_PATH.get() {
        return p.clone();
    }
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = dirs_home().unwrap_or_else(|| PathBuf::from("/tmp"));
            p.push(".phantom");
            p
        });
    runtime_dir.join("phantom.sock")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

pub async fn ensure_daemon() -> Result<Connection> {
    let path = socket_path();

    // Try connecting first
    if let Ok(conn) = Connection::connect(&path).await {
        return Ok(conn);
    }

    // Start the daemon
    let daemon_bin = std::env::current_exe()?
        .parent()
        .context("no parent dir")?
        .join("phantom-daemon");

    if !daemon_bin.exists() {
        bail!(
            "Daemon binary not found at {}. Build phantom-daemon first.",
            daemon_bin.display()
        );
    }

    // Create socket directory
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    Command::new(&daemon_bin)
        .arg("--socket")
        .arg(&path)
        .spawn()
        .with_context(|| format!("Failed to start daemon: {}", daemon_bin.display()))?;

    // Wait for daemon to be ready
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Ok(conn) = Connection::connect(&path).await {
            return Ok(conn);
        }
    }

    bail!("Daemon failed to start within 5 seconds")
}
