use phantom_core::protocol::ResponseData;
use phantom_daemon::engine::EngineCommand;

use crate::error::response_to_result;
use crate::phantom::Phantom;
use crate::session::Session;

/// Builder for creating a new session.
pub struct SessionBuilder<'a> {
    phantom: &'a Phantom,
    command: String,
    args: Vec<String>,
    name: Option<String>,
    cols: u16,
    rows: u16,
    scrollback: u32,
    env: Vec<(String, String)>,
    cwd: Option<String>,
}

impl<'a> SessionBuilder<'a> {
    pub(crate) fn new(phantom: &'a Phantom, command: &str) -> Self {
        Self {
            phantom,
            command: command.to_string(),
            args: Vec::new(),
            name: None,
            cols: 80,
            rows: 24,
            scrollback: 1000,
            env: vec![("LANG".into(), "C".into()), ("LC_ALL".into(), "C".into())],
            cwd: None,
        }
    }

    /// Set command arguments.
    pub fn args(mut self, args: &[&str]) -> Self {
        self.args = args.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Set the session name (auto-generated if not specified).
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Set terminal column count.
    pub fn cols(mut self, cols: u16) -> Self {
        self.cols = cols;
        self
    }

    /// Set terminal row count.
    pub fn rows(mut self, rows: u16) -> Self {
        self.rows = rows;
        self
    }

    /// Set terminal dimensions.
    pub fn size(mut self, cols: u16, rows: u16) -> Self {
        self.cols = cols;
        self.rows = rows;
        self
    }

    /// Set scrollback buffer size.
    pub fn scrollback(mut self, lines: u32) -> Self {
        self.scrollback = lines;
        self
    }

    /// Add an environment variable.
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    /// Set the working directory.
    pub fn cwd(mut self, path: &str) -> Self {
        self.cwd = Some(path.to_string());
        self
    }

    /// Spawn the session and return a handle.
    pub fn start(self) -> crate::Result<Session> {
        let name = self
            .name
            .unwrap_or_else(|| self.phantom.next_session_name());

        let resp = self
            .phantom
            .inner
            .send_command(|reply| EngineCommand::CreateSession {
                name: name.clone(),
                command: self.command,
                args: self.args,
                env: self.env,
                cwd: self.cwd,
                cols: self.cols,
                rows: self.rows,
                scrollback: self.scrollback,
                reply,
            })?;

        match response_to_result(resp)? {
            Some(ResponseData::Session(_)) => {
                if let Some(ref hook) = self.phantom.on_session_created {
                    hook(self.phantom.inner.clone(), name.clone());
                }
                Ok(Session::new(self.phantom.inner.clone(), name))
            }
            _ => panic!("unexpected response from create_session"),
        }
    }
}
