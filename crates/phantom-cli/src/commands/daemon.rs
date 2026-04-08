use anyhow::Result;

use crate::daemon_ctl;

pub async fn start(foreground: bool, socket: Option<String>) -> Result<()> {
    if foreground {
        let daemon_bin = std::env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("cannot determine executable directory"))?
            .join("phantom-daemon");

        let mut cmd = tokio::process::Command::new(&daemon_bin);
        cmd.arg("--foreground");
        if let Some(s) = socket {
            cmd.arg("--socket").arg(s);
        }
        let status = cmd.status().await?;
        std::process::exit(status.code().unwrap_or(1));
    } else {
        let _ = daemon_ctl::ensure_daemon().await?;
        let path = daemon_ctl::socket_path();
        println!("Daemon started (socket: {})", path.display());
        Ok(())
    }
}

pub async fn stop() -> Result<()> {
    let path = daemon_ctl::socket_path();
    match crate::connection::Connection::connect(&path).await {
        Ok(mut conn) => {
            let _ = conn.send(&phantom_core::protocol::Request::Shutdown).await;
            let _ = std::fs::remove_file(&path);
            println!("Daemon stopped");
        }
        Err(_) => {
            // Clean up stale socket if present
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
            println!("Daemon is not running");
        }
    }
    Ok(())
}

pub async fn status() -> Result<()> {
    let path = daemon_ctl::socket_path();
    match crate::connection::Connection::connect(&path).await {
        Ok(mut conn) => {
            // Try listing sessions to confirm the daemon is responsive
            match conn
                .send(&phantom_core::protocol::Request::ListSessions)
                .await
            {
                Ok(phantom_core::protocol::Response::Ok { data }) => {
                    let session_count = match &data {
                        Some(phantom_core::protocol::ResponseData::Sessions(s)) => s.len(),
                        _ => 0,
                    };
                    println!("Daemon is running");
                    println!("  socket:   {}", path.display());
                    println!("  version:  {}", env!("CARGO_PKG_VERSION"));
                    println!("  sessions: {session_count}");
                }
                _ => {
                    println!("Daemon is running (socket: {})", path.display());
                }
            }
        }
        Err(_) => {
            if path.exists() {
                println!(
                    "Daemon is not running (stale socket file at {})",
                    path.display()
                );
                println!("  Run `phantom daemon stop` to clean up");
            } else {
                println!("Daemon is not running");
            }
            std::process::exit(1);
        }
    }
    Ok(())
}
