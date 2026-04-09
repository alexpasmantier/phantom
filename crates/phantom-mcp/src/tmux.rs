//! Spawn a live `phantom monitor` viewer in a tmux pane when running inside
//! tmux. Used by the `phantom_show` MCP tool to give the human user an
//! in-terminal live view of a session without leaving Claude Code.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Returns true if the process is running inside a tmux session.
pub fn in_tmux() -> bool {
    std::env::var_os("TMUX").is_some()
}

/// Resolve the path to the `phantom` CLI binary. We look for it as a sibling
/// of the current executable first (the build/install layout where
/// `phantom-mcp` and `phantom` ship together), and fall back to `$PATH`.
pub fn locate_phantom_cli() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join("phantom");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    // Fall back to bare `phantom` and let the OS resolve it from PATH.
    Ok(PathBuf::from("phantom"))
}

/// How to display the live viewer inside tmux.
#[derive(Debug, Clone, Copy)]
pub enum SplitMode {
    /// `tmux split-window -h` — vertical bar, side-by-side panes.
    Horizontal,
    /// `tmux split-window -v` — horizontal bar, stacked panes.
    Vertical,
    /// `tmux display-popup -E` — modal floating popup.
    Popup,
}

impl SplitMode {
    /// Pick a default mode from the `PHANTOM_MCP_SPLIT` env var.
    /// Recognized values: `horizontal` (default), `vertical`, `popup`.
    pub fn from_env() -> Self {
        match std::env::var("PHANTOM_MCP_SPLIT")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "vertical" | "v" => Self::Vertical,
            "popup" | "p" => Self::Popup,
            _ => Self::Horizontal,
        }
    }
}

/// Spawn a live viewer for `session_name` against the given observer socket.
///
/// On success returns the human-friendly description of what was opened.
/// Returns an error if not running in tmux, if `tmux` is not on PATH, or if
/// the spawn itself fails.
pub fn open_viewer(session_name: &str, socket_path: &Path, mode: SplitMode) -> Result<String> {
    if !in_tmux() {
        bail!("not running inside tmux (set $TMUX or use phantom_screenshot for in-chat viewing)");
    }

    let phantom_cli = locate_phantom_cli()?;

    // Quote the inner command for tmux's shell parser. The session name and
    // socket path are arguments we control, but we still shell-escape them in
    // case a user picks an exotic session name.
    let inner = format!(
        "{} monitor -s {} --socket {}",
        shell_escape(&phantom_cli.to_string_lossy()),
        shell_escape(session_name),
        shell_escape(&socket_path.to_string_lossy()),
    );

    // tmux sets $TMUX_PANE for child processes — it identifies the pane that
    // the calling process lives in (Claude Code's pane, in our case). Using
    // it as the split target guarantees the new pane is created next to the
    // chat, not next to some unrelated pane the user last touched.
    let target = std::env::var("TMUX_PANE").ok();

    let mut cmd = Command::new("tmux");
    let title = format!("phantom: {session_name}");
    let description = match mode {
        SplitMode::Horizontal => {
            cmd.arg("split-window").arg("-h").arg("-d");
            if let Some(ref t) = target {
                cmd.arg("-t").arg(t);
            }
            cmd.arg(&inner);
            "horizontal split pane"
        }
        SplitMode::Vertical => {
            cmd.arg("split-window").arg("-v").arg("-d");
            if let Some(ref t) = target {
                cmd.arg("-t").arg(t);
            }
            cmd.arg(&inner);
            "vertical split pane"
        }
        SplitMode::Popup => {
            cmd.arg("display-popup")
                .arg("-E")
                .arg("-w")
                .arg("90%")
                .arg("-h")
                .arg("90%")
                .arg("-T")
                .arg(&title);
            if let Some(ref t) = target {
                cmd.arg("-t").arg(t);
            }
            cmd.arg(&inner);
            "popup"
        }
    };

    let status = cmd
        .status()
        .with_context(|| "failed to spawn `tmux` — is tmux installed and on PATH?")?;
    if !status.success() {
        bail!("tmux exited with status {status}");
    }

    Ok(format!("opened {description} for session '{session_name}'"))
}

/// Minimal shell escaping for the inner tmux command. Wraps in single quotes
/// and escapes embedded single quotes via `'\''`.
fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | '=' | ':'))
    {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_escape_simple() {
        assert_eq!(shell_escape("simple"), "simple");
        assert_eq!(shell_escape("/usr/bin/phantom"), "/usr/bin/phantom");
        assert_eq!(shell_escape("session-1"), "session-1");
    }

    #[test]
    fn shell_escape_special_chars() {
        assert_eq!(shell_escape("with space"), "'with space'");
        assert_eq!(shell_escape("a$b"), "'a$b'");
        assert_eq!(shell_escape("a'b"), "'a'\\''b'");
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn split_mode_from_env_default() {
        // Just smoke-test the parser; we don't pollute the real env.
        assert!(matches!(SplitMode::from_env(), SplitMode::Horizontal));
    }
}
