use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::{SessionInfo, SessionStatus};

use crate::daemon_ctl;
use crate::output::OutputMode;

pub async fn execute(output: OutputMode) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn.send(&Request::ListSessions).await?;

    match resp {
        Response::Ok { data } => {
            if let Some(ResponseData::Sessions(sessions)) = data {
                print_sessions(&sessions, output);
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}

fn print_sessions(sessions: &[SessionInfo], output: OutputMode) {
    if output.is_json() {
        println!("{}", serde_json::to_string(sessions).unwrap());
    } else if sessions.is_empty() {
        println!("No active sessions");
    } else {
        for s in sessions {
            let status = match &s.status {
                SessionStatus::Running => "running",
                SessionStatus::Exited { .. } => "exited",
            };
            println!(
                "  {} (PID {}, {}x{}, {})",
                s.name, s.pid, s.cols, s.rows, status
            );
        }
    }
}
