mod common;

use common::{TestHarness, assert_error, assert_ok};
use phantom_core::exit_codes;
use phantom_core::types::{SessionStatus, WaitCondition};

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
    assert!(
        names.contains(&"list_a"),
        "should contain list_a: {names:?}"
    );
    assert!(
        names.contains(&"list_b"),
        "should contain list_b: {names:?}"
    );
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
    h.send_type(
        "scroll",
        "for i in $(seq 1 50); do echo \"scrollback_line_$i\"; done\n",
    );
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

// ═══════════════════════════════════════════════════════════
// Output capture
// ═══════════════════════════════════════════════════════════

#[test]
fn test_output_capture() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("output", "bash", &["-c", "echo the_final_output"], 80, 24));

    // Wait for the process to exit
    assert_ok(&h.wait_for_exit("output", 5000));

    // Capture what was written to the primary screen
    let output = h.get_output("output");
    assert!(
        output.contains("the_final_output"),
        "output should contain process stdout: {output}"
    );
}

// ═══════════════════════════════════════════════════════════
// Cell inspection
// ═══════════════════════════════════════════════════════════

#[test]
fn test_cell_inspection() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("cell", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("cell", 300, 5000));

    h.send_type("cell", "echo ABCDEF\n");
    // wait_for_text matches as soon as the typed command is echoed back, which
    // can fire before bash has actually executed `echo` and printed the result
    // line. Wait for the screen to settle so we know both the typed command
    // and the output line are present before screenshotting.
    assert_ok(&h.wait_for_text("cell", "ABCDEF", 5000));
    assert_ok(&h.wait_for_stable("cell", 300, 5000));

    // Find the row with ABCDEF in the screenshot to know the y coordinate
    let text = h.screenshot_text("cell");
    let row = text
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains("ABCDEF") && !l.contains("echo"))
        .map(|(i, _)| i)
        .expect("should find ABCDEF output row");

    // Inspect cell at position of 'A'
    let line = text.lines().nth(row).unwrap();
    let col = line.find('A').unwrap() as u16;
    let cell = h.get_cell("cell", col, row as u16);
    assert_eq!(cell.grapheme, "A", "cell grapheme should be 'A'");

    // Next cell should be 'B'
    let cell = h.get_cell("cell", col + 1, row as u16);
    assert_eq!(cell.grapheme, "B", "next cell should be 'B'");
}

// ═══════════════════════════════════════════════════════════
// Region screenshot
// ═══════════════════════════════════════════════════════════

#[test]
fn test_region_screenshot() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("region", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("region", 300, 5000));

    h.send_type("region", "echo LINE_ONE\n");
    assert_ok(&h.wait_for_text("region", "LINE_ONE", 5000));
    h.send_type("region", "echo LINE_TWO\n");
    assert_ok(&h.wait_for_text("region", "LINE_TWO", 5000));

    // Full screenshot should have all 24 rows
    let full = h.screenshot_json("region");
    assert_eq!(full.screen.len(), 24);

    // Region screenshot: only rows 0-2
    let partial = h.screenshot_region("region", 0, 0, 2, 79);
    // Should have exactly 3 rows
    let non_skipped: Vec<_> = partial
        .screen
        .iter()
        .filter(|r| !r.text.is_empty() || r.row <= 2)
        .collect();
    assert!(
        non_skipped.len() <= 3,
        "region should have at most 3 rows, got {}",
        non_skipped.len()
    );

    // Region with column filter
    let narrow = h.screenshot_region("region", 0, 0, 23, 9);
    for row in &narrow.screen {
        assert!(
            row.text.len() <= 10,
            "narrow region row should be at most 10 chars, got {}: '{}'",
            row.text.len(),
            row.text
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Screen changed wait
// ═══════════════════════════════════════════════════════════

#[test]
fn test_wait_screen_changed() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("changed", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("changed", 300, 5000));

    // Send a command that will produce output
    h.send_type("changed", "echo CHANGE_MARKER\n");

    // Wait for screen to change — should succeed quickly
    let resp = h.wait_for_changed("changed", 5000);
    assert_ok(&resp);

    // The changed text should now be visible
    let text = h.screenshot_text("changed");
    assert!(text.contains("CHANGE_MARKER"));
}

#[test]
fn test_wait_screen_changed_timeout() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("nochange", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("nochange", 300, 5000));

    // Don't send any input — screen won't change
    let resp = h.wait_for_changed("nochange", 500);
    assert_error(&resp, exit_codes::WAIT_TIMEOUT);
}

// ═══════════════════════════════════════════════════════════
// Regex wait
// ═══════════════════════════════════════════════════════════

#[test]
fn test_wait_regex() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("regex", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("regex", 300, 5000));

    h.send_type("regex", "echo 'count: 42 items'\n");
    let resp = h.wait_for_regex("regex", r"count: \d+ items", 5000);
    assert_ok(&resp);
}

#[test]
fn test_wait_regex_no_match() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("regex_no", "bash", &["--norc", "--noprofile"], 80, 24));

    let resp = h.wait_for_regex("regex_no", r"WILL_NEVER_MATCH_\d+", 500);
    assert_error(&resp, exit_codes::WAIT_TIMEOUT);
}

// ═══════════════════════════════════════════════════════════
// Combined wait conditions
// ═══════════════════════════════════════════════════════════

#[test]
fn test_combined_wait_conditions() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("combo", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("combo", 300, 5000));

    h.send_type("combo", "echo ALPHA; echo BETA\n");

    // Wait for BOTH texts to appear
    let resp = h.wait_with_conditions(
        "combo",
        vec![
            WaitCondition::TextPresent("ALPHA".into()),
            WaitCondition::TextPresent("BETA".into()),
        ],
        5000,
    );
    assert_ok(&resp);
}

#[test]
fn test_combined_wait_partial_fail() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("partial", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("partial", 300, 5000));

    h.send_type("partial", "echo PRESENT\n");
    assert_ok(&h.wait_for_text("partial", "PRESENT", 5000));

    // One condition met, one not — should timeout
    let resp = h.wait_with_conditions(
        "partial",
        vec![
            WaitCondition::TextPresent("PRESENT".into()),
            WaitCondition::TextPresent("ABSENT_TEXT".into()),
        ],
        500,
    );
    assert_error(&resp, exit_codes::WAIT_TIMEOUT);
}

// ═══════════════════════════════════════════════════════════
// Session status after exit
// ═══════════════════════════════════════════════════════════

#[test]
fn test_status_after_exit() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("exitst", "bash", &["-c", "exit 42"], 80, 24));

    assert_ok(&h.wait_for_exit("exitst", 5000));

    let resp = h.get_status("exitst");
    match resp {
        phantom_core::protocol::Response::Ok {
            data: Some(phantom_core::protocol::ResponseData::Session(info)),
        } => match info.status {
            SessionStatus::Exited { code } => {
                assert_eq!(code, Some(42), "exit code should be 42");
            }
            _ => panic!("session should be exited"),
        },
        _ => panic!("get_status failed: {resp:?}"),
    }
}

// ═══════════════════════════════════════════════════════════
// Wide characters / Unicode
// ═══════════════════════════════════════════════════════════

#[test]
fn test_wide_characters() {
    let h = TestHarness::new();
    // Use printf with escape sequences — works reliably across shells
    assert_ok(&h.create_session(
        "wide",
        "bash",
        &["-c", "printf '日本語\\n'; sleep 30"],
        80,
        24,
    ));

    assert_ok(&h.wait_for_stable("wide", 500, 10000));

    let text = h.screenshot_text("wide");
    assert!(
        text.contains("日"),
        "screenshot should contain CJK characters: {text}"
    );
}

#[test]
fn test_emoji() {
    let h = TestHarness::new();
    assert_ok(&h.create_session(
        "emoji",
        "bash",
        &["-c", "printf '🎉🚀\\n'; sleep 30"],
        80,
        24,
    ));

    assert_ok(&h.wait_for_stable("emoji", 500, 10000));

    let text = h.screenshot_text("emoji");
    assert!(
        text.contains("🎉") || text.contains("🚀"),
        "screenshot should contain emoji: {text}"
    );
}

// ═══════════════════════════════════════════════════════════
// Snapshot save and diff
// ═══════════════════════════════════════════════════════════

#[test]
fn test_snapshot_save_and_diff() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("snap", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("snap", 300, 5000));

    h.send_type("snap", "echo SNAPSHOT_CONTENT\n");
    assert_ok(&h.wait_for_text("snap", "SNAPSHOT_CONTENT", 5000));

    // Take a reference screenshot
    let ref_text = h.screenshot_text("snap");

    // Take another screenshot — should be identical
    let current = h.screenshot_text("snap");
    assert_eq!(ref_text, current, "consecutive screenshots should match");

    // Now change the screen
    h.send_type("snap", "echo CHANGED\n");
    assert_ok(&h.wait_for_text("snap", "CHANGED", 5000));

    let changed = h.screenshot_text("snap");
    assert_ne!(ref_text, changed, "screen should differ after new output");
}

// ═══════════════════════════════════════════════════════════
// Process exit with specific exit code
// ═══════════════════════════════════════════════════════════

#[test]
fn test_wait_process_exit_with_code() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("exitcode", "bash", &["-c", "exit 7"], 80, 24));

    // Wait for exit with specific code
    let resp = h.wait_with_conditions(
        "exitcode",
        vec![WaitCondition::ProcessExited { exit_code: Some(7) }],
        5000,
    );
    assert_ok(&resp);
}

#[test]
fn test_wait_process_exit_wrong_code() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("wrongcode", "bash", &["-c", "exit 1"], 80, 24));

    // Wait for exit with wrong code — should timeout
    let resp = h.wait_with_conditions(
        "wrongcode",
        vec![WaitCondition::ProcessExited {
            exit_code: Some(99),
        }],
        1000,
    );
    assert_error(&resp, exit_codes::WAIT_TIMEOUT);
}

// ═══════════════════════════════════════════════════════════
// Type with delay
// ═══════════════════════════════════════════════════════════

#[test]
fn test_type_with_delay() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("delay", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("delay", 300, 5000));

    // Type with delay — should still work, just slower
    let resp = h.send_command_raw(|reply| phantom_daemon::engine::EngineCommand::SendInput {
        session: "delay".to_string(),
        action: phantom_core::types::InputAction::Type {
            text: "echo delayed\n".to_string(),
            delay_ms: Some(10),
        },
        reply,
    });
    assert_ok(&resp);
    assert_ok(&h.wait_for_text("delay", "delayed", 5000));
}

// ═══════════════════════════════════════════════════════════
// Cursor wait conditions
// ═══════════════════════════════════════════════════════════

#[test]
fn test_cursor_at_wait() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("curat", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("curat", 300, 5000));

    // Get current cursor position
    let cursor = h.get_cursor("curat");

    // Wait with CursorAt matching the current position — should succeed
    let resp = h.wait_with_conditions(
        "curat",
        vec![WaitCondition::CursorAt {
            x: cursor.x,
            y: cursor.y,
        }],
        5000,
    );
    assert_ok(&resp);
}

#[test]
fn test_cursor_at_wait_wrong_position() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("curwrong", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("curwrong", 300, 5000));

    // Wait for cursor at a position that's definitely wrong — should timeout
    let resp = h.wait_with_conditions(
        "curwrong",
        vec![WaitCondition::CursorAt { x: 79, y: 23 }],
        500,
    );
    assert_error(&resp, exit_codes::WAIT_TIMEOUT);
}

#[test]
fn test_cursor_visible_wait() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("curvis", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("curvis", 300, 5000));

    // Bash cursor should be visible
    let resp = h.wait_with_conditions("curvis", vec![WaitCondition::CursorVisible(true)], 5000);
    assert_ok(&resp);
}

// ═══════════════════════════════════════════════════════════
// Modifier keys
// ═══════════════════════════════════════════════════════════

#[test]
fn test_modifier_keys() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("modkeys", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("modkeys", 300, 5000));

    // Sending modifier key combos should not error
    assert_ok(&h.send_keys("modkeys", &["alt-a"]));
    assert_ok(&h.send_keys("modkeys", &["shift-a"]));
    assert_ok(&h.send_keys("modkeys", &["ctrl-a"]));
}

// ═══════════════════════════════════════════════════════════
// Kill with signal
// ═══════════════════════════════════════════════════════════

#[test]
fn test_kill_with_signal() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("killsig", "sleep", &["999"], 80, 24));

    // Send SIGKILL (signal 9) directly via send_command_raw
    let resp = h.send_command_raw(|reply| phantom_daemon::engine::EngineCommand::KillSession {
        session: "killsig".to_string(),
        signal: Some(9),
        reply,
    });
    assert_ok(&resp);

    // Wait for the process to exit
    assert_ok(&h.wait_for_exit("killsig", 5000));
}

// ═══════════════════════════════════════════════════════════
// Session isolation
// ═══════════════════════════════════════════════════════════

#[test]
fn test_session_isolation() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("iso_a", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.create_session("iso_b", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("iso_a", 300, 5000));
    assert_ok(&h.wait_for_stable("iso_b", 300, 5000));

    // Send output only to iso_a
    h.send_type("iso_a", "echo ONLY_IN_A\n");
    assert_ok(&h.wait_for_text("iso_a", "ONLY_IN_A", 5000));

    // iso_b should NOT contain that text
    let text_b = h.screenshot_text("iso_b");
    assert!(
        !text_b.contains("ONLY_IN_A"),
        "iso_b should not contain iso_a's output, got:\n{text_b}"
    );
}

// ═══════════════════════════════════════════════════════════
// Custom dimensions
// ═══════════════════════════════════════════════════════════

#[test]
fn test_custom_dimensions() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("dims", "bash", &["--norc", "--noprofile"], 40, 10));
    assert_ok(&h.wait_for_stable("dims", 300, 5000));

    h.send_type("dims", "tput cols\n");
    assert_ok(&h.wait_for_text("dims", "40", 5000));

    h.send_type("dims", "tput lines\n");
    assert_ok(&h.wait_for_text("dims", "10", 5000));
}

// ═══════════════════════════════════════════════════════════
// Create session with env
// ═══════════════════════════════════════════════════════════

#[test]
fn test_create_session_with_env() {
    let h = TestHarness::new();

    // Create session with custom env var via send_command_raw
    let resp = h.send_command_raw(
        |reply| phantom_daemon::engine::EngineCommand::CreateSession {
            name: "envtest".to_string(),
            command: "bash".to_string(),
            args: vec!["--norc".to_string(), "--noprofile".to_string()],
            env: vec![
                ("LANG".into(), "C".into()),
                ("LC_ALL".into(), "C".into()),
                ("PHANTOM_TEST_VAR".into(), "hello123".into()),
            ],
            cwd: None,
            cols: 80,
            rows: 24,
            scrollback: 1000,
            reply,
        },
    );
    assert_ok(&resp);
    assert_ok(&h.wait_for_stable("envtest", 300, 5000));

    h.send_type("envtest", "echo $PHANTOM_TEST_VAR\n");
    assert_ok(&h.wait_for_text("envtest", "hello123", 5000));
}

// ═══════════════════════════════════════════════════════════
// Create session with cwd
// ═══════════════════════════════════════════════════════════

#[test]
fn test_create_session_with_cwd() {
    let h = TestHarness::new();

    // Create session with cwd set to /tmp
    let resp = h.send_command_raw(
        |reply| phantom_daemon::engine::EngineCommand::CreateSession {
            name: "cwdtest".to_string(),
            command: "bash".to_string(),
            args: vec!["--norc".to_string(), "--noprofile".to_string()],
            env: vec![("LANG".into(), "C".into()), ("LC_ALL".into(), "C".into())],
            cwd: Some("/tmp".to_string()),
            cols: 80,
            rows: 24,
            scrollback: 1000,
            reply,
        },
    );
    assert_ok(&resp);
    assert_ok(&h.wait_for_stable("cwdtest", 300, 5000));

    h.send_type("cwdtest", "pwd\n");
    // On macOS /tmp is a symlink to /private/tmp, so check for either
    let wait_resp = h.wait_for_regex("cwdtest", r"(/private)?/tmp", 5000);
    assert_ok(&wait_resp);
}

// ═══════════════════════════════════════════════════════════
// Invalid input specs
// ═══════════════════════════════════════════════════════════

#[test]
fn test_invalid_key_spec() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("badkey", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("badkey", 300, 5000));

    let resp = h.send_keys("badkey", &["not-a-real-key"]);
    assert!(
        matches!(resp, phantom_core::protocol::Response::Error { .. }),
        "invalid key spec should return an error, got: {resp:?}"
    );
}

#[test]
fn test_invalid_mouse_spec() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("badmouse", "bash", &["--norc", "--noprofile"], 80, 24));
    assert_ok(&h.wait_for_stable("badmouse", 300, 5000));

    let resp = h.send_mouse("badmouse", "invalid");
    assert!(
        matches!(resp, phantom_core::protocol::Response::Error { .. }),
        "invalid mouse spec should return an error, got: {resp:?}"
    );
}

// ═══════════════════════════════════════════════════════════
// Send to exited session
// ═══════════════════════════════════════════════════════════

#[test]
fn test_send_to_exited_session() {
    let h = TestHarness::new();
    assert_ok(&h.create_session("exited", "bash", &["-c", "exit 0"], 80, 24));

    // Wait for the process to exit
    assert_ok(&h.wait_for_exit("exited", 5000));

    // Sending to an exited session: behavior is platform-dependent.
    // On macOS the PTY write fails immediately; on Linux the kernel may
    // buffer the write and return Ok. Either outcome is acceptable —
    // the important thing is that it doesn't panic.
    let _resp = h.send_type("exited", "echo should_fail\n");
}
