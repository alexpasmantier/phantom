//! phantom-mcp — MCP server exposing phantom for AI agents over stdio.

use anyhow::Result;
use phantom_mcp::{observer, server::PhantomMcpServer};
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Logs go to stderr — stdout is reserved for MCP protocol traffic.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("phantom_mcp=info,phantom_daemon=warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("starting phantom-mcp");

    // Build the server (this spawns the engine thread).
    let server = PhantomMcpServer::new()?;

    // Bind the observer socket and spawn its accept loop. We do this before
    // serving stdio so the socket file is guaranteed to exist by the time the
    // first tool call lands. The path can be overridden via PHANTOM_MCP_SOCKET.
    let socket_path = observer::resolve_socket_path();
    let (cmd_tx, waker) = server.engine_handle();
    let _observer_handle = observer::serve(&socket_path, cmd_tx, waker).await?;
    let server = server.with_observer_socket(socket_path.clone());

    // Make sure we tear the socket file down on shutdown — both via Ctrl-C and
    // via normal stdio EOF.
    let cleanup_path = socket_path.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = std::fs::remove_file(&cleanup_path);
        std::process::exit(0);
    });

    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serving error: {e}");
    })?;

    let result = service.waiting().await;

    // Best-effort cleanup of the observer socket file.
    let _ = std::fs::remove_file(&socket_path);

    result?;
    Ok(())
}
