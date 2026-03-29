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

    // Set up ctrl-c handler to restore terminal
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })?;

    let mut conn = daemon_ctl::ensure_daemon().await?;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        let resp = conn
            .send(&Request::Screenshot {
                session: session.clone(),
                format: ScreenFormat::Text,
            })
            .await?;

        match resp {
            Response::Ok {
                data: Some(ResponseData::Screen(screen)),
            } => {
                // Move cursor to top-left and draw each row
                let mut buf = String::new();
                buf.push_str("\x1b[H"); // cursor home
                for (i, row) in screen.screen.iter().enumerate() {
                    if i > 0 {
                        buf.push_str("\r\n");
                    }
                    buf.push_str(&row.text);
                    buf.push_str("\x1b[K"); // clear to end of line
                }
                // Clear any remaining lines below
                buf.push_str("\x1b[J");
                print!("{buf}");
                stdout.flush()?;
            }
            Response::Error { message, .. } => {
                // Session gone — show message and exit
                print!("\x1b[H\x1b[2J{message}");
                stdout.flush()?;
                break;
            }
            _ => {}
        }

        tokio::time::sleep(interval).await;
    }

    // Restore terminal
    print!("\x1b[?25h\x1b[?1049l");
    stdout.flush()?;

    Ok(())
}
