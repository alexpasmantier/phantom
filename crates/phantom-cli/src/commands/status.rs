use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::{SessionInfo, SessionStatus};

use crate::daemon_ctl;
use crate::output::OutputMode;

pub async fn execute(session: String, output: OutputMode) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn.send(&Request::GetStatus { session }).await?;

    match resp {
        Response::Ok { data } => {
            if let Some(ResponseData::Session(info)) = data {
                print_status(&info, output);
                if matches!(info.status, SessionStatus::Exited { .. }) {
                    std::process::exit(phantom_core::exit_codes::PROCESS_EXITED);
                }
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}

fn print_status(info: &SessionInfo, output: OutputMode) {
    if output.is_json() {
        println!("{}", serde_json::to_string(info).unwrap());
    } else {
        let status_str = match &info.status {
            SessionStatus::Running => "running".to_string(),
            SessionStatus::Exited { code } => match code {
                Some(c) => format!("exited (code {c})"),
                None => "exited".to_string(),
            },
        };
        println!(
            "{}: {} (PID {}, {}x{})",
            info.name, status_str, info.pid, info.cols, info.rows
        );
        if let Some(title) = &info.title {
            println!("  title: {title}");
        }
        if let Some(pwd) = &info.pwd {
            println!("  pwd: {pwd}");
        }
    }
}
