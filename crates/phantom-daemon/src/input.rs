use anyhow::{Result, bail};
use libghostty_vt::key::{Action, Key, Mods};
use libghostty_vt::mouse;

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

/// Send a key sequence encoded via libghostty's key encoder.
pub fn send_key(session: &mut Session, key_spec: &str) -> Result<()> {
    let (key, mods) = parse_key_spec(key_spec)?;

    session
        .key_encoder
        .set_options_from_terminal(&session.terminal);

    let mut event = libghostty_vt::key::Event::new()?;
    event.set_key(key).set_mods(mods).set_action(Action::Press);

    // Set UTF-8 codepoint for character keys without modifiers
    if mods.is_empty()
        && let Some(ch) = key_to_char(key)
    {
        event.set_utf8(Some(ch.to_string()));
    }

    let mut buf = Vec::new();
    session.key_encoder.encode_to_vec(&event, &mut buf)?;
    if !buf.is_empty() {
        session.pty.write(&buf)?;
    }
    Ok(())
}

/// Send bracketed paste. Only wraps with escape sequences if the terminal
/// has bracketed paste mode enabled, otherwise sends raw text.
pub fn paste(session: &mut Session, text: &str) -> Result<()> {
    let bracketed = session
        .terminal
        .mode(libghostty_vt::terminal::Mode::BRACKETED_PASTE)
        .unwrap_or(false);

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
        "click" | "left-click" => (mouse::Action::Press, Some(mouse::Button::Left)),
        "right-click" => (mouse::Action::Press, Some(mouse::Button::Right)),
        "middle-click" => (mouse::Action::Press, Some(mouse::Button::Middle)),
        "scroll-up" => (mouse::Action::Press, Some(mouse::Button::Four)),
        "scroll-down" => (mouse::Action::Press, Some(mouse::Button::Five)),
        "move" => (mouse::Action::Motion, None),
        "release" => (mouse::Action::Release, None),
        other => bail!("Unknown mouse action: {other}"),
    };

    session
        .mouse_encoder
        .set_options_from_terminal(&session.terminal);

    let mut event = mouse::Event::new()?;
    event
        .set_action(action)
        .set_button(button)
        .set_position(mouse::Position { x, y })
        .set_mods(Mods::empty());

    let mut buf = Vec::new();
    session.mouse_encoder.encode_to_vec(&event, &mut buf)?;
    if !buf.is_empty() {
        session.pty.write(&buf)?;
    }

    // For click actions, also send a release event
    if matches!(action, mouse::Action::Press) && !matches!(action_str, "scroll-up" | "scroll-down")
    {
        let mut release = mouse::Event::new()?;
        release
            .set_action(mouse::Action::Release)
            .set_button(button)
            .set_position(mouse::Position { x, y })
            .set_mods(Mods::empty());

        let mut buf = Vec::new();
        session.mouse_encoder.encode_to_vec(&release, &mut buf)?;
        if !buf.is_empty() {
            session.pty.write(&buf)?;
        }
    }

    Ok(())
}

fn parse_key_spec(spec: &str) -> Result<(Key, Mods)> {
    let parts: Vec<&str> = spec.split('-').collect();
    let mut mods = Mods::empty();

    let key_str = if parts.len() == 1 {
        parts[0]
    } else {
        for &modifier in &parts[..parts.len() - 1] {
            match modifier.to_lowercase().as_str() {
                "ctrl" | "c" => mods |= Mods::CTRL,
                "alt" | "a" | "meta" | "m" => mods |= Mods::ALT,
                "shift" | "s" => mods |= Mods::SHIFT,
                "super" => mods |= Mods::SUPER,
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
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "insert" | "ins" => Key::Insert,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        s if s.len() == 1 => {
            let ch = s.chars().next().unwrap();
            char_to_key(ch).ok_or_else(|| anyhow::anyhow!("Unknown key: {s}"))?
        }
        other => bail!("Unknown key: {other}"),
    };

    Ok((key, mods))
}

fn char_to_key(ch: char) -> Option<Key> {
    match ch {
        'a' => Some(Key::A),
        'b' => Some(Key::B),
        'c' => Some(Key::C),
        'd' => Some(Key::D),
        'e' => Some(Key::E),
        'f' => Some(Key::F),
        'g' => Some(Key::G),
        'h' => Some(Key::H),
        'i' => Some(Key::I),
        'j' => Some(Key::J),
        'k' => Some(Key::K),
        'l' => Some(Key::L),
        'm' => Some(Key::M),
        'n' => Some(Key::N),
        'o' => Some(Key::O),
        'p' => Some(Key::P),
        'q' => Some(Key::Q),
        'r' => Some(Key::R),
        's' => Some(Key::S),
        't' => Some(Key::T),
        'u' => Some(Key::U),
        'v' => Some(Key::V),
        'w' => Some(Key::W),
        'x' => Some(Key::X),
        'y' => Some(Key::Y),
        'z' => Some(Key::Z),
        '0' => Some(Key::Digit0),
        '1' => Some(Key::Digit1),
        '2' => Some(Key::Digit2),
        '3' => Some(Key::Digit3),
        '4' => Some(Key::Digit4),
        '5' => Some(Key::Digit5),
        '6' => Some(Key::Digit6),
        '7' => Some(Key::Digit7),
        '8' => Some(Key::Digit8),
        '9' => Some(Key::Digit9),
        _ => None,
    }
}

fn key_to_char(key: Key) -> Option<char> {
    match key {
        Key::A => Some('a'),
        Key::B => Some('b'),
        Key::C => Some('c'),
        Key::D => Some('d'),
        Key::E => Some('e'),
        Key::F => Some('f'),
        Key::G => Some('g'),
        Key::H => Some('h'),
        Key::I => Some('i'),
        Key::J => Some('j'),
        Key::K => Some('k'),
        Key::L => Some('l'),
        Key::M => Some('m'),
        Key::N => Some('n'),
        Key::O => Some('o'),
        Key::P => Some('p'),
        Key::Q => Some('q'),
        Key::R => Some('r'),
        Key::S => Some('s'),
        Key::T => Some('t'),
        Key::U => Some('u'),
        Key::V => Some('v'),
        Key::W => Some('w'),
        Key::X => Some('x'),
        Key::Y => Some('y'),
        Key::Z => Some('z'),
        Key::Space => Some(' '),
        Key::Enter => Some('\r'),
        Key::Tab => Some('\t'),
        _ => None,
    }
}
