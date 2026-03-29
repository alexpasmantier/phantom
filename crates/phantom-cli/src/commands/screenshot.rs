use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::{ScreenContent, ScreenFormat};

use crate::daemon_ctl;
use crate::output::OutputMode;

pub async fn execute(session: String, format: String, output: OutputMode) -> Result<()> {
    let fmt = match format.as_str() {
        "text" => ScreenFormat::Text,
        "json" => ScreenFormat::Json,
        "html" => ScreenFormat::Html,
        _ => anyhow::bail!("Invalid format: {format}. Use text, json, or html"),
    };

    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::Screenshot {
            session,
            format: fmt.clone(),
        })
        .await?;

    match resp {
        Response::Ok { data } => match data {
            Some(ResponseData::Screen(screen)) => print_screen(&screen, &fmt, output),
            Some(ResponseData::Text(text)) => println!("{text}"),
            _ => {}
        },
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}

fn print_screen(screen: &ScreenContent, format: &ScreenFormat, output: OutputMode) {
    match format {
        ScreenFormat::Text => {
            for row in &screen.screen {
                println!("{}", row.text);
            }
        }
        ScreenFormat::Json | ScreenFormat::Html => {
            if output.is_json() {
                println!("{}", serde_json::to_string_pretty(screen).unwrap());
            } else {
                println!("{}", serde_json::to_string_pretty(screen).unwrap());
            }
        }
    }
}
