use anyhow::{Result, bail};
use phantom_core::protocol::{Request, Response};
use phantom_core::types::InputAction;

use crate::daemon_ctl;

pub async fn execute(
    session: String,
    type_text: Option<String>,
    keys: Vec<String>,
    paste: Option<String>,
    mouse: Option<String>,
    delay: u64,
) -> Result<()> {
    let action = if let Some(text) = type_text {
        InputAction::Type {
            text,
            delay_ms: if delay > 0 { Some(delay) } else { None },
        }
    } else if !keys.is_empty() {
        InputAction::Key { keys }
    } else if let Some(text) = paste {
        InputAction::Paste { text }
    } else if let Some(spec) = mouse {
        InputAction::Mouse { spec }
    } else {
        bail!("Specify --type, --key, --paste, or --mouse");
    };

    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn.send(&Request::SendInput { session, action }).await?;

    match resp {
        Response::Ok { .. } => {}
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}
