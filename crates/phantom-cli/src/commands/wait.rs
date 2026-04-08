use anyhow::Result;
use phantom_core::protocol::{Request, Response};
use phantom_core::types::WaitCondition;

use crate::daemon_ctl;

#[allow(clippy::too_many_arguments)]
pub async fn execute(
    session: String,
    text: Option<String>,
    regex: Option<String>,
    stable: bool,
    stable_duration: u64,
    cursor: Option<String>,
    cursor_visible: bool,
    cursor_hidden: bool,
    process_exit: bool,
    exit_code: Option<i32>,
    text_disappear: Option<String>,
    changed: bool,
    timeout: u64,
    poll: u64,
) -> Result<()> {
    let mut conditions = Vec::new();

    if let Some(t) = text {
        conditions.push(WaitCondition::TextPresent(t));
    }
    if let Some(t) = text_disappear {
        conditions.push(WaitCondition::TextAbsent(t));
    }
    if let Some(r) = regex {
        conditions.push(WaitCondition::Regex(::regex::Regex::new(&r)?));
    }
    if stable {
        conditions.push(WaitCondition::ScreenStable {
            duration_ms: stable_duration,
        });
    }
    if let Some(c) = cursor {
        let parts: Vec<&str> = c.split(',').collect();
        if parts.len() != 2 {
            anyhow::bail!("Cursor position must be x,y");
        }
        conditions.push(WaitCondition::CursorAt {
            x: parts[0].parse()?,
            y: parts[1].parse()?,
        });
    }
    if cursor_visible {
        conditions.push(WaitCondition::CursorVisible(true));
    }
    if cursor_hidden {
        conditions.push(WaitCondition::CursorVisible(false));
    }
    if process_exit {
        conditions.push(WaitCondition::ProcessExited { exit_code });
    }
    if changed {
        conditions.push(WaitCondition::ScreenChanged);
    }

    if conditions.is_empty() {
        anyhow::bail!("Specify at least one wait condition");
    }

    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::Wait {
            session,
            conditions,
            timeout_ms: timeout,
            poll_ms: poll,
        })
        .await?;

    match resp {
        Response::Ok { .. } => {}
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}
