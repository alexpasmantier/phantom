use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};

use crate::daemon_ctl;
use crate::output::OutputMode;

pub async fn execute(session: String, x: u16, y: u16, output: OutputMode) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn.send(&Request::GetCell { session, x, y }).await?;

    match resp {
        Response::Ok { data } => {
            if let Some(ResponseData::Cell(cell)) = data {
                if output.is_json() {
                    println!("{}", serde_json::to_string(&cell).unwrap());
                } else {
                    let mut attrs = Vec::new();
                    if cell.bold {
                        attrs.push("bold");
                    }
                    if cell.italic {
                        attrs.push("italic");
                    }
                    if cell.underline {
                        attrs.push("underline");
                    }
                    if cell.strikethrough {
                        attrs.push("strikethrough");
                    }
                    if cell.inverse {
                        attrs.push("inverse");
                    }
                    if cell.faint {
                        attrs.push("faint");
                    }
                    println!("({x}, {y}): {:?}", cell.grapheme);
                    if let Some(fg) = &cell.fg {
                        println!("  fg: {fg}");
                    }
                    if let Some(bg) = &cell.bg {
                        println!("  bg: {bg}");
                    }
                    if !attrs.is_empty() {
                        println!("  attrs: {}", attrs.join(", "));
                    }
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
