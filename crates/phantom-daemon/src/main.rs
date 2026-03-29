mod handler;
mod listener;

use phantom_daemon::engine;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use mio::Waker;

#[derive(Parser)]
#[command(name = "phantom-daemon")]
struct Args {
    /// Socket path
    #[arg(long)]
    socket: Option<String>,

    /// Run in foreground (don't daemonize)
    #[arg(long)]
    foreground: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("phantom_daemon=info".parse().unwrap()),
        )
        .init();

    let socket_path = args
        .socket
        .map(PathBuf::from)
        .unwrap_or_else(default_socket_path);

    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();

    // We need to get the Waker from the engine, but the engine must be created
    // on the engine thread (because it contains !Send types).
    // Solution: create a oneshot channel to receive the Waker from the engine thread.
    let (waker_tx, waker_rx) = crossbeam_channel::bounded::<Arc<Waker>>(1);

    let engine_handle = std::thread::Builder::new()
        .name("phantom-engine".into())
        .spawn(move || {
            match engine::Engine::new(cmd_rx) {
                Ok((mut engine, waker)) => {
                    let _ = waker_tx.send(Arc::new(waker));
                    if let Err(e) = engine.run() {
                        tracing::error!("Engine error: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to create engine: {e}");
                }
            }
        })?;

    // Wait for the waker from the engine thread
    let waker = waker_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| anyhow::anyhow!("Engine thread failed to start"))?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let cmd_tx_signal = cmd_tx.clone();
        let waker_signal = Arc::clone(&waker);
        let socket_path_signal = socket_path.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("Received shutdown signal");
            let _ = cmd_tx_signal.send(engine::EngineCommand::Shutdown);
            let _ = waker_signal.wake();
            let _ = std::fs::remove_file(&socket_path_signal);
            std::process::exit(0);
        });

        if let Err(e) = listener::listen(&socket_path, cmd_tx, waker).await {
            tracing::error!("Listener error: {e}");
        }
    });

    let _ = engine_handle.join();
    let _ = std::fs::remove_file(&socket_path);

    Ok(())
}

fn default_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".phantom")
        });
    runtime_dir.join("phantom.sock")
}
