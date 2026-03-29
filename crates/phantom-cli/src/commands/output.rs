use anyhow::Result;
use phantom_core::protocol::{Request, Response, ResponseData};

use crate::daemon_ctl;

pub async fn execute(session: String) -> Result<()> {
    let mut conn = daemon_ctl::ensure_daemon().await?;
    let resp = conn
        .send(&Request::GetOutput { session })
        .await?;

    match resp {
        Response::Ok { data } => {
            if let Some(ResponseData::Text(text)) = data {
                print!("{text}");
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error: {message}");
            std::process::exit(code);
        }
    }
    Ok(())
}
