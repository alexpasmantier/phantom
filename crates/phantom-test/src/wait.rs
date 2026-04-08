use std::time::Duration;

use phantom_core::types::WaitCondition;
use phantom_daemon::engine::EngineCommand;

use crate::error::response_to_result;
use crate::session::Session;

/// Builder for composing wait conditions. Conditions are AND-ed together.
pub struct WaitBuilder<'a> {
    session: &'a Session,
    conditions: Vec<WaitCondition>,
    timeout_ms: u64,
    poll_ms: u64,
}

impl<'a> WaitBuilder<'a> {
    pub(crate) fn new(session: &'a Session) -> Self {
        Self {
            session,
            conditions: Vec::new(),
            timeout_ms: session.inner.default_timeout_ms,
            poll_ms: 50,
        }
    }

    /// Wait for text to appear on screen.
    pub fn text(mut self, text: &str) -> Self {
        self.conditions
            .push(WaitCondition::TextPresent(text.to_string()));
        self
    }

    /// Wait for text to disappear from screen.
    pub fn text_absent(mut self, text: &str) -> Self {
        self.conditions
            .push(WaitCondition::TextAbsent(text.to_string()));
        self
    }

    /// Wait for a regex pattern to match on screen.
    pub fn regex(mut self, pattern: &str) -> Self {
        let re = regex::Regex::new(pattern).expect("invalid regex pattern");
        self.conditions.push(WaitCondition::Regex(re));
        self
    }

    /// Wait for screen to stabilize for the given duration.
    pub fn stable(mut self, duration_ms: u64) -> Self {
        self.conditions
            .push(WaitCondition::ScreenStable { duration_ms });
        self
    }

    /// Wait for process to exit (any exit code).
    pub fn process_exit(mut self) -> Self {
        self.conditions
            .push(WaitCondition::ProcessExited { exit_code: None });
        self
    }

    /// Wait for process to exit with a specific code.
    pub fn exit_code(mut self, code: i32) -> Self {
        self.conditions.push(WaitCondition::ProcessExited {
            exit_code: Some(code),
        });
        self
    }

    /// Wait for cursor to reach a specific position.
    pub fn cursor_at(mut self, x: u16, y: u16) -> Self {
        self.conditions.push(WaitCondition::CursorAt { x, y });
        self
    }

    /// Wait for cursor to become visible.
    pub fn cursor_visible(mut self) -> Self {
        self.conditions.push(WaitCondition::CursorVisible(true));
        self
    }

    /// Wait for cursor to become hidden.
    pub fn cursor_hidden(mut self) -> Self {
        self.conditions.push(WaitCondition::CursorVisible(false));
        self
    }

    /// Wait for screen content to change from its current state.
    pub fn screen_changed(mut self) -> Self {
        self.conditions.push(WaitCondition::ScreenChanged);
        self
    }

    /// Override the timeout for this wait (in milliseconds).
    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// Override the poll interval for this wait (in milliseconds).
    pub fn poll_ms(mut self, ms: u64) -> Self {
        self.poll_ms = ms;
        self
    }

    /// Execute the wait, blocking until all conditions are met or timeout.
    pub fn until(self) -> crate::Result<()> {
        assert!(
            !self.conditions.is_empty(),
            "at least one wait condition is required"
        );
        let name = self.session.name.clone();
        let timeout_ms = self.timeout_ms;
        let poll_ms = self.poll_ms;
        let conditions = self.conditions;
        let resp = self.session.inner.send_wait_command(
            |reply| EngineCommand::Wait {
                session: name,
                conditions,
                timeout_ms,
                poll_ms,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )?;
        response_to_result(resp)?;
        Ok(())
    }
}
