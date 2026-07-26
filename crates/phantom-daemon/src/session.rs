use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::Result;

use phantom_core::types::{
    CellData, CursorInfo, ScreenContent, ScreenFormat, SessionInfo, SessionStatus,
};

use crate::backend::{DefaultBackend, Region, TerminalBackend};
use crate::pty::Pty;

/// A running child process plus the terminal it draws into.
///
/// The terminal itself lives behind [`TerminalBackend`]; everything here is
/// backend-agnostic bookkeeping — process state, screen text caching and
/// stability tracking for wait conditions.
pub struct Session {
    pub name: String,
    pub pty: Pty,
    pub(crate) term: DefaultBackend,
    pub cols: u16,
    pub rows: u16,
    exit_code: Option<i32>,
    /// Cached screen text for wait condition evaluation
    screen_text_cache: Option<String>,
    /// Hash of last screen content for stability detection
    pub last_screen_hash: u64,
    pub screen_stable_since: std::time::Instant,
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        command: &str,
        args: &[String],
        env: &[(String, String)],
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
        scrollback: u32,
    ) -> Result<Self> {
        let pty = Pty::spawn(command, args, env, cwd, cols, rows)?;
        let term = DefaultBackend::new(cols, rows, scrollback)?;

        Ok(Self {
            name,
            pty,
            term,
            cols,
            rows,
            exit_code: None,
            screen_text_cache: None,
            last_screen_hash: 0,
            screen_stable_since: std::time::Instant::now(),
        })
    }

    /// Feed bytes from PTY into the terminal emulator.
    /// After processing, flushes any terminal responses (DA queries, etc.) back to the PTY.
    pub fn process_pty_output(&mut self, data: &[u8]) {
        let response = self.term.feed(data);
        self.screen_text_cache = None;

        if !response.is_empty() {
            let _ = self.pty.write(&response);
        }
    }

    /// Check and update process exit status.
    pub fn check_exit(&mut self) -> Option<i32> {
        if self.exit_code.is_none() {
            self.exit_code = self.pty.try_wait();
        }
        self.exit_code
    }

    pub fn info(&mut self) -> SessionInfo {
        let status = match self.check_exit() {
            Some(code) => SessionStatus::Exited { code: Some(code) },
            None => SessionStatus::Running,
        };
        SessionInfo {
            name: self.name.clone(),
            pid: self.pty.child_pid.as_raw() as u32,
            cols: self.cols,
            rows: self.rows,
            title: self.term.title(),
            pwd: self.term.pwd(),
            status,
        }
    }

    pub fn cursor_info(&self) -> CursorInfo {
        self.term.cursor()
    }

    /// Capture the active screen.
    pub fn capture(
        &mut self,
        format: &ScreenFormat,
        region: Option<Region>,
    ) -> Result<ScreenContent> {
        self.term.capture(format, region)
    }

    /// Get screen as plain text (cached).
    pub fn screen_text(&mut self) -> &str {
        if self.screen_text_cache.is_none() {
            self.screen_text_cache = Some(self.term.screen_text());
        }
        self.screen_text_cache.as_ref().unwrap()
    }

    /// Compute a hash of the current screen content for stability detection.
    pub fn screen_hash(&mut self) -> u64 {
        let text = self.screen_text().to_string();
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    /// Get scrollback content as plain text lines.
    /// If `max_lines` is Some, returns only the last N lines of scrollback.
    pub fn scrollback_text(&self, max_lines: Option<u32>) -> Result<Vec<String>> {
        self.term.scrollback(max_lines)
    }

    /// Get the process output — the primary screen content after process exit.
    /// This captures what a TUI like fzf/tv writes to stdout after leaving
    /// alternate screen mode.
    pub fn get_output(&mut self) -> Result<String> {
        // The output is whatever is on the primary screen, trimmed.
        let text = self.screen_text().to_string();
        let trimmed: Vec<&str> = text
            .lines()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .skip_while(|l| l.trim().is_empty())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Ok(trimmed.join("\n"))
    }

    /// Get a single cell's data at (x, y) on the active screen.
    pub fn get_cell(&self, x: u16, y: u16) -> Result<CellData> {
        self.term.cell(x, y)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.term.resize(cols, rows)?;
        self.pty.resize(cols, rows)?;
        self.cols = cols;
        self.rows = rows;
        self.screen_text_cache = None;
        Ok(())
    }
}
