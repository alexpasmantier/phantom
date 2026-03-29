use std::path::Path;

use anyhow::{Context, Result};
use phantom_core::protocol::{Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub struct Connection {
    stream: BufReader<UnixStream>,
}

impl Connection {
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)
            .await
            .with_context(|| format!("Failed to connect to daemon at {}", socket_path.display()))?;
        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    pub async fn send(&mut self, request: &Request) -> Result<Response> {
        let mut buf = serde_json::to_vec(request)?;
        buf.push(b'\n');
        self.stream.get_mut().write_all(&buf).await?;
        self.stream.get_mut().flush().await?;

        let mut line = String::new();
        self.stream.read_line(&mut line).await?;
        let response: Response = serde_json::from_str(&line)?;
        Ok(response)
    }
}
