use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use anyhow::Result;
use libghostty_vt::ffi;
use libghostty_vt::render::{CellIterator, RenderState, RowIterator};
use libghostty_vt::terminal::{Options as TerminalOptions, Point, PointCoordinate, Terminal};

use phantom_core::types::{CursorInfo, CursorStyle, SessionInfo, SessionStatus};

use crate::pty::Pty;

pub struct Session {
    pub name: String,
    pub pty: Pty,
    pub terminal: Terminal<'static, 'static>,
    pub render_state: RenderState<'static>,
    pub key_encoder: libghostty_vt::key::Encoder<'static>,
    pub mouse_encoder: libghostty_vt::mouse::Encoder<'static>,
    pub row_iter: RowIterator<'static>,
    pub cell_iter: CellIterator<'static>,
    pub cols: u16,
    pub rows: u16,
    exit_code: Option<i32>,
    /// Buffer for terminal responses (DA1, cursor position reports, etc.)
    /// Populated by the on_pty_write callback during vt_write, flushed after.
    pty_write_buf: Rc<RefCell<Vec<u8>>>,
    /// Cached screen text for wait condition evaluation
    screen_text_cache: Option<String>,
    /// Hash of last screen content for stability detection
    pub last_screen_hash: u64,
    pub screen_stable_since: std::time::Instant,
}

impl Session {
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

        let mut terminal = Terminal::new(TerminalOptions {
            cols,
            rows,
            max_scrollback: scrollback as usize,
        })?;

        // Buffer for terminal responses (DA1/DA2/DA3, cursor position reports, etc.)
        // The callback is invoked synchronously during vt_write(), so we buffer
        // and flush after vt_write returns.
        let pty_write_buf = Rc::new(RefCell::new(Vec::new()));
        let buf_clone = pty_write_buf.clone();
        terminal.on_pty_write(move |_term, data: &[u8]| {
            buf_clone.borrow_mut().extend_from_slice(data);
        })?;

        let render_state = RenderState::new()?;
        let key_encoder = libghostty_vt::key::Encoder::new()?;
        let mouse_encoder = libghostty_vt::mouse::Encoder::new()?;
        let row_iter = RowIterator::new()?;
        let cell_iter = CellIterator::new()?;

        Ok(Self {
            name,
            pty,
            terminal,
            render_state,
            key_encoder,
            mouse_encoder,
            row_iter,
            cell_iter,
            cols,
            rows,
            exit_code: None,
            pty_write_buf,
            screen_text_cache: None,
            last_screen_hash: 0,
            screen_stable_since: std::time::Instant::now(),
        })
    }

    /// Feed bytes from PTY into the terminal emulator.
    /// After processing, flushes any terminal responses (DA queries, etc.) back to the PTY.
    pub fn process_pty_output(&mut self, data: &[u8]) {
        self.terminal.vt_write(data);
        self.screen_text_cache = None;

        // Flush terminal responses (buffered by on_pty_write callback)
        let buf: Vec<u8> = self.pty_write_buf.borrow_mut().drain(..).collect();
        if !buf.is_empty() {
            let _ = self.pty.write(&buf);
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
            title: self.terminal.title().ok().map(|s| s.to_string()),
            pwd: self.terminal.pwd().ok().map(|s| s.to_string()),
            status,
        }
    }

    pub fn cursor_info(&self) -> CursorInfo {
        CursorInfo {
            x: self.terminal.cursor_x().unwrap_or(0),
            y: self.terminal.cursor_y().unwrap_or(0),
            visible: self.terminal.is_cursor_visible().unwrap_or(true),
            style: CursorStyle::Unknown,
        }
    }

    /// Get screen as plain text (cached).
    pub fn screen_text(&mut self) -> &str {
        if self.screen_text_cache.is_none() {
            self.screen_text_cache = Some(self.compute_screen_text());
        }
        self.screen_text_cache.as_ref().unwrap()
    }

    fn compute_screen_text(&mut self) -> String {
        let mut text = String::new();
        let Ok(snapshot) = self.render_state.update(&self.terminal) else {
            return text;
        };
        let Ok(mut row_it) = self.row_iter.update(&snapshot) else {
            return text;
        };
        while let Some(row) = row_it.next() {
            if !text.is_empty() {
                text.push('\n');
            }
            if let Ok(mut cell_it) = self.cell_iter.update(row) {
                while let Some(cell) = cell_it.next() {
                    if let Ok(graphemes) = cell.graphemes() {
                        if graphemes.is_empty() {
                            text.push(' ');
                        } else {
                            for ch in graphemes {
                                text.push(ch);
                            }
                        }
                    }
                }
            }
        }
        text
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
        let total = self.terminal.total_rows()?;
        let viewport = self.terminal.rows()? as usize;
        let scrollback = total.saturating_sub(viewport);

        if scrollback == 0 {
            return Ok(Vec::new());
        }

        let start_row = match max_lines {
            Some(n) => scrollback.saturating_sub(n as usize),
            None => 0,
        };
        let end_row = scrollback;
        let cols = self.terminal.cols()?;

        let mut lines = Vec::new();
        for row_idx in start_row..end_row {
            let mut line = String::new();
            for col_idx in 0..cols {
                let coord: PointCoordinate =
                    ffi::GhosttyPointCoordinate {
                        x: col_idx,
                        y: row_idx as u32,
                    }
                    .into();
                if let Ok(grid_ref) = self.terminal.grid_ref(Point::History(coord)) {
                    let mut buf = ['\0'; 8];
                    if grid_ref.graphemes(&mut buf).is_ok() {
                        for &ch in &buf {
                            if ch == '\0' {
                                break;
                            }
                            line.push(ch);
                        }
                    }
                }
            }
            // Trim trailing whitespace
            let trimmed = line.trim_end().to_string();
            lines.push(trimmed);
        }

        Ok(lines)
    }

    /// Get the process output — the primary screen content after process exit.
    /// This captures what a TUI like fzf/tv writes to stdout after leaving
    /// alternate screen mode.
    pub fn get_output(&mut self) -> Result<String> {
        // The output is whatever is on the primary screen, trimmed.
        let text = self.compute_screen_text();
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
    pub fn get_cell(&self, x: u16, y: u16) -> Result<phantom_core::types::CellData> {
        let coord: PointCoordinate = ffi::GhosttyPointCoordinate {
            x,
            y: y as u32,
        }
        .into();
        let grid_ref = self.terminal.grid_ref(Point::Active(coord))?;
        let style = grid_ref.style()?;
        let mut buf = ['\0'; 8];
        grid_ref.graphemes(&mut buf)?;
        let grapheme: String = buf.iter().take_while(|&&c| c != '\0').collect();
        let grapheme = if grapheme.is_empty() {
            " ".to_string()
        } else {
            grapheme
        };

        Ok(phantom_core::types::CellData {
            grapheme,
            fg: style_color_to_string(&style.fg_color),
            bg: style_color_to_string(&style.bg_color),
            bold: style.bold,
            italic: style.italic,
            underline: !matches!(style.underline, libghostty_vt::style::Underline::None),
            strikethrough: style.strikethrough,
            inverse: style.inverse,
            faint: style.faint,
        })
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.terminal.resize(cols, rows, 0, 0)?;
        self.pty.resize(cols, rows)?;
        self.cols = cols;
        self.rows = rows;
        self.screen_text_cache = None;
        Ok(())
    }
}

fn style_color_to_string(color: &libghostty_vt::style::StyleColor) -> Option<String> {
    match color {
        libghostty_vt::style::StyleColor::None => None,
        libghostty_vt::style::StyleColor::Rgb(c) => {
            Some(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
        }
        libghostty_vt::style::StyleColor::Palette(idx) => Some(format!("palette:{}", idx.0)),
    }
}
