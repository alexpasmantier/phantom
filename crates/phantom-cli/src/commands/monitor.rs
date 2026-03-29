use std::io::{Write, stdout};

use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::ScreenFormat;

use crate::daemon_ctl;

pub async fn execute(session: String, rate: u64) -> Result<()> {
    let interval = std::time::Duration::from_millis(1000 / rate.max(1));
    let mut stdout = stdout();

    // Enter alternate screen and hide cursor
    print!("\x1b[?1049h\x1b[?25l");
    stdout.flush()?;

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })?;

    let result = run_loop(&session, interval, &running).await;

    // Always restore terminal, no matter what happened
    print!("\x1b[?25h\x1b[?1049l");
    stdout.flush()?;

    match result {
        Ok(msg) => {
            if let Some(msg) = msg {
                eprintln!("{msg}");
            }
            Ok(())
        }
        Err(e) => {
            // Don't propagate connection errors as ugly stack traces
            eprintln!("Monitor stopped: {e}");
            Ok(())
        }
    }
}

async fn run_loop(
    session: &str,
    interval: std::time::Duration,
    running: &std::sync::atomic::AtomicBool,
) -> Result<Option<String>, anyhow::Error> {
    let mut stdout = stdout();
    let mut conn = daemon_ctl::ensure_daemon().await?;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        let resp = match conn
            .send(&Request::Screenshot {
                session: session.to_string(),
                format: ScreenFormat::Text,
            })
            .await
        {
            Ok(resp) => resp,
            Err(_) => {
                // Connection lost (daemon died, pipe broken, etc.)
                return Ok(Some("Session ended (connection lost)".into()));
            }
        };

        match resp {
            Response::Ok {
                data: Some(ResponseData::Screen(screen)),
            } => {
                let mut buf = String::new();
                buf.push_str("\x1b[H");
                for (i, row) in screen.screen.iter().enumerate() {
                    if i > 0 {
                        buf.push_str("\r\n");
                    }
                    buf.push_str(&row.text);
                    buf.push_str("\x1b[K");
                }
                buf.push_str("\x1b[J");
                print!("{buf}");
                stdout.flush()?;
            }
            Response::Error { code, message } => {
                // Session not found or exited — check if process exited
                if code == phantom_core::exit_codes::SESSION_NOT_FOUND {
                    return Ok(Some(format!("Session '{session}' not found")));
                }
                return Ok(Some(message));
            }
            _ => {}
        }

        tokio::time::sleep(interval).await;
    }

    Ok(None)
}
