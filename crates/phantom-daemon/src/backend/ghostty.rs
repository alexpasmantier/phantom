//! [`TerminalBackend`] implementation on top of libghostty-vt, Ghostty's
//! terminal emulation core.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Result, bail};
use libghostty_vt::ffi;
use libghostty_vt::key::{self, Action, Key as GKey, Mods as GMods};
use libghostty_vt::mouse;
use libghostty_vt::render::{CellIterator, RenderState, RowIterator};
use libghostty_vt::style::{RgbColor, StyleColor, Underline};
use libghostty_vt::terminal::{Mode, Options as TerminalOptions, Point, PointCoordinate, Terminal};

use phantom_core::types::{
    CellData, CursorInfo, CursorStyle, RowContent, ScreenContent, ScreenFormat,
};

use super::{Key, KeySpec, Mods, MouseAction, MouseButton, MouseSpec, Region, TerminalBackend};

pub struct GhosttyBackend {
    terminal: Terminal<'static, 'static>,
    render_state: RenderState<'static>,
    key_encoder: key::Encoder<'static>,
    mouse_encoder: mouse::Encoder<'static>,
    row_iter: RowIterator<'static>,
    cell_iter: CellIterator<'static>,
    cols: u16,
    rows: u16,
    /// Buffer for terminal responses (DA1, cursor position reports, etc.)
    /// Populated by the on_pty_write callback during vt_write, drained by `feed`.
    pty_write_buf: Rc<RefCell<Vec<u8>>>,
}

impl TerminalBackend for GhosttyBackend {
    fn new(cols: u16, rows: u16, scrollback: u32) -> Result<Self> {
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

        Ok(Self {
            terminal,
            render_state: RenderState::new()?,
            key_encoder: key::Encoder::new()?,
            mouse_encoder: mouse::Encoder::new()?,
            row_iter: RowIterator::new()?,
            cell_iter: CellIterator::new()?,
            cols,
            rows,
            pty_write_buf,
        })
    }

    fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        self.terminal.vt_write(data);
        self.pty_write_buf.borrow_mut().drain(..).collect()
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.terminal.resize(cols, rows, 0, 0)?;
        self.cols = cols;
        self.rows = rows;
        Ok(())
    }

    fn capture(&mut self, format: &ScreenFormat, region: Option<Region>) -> Result<ScreenContent> {
        let cursor = self.cursor();
        let title = self.title();

        let snapshot = self.render_state.update(&self.terminal)?;

        let mut rows = Vec::new();
        let mut row_it = self.row_iter.update(&snapshot)?;
        let mut row_idx: u16 = 0;
        let mut col_idx: u16;

        while let Some(row) = row_it.next() {
            // Region filter: skip rows outside the region
            if let Some((top, _, bottom, _)) = region
                && (row_idx < top || row_idx > bottom)
            {
                row_idx += 1;
                continue;
            }

            let mut text = String::new();
            let mut cells = Vec::new();
            let mut cell_it = self.cell_iter.update(row)?;
            col_idx = 0;

            while let Some(cell) = cell_it.next() {
                let in_region = match region {
                    Some((_, left, _, right)) => col_idx >= left && col_idx <= right,
                    None => true,
                };

                let graphemes = cell.graphemes()?;
                let grapheme_str = if graphemes.is_empty() {
                    " ".to_string()
                } else {
                    graphemes.iter().collect()
                };

                if in_region {
                    text.push_str(&grapheme_str);

                    if matches!(format, ScreenFormat::Json) {
                        let style = cell.style()?;
                        let fg = cell.fg_color()?.map(|c| rgb_to_hex(&c));
                        let bg = cell.bg_color()?.map(|c| rgb_to_hex(&c));

                        cells.push(CellData {
                            grapheme: grapheme_str,
                            fg,
                            bg,
                            bold: style.bold,
                            italic: style.italic,
                            underline: !matches!(style.underline, Underline::None),
                            strikethrough: style.strikethrough,
                            inverse: style.inverse,
                            faint: style.faint,
                        });
                    }
                }

                col_idx += 1;
            }

            rows.push(RowContent {
                row: row_idx,
                text,
                cells,
            });
            row_idx += 1;
        }

        Ok(ScreenContent {
            cols: self.cols,
            rows: self.rows,
            cursor,
            title,
            screen: rows,
        })
    }

    fn screen_text(&mut self) -> String {
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

    fn cell(&self, x: u16, y: u16) -> Result<CellData> {
        let coord: PointCoordinate = ffi::PointCoordinate { x, y: y as u32 }.into();
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

        Ok(CellData {
            grapheme,
            fg: style_color_to_string(&style.fg_color),
            bg: style_color_to_string(&style.bg_color),
            bold: style.bold,
            italic: style.italic,
            underline: !matches!(style.underline, Underline::None),
            strikethrough: style.strikethrough,
            inverse: style.inverse,
            faint: style.faint,
        })
    }

    fn scrollback(&self, max_lines: Option<u32>) -> Result<Vec<String>> {
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
                let coord: PointCoordinate = ffi::PointCoordinate {
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

    fn cursor(&self) -> CursorInfo {
        CursorInfo {
            x: self.terminal.cursor_x().unwrap_or(0),
            y: self.terminal.cursor_y().unwrap_or(0),
            visible: self.terminal.is_cursor_visible().unwrap_or(true),
            style: CursorStyle::Unknown,
        }
    }

    fn title(&self) -> Option<String> {
        self.terminal.title().ok().map(|s| s.to_string())
    }

    fn pwd(&self) -> Option<String> {
        self.terminal.pwd().ok().map(|s| s.to_string())
    }

    fn encode_key(&mut self, spec: &KeySpec) -> Result<Vec<u8>> {
        let key = to_ghostty_key(spec.key)?;
        let mods = to_ghostty_mods(spec.mods);

        self.key_encoder.set_options_from_terminal(&self.terminal);

        let mut event = key::Event::new()?;
        event.set_key(key).set_mods(mods).set_action(Action::Press);

        if let Some(ch) = key_to_char(key) {
            // The unshifted codepoint is what the kitty encoder keys off to
            // build `CSI code u` sequences; without it, modified keys (ctrl+c
            // and friends) encode to nothing under the kitty protocol.
            event.set_unshifted_codepoint(ch);
            // UTF-8 text stands in for the produced character, but only for an
            // unmodified press — a modifier means the child wants the encoded
            // sequence, not the raw byte.
            if mods.is_empty() {
                event.set_utf8(Some(ch.to_string()));
            }
        }

        let mut buf = Vec::new();
        self.key_encoder.encode_to_vec(&event, &mut buf)?;
        Ok(buf)
    }

    fn encode_mouse(&mut self, spec: &MouseSpec) -> Result<Vec<u8>> {
        let action = match spec.action {
            MouseAction::Press => mouse::Action::Press,
            MouseAction::Release => mouse::Action::Release,
            MouseAction::Motion => mouse::Action::Motion,
        };
        let button = spec.button.map(|b| match b {
            MouseButton::Left => mouse::Button::Left,
            MouseButton::Right => mouse::Button::Right,
            MouseButton::Middle => mouse::Button::Middle,
            MouseButton::ScrollUp => mouse::Button::Four,
            MouseButton::ScrollDown => mouse::Button::Five,
        });

        self.mouse_encoder.set_options_from_terminal(&self.terminal);

        let mut event = mouse::Event::new()?;
        event
            .set_action(action)
            .set_button(button)
            .set_position(mouse::Position {
                x: spec.x,
                y: spec.y,
            })
            .set_mods(GMods::empty());

        let mut buf = Vec::new();
        self.mouse_encoder.encode_to_vec(&event, &mut buf)?;
        Ok(buf)
    }

    fn bracketed_paste_enabled(&self) -> bool {
        self.terminal.mode(Mode::BRACKETED_PASTE).unwrap_or(false)
    }
}

fn rgb_to_hex(color: &RgbColor) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn style_color_to_string(color: &StyleColor) -> Option<String> {
    match color {
        StyleColor::None => None,
        StyleColor::Rgb(c) => Some(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)),
        StyleColor::Palette(idx) => Some(format!("palette:{}", idx.0)),
    }
}

fn to_ghostty_mods(mods: Mods) -> GMods {
    let mut out = GMods::empty();
    if mods.ctrl {
        out |= GMods::CTRL;
    }
    if mods.alt {
        out |= GMods::ALT;
    }
    if mods.shift {
        out |= GMods::SHIFT;
    }
    if mods.super_key {
        out |= GMods::SUPER;
    }
    out
}

fn to_ghostty_key(key: Key) -> Result<GKey> {
    let k = match key {
        Key::Char(ch) => {
            return char_to_key(ch).ok_or_else(|| anyhow::anyhow!("Unknown key: {ch}"));
        }
        Key::Enter => GKey::Enter,
        Key::Tab => GKey::Tab,
        Key::Escape => GKey::Escape,
        Key::Space => GKey::Space,
        Key::Backspace => GKey::Backspace,
        Key::Delete => GKey::Delete,
        Key::Insert => GKey::Insert,
        Key::Up => GKey::ArrowUp,
        Key::Down => GKey::ArrowDown,
        Key::Left => GKey::ArrowLeft,
        Key::Right => GKey::ArrowRight,
        Key::Home => GKey::Home,
        Key::End => GKey::End,
        Key::PageUp => GKey::PageUp,
        Key::PageDown => GKey::PageDown,
        Key::F(n) => match n {
            1 => GKey::F1,
            2 => GKey::F2,
            3 => GKey::F3,
            4 => GKey::F4,
            5 => GKey::F5,
            6 => GKey::F6,
            7 => GKey::F7,
            8 => GKey::F8,
            9 => GKey::F9,
            10 => GKey::F10,
            11 => GKey::F11,
            12 => GKey::F12,
            other => bail!("Unknown key: f{other}"),
        },
    };
    Ok(k)
}

fn char_to_key(ch: char) -> Option<GKey> {
    match ch {
        'a' => Some(GKey::A),
        'b' => Some(GKey::B),
        'c' => Some(GKey::C),
        'd' => Some(GKey::D),
        'e' => Some(GKey::E),
        'f' => Some(GKey::F),
        'g' => Some(GKey::G),
        'h' => Some(GKey::H),
        'i' => Some(GKey::I),
        'j' => Some(GKey::J),
        'k' => Some(GKey::K),
        'l' => Some(GKey::L),
        'm' => Some(GKey::M),
        'n' => Some(GKey::N),
        'o' => Some(GKey::O),
        'p' => Some(GKey::P),
        'q' => Some(GKey::Q),
        'r' => Some(GKey::R),
        's' => Some(GKey::S),
        't' => Some(GKey::T),
        'u' => Some(GKey::U),
        'v' => Some(GKey::V),
        'w' => Some(GKey::W),
        'x' => Some(GKey::X),
        'y' => Some(GKey::Y),
        'z' => Some(GKey::Z),
        '0' => Some(GKey::Digit0),
        '1' => Some(GKey::Digit1),
        '2' => Some(GKey::Digit2),
        '3' => Some(GKey::Digit3),
        '4' => Some(GKey::Digit4),
        '5' => Some(GKey::Digit5),
        '6' => Some(GKey::Digit6),
        '7' => Some(GKey::Digit7),
        '8' => Some(GKey::Digit8),
        '9' => Some(GKey::Digit9),
        _ => None,
    }
}

fn key_to_char(key: GKey) -> Option<char> {
    match key {
        GKey::A => Some('a'),
        GKey::B => Some('b'),
        GKey::C => Some('c'),
        GKey::D => Some('d'),
        GKey::E => Some('e'),
        GKey::F => Some('f'),
        GKey::G => Some('g'),
        GKey::H => Some('h'),
        GKey::I => Some('i'),
        GKey::J => Some('j'),
        GKey::K => Some('k'),
        GKey::L => Some('l'),
        GKey::M => Some('m'),
        GKey::N => Some('n'),
        GKey::O => Some('o'),
        GKey::P => Some('p'),
        GKey::Q => Some('q'),
        GKey::R => Some('r'),
        GKey::S => Some('s'),
        GKey::T => Some('t'),
        GKey::U => Some('u'),
        GKey::V => Some('v'),
        GKey::W => Some('w'),
        GKey::X => Some('x'),
        GKey::Y => Some('y'),
        GKey::Z => Some('z'),
        GKey::Space => Some(' '),
        GKey::Enter => Some('\r'),
        GKey::Tab => Some('\t'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, mods: Mods) -> KeySpec {
        KeySpec { key, mods }
    }

    /// The alacritty backend has the mirror of this test; the expected bytes
    /// must stay identical so children see the same input on both backends.
    #[test]
    fn kitty_encoding_matches_the_alacritty_backend() {
        let none = Mods::default();
        let ctrl = Mods {
            ctrl: true,
            ..Default::default()
        };

        let mut t = GhosttyBackend::new(80, 24, 0).unwrap();
        t.feed(b"\x1b[>1u");
        assert_eq!(t.encode_key(&key(Key::Escape, none)).unwrap(), b"\x1b[27u");
        assert_eq!(t.encode_key(&key(Key::Char('a'), none)).unwrap(), b"a");
        assert_eq!(
            t.encode_key(&key(Key::Char('c'), ctrl)).unwrap(),
            b"\x1b[99;5u"
        );

        let mut t = GhosttyBackend::new(80, 24, 0).unwrap();
        t.feed(b"\x1b[>27u");
        assert_eq!(
            t.encode_key(&key(Key::Char('a'), none)).unwrap(),
            b"\x1b[97;;97u"
        );
        assert_eq!(t.encode_key(&key(Key::Enter, none)).unwrap(), b"\x1b[13u");
        assert_eq!(t.encode_key(&key(Key::Up, none)).unwrap(), b"\x1b[1;1:1A");
    }
}
