mod common;

use common::{TestHarness, assert_error, assert_ok};
use phantom_core::exit_codes;

#[test]
fn test_echo_and_screenshot() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("echo", "bash", &["--norc", "--noprofile"], 80, 24));

    // Wait for bash to be ready, then echo
    assert_ok(&h.wait_for_stable("echo", 300, 5000));
    h.send_type("echo", "echo hello_phantom\n");
    assert_ok(&h.wait_for_text("echo", "hello_phantom", 5000));

    let text = h.screenshot_text("echo");
    assert!(
        text.contains("hello_phantom"),
        "screenshot should contain 'hello_phantom', got:\n{text}"
    );
}

#[test]
fn test_wait_text_present_delayed() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("delay", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("delay", 300, 5000));

    h.send_type("delay", "sleep 1 && echo MARKER_42\n");
    let resp = h.wait_for_text("delay", "MARKER_42", 5000);
    assert_ok(&resp);
}

#[test]
fn test_wait_text_absent() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("absent", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("absent", 300, 5000));

    h.send_type("absent", "echo TEMP_TEXT\n");
    assert_ok(&h.wait_for_text("absent", "TEMP_TEXT", 5000));

    h.send_type("absent", "clear\n");
    let resp = h.wait_for_text_absent("absent", "TEMP_TEXT", 5000);
    assert_ok(&resp);
}

#[test]
fn test_wait_timeout() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("timeout", "bash", &["--norc", "--noprofile"], 80, 24));

    let resp = h.wait_for_text("timeout", "THIS_WILL_NEVER_APPEAR", 500);
    assert_error(&resp, exit_codes::WAIT_TIMEOUT);
}

#[test]
fn test_paste_input() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("paste", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("paste", 300, 5000));

    h.send_paste("paste", "echo pasted_text\n");
    assert_ok(&h.wait_for_text("paste", "pasted_text", 5000));
}

#[test]
fn test_send_key_ctrl_c() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("ctrlc", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("ctrlc", 300, 5000));

    // Start a long-running command
    h.send_type("ctrlc", "sleep 999\n");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Ctrl-C should interrupt it
    h.send_keys("ctrlc", &["ctrl-c"]);

    // Bash should still be running (ctrl-c kills the child, not bash)
    assert_ok(&h.wait_for_stable("ctrlc", 500, 5000));

    // Verify we're back at a prompt by typing a command
    h.send_type("ctrlc", "echo back_at_prompt\n");
    assert_ok(&h.wait_for_text("ctrlc", "back_at_prompt", 5000));
}

#[test]
fn test_cursor_position() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("cursor", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("cursor", 300, 5000));

    let cursor = h.get_cursor("cursor");
    // Cursor should be at a valid position (somewhere on the prompt line)
    assert!(cursor.x < 80, "cursor x={} should be < 80", cursor.x);
    assert!(cursor.y < 24, "cursor y={} should be < 24", cursor.y);
    assert!(cursor.visible, "cursor should be visible");
}

#[test]
fn test_resize() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("resize", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("resize", 300, 5000));

    h.send_type("resize", "tput cols\n");
    assert_ok(&h.wait_for_text("resize", "80", 5000));

    assert_ok(&h.resize("resize", 120, 40));
    std::thread::sleep(std::time::Duration::from_millis(200));

    h.send_type("resize", "tput cols\n");
    assert_ok(&h.wait_for_text("resize", "120", 5000));
}

#[test]
fn test_session_not_found() {
    let h = TestHarness::new();
    let resp = h.get_status("nonexistent");
    assert_error(&resp, exit_codes::SESSION_NOT_FOUND);
}

#[test]
fn test_session_collision() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("dup", "bash", &["--norc", "--noprofile"], 80, 24));

    let resp = h.create_session("dup", "bash", &["--norc", "--noprofile"], 80, 24);
    assert_error(&resp, exit_codes::SESSION_COLLISION);
}

#[test]
fn test_list_sessions() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("list_a", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.create_session("list_b", "bash", &["--norc", "--noprofile"], 80, 24));

    let sessions = h.list_sessions();
    let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"list_a"), "should contain list_a: {names:?}");
    assert!(names.contains(&"list_b"), "should contain list_b: {names:?}");
}

#[test]
fn test_kill_session() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("killme", "sleep", &["999"], 80, 24));

    assert_ok(&h.kill_session("killme"));
    let resp = h.wait_for_exit("killme", 5000);
    assert_ok(&resp);
}

#[test]
fn test_screen_stable_condition() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("stable", "bash", &["--norc", "--noprofile"], 80, 24));

    // After initial rendering settles, screen should be stable
    let resp = h.wait_for_stable("stable", 500, 10000);
    assert_ok(&resp);
}

#[test]
fn test_json_screenshot_has_cell_data() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("json", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("json", 300, 5000));

    h.send_type("json", "echo hello\n");
    assert_ok(&h.wait_for_text("json", "hello", 5000));

    let screen = h.screenshot_json("json");
    assert_eq!(screen.cols, 80);
    assert_eq!(screen.rows, 24);
    assert!(!screen.screen.is_empty(), "should have rows");

    // JSON format should include cell data
    let non_empty_row = screen.screen.iter().find(|r| !r.text.trim().is_empty());
    assert!(non_empty_row.is_some(), "should have a non-empty row");
    let row = non_empty_row.unwrap();
    assert!(!row.cells.is_empty(), "JSON format should include cells");
}

#[test]
fn test_scrollback() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("scroll", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("scroll", 300, 5000));

    // Generate enough output to push content into scrollback (> 24 lines)
    h.send_type("scroll", "for i in $(seq 1 50); do echo \"scrollback_line_$i\"; done\n");
    assert_ok(&h.wait_for_text("scroll", "scrollback_line_50", 5000));

    // The first lines should have scrolled off the visible screen into scrollback
    let text = h.screenshot_text("scroll");
    assert!(
        !text.contains("scrollback_line_1\n"),
        "line 1 should have scrolled off the visible screen"
    );

    // But scrollback should contain them
    let scrollback = h.get_scrollback("scroll", None);
    assert!(
        scrollback.contains("scrollback_line_1"),
        "scrollback should contain line 1:\n{scrollback}"
    );

    // Test limited scrollback retrieval
    let limited = h.get_scrollback("scroll", Some(5));
    let line_count = limited.lines().filter(|l| !l.is_empty()).count();
    assert!(
        line_count <= 5,
        "limited scrollback should have at most 5 non-empty lines, got {line_count}"
    );
}

#[test]
fn test_mouse_input() {
    let h = TestHarness::new();
    // Mouse encoding requires the terminal to have mouse tracking enabled.
    // In a plain bash session, mouse events won't be processed, but we can
    // verify the encoding doesn't error out.
    assert_ok(&h.create_session("mouse", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("mouse", 300, 5000));

    // These should not error even though bash ignores mouse events
    assert_ok(&h.send_mouse("mouse", "click:10,5"));
    assert_ok(&h.send_mouse("mouse", "right-click:20,10"));
    assert_ok(&h.send_mouse("mouse", "scroll-up:5,5"));
    assert_ok(&h.send_mouse("mouse", "scroll-down:5,5"));
    assert_ok(&h.send_mouse("mouse", "move:15,8"));
}
