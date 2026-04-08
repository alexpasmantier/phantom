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

/// Remove a stale socket file left behind by a crashed daemon.
/// Returns true if a stale socket was cleaned up.
fn cleanup_stale_socket(path: &PathBuf) -> bool {
    if !path.exists() {
        return false;
    }
    // Try a quick connect — if it fails, the socket is stale
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_) => false, // daemon is actually running
        Err(_) => {
            let _ = std::fs::remove_file(path);
            true
        }
    }
}

pub async fn ensure_daemon() -> Result<Connection> {
    let path = socket_path();

    // Try connecting first
    if let Ok(conn) = Connection::connect(&path).await {
        return Ok(conn);
    }

    // Clean up stale socket from a previous crashed daemon
    cleanup_stale_socket(&path);

    // Start the daemon
    let daemon_bin = std::env::current_exe()?
        .parent()
        .context("cannot determine executable directory")?
        .join("phantom-daemon");

    if !daemon_bin.exists() {
        bail!(
            "Daemon binary not found at {path}\n\
             \n\
             Make sure phantom-daemon is built and in the same directory as phantom:\n\
             \n\
                 cargo build --workspace",
            path = daemon_bin.display()
        );
    }

    // Create socket directory
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
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

    bail!(
        "Daemon failed to start within 5 seconds\n\
         \n\
         Try running it manually to see errors:\n\
         \n\
             phantom daemon start --foreground"
    )
}
