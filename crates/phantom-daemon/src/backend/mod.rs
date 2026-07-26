//! Terminal backend abstraction.
//!
//! Everything that talks to a terminal emulation library sits behind
//! [`TerminalBackend`]. The trait only deals in `phantom_core::types`, which
//! are the same types the daemon puts on the wire — so the engine, the
//! protocol and the CLI stay independent of which emulator is compiled in.
//!
//! Exactly one backend is selected at compile time via cargo features and
//! exposed as [`DefaultBackend`].

use anyhow::Result;
use phantom_core::types::{CellData, CursorInfo, ScreenContent, ScreenFormat};

#[cfg(feature = "alacritty")]
pub mod alacritty;
#[cfg(feature = "ghostty")]
pub mod ghostty;

// Cargo features are additive, so both backends can end up enabled at once.
// ghostty wins in that case — it's the higher-fidelity one.
#[cfg(feature = "ghostty")]
pub type DefaultBackend = ghostty::GhosttyBackend;

#[cfg(all(feature = "alacritty", not(feature = "ghostty")))]
pub type DefaultBackend = alacritty::AlacrittyBackend;

#[cfg(not(any(feature = "ghostty", feature = "alacritty")))]
compile_error!(
    "phantom-daemon needs a terminal backend — enable `ghostty` (needs Zig) or `alacritty` (pure Rust)"
);

/// Screen region as `(top, left, bottom, right)`, 0-indexed and inclusive.
/// Same convention as `phantom_core::protocol::Request::Screenshot::region`.
pub type Region = (u16, u16, u16, u16);

/// A key, independent of any backend's key enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character key (letters and digits).
    Char(char),
    Enter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// Function key, 1-indexed.
    F(u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl Mods {
    pub fn is_empty(&self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.super_key)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KeySpec {
    pub key: Key,
    pub mods: Mods,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseSpec {
    pub action: MouseAction,
    pub button: Option<MouseButton>,
    pub x: f32,
    pub y: f32,
}

/// A terminal emulator phantom can drive.
///
/// Implementors own the screen state for one session. The PTY itself is not
/// their concern — [`Session`](crate::session::Session) feeds them bytes and
/// writes back whatever they produce.
pub trait TerminalBackend {
    fn new(cols: u16, rows: u16, scrollback: u32) -> Result<Self>
    where
        Self: Sized;

    /// Feed bytes read from the PTY into the emulator.
    ///
    /// Returns any bytes the terminal wants sent back to the child — device
    /// attribute replies, cursor position reports and friends. The caller is
    /// responsible for writing them to the PTY.
    fn feed(&mut self, data: &[u8]) -> Vec<u8>;

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()>;

    /// Capture the active screen. Per-cell attributes are only filled in for
    /// [`ScreenFormat::Json`]; other formats just need the text.
    fn capture(&mut self, format: &ScreenFormat, region: Option<Region>) -> Result<ScreenContent>;

    /// The active screen as plain text, one line per row, rows joined by `\n`.
    /// Cells with no grapheme content render as a space.
    fn screen_text(&mut self) -> String;

    fn cell(&self, x: u16, y: u16) -> Result<CellData>;

    /// Scrollback lines above the viewport, oldest first, trailing whitespace
    /// trimmed. `max_lines` keeps only the last N lines.
    fn scrollback(&self, max_lines: Option<u32>) -> Result<Vec<String>>;

    fn cursor(&self) -> CursorInfo;

    fn title(&self) -> Option<String>;

    fn pwd(&self) -> Option<String>;

    /// Encode a key press for the child process. Encoding depends on terminal
    /// state (application cursor keys, kitty keyboard protocol, ...), which is
    /// why it belongs to the backend rather than to `input`.
    fn encode_key(&mut self, spec: &KeySpec) -> Result<Vec<u8>>;

    fn encode_mouse(&mut self, spec: &MouseSpec) -> Result<Vec<u8>>;

    fn bracketed_paste_enabled(&self) -> bool;
}
