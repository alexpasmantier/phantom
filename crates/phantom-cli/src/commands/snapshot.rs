use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::ScreenFormat;

use crate::daemon_ctl;

/// Save a screenshot to a file.
pub async fn save(session: String, file: String) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::Screenshot {
            session,
            format: ScreenFormat::Text,
            region: None,
        })
        .await?;

    match resp {
        Response::Ok {
            data: Some(ResponseData::Screen(screen)),
        } => {
            let text: String = screen
                .screen
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&file, &text)?;
            eprintln!("Saved snapshot to {file}");
        }
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
        _ => {}
    }
    Ok(())
}

/// Compare current screen against a saved snapshot.
pub async fn diff(session: String, file: String) -> Result<()> {
    let reference = std::fs::read_to_string(&file)?;

    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::Screenshot {
            session,
            format: ScreenFormat::Text,
            region: None,
        })
        .await?;

    match resp {
        Response::Ok {
            data: Some(ResponseData::Screen(screen)),
        } => {
            let current: String = screen
                .screen
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");

            if current == reference {
                println!("No differences");
            } else {
                // Simple line-by-line diff
                let ref_lines: Vec<&str> = reference.lines().collect();
                let cur_lines: Vec<&str> = current.lines().collect();
                let max = ref_lines.len().max(cur_lines.len());
                let mut diffs = 0;

                for i in 0..max {
                    let r = ref_lines.get(i).unwrap_or(&"");
                    let c = cur_lines.get(i).unwrap_or(&"");
                    if r != c {
                        println!("Line {i}:");
                        println!("  - {r}");
                        println!("  + {c}");
                        diffs += 1;
                    }
                }
                println!("\n{diffs} line(s) differ");
                std::process::exit(1);
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
        _ => {}
    }
    Ok(())
}
