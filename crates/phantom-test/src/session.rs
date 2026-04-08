use std::sync::Arc;

use phantom_core::protocol::ResponseData;
use phantom_core::types::{CellData, CursorInfo, ScreenContent, ScreenFormat, SessionInfo};
use phantom_daemon::engine::EngineCommand;

use crate::error::response_to_result;
use crate::phantom::PhantomInner;
use crate::screen::Screen;
use crate::send::SendBuilder;
use crate::wait::WaitBuilder;

/// A handle to a running TUI session.
pub struct Session {
    pub(crate) inner: Arc<PhantomInner>,
    pub(crate) name: String,
}

impl Session {
    pub(crate) fn new(inner: Arc<PhantomInner>, name: String) -> Self {
        Self { inner, name }
    }

    /// The session name.
    pub fn name(&self) -> &str {
        &self.name
    }

    // ── Input ───────────────────────────────────────────────

    /// Build and send input to this session.
    pub fn send(&self) -> SendBuilder<'_> {
        SendBuilder::new(self)
    }

    // ── Waiting ─────────────────────────────────────────────

    /// Build a wait condition for this session.
    pub fn wait(&self) -> WaitBuilder<'_> {
        WaitBuilder::new(self)
    }

    // ── Screen capture ──────────────────────────────────────

    /// Take a text screenshot of the current screen.
    pub fn screenshot(&self) -> crate::Result<Screen> {
        let resp = self.inner.send_command(|reply| EngineCommand::Screenshot {
            session: self.name.clone(),
            format: ScreenFormat::Text,
            region: None,
            reply,
        })?;
        match response_to_result(resp)? {
            Some(ResponseData::Screen(screen)) => Ok(Screen::new(screen)),
            _ => panic!("unexpected response from screenshot"),
        }
    }

    /// Take a JSON screenshot with full cell data.
    pub fn screenshot_json(&self) -> crate::Result<ScreenContent> {
        let resp = self.inner.send_command(|reply| EngineCommand::Screenshot {
            session: self.name.clone(),
            format: ScreenFormat::Json,
            region: None,
            reply,
        })?;
        match response_to_result(resp)? {
            Some(ResponseData::Screen(screen)) => Ok(screen),
            _ => panic!("unexpected response from screenshot"),
        }
    }

    /// Take a screenshot of a specific region (top, left, bottom, right — 0-indexed, inclusive).
    pub fn screenshot_region(
        &self,
        top: u16,
        left: u16,
        bottom: u16,
        right: u16,
    ) -> crate::Result<Screen> {
        let resp = self.inner.send_command(|reply| EngineCommand::Screenshot {
            session: self.name.clone(),
            format: ScreenFormat::Text,
            region: Some((top, left, bottom, right)),
            reply,
        })?;
        match response_to_result(resp)? {
            Some(ResponseData::Screen(screen)) => Ok(Screen::new(screen)),
            _ => panic!("unexpected response from screenshot"),
        }
    }

    // ── Inspection ──────────────────────────────────────────

    /// Get cursor position and visibility.
    pub fn cursor(&self) -> crate::Result<CursorInfo> {
        let resp = self.inner.send_command(|reply| EngineCommand::GetCursor {
            session: self.name.clone(),
            reply,
        })?;
        match response_to_result(resp)? {
            Some(ResponseData::Cursor(c)) => Ok(c),
            _ => panic!("unexpected response from get_cursor"),
        }
    }

    /// Inspect a single cell at (x, y).
    pub fn cell(&self, x: u16, y: u16) -> crate::Result<CellData> {
        let resp = self.inner.send_command(|reply| EngineCommand::GetCell {
            session: self.name.clone(),
            x,
            y,
            reply,
        })?;
        match response_to_result(resp)? {
            Some(ResponseData::Cell(cell)) => Ok(cell),
            _ => panic!("unexpected response from get_cell"),
        }
    }

    /// Get scrollback buffer content.
    pub fn scrollback(&self, lines: Option<u32>) -> crate::Result<String> {
        let resp = self
            .inner
            .send_command(|reply| EngineCommand::GetScrollback {
                session: self.name.clone(),
                lines,
                reply,
            })?;
        match response_to_result(resp)? {
            Some(ResponseData::Text(text)) => Ok(text),
            _ => panic!("unexpected response from get_scrollback"),
        }
    }

    /// Get process output (primary screen content after TUI exit).
    pub fn output(&self) -> crate::Result<String> {
        let resp = self.inner.send_command(|reply| EngineCommand::GetOutput {
            session: self.name.clone(),
            reply,
        })?;
        match response_to_result(resp)? {
            Some(ResponseData::Text(text)) => Ok(text),
            _ => panic!("unexpected response from get_output"),
        }
    }

    /// Query session status.
    pub fn status(&self) -> crate::Result<SessionInfo> {
        let resp = self.inner.send_command(|reply| EngineCommand::GetStatus {
            session: self.name.clone(),
            reply,
        })?;
        match response_to_result(resp)? {
            Some(ResponseData::Session(info)) => Ok(info),
            _ => panic!("unexpected response from get_status"),
        }
    }

    // ── Control ─────────────────────────────────────────────

    /// Resize the terminal.
    pub fn resize(&self, cols: u16, rows: u16) -> crate::Result<()> {
        let resp = self.inner.send_command(|reply| EngineCommand::Resize {
            session: self.name.clone(),
            cols,
            rows,
            reply,
        })?;
        response_to_result(resp)?;
        Ok(())
    }

    /// Kill the session (sends SIGTERM).
    pub fn kill(&self) -> crate::Result<()> {
        let resp = self
            .inner
            .send_command(|reply| EngineCommand::KillSession {
                session: self.name.clone(),
                signal: None,
                reply,
            })?;
        response_to_result(resp)?;
        Ok(())
    }

    /// Kill the session with a specific signal number.
    pub fn kill_with_signal(&self, signal: i32) -> crate::Result<()> {
        let resp = self
            .inner
            .send_command(|reply| EngineCommand::KillSession {
                session: self.name.clone(),
                signal: Some(signal),
                reply,
            })?;
        response_to_result(resp)?;
        Ok(())
    }
}
