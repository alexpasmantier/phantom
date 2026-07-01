use std::io::{Write, stdout};

use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::{RowContent, ScreenFormat};

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
                format: ScreenFormat::Json,
                region: None,
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
                    render_row(&mut buf, row);
                    // Reset before clearing to EOL so any cell background color
                    // doesn't bleed across the cleared region.
                    buf.push_str("\x1b[0m\x1b[K");
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

/// Style state emitted as an SGR sequence. Cells are compared against the
/// previously emitted style so we only write escapes when something changes,
/// keeping runs of same-styled cells cheap.
#[derive(Default, PartialEq)]
struct Style {
    fg: Option<(u8, u8, u8)>,
    bg: Option<(u8, u8, u8)>,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    inverse: bool,
    faint: bool,
}

/// Render a single row's cells into `buf` as text plus truecolor SGR escapes.
/// Falls back to the plain `row.text` if per-cell data isn't present.
fn render_row(buf: &mut String, row: &RowContent) {
    if row.cells.is_empty() {
        buf.push_str(&row.text);
        return;
    }

    let mut cur = Style::default();
    for cell in &row.cells {
        let next = Style {
            fg: cell.fg.as_deref().and_then(parse_hex),
            bg: cell.bg.as_deref().and_then(parse_hex),
            bold: cell.bold,
            italic: cell.italic,
            underline: cell.underline,
            strikethrough: cell.strikethrough,
            inverse: cell.inverse,
            faint: cell.faint,
        };

        if next != cur {
            // Reset then re-emit the full style; simpler than tracking which
            // individual attributes were turned on or off.
            buf.push_str("\x1b[0m");
            if let Some((r, g, b)) = next.fg {
                buf.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
            }
            if let Some((r, g, b)) = next.bg {
                buf.push_str(&format!("\x1b[48;2;{r};{g};{b}m"));
            }
            if next.bold {
                buf.push_str("\x1b[1m");
            }
            if next.faint {
                buf.push_str("\x1b[2m");
            }
            if next.italic {
                buf.push_str("\x1b[3m");
            }
            if next.underline {
                buf.push_str("\x1b[4m");
            }
            if next.inverse {
                buf.push_str("\x1b[7m");
            }
            if next.strikethrough {
                buf.push_str("\x1b[9m");
            }
            cur = next;
        }

        // A cell with no grapheme (e.g. the trailing half of a wide char, or a
        // blank cell) still occupies a column.
        if cell.grapheme.is_empty() {
            buf.push(' ');
        } else {
            buf.push_str(&cell.grapheme);
        }
    }
}

/// Parse a "#rrggbb" hex color into an (r, g, b) triple.
fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}
