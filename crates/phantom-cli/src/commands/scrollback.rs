use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::ScreenFormat;

use crate::daemon_ctl;
use crate::output::OutputMode;

pub async fn execute(
    session: String,
    lines: Option<u32>,
    format: String,
    output: OutputMode,
) -> Result<()> {
    let fmt = match format.as_str() {
        "text" => ScreenFormat::Text,
        "json" => ScreenFormat::Json,
        _ => anyhow::bail!("Invalid format: {format}. Use text or json"),
    };

    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::GetScrollback {
            session,
            lines,
            format: fmt,
        })
        .await?;

    match resp {
        Response::Ok { data } => match data {
            Some(ResponseData::Text(text)) => println!("{text}"),
            Some(ResponseData::Screen(screen)) => {
                if output.is_json() {
                    println!("{}", serde_json::to_string_pretty(&screen).unwrap());
                } else {
                    for row in &screen.screen {
                        println!("{}", row.text);
                    }
                }
            }
            _ => {}
        },
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}
