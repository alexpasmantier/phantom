//! Input handling: parse the user-facing key and mouse specs into
//! backend-neutral events, then let the backend encode them for the child.

use anyhow::{Result, bail};

use crate::backend::{Key, KeySpec, Mods, MouseAction, MouseButton, MouseSpec, TerminalBackend};
use crate::session::Session;

/// Send typed text to the session, character by character.
pub fn type_text(session: &mut Session, text: &str, delay_ms: Option<u64>) -> Result<()> {
    for ch in text.chars() {
        let bytes = ch.to_string().into_bytes();
        session.pty.write(&bytes)?;
        if let Some(delay) = delay_ms
            && delay > 0
        {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
    }
    Ok(())
}

/// Send a key sequence, encoded by the backend against current terminal modes.
pub fn send_key(session: &mut Session, key_spec: &str) -> Result<()> {
    let spec = parse_key_spec(key_spec)?;
    let bytes = session.term.encode_key(&spec)?;
    if !bytes.is_empty() {
        session.pty.write(&bytes)?;
    }
    Ok(())
}

/// Send bracketed paste. Only wraps with escape sequences if the terminal
/// has bracketed paste mode enabled, otherwise sends raw text.
pub fn paste(session: &mut Session, text: &str) -> Result<()> {
    let bracketed = session.term.bracketed_paste_enabled();

    if bracketed {
        session.pty.write(b"\x1b[200~")?;
    }
    session.pty.write(text.as_bytes())?;
    if bracketed {
        session.pty.write(b"\x1b[201~")?;
    }
    Ok(())
}

/// Send a mouse event.
/// Specs: `click:x,y`, `right-click:x,y`, `middle-click:x,y`,
///        `scroll-up:x,y`, `scroll-down:x,y`, `move:x,y`
pub fn send_mouse(session: &mut Session, spec: &str) -> Result<()> {
    let event = parse_mouse_spec(spec)?;

    let bytes = session.term.encode_mouse(&event)?;
    if !bytes.is_empty() {
        session.pty.write(&bytes)?;
    }

    // For click actions, also send a release event
    let is_scroll = matches!(
        event.button,
        Some(MouseButton::ScrollUp | MouseButton::ScrollDown)
    );
    if event.action == MouseAction::Press && !is_scroll {
        let release = MouseSpec {
            action: MouseAction::Release,
            ..event
        };
        let bytes = session.term.encode_mouse(&release)?;
        if !bytes.is_empty() {
            session.pty.write(&bytes)?;
        }
    }

    Ok(())
}

fn parse_mouse_spec(spec: &str) -> Result<MouseSpec> {
    let (action_str, coords_str) = spec
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("Mouse spec must be action:x,y (e.g. click:10,5)"))?;

    let coords: Vec<&str> = coords_str.split(',').collect();
    if coords.len() != 2 {
        bail!("Mouse coordinates must be x,y");
    }
    let x: f32 = coords[0].parse()?;
    let y: f32 = coords[1].parse()?;

    let (action, button) = match action_str {
        "click" | "left-click" => (MouseAction::Press, Some(MouseButton::Left)),
        "right-click" => (MouseAction::Press, Some(MouseButton::Right)),
        "middle-click" => (MouseAction::Press, Some(MouseButton::Middle)),
        "scroll-up" => (MouseAction::Press, Some(MouseButton::ScrollUp)),
        "scroll-down" => (MouseAction::Press, Some(MouseButton::ScrollDown)),
        "move" => (MouseAction::Motion, None),
        "release" => (MouseAction::Release, None),
        other => bail!("Unknown mouse action: {other}"),
    };

    Ok(MouseSpec {
        action,
        button,
        x,
        y,
    })
}

fn parse_key_spec(spec: &str) -> Result<KeySpec> {
    let parts: Vec<&str> = spec.split('-').collect();
    let mut mods = Mods::default();

    let key_str = if parts.len() == 1 {
        parts[0]
    } else {
        for &modifier in &parts[..parts.len() - 1] {
            match modifier.to_lowercase().as_str() {
                "ctrl" | "c" => mods.ctrl = true,
                "alt" | "a" | "meta" | "m" => mods.alt = true,
                "shift" | "s" => mods.shift = true,
                "super" => mods.super_key = true,
                _ => bail!("Unknown modifier: {modifier}"),
            }
        }
        parts[parts.len() - 1]
    };

    let key = match key_str.to_lowercase().as_str() {
        "enter" | "return" | "cr" => Key::Enter,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        "backspace" | "bs" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "insert" | "ins" => Key::Insert,
        s if s.len() > 1 && s.starts_with('f') => {
            let n: u8 = s[1..]
                .parse()
                .map_err(|_| anyhow::anyhow!("Unknown key: {s}"))?;
            if !(1..=12).contains(&n) {
                bail!("Unknown key: {s}");
            }
            Key::F(n)
        }
        s if s.chars().count() == 1 => Key::Char(s.chars().next().unwrap()),
        other => bail!("Unknown key: {other}"),
    };

    Ok(KeySpec { key, mods })
}
