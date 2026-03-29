use anyhow::Result;

use crate::daemon_ctl;

pub async fn start(foreground: bool, socket: Option<String>) -> Result<()> {
    if foreground {
        // In foreground mode, exec the daemon directly
        let daemon_bin = std::env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent dir"))?
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
        println!("Daemon started");
        Ok(())
    }
}

pub async fn stop() -> Result<()> {
    let path = daemon_ctl::socket_path();
    if let Ok(mut conn) = crate::connection::Connection::connect(&path).await {
        let _ = conn
            .send(&phantom_core::protocol::Request::Shutdown)
            .await;
    }
    // Clean up socket file
    let _ = std::fs::remove_file(&path);
    println!("Daemon stopped");
    Ok(())
}

pub async fn status() -> Result<()> {
    let path = daemon_ctl::socket_path();
    match crate::connection::Connection::connect(&path).await {
        Ok(_) => println!("Daemon is running (socket: {})", path.display()),
        Err(_) => {
            println!("Daemon is not running");
            std::process::exit(1);
        }
    }
    Ok(())
}
