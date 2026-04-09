//! Observer socket: a Unix socket that exposes the embedded engine via the
//! same wire protocol as `phantom-daemon`. This lets external processes
//! (e.g. `phantom monitor`) connect and watch sessions live.
//!
//! The path defaults to `$XDG_RUNTIME_DIR/phantom-mcp-$PID.sock`, falling back
//! to `~/.phantom/phantom-mcp-$PID.sock`. Set `PHANTOM_MCP_SOCKET` to override
//! the path entirely.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use mio::Waker;
use phantom_daemon::engine::EngineCommand;
use phantom_daemon::handler;
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

/// Default observer socket path: `$XDG_RUNTIME_DIR/phantom-mcp-$PID.sock`
/// or `$HOME/.phantom/phantom-mcp-$PID.sock` when `XDG_RUNTIME_DIR` is unset.
pub fn default_socket_path() -> PathBuf {
    let pid = std::process::id();
    let dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".phantom")
        });
    dir.join(format!("phantom-mcp-{pid}.sock"))
}

/// Resolve the observer socket path, honoring the `PHANTOM_MCP_SOCKET`
/// environment variable as an explicit override. Falls back to
/// [`default_socket_path`] when the env var is unset or empty.
pub fn resolve_socket_path() -> PathBuf {
    match std::env::var("PHANTOM_MCP_SOCKET") {
        Ok(s) if !s.is_empty() => PathBuf::from(s),
        _ => default_socket_path(),
    }
}

/// Bind a Unix socket and spawn an accept loop that dispatches each
/// connection to `phantom_daemon::handler::handle_connection`.
///
/// Returns the spawned task handle. The bind happens synchronously inside
/// this function so callers can be sure the socket file exists by the time
/// `serve` returns.
pub async fn serve(
    socket_path: &Path,
    cmd_tx: Sender<EngineCommand>,
    waker: Arc<Waker>,
) -> Result<JoinHandle<()>> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating socket directory {}", parent.display()))?;
    }
    // Stale socket cleanup — same logic the daemon uses.
    let _ = std::fs::remove_file(socket_path);

    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("binding observer socket at {}", socket_path.display()))?;

    tracing::info!("observer socket listening on {}", socket_path.display());

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let cmd_tx = cmd_tx.clone();
                    let waker = Arc::clone(&waker);
                    tokio::spawn(async move {
                        if let Err(e) = handler::handle_connection(stream, cmd_tx, waker).await {
                            tracing::warn!("observer connection error: {e}");
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("observer accept error: {e}");
                    // Brief backoff before retrying so a runaway error loop
                    // doesn't pin a CPU.
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Save+restore an env var around a closure so tests don't pollute each
    /// other. Tests in this module run sequentially because they all touch
    /// `PHANTOM_MCP_SOCKET`; cargo test serializes within a single test
    /// binary by default for `#[test]` functions in the same module only via
    /// the test runner — to be safe we use a mutex.
    fn with_env_var<F: FnOnce()>(key: &str, value: Option<&str>, f: F) {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        let _g = LOCK.lock().unwrap();

        let prev = std::env::var(key).ok();
        // SAFETY: env mutation is guarded by the mutex above; the test runner
        // may still run other tests in parallel that touch unrelated env vars,
        // but this key is local to these tests.
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn resolve_uses_env_override_when_set() {
        with_env_var(
            "PHANTOM_MCP_SOCKET",
            Some("/tmp/custom-phantom.sock"),
            || {
                assert_eq!(
                    resolve_socket_path(),
                    PathBuf::from("/tmp/custom-phantom.sock")
                );
            },
        );
    }

    #[test]
    fn resolve_falls_back_when_env_empty() {
        with_env_var("PHANTOM_MCP_SOCKET", Some(""), || {
            assert_eq!(resolve_socket_path(), default_socket_path());
        });
    }

    #[test]
    fn resolve_falls_back_when_env_unset() {
        with_env_var("PHANTOM_MCP_SOCKET", None, || {
            assert_eq!(resolve_socket_path(), default_socket_path());
        });
    }
}
