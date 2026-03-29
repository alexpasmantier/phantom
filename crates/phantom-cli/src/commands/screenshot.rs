use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::{ScreenContent, ScreenFormat};

use crate::daemon_ctl;
use crate::output::OutputMode;

pub async fn execute(
    session: String,
    format: String,
    region: Option<String>,
    output: OutputMode,
) -> Result<()> {
    let fmt = match format.as_str() {
        "text" => ScreenFormat::Text,
        "json" => ScreenFormat::Json,
        "html" => ScreenFormat::Html,
        _ => anyhow::bail!("Invalid format: {format}. Use text, json, or html"),
    };

    let region = match region {
        Some(r) => {
            let parts: Vec<u16> = r
                .split(',')
                .map(|s| s.parse())
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if parts.len() != 4 {
                anyhow::bail!("Region must be top,left,bottom,right");
            }
            Some((parts[0], parts[1], parts[2], parts[3]))
        }
        None => None,
    };

    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::Screenshot {
            session,
            format: fmt.clone(),
            region,
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

fn print_screen(screen: &ScreenContent, format: &ScreenFormat, _output: OutputMode) {
    match format {
        ScreenFormat::Text => {
            for row in &screen.screen {
                println!("{}", row.text);
            }
        }
        ScreenFormat::Json | ScreenFormat::Html => {
            println!("{}", serde_json::to_string_pretty(screen).unwrap());
        }
    }
}
