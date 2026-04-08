use std::sync::Arc;

use anyhow::Result;
use crossbeam_channel::Sender;
use mio::Waker;
use phantom_core::protocol::{Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::engine::EngineCommand;

pub async fn handle_connection(
    stream: UnixStream,
    cmd_tx: Sender<EngineCommand>,
    waker: Arc<Waker>,
) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = Response::error(1, format!("Invalid request: {e}"));
                let mut buf = serde_json::to_vec(&resp)?;
                buf.push(b'\n');
                reader.get_mut().write_all(&buf).await?;
                line.clear();
                continue;
            }
        };

        let response = dispatch_request(request, &cmd_tx, &waker).await?;

        let mut buf = serde_json::to_vec(&response)?;
        buf.push(b'\n');
        reader.get_mut().write_all(&buf).await?;
        reader.get_mut().flush().await?;
        line.clear();
    }

    Ok(())
}

async fn dispatch_request(
    request: Request,
    cmd_tx: &Sender<EngineCommand>,
    waker: &Waker,
) -> Result<Response> {
    let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);

    let cmd = match request {
        Request::CreateSession {
            name,
            command,
            args,
            env,
            cwd,
            cols,
            rows,
            scrollback,
        } => EngineCommand::CreateSession {
            name,
            command,
            args,
            env,
            cwd,
            cols,
            rows,
            scrollback,
            reply: reply_tx,
        },
        Request::SendInput { session, action } => EngineCommand::SendInput {
            session,
            action,
            reply: reply_tx,
        },
        Request::Screenshot {
            session,
            format,
            region,
        } => EngineCommand::Screenshot {
            session,
            format,
            region,
            reply: reply_tx,
        },
        Request::Wait {
            session,
            conditions,
            timeout_ms,
            poll_ms,
        } => EngineCommand::Wait {
            session,
            conditions,
            timeout_ms,
            poll_ms,
            reply: reply_tx,
        },
        Request::GetCursor { session } => EngineCommand::GetCursor {
            session,
            reply: reply_tx,
        },
        Request::GetScrollback { session, lines, .. } => EngineCommand::GetScrollback {
            session,
            lines,
            reply: reply_tx,
        },
        Request::Resize {
            session,
            cols,
            rows,
        } => EngineCommand::Resize {
            session,
            cols,
            rows,
            reply: reply_tx,
        },
        Request::GetStatus { session } => EngineCommand::GetStatus {
            session,
            reply: reply_tx,
        },
        Request::ListSessions => EngineCommand::ListSessions { reply: reply_tx },
        Request::GetOutput { session } => EngineCommand::GetOutput {
            session,
            reply: reply_tx,
        },
        Request::GetCell { session, x, y } => EngineCommand::GetCell {
            session,
            x,
            y,
            reply: reply_tx,
        },
        Request::KillSession { session, signal } => EngineCommand::KillSession {
            session,
            signal,
            reply: reply_tx,
        },
        Request::Shutdown => {
            let _ = cmd_tx.send(EngineCommand::Shutdown);
            let _ = waker.wake();
            return Ok(Response::ok());
        }
    };

    cmd_tx.send(cmd)?;
    waker.wake()?;

    let response = tokio::task::spawn_blocking(move || {
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap_or_else(|_| Response::error(1, "Engine timeout"))
    })
    .await?;

    Ok(response)
}
