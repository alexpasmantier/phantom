use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};
use phantom_core::types::CursorInfo;

use crate::daemon_ctl;
use crate::output::OutputMode;

pub async fn execute(session: String, output: OutputMode) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn.send(&Request::GetCursor { session }).await?;

    match resp {
        Response::Ok { data } => {
            if let Some(ResponseData::Cursor(cursor)) = data {
                print_cursor(&cursor, output);
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}

fn print_cursor(cursor: &CursorInfo, output: OutputMode) {
    if output.is_json() {
        println!("{}", serde_json::to_string(cursor).unwrap());
    } else {
        println!(
            "cursor: ({}, {}) {} {:?}",
            cursor.x,
            cursor.y,
            if cursor.visible { "visible" } else { "hidden" },
            cursor.style
        );
    }
}
