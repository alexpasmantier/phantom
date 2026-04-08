use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::SessionInfo;

use crate::daemon_ctl;
use crate::output::OutputMode;

#[allow(clippy::too_many_arguments)]
pub async fn execute(
    session: Option<String>,
    cols: u16,
    rows: u16,
    scrollback: u32,
    envs: Vec<String>,
    cwd: Option<String>,
    command: Vec<String>,
    output: OutputMode,
) -> Result<()> {
    let name = session.unwrap_or_else(|| format!("s{}", std::process::id()));
    let (cmd, args) = command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("No command specified"))?;
    let env: Vec<(String, String)> = envs
        .into_iter()
        .filter_map(|e| {
            let (k, v) = e.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect();

    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::CreateSession {
            name: name.clone(),
            command: cmd.to_string(),
            args: args.to_vec(),
            env,
            cwd,
            cols,
            rows,
            scrollback,
        })
        .await?;

    match resp {
        Response::Ok { data } => {
            if let Some(ResponseData::Session(info)) = data {
                print_session_info(&info, output);
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}

fn print_session_info(info: &SessionInfo, output: OutputMode) {
    if output.is_json() {
        println!("{}", serde_json::to_string(info).unwrap());
    } else {
        println!(
            "Session '{}' started (PID {}, {}x{})",
            info.name, info.pid, info.cols, info.rows
        );
    }
}
