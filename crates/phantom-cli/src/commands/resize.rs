use anyhow::Result;
use phantom_core::protocol::{Request, Response};

use crate::daemon_ctl;

pub async fn execute(session: String, cols: u16, rows: u16) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::Resize {
            session,
            cols,
            rows,
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
