mod common;

use common::{TestHarness, assert_ok, has_command};

/// vim args for predictable behavior. `-u NONE` disables all config.
/// Note: this starts vim in Vi-compatible mode (no "-- INSERT --" shown).
const VIM_ARGS: &[&str] = &["--clean", "-u", "NONE"];

#[test]
fn test_vim_startup() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let h = TestHarness::new();
    assert_ok(&h.create_session("vim_start", "vim", VIM_ARGS, 80, 24));

    // Wait for vim to render — look for tilde lines (empty buffer indicator)
    assert_ok(&h.wait_for_text("vim_start", "~", 10000));

    let text = h.screenshot_text("vim_start");
    let tilde_count = text.lines().filter(|l| l.starts_with('~')).count();
    assert!(
        tilde_count > 5,
        "vim should show many ~ lines for empty buffer, got {tilde_count}:\n{text}"
    );

    h.kill_session("vim_start");
}

#[test]
fn test_vim_insert_and_type() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let h = TestHarness::new();
    assert_ok(&h.create_session("vim_ins", "vim", VIM_ARGS, 80, 24));
    assert_ok(&h.wait_for_text("vim_ins", "~", 10000));

    // Enter insert mode and type text (raw bytes — vim interprets 'i' as insert mode command)
    h.send_type("vim_ins", "i");
    std::thread::sleep(std::time::Duration::from_millis(300));

    h.send_type("vim_ins", "Hello from phantom");
    assert_ok(&h.wait_for_text("vim_ins", "Hello from phantom", 5000));

    let text = h.screenshot_text("vim_ins");
    assert!(
        text.contains("Hello from phantom"),
        "screen should show typed text:\n{text}"
    );

    // Escape back to normal mode
    h.send_keys("vim_ins", &["escape"]);
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Type dd to delete the line — proves we're in normal mode
    h.send_type("vim_ins", "dd");
    std::thread::sleep(std::time::Duration::from_millis(300));

    let text = h.screenshot_text("vim_ins");
    assert!(
        !text.contains("Hello from phantom"),
        "dd should have deleted the line:\n{text}"
    );

    h.kill_session("vim_ins");
}

#[test]
fn test_vim_navigation() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let h = TestHarness::new();
    assert_ok(&h.create_session("vim_nav", "vim", VIM_ARGS, 80, 24));
    assert_ok(&h.wait_for_text("vim_nav", "~", 10000));

    // Insert multiple lines
    h.send_type("vim_nav", "iLine one\nLine two\nLine three");
    assert_ok(&h.wait_for_text("vim_nav", "Line three", 5000));
    h.send_keys("vim_nav", &["escape"]);
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Cursor should be on line 3. Go to line 1 with gg.
    h.send_type("vim_nav", "gg");
    std::thread::sleep(std::time::Duration::from_millis(200));

    let cursor = h.get_cursor("vim_nav");
    assert_eq!(cursor.y, 0, "gg should put cursor on first line");

    h.kill_session("vim_nav");
}

#[test]
fn test_vim_command_mode() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let h = TestHarness::new();
    assert_ok(&h.create_session("vim_cmd", "vim", VIM_ARGS, 80, 24));
    assert_ok(&h.wait_for_text("vim_cmd", "~", 10000));

    // Enable line numbers via ex command
    h.send_type("vim_cmd", ":set number\n");
    assert_ok(&h.wait_for_stable("vim_cmd", 500, 5000));

    let text = h.screenshot_text("vim_cmd");
    // With :set number, first line should show "  1 " prefix
    assert!(
        text.contains("  1 "),
        "screen should show line numbers:\n{text}"
    );

    h.kill_session("vim_cmd");
}

#[test]
fn test_vim_quit() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let h = TestHarness::new();
    assert_ok(&h.create_session("vim_quit", "vim", VIM_ARGS, 80, 24));
    assert_ok(&h.wait_for_text("vim_quit", "~", 10000));

    // Quit vim via ex command
    h.send_type("vim_quit", ":q!\n");
    let resp = h.wait_for_exit("vim_quit", 5000);
    assert_ok(&resp);
}
