use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use crossbeam_channel::Sender;
use mio::Waker;
use tokio::net::UnixListener;

use crate::engine::EngineCommand;
use crate::handler;

pub async fn listen(
    socket_path: &Path,
    cmd_tx: Sender<EngineCommand>,
    waker: Arc<Waker>,
) -> Result<()> {
    // Remove existing socket file
    let _ = std::fs::remove_file(socket_path);

    // Create parent directory
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    tracing::info!("Listening on {}", socket_path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let cmd_tx = cmd_tx.clone();
                let waker = Arc::clone(&waker);
                tokio::spawn(async move {
                    if let Err(e) = handler::handle_connection(stream, cmd_tx, waker).await {
                        tracing::warn!("Connection error: {e}");
                    }
                });
            }
            Err(e) => {
                tracing::error!("Accept error: {e}");
            }
        }
    }
}
