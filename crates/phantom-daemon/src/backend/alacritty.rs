//! [`TerminalBackend`] implementation on top of `alacritty_terminal`.
//!
//! Pure Rust, so it builds without a Zig toolchain. `alacritty_terminal` has
//! no key encoder of its own: legacy key encoding comes from `termwiz`, driven
//! by the modes alacritty tracks, while the kitty keyboard protocol and mouse
//! reports are encoded here directly.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Processor, Rgb};
use anyhow::{Result, bail};
use termwiz::input::{KeyCode, KeyCodeEncodeModes, KeyboardEncoding, Modifiers};

use phantom_core::types::{
    CellData, CursorInfo, CursorStyle, RowContent, ScreenContent, ScreenFormat,
};

use super::{Key, KeySpec, Mods, MouseAction, MouseButton, MouseSpec, Region, TerminalBackend};

/// Terminal dimensions. `alacritty_terminal` takes any `Dimensions` for sizing;
/// for that purpose history is irrelevant, so `total_lines` is just the viewport.
struct TermSize {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// State the terminal reports back through events rather than through the grid.
#[derive(Default)]
struct EventState {
    /// Replies destined for the child process (DA, cursor reports, color queries).
    pty_write: Vec<u8>,
    title: Option<String>,
    cols: u16,
    rows: u16,
}

/// Collects the events we care about. `alacritty_terminal` hands events out
/// through `&self`, hence the `RefCell`.
struct Listener(Rc<RefCell<EventState>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let mut state = self.0.borrow_mut();
        match event {
            Event::PtyWrite(text) => state.pty_write.extend_from_slice(text.as_bytes()),
            Event::Title(title) => state.title = Some(title),
            Event::ResetTitle => state.title = None,
            Event::ColorRequest(index, format) => {
                let reply = format(default_color(index));
                state.pty_write.extend_from_slice(reply.as_bytes());
            }
            Event::TextAreaSizeRequest(format) => {
                let reply = format(WindowSize {
                    num_lines: state.rows,
                    num_cols: state.cols,
                    cell_width: 8,
                    cell_height: 16,
                });
                state.pty_write.extend_from_slice(reply.as_bytes());
            }
            _ => {}
        }
    }
}

pub struct AlacrittyBackend {
    term: Term<Listener>,
    parser: Processor,
    state: Rc<RefCell<EventState>>,
    cols: u16,
    rows: u16,
}

impl AlacrittyBackend {
    /// Text of one grid line. Wide-character spacers are skipped so the text
    /// keeps the same display width as the grid row.
    fn line_text(&self, line: Line) -> String {
        let grid = self.term.grid();
        let mut text = String::with_capacity(self.cols as usize);
        for col in 0..grid.columns() {
            let cell = &grid[line][Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            push_grapheme(&mut text, cell);
        }
        text
    }
}

impl TerminalBackend for AlacrittyBackend {
    fn new(cols: u16, rows: u16, scrollback: u32) -> Result<Self> {
        let state = Rc::new(RefCell::new(EventState {
            cols,
            rows,
            ..Default::default()
        }));

        let config = Config {
            scrolling_history: scrollback as usize,
            kitty_keyboard: true,
            ..Default::default()
        };
        let size = TermSize {
            columns: cols as usize,
            screen_lines: rows as usize,
        };
        let term = Term::new(config, &size, Listener(Rc::clone(&state)));

        Ok(Self {
            term,
            parser: Processor::new(),
            state,
            cols,
            rows,
        })
    }

    fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        self.parser.advance(&mut self.term, data);
        std::mem::take(&mut self.state.borrow_mut().pty_write)
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.term.resize(TermSize {
            columns: cols as usize,
            screen_lines: rows as usize,
        });
        self.cols = cols;
        self.rows = rows;
        let mut state = self.state.borrow_mut();
        state.cols = cols;
        state.rows = rows;
        Ok(())
    }

    fn capture(&mut self, format: &ScreenFormat, region: Option<Region>) -> Result<ScreenContent> {
        let cursor = self.cursor();
        let title = self.title();
        let want_cells = matches!(format, ScreenFormat::Json);

        let mut rows = Vec::new();
        for row_idx in 0..self.rows {
            if let Some((top, _, bottom, _)) = region
                && (row_idx < top || row_idx > bottom)
            {
                continue;
            }

            let line = Line(row_idx as i32);
            let mut text = String::new();
            let mut cells = Vec::new();

            for col_idx in 0..self.cols {
                if let Some((_, left, _, right)) = region
                    && (col_idx < left || col_idx > right)
                {
                    continue;
                }

                let cell = &self.term.grid()[line][Column(col_idx as usize)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                push_grapheme(&mut text, cell);

                if want_cells {
                    cells.push(cell_data(cell));
                }
            }

            rows.push(RowContent {
                row: row_idx,
                text,
                cells,
            });
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
        for row_idx in 0..self.rows {
            if row_idx > 0 {
                text.push('\n');
            }
            text.push_str(&self.line_text(Line(row_idx as i32)));
        }
        text
    }

    fn cell(&self, x: u16, y: u16) -> Result<CellData> {
        if x >= self.cols || y >= self.rows {
            bail!(
                "Cell ({x}, {y}) is outside the {}x{} screen",
                self.cols,
                self.rows
            );
        }
        let cell = &self.term.grid()[Line(y as i32)][Column(x as usize)];
        Ok(cell_data(cell))
    }

    fn scrollback(&self, max_lines: Option<u32>) -> Result<Vec<String>> {
        let history = self.term.grid().history_size();
        if history == 0 {
            return Ok(Vec::new());
        }

        let start = match max_lines {
            Some(n) => history.saturating_sub(n as usize),
            None => 0,
        };

        // History lines are negative, oldest first: -history ..= -1.
        let mut lines = Vec::new();
        for offset in start..history {
            let line = Line(-((history - offset) as i32));
            lines.push(self.line_text(line).trim_end().to_string());
        }
        Ok(lines)
    }

    fn cursor(&self) -> CursorInfo {
        let point = self.term.grid().cursor.point;
        CursorInfo {
            x: point.column.0 as u16,
            y: point.line.0.max(0) as u16,
            visible: self.term.mode().contains(TermMode::SHOW_CURSOR),
            style: CursorStyle::Unknown,
        }
    }

    fn title(&self) -> Option<String> {
        self.state.borrow().title.clone()
    }

    fn pwd(&self) -> Option<String> {
        // alacritty_terminal does not track OSC 7.
        None
    }

    fn encode_key(&mut self, spec: &KeySpec) -> Result<Vec<u8>> {
        let mode = self.term.mode();
        if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) {
            return encode_kitty(spec, *mode);
        }

        let key = to_termwiz_key(spec.key)?;
        let mods = to_termwiz_mods(spec.mods);
        let modes = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            application_cursor_keys: mode.contains(TermMode::APP_CURSOR),
            newline_mode: mode.contains(TermMode::LINE_FEED_NEW_LINE),
            modify_other_keys: None,
        };

        let encoded = key
            .encode(mods, modes, true)
            .map_err(|e| anyhow::anyhow!("Key encoding failed: {e}"))?;
        Ok(encoded.into_bytes())
    }

    fn encode_mouse(&mut self, spec: &MouseSpec) -> Result<Vec<u8>> {
        let mode = self.term.mode();

        // Same reporting rules as ghostty: normal mode (1000) never reports
        // motion, button mode (1002) only reports motion while a button is
        // held, any-motion mode (1003) reports everything.
        let report = if mode.contains(TermMode::MOUSE_MOTION) {
            true
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            spec.button.is_some()
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            spec.action != MouseAction::Motion
        } else {
            false
        };
        if !report {
            return Ok(Vec::new());
        }

        let sgr = mode.contains(TermMode::SGR_MOUSE);
        let mut code: u32 = match spec.button {
            // No button means motion with nothing pressed.
            None => 3,
            // Legacy encodings can't say which button was released.
            Some(_) if spec.action == MouseAction::Release && !sgr => 3,
            Some(MouseButton::Left) => 0,
            Some(MouseButton::Middle) => 1,
            Some(MouseButton::Right) => 2,
            Some(MouseButton::ScrollUp) => 64,
            Some(MouseButton::ScrollDown) => 65,
        };
        if spec.action == MouseAction::Motion {
            code += 32;
        }

        // Mouse reports are 1-indexed.
        let x = spec.x.max(0.0) as u32 + 1;
        let y = spec.y.max(0.0) as u32 + 1;

        if sgr {
            let final_byte = if spec.action == MouseAction::Release {
                'm'
            } else {
                'M'
            };
            return Ok(format!("\x1b[<{code};{x};{y}{final_byte}").into_bytes());
        }

        let mut out = vec![0x1b, b'[', b'M', (32 + code) as u8];
        if mode.contains(TermMode::UTF8_MOUSE) {
            // Mode 1005: each coordinate is a UTF-8 encoded code point.
            let mut buf = [0u8; 4];
            for v in [x, y] {
                let Some(ch) = char::from_u32(32 + v) else {
                    return Ok(Vec::new());
                };
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
        } else {
            // Plain X10: single bytes, so anything past column 223 is
            // unrepresentable.
            if x > 223 || y > 223 {
                return Ok(Vec::new());
            }
            out.push((32 + x) as u8);
            out.push((32 + y) as u8);
        }
        Ok(out)
    }

    fn bracketed_paste_enabled(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }
}

/// Append the cell's grapheme: its base character plus any combining marks,
/// variation selectors or joiners alacritty stacked on it.
fn push_grapheme(text: &mut String, cell: &Cell) {
    text.push(cell.c);
    if let Some(zerowidth) = cell.zerowidth() {
        text.extend(zerowidth);
    }
}

fn cell_data(cell: &Cell) -> CellData {
    let mut grapheme = String::new();
    push_grapheme(&mut grapheme, cell);
    let flags = cell.flags;
    CellData {
        grapheme,
        fg: color_to_string(cell.fg),
        bg: color_to_string(cell.bg),
        bold: flags.contains(Flags::BOLD),
        italic: flags.contains(Flags::ITALIC),
        underline: flags.intersects(Flags::ALL_UNDERLINES),
        strikethrough: flags.contains(Flags::STRIKEOUT),
        inverse: flags.contains(Flags::INVERSE),
        faint: flags.contains(Flags::DIM),
    }
}

/// Same shape as the ghostty backend: `#rrggbb`, `palette:N`, or `None` for
/// "whatever the default is".
fn color_to_string(color: AnsiColor) -> Option<String> {
    match color {
        AnsiColor::Spec(rgb) => Some(format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)),
        AnsiColor::Indexed(idx) => Some(format!("palette:{idx}")),
        // Named colors below 256 are palette entries; the rest (Foreground,
        // Background, Cursor, ...) are defaults with no fixed value.
        AnsiColor::Named(named) => {
            let idx = named as usize;
            (idx < 256).then(|| format!("palette:{idx}"))
        }
    }
}

/// Colors to report when the child queries the palette (OSC 4/10/11).
/// alacritty_terminal leaves the palette to its embedder, so we answer with
/// the standard xterm 256-color table on a dark background.
fn default_color(index: usize) -> Rgb {
    const BASE16: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];

    match index {
        0..=15 => {
            let (r, g, b) = BASE16[index];
            Rgb { r, g, b }
        }
        16..=231 => {
            // 6x6x6 color cube.
            let i = index - 16;
            let level = |v: usize| if v == 0 { 0 } else { (v * 40 + 55) as u8 };
            Rgb {
                r: level(i / 36),
                g: level((i / 6) % 6),
                b: level(i % 6),
            }
        }
        232..=255 => {
            // Grayscale ramp.
            let v = (8 + (index - 232) * 10) as u8;
            Rgb { r: v, g: v, b: v }
        }
        // NamedColor::Background and friends live above 255.
        _ if index == NamedColor::Background as usize => Rgb {
            r: 30,
            g: 30,
            b: 30,
        },
        _ => Rgb {
            r: 220,
            g: 220,
            b: 220,
        },
    }
}

/// Kitty keyboard protocol encoding of a key press. Follows ghostty's encoder
/// so both backends hand the child the same bytes; in particular, the press
/// event type is written out for the `CSI 1;mods:1 X` keys but omitted for
/// `CSI code u` / `CSI code ~` ones, like ghostty does.
fn encode_kitty(spec: &KeySpec, mode: TermMode) -> Result<Vec<u8>> {
    let report_all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    let report_events = mode.contains(TermMode::REPORT_EVENT_TYPES);
    let report_text = mode.contains(TermMode::REPORT_ASSOCIATED_TEXT);
    let mods = spec.mods;

    let (code, final_byte) = match spec.key {
        Key::Char(c) => (c as u32, b'u'),
        Key::Space => (32, b'u'),
        Key::Escape => (27, b'u'),
        Key::Enter => (13, b'u'),
        Key::Tab => (9, b'u'),
        Key::Backspace => (127, b'u'),
        Key::Insert => (2, b'~'),
        Key::Delete => (3, b'~'),
        Key::PageUp => (5, b'~'),
        Key::PageDown => (6, b'~'),
        Key::Up => (1, b'A'),
        Key::Down => (1, b'B'),
        Key::Right => (1, b'C'),
        Key::Left => (1, b'D'),
        Key::Home => (1, b'H'),
        Key::End => (1, b'F'),
        Key::F(n) => match n {
            1 => (1, b'P'),
            2 => (1, b'Q'),
            3 => (13, b'~'),
            4 => (1, b'S'),
            5 => (15, b'~'),
            6 => (17, b'~'),
            7 => (18, b'~'),
            8 => (19, b'~'),
            9 => (20, b'~'),
            10 => (21, b'~'),
            11 => (23, b'~'),
            12 => (24, b'~'),
            _ => bail!("Unknown key: f{n}"),
        },
    };

    // The text an unmodified press produces, if any.
    let text = match spec.key {
        Key::Char(c) if !c.is_control() => Some(c),
        Key::Space => Some(' '),
        _ => None,
    };

    // Unless every key is reported as an escape code, unmodified text keys
    // stay text and enter/tab/backspace keep their legacy bytes so a shell
    // stays usable if a program dies with the mode still on.
    if !report_all && mods.is_empty() {
        match spec.key {
            Key::Enter => return Ok(b"\r".to_vec()),
            Key::Tab => return Ok(b"\t".to_vec()),
            Key::Backspace => return Ok(b"\x7f".to_vec()),
            _ => {}
        }
        if let Some(c) = text {
            return Ok(c.to_string().into_bytes());
        }
    }

    let mods_int = 1
        + u32::from(mods.shift)
        + 2 * u32::from(mods.alt)
        + 4 * u32::from(mods.ctrl)
        + 8 * u32::from(mods.super_key);

    let mut out = String::from("\x1b[");
    if final_byte == b'u' || final_byte == b'~' {
        out.push_str(&code.to_string());
        if mods_int > 1 {
            out.push_str(&format!(";{mods_int}"));
        }
        if report_text
            && mods.is_empty()
            && let Some(c) = text
        {
            out.push_str(&format!(";;{}", c as u32));
        }
    } else if report_events {
        out.push_str(&format!("1;{mods_int}:1"));
    } else if mods_int > 1 {
        out.push_str(&format!("1;{mods_int}"));
    }
    out.push(final_byte as char);
    Ok(out.into_bytes())
}

fn to_termwiz_mods(mods: Mods) -> Modifiers {
    let mut out = Modifiers::NONE;
    if mods.ctrl {
        out |= Modifiers::CTRL;
    }
    if mods.alt {
        out |= Modifiers::ALT;
    }
    if mods.shift {
        out |= Modifiers::SHIFT;
    }
    if mods.super_key {
        out |= Modifiers::SUPER;
    }
    out
}

fn to_termwiz_key(key: Key) -> Result<KeyCode> {
    let k = match key {
        Key::Char(ch) => KeyCode::Char(ch),
        Key::Enter => KeyCode::Enter,
        Key::Tab => KeyCode::Tab,
        Key::Escape => KeyCode::Escape,
        Key::Space => KeyCode::Char(' '),
        Key::Backspace => KeyCode::Backspace,
        Key::Delete => KeyCode::Delete,
        Key::Insert => KeyCode::Insert,
        Key::Up => KeyCode::UpArrow,
        Key::Down => KeyCode::DownArrow,
        Key::Left => KeyCode::LeftArrow,
        Key::Right => KeyCode::RightArrow,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::F(n) => {
            if !(1..=12).contains(&n) {
                bail!("Unknown key: f{n}");
            }
            KeyCode::Function(n)
        }
    };
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(modes: &str) -> AlacrittyBackend {
        let mut term = AlacrittyBackend::new(80, 24, 0).unwrap();
        term.feed(modes.as_bytes());
        term
    }

    fn mouse(action: MouseAction, button: Option<MouseButton>, x: f32, y: f32) -> MouseSpec {
        MouseSpec {
            action,
            button,
            x,
            y,
        }
    }

    fn key(key: Key, mods: Mods) -> KeySpec {
        KeySpec { key, mods }
    }

    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
        super_key: false,
    };
    const SHIFT: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: true,
        super_key: false,
    };

    #[test]
    fn kitty_query_is_answered() {
        let mut term = AlacrittyBackend::new(80, 24, 0).unwrap();
        assert_eq!(term.feed(b"\x1b[>1u\x1b[?u"), b"\x1b[?1u");
    }

    #[test]
    fn kitty_disambiguate_mode() {
        let mut term = backend("\x1b[>1u");
        let none = Mods::default();
        assert_eq!(
            term.encode_key(&key(Key::Escape, none)).unwrap(),
            b"\x1b[27u"
        );
        assert_eq!(term.encode_key(&key(Key::Char('a'), none)).unwrap(), b"a");
        assert_eq!(term.encode_key(&key(Key::Enter, none)).unwrap(), b"\r");
        assert_eq!(term.encode_key(&key(Key::Up, none)).unwrap(), b"\x1b[A");
        assert_eq!(
            term.encode_key(&key(Key::Char('c'), CTRL)).unwrap(),
            b"\x1b[99;5u"
        );
        assert_eq!(term.encode_key(&key(Key::Up, SHIFT)).unwrap(), b"\x1b[1;2A");
        assert_eq!(
            term.encode_key(&key(Key::Tab, SHIFT)).unwrap(),
            b"\x1b[9;2u"
        );

        // Popping the mode restores legacy encoding.
        term.feed(b"\x1b[<u");
        assert_eq!(term.encode_key(&key(Key::Escape, none)).unwrap(), b"\x1b");
    }

    #[test]
    fn kitty_report_all_events_and_text() {
        // disambiguate | report events | report all | report associated text
        let mut term = backend("\x1b[>27u");
        let none = Mods::default();
        assert_eq!(
            term.encode_key(&key(Key::Char('a'), none)).unwrap(),
            b"\x1b[97;;97u"
        );
        assert_eq!(
            term.encode_key(&key(Key::Enter, none)).unwrap(),
            b"\x1b[13u"
        );
        assert_eq!(
            term.encode_key(&key(Key::Up, none)).unwrap(),
            b"\x1b[1;1:1A"
        );
        assert_eq!(
            term.encode_key(&key(Key::Char('c'), CTRL)).unwrap(),
            b"\x1b[99;5u"
        );
    }

    #[test]
    fn combining_characters_stay_attached_to_their_cell() {
        // Decomposed "é" plus a heart with an emoji variation selector.
        let mut term = backend("e\u{301} \u{2764}\u{fe0f}");
        assert!(term.screen_text().starts_with("e\u{301} \u{2764}\u{fe0f} "));
        assert_eq!(term.cell(0, 0).unwrap().grapheme, "e\u{301}");
        assert_eq!(term.cell(2, 0).unwrap().grapheme, "\u{2764}\u{fe0f}");
    }

    #[test]
    fn no_mouse_mode_reports_nothing() {
        let mut term = backend("");
        let press = mouse(MouseAction::Press, Some(MouseButton::Left), 0.0, 0.0);
        assert!(term.encode_mouse(&press).unwrap().is_empty());
    }

    #[test]
    fn sgr_click_reports_press_and_release() {
        let mut term = backend("\x1b[?1000h\x1b[?1006h");
        let press = mouse(MouseAction::Press, Some(MouseButton::Left), 9.0, 4.0);
        let release = mouse(MouseAction::Release, Some(MouseButton::Left), 9.0, 4.0);
        assert_eq!(term.encode_mouse(&press).unwrap(), b"\x1b[<0;10;5M");
        assert_eq!(term.encode_mouse(&release).unwrap(), b"\x1b[<0;10;5m");
    }

    #[test]
    fn x10_release_is_always_button_3() {
        let mut term = backend("\x1b[?1000h");
        let press = mouse(MouseAction::Press, Some(MouseButton::Right), 9.0, 4.0);
        let release = mouse(MouseAction::Release, Some(MouseButton::Right), 9.0, 4.0);
        assert_eq!(term.encode_mouse(&press).unwrap(), b"\x1b[M\"*%");
        assert_eq!(term.encode_mouse(&release).unwrap(), b"\x1b[M#*%");
    }

    #[test]
    fn utf8_mouse_encodes_wide_coordinates_as_code_points() {
        let mut term = backend("\x1b[?1000h\x1b[?1005h");
        // Column 200 -> code point 232 (U+00E8), two bytes in UTF-8.
        let press = mouse(MouseAction::Press, Some(MouseButton::Left), 199.0, 0.0);
        assert_eq!(
            term.encode_mouse(&press).unwrap(),
            [0x1b, b'[', b'M', 32, 0xc3, 0xa8, 33]
        );
    }

    #[test]
    fn motion_is_gated_by_the_tracking_mode() {
        let motion = mouse(MouseAction::Motion, None, 0.0, 0.0);

        // Normal mode: presses only.
        let mut term = backend("\x1b[?1000h\x1b[?1006h");
        assert!(term.encode_mouse(&motion).unwrap().is_empty());

        // Button mode: motion needs a button held.
        let mut term = backend("\x1b[?1002h\x1b[?1006h");
        assert!(term.encode_mouse(&motion).unwrap().is_empty());
        let drag = mouse(MouseAction::Motion, Some(MouseButton::Left), 0.0, 0.0);
        assert_eq!(term.encode_mouse(&drag).unwrap(), b"\x1b[<32;1;1M");

        // Any-motion mode: reported with the "no button" code.
        let mut term = backend("\x1b[?1003h\x1b[?1006h");
        assert_eq!(term.encode_mouse(&motion).unwrap(), b"\x1b[<35;1;1M");
    }
}
