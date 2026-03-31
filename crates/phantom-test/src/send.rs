use phantom_core::types::InputAction;
use phantom_daemon::engine::EngineCommand;

use crate::error::response_to_result;
use crate::session::Session;

/// Builder for sending input to a session.
pub struct SendBuilder<'a> {
    session: &'a Session,
}

impl<'a> SendBuilder<'a> {
    pub(crate) fn new(session: &'a Session) -> Self {
        Self { session }
    }

    /// Type text character by character.
    pub fn type_text(self, text: &str) -> crate::Result<()> {
        let text = text.to_string();
        let name = self.session.name.clone();
        let resp = self.session.inner.send_command(|reply| EngineCommand::SendInput {
            session: name,
            action: InputAction::Type {
                text,
                delay_ms: None,
            },
            reply,
        })?;
        response_to_result(resp)?;
        Ok(())
    }

    /// Type text with a per-character delay in milliseconds.
    pub fn type_text_slow(self, text: &str, delay_ms: u64) -> crate::Result<()> {
        let text = text.to_string();
        let name = self.session.name.clone();
        let resp = self.session.inner.send_command(|reply| EngineCommand::SendInput {
            session: name,
            action: InputAction::Type {
                text,
                delay_ms: Some(delay_ms),
            },
            reply,
        })?;
        response_to_result(resp)?;
        Ok(())
    }

    /// Send a single key (e.g., "escape", "enter", "ctrl-c", "f1").
    pub fn key(self, key: &str) -> crate::Result<()> {
        let name = self.session.name.clone();
        let resp = self.session.inner.send_command(|reply| EngineCommand::SendInput {
            session: name,
            action: InputAction::Key {
                keys: vec![key.to_string()],
            },
            reply,
        })?;
        response_to_result(resp)?;
        Ok(())
    }

    /// Send multiple keys in sequence.
    pub fn keys(self, keys: &[&str]) -> crate::Result<()> {
        let name = self.session.name.clone();
        let keys: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
        let resp = self.session.inner.send_command(|reply| EngineCommand::SendInput {
            session: name,
            action: InputAction::Key { keys },
            reply,
        })?;
        response_to_result(resp)?;
        Ok(())
    }

    /// Send text via bracketed paste.
    pub fn paste(self, text: &str) -> crate::Result<()> {
        let text = text.to_string();
        let name = self.session.name.clone();
        let resp = self.session.inner.send_command(|reply| EngineCommand::SendInput {
            session: name,
            action: InputAction::Paste { text },
            reply,
        })?;
        response_to_result(resp)?;
        Ok(())
    }

    /// Send a mouse event (e.g., "click:10,5", "right-click:20,10").
    pub fn mouse(self, spec: &str) -> crate::Result<()> {
        let spec = spec.to_string();
        let name = self.session.name.clone();
        let resp = self.session.inner.send_command(|reply| EngineCommand::SendInput {
            session: name,
            action: InputAction::Mouse { spec },
            reply,
        })?;
        response_to_result(resp)?;
        Ok(())
    }
}
