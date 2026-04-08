use anyhow::Result;
use phantom_core::protocol::{Request, Response};

use crate::daemon_ctl;

pub async fn execute(session: String, signal: Option<i32>) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn.send(&Request::KillSession { session, signal }).await?;

    match resp {
        Response::Ok { .. } => {}
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}
