//! # phantom-test
//!
//! Ergonomic Rust library for TUI integration testing.
//! Embeds the phantom terminal emulation engine directly — no daemon process needed.
//!
//! ```rust,no_run
//! use phantom_test::Phantom;
//!
//! let pt = Phantom::new().unwrap();
//! let s = pt.run("bash").args(&["--norc", "--noprofile"]).start().unwrap();
//! s.wait().stable(300).until().unwrap();
//! s.send().type_text("echo hello\n").unwrap();
//! s.wait().text("hello").until().unwrap();
//! let screen = s.screenshot().unwrap();
//! assert!(screen.contains("hello"));
//! ```

mod builder;
mod error;
mod phantom;
mod screen;
mod send;
mod session;
mod wait;

pub use builder::SessionBuilder;
pub use error::{PhantomError, Result};
pub use phantom::Phantom;
pub use screen::Screen;
pub use send::SendBuilder;
pub use session::Session;
pub use wait::WaitBuilder;

// Re-export useful types from phantom-core
pub use phantom_core::types::{
    CellData, CursorInfo, CursorStyle, ScreenContent, SessionInfo, SessionStatus,
};

/// Check if a command is available on the system.
pub fn has_command(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
