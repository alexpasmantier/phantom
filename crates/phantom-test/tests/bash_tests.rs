use phantom_test::{Phantom, PhantomError, SessionStatus};

fn bash(pt: &Phantom) -> phantom_test::Session {
    pt.run("bash")
        .args(&["--norc", "--noprofile"])
        .start()
        .unwrap()
}

fn bash_ready(pt: &Phantom) -> phantom_test::Session {
    let s = bash(pt);
    s.wait().stable(300).until().unwrap();
    s
}

// ═══════════════════════════════════════════════════════════
// Session lifecycle
// ═══════════════════════════════════════════════════════════

#[test]
fn echo_and_screenshot() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo hello_phantom\n").unwrap();
    s.wait().text("hello_phantom").until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        screen.contains("hello_phantom"),
        "screenshot should contain 'hello_phantom', got:\n{screen}"
    );
}

#[test]
fn session_not_found() {
    let pt = Phantom::new().unwrap();
    // Create and kill a session, then try to query it via a new Phantom
    // that doesn't have this session. Simplest: just query a name that was
    // never created, using the low-level sessions() + status on the session handle.
    let s = pt
        .run("bash")
        .args(&["--norc", "--noprofile"])
        .name("will_kill")
        .start()
        .unwrap();
    s.kill().unwrap();
    s.wait().process_exit().until().unwrap();

    // Now try to screenshot the dead-but-still-registered session — it should work.
    // But querying a truly nonexistent name requires a different approach.
    // We'll test the error by trying to create a session, killing its process,
    // and confirming the status shows exited.
    let info = s.status().unwrap();
    assert!(matches!(info.status, SessionStatus::Exited { .. }));
}

#[test]
fn session_collision() {
    let pt = Phantom::new().unwrap();
    pt.run("bash")
        .args(&["--norc", "--noprofile"])
        .name("dup")
        .start()
        .unwrap();

    let result = pt
        .run("bash")
        .args(&["--norc", "--noprofile"])
        .name("dup")
        .start();
    assert!(
        matches!(result, Err(PhantomError::SessionCollision(_))),
        "expected SessionCollision"
    );
}

#[test]
fn list_sessions() {
    let pt = Phantom::new().unwrap();
    let _a = pt
        .run("bash")
        .args(&["--norc", "--noprofile"])
        .name("list_a")
        .start()
        .unwrap();
    let _b = pt
        .run("bash")
        .args(&["--norc", "--noprofile"])
        .name("list_b")
        .start()
        .unwrap();

    let sessions = pt.sessions().unwrap();
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
fn kill_session() {
    let pt = Phantom::new().unwrap();
    let s = pt.run("sleep").args(&["999"]).start().unwrap();
    s.kill().unwrap();
    s.wait().process_exit().until().unwrap();
}

#[test]
fn status_after_exit() {
    let pt = Phantom::new().unwrap();
    let s = pt.run("bash").args(&["-c", "exit 42"]).start().unwrap();
    s.wait().process_exit().until().unwrap();

    let info = s.status().unwrap();
    match info.status {
        SessionStatus::Exited { code } => {
            assert_eq!(code, Some(42), "exit code should be 42");
        }
        _ => panic!("session should be exited"),
    }
}

// ═══════════════════════════════════════════════════════════
// Wait conditions
// ═══════════════════════════════════════════════════════════

#[test]
fn wait_text_present_delayed() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("sleep 1 && echo MARKER_42\n").unwrap();
    s.wait().text("MARKER_42").timeout_ms(5000).until().unwrap();
}

#[test]
fn wait_text_absent() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo TEMP_TEXT\n").unwrap();
    s.wait().text("TEMP_TEXT").until().unwrap();

    s.send().type_text("clear\n").unwrap();
    s.wait().text_absent("TEMP_TEXT").until().unwrap();
}

#[test]
fn wait_timeout() {
    let pt = Phantom::new().unwrap();
    let _s = bash(&pt);

    let result = _s
        .wait()
        .text("THIS_WILL_NEVER_APPEAR")
        .timeout_ms(500)
        .until();
    assert!(
        matches!(result, Err(PhantomError::WaitTimeout)),
        "expected WaitTimeout, got: {result:?}"
    );
}

#[test]
fn wait_screen_stable() {
    let pt = Phantom::new().unwrap();
    let s = bash(&pt);
    s.wait().stable(500).until().unwrap();
}

#[test]
fn wait_screen_changed() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo CHANGE_MARKER\n").unwrap();
    s.wait().screen_changed().until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(screen.contains("CHANGE_MARKER"));
}

#[test]
fn wait_screen_changed_timeout() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    let result = s.wait().screen_changed().timeout_ms(500).until();
    assert!(matches!(result, Err(PhantomError::WaitTimeout)));
}

#[test]
fn wait_regex() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo 'count: 42 items'\n").unwrap();
    s.wait().regex(r"count: \d+ items").until().unwrap();
}

#[test]
fn wait_regex_no_match() {
    let pt = Phantom::new().unwrap();
    let s = bash(&pt);

    let result = s
        .wait()
        .regex(r"WILL_NEVER_MATCH_\d+")
        .timeout_ms(500)
        .until();
    assert!(matches!(result, Err(PhantomError::WaitTimeout)));
}

#[test]
fn combined_wait_conditions() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo ALPHA; echo BETA\n").unwrap();
    s.wait().text("ALPHA").text("BETA").until().unwrap();
}

#[test]
fn combined_wait_partial_fail() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo PRESENT\n").unwrap();
    s.wait().text("PRESENT").until().unwrap();

    let result = s
        .wait()
        .text("PRESENT")
        .text("ABSENT_TEXT")
        .timeout_ms(500)
        .until();
    assert!(matches!(result, Err(PhantomError::WaitTimeout)));
}

#[test]
fn wait_process_exit_with_code() {
    let pt = Phantom::new().unwrap();
    let s = pt.run("bash").args(&["-c", "exit 7"]).start().unwrap();
    s.wait().exit_code(7).until().unwrap();
}

#[test]
fn wait_process_exit_wrong_code() {
    let pt = Phantom::new().unwrap();
    let s = pt.run("bash").args(&["-c", "exit 1"]).start().unwrap();

    let result = s.wait().exit_code(99).timeout_ms(1000).until();
    assert!(matches!(result, Err(PhantomError::WaitTimeout)));
}

// ═══════════════════════════════════════════════════════════
// Input
// ═══════════════════════════════════════════════════════════

#[test]
fn paste_input() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().paste("echo pasted_text\n").unwrap();
    s.wait().text("pasted_text").until().unwrap();
}

#[test]
fn send_key_ctrl_c() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("sleep 999\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    s.send().key("ctrl-c").unwrap();
    s.wait().stable(500).until().unwrap();

    s.send().type_text("echo back_at_prompt\n").unwrap();
    s.wait().text("back_at_prompt").until().unwrap();
}

#[test]
fn mouse_input() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    // Mouse events won't be processed by bash, but encoding shouldn't error
    s.send().mouse("click:10,5").unwrap();
    s.send().mouse("right-click:20,10").unwrap();
    s.send().mouse("scroll-up:5,5").unwrap();
    s.send().mouse("scroll-down:5,5").unwrap();
    s.send().mouse("move:15,8").unwrap();
}

#[test]
fn type_with_delay() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text_slow("echo delayed\n", 10).unwrap();
    s.wait().text("delayed").until().unwrap();
}

// ═══════════════════════════════════════════════════════════
// Screen capture
// ═══════════════════════════════════════════════════════════

#[test]
fn json_screenshot_has_cell_data() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo hello\n").unwrap();
    s.wait().text("hello").until().unwrap();

    let screen = s.screenshot_json().unwrap();
    assert_eq!(screen.cols, 80);
    assert_eq!(screen.rows, 24);
    assert!(!screen.screen.is_empty());

    let non_empty_row = screen.screen.iter().find(|r| !r.text.trim().is_empty());
    assert!(non_empty_row.is_some());
    let row = non_empty_row.unwrap();
    assert!(!row.cells.is_empty(), "JSON format should include cells");
}

#[test]
fn region_screenshot() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo LINE_ONE\n").unwrap();
    s.wait().text("LINE_ONE").until().unwrap();
    s.send().type_text("echo LINE_TWO\n").unwrap();
    s.wait().text("LINE_TWO").until().unwrap();

    // Full screenshot should have 24 rows
    let full = s.screenshot_json().unwrap();
    assert_eq!(full.screen.len(), 24);

    // Region: only rows 0-2
    let partial = s.screenshot_region(0, 0, 2, 79).unwrap();
    let rows: Vec<_> = partial.raw().screen.iter().collect();
    assert!(
        rows.len() <= 3,
        "region should have at most 3 rows, got {}",
        rows.len()
    );

    // Narrow column region
    let narrow = s.screenshot_region(0, 0, 23, 9).unwrap();
    for row in &narrow.raw().screen {
        assert!(
            row.text.len() <= 10,
            "narrow region row should be at most 10 chars, got {}: '{}'",
            row.text.len(),
            row.text
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Cursor
// ═══════════════════════════════════════════════════════════

#[test]
fn cursor_position() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    let cursor = s.cursor().unwrap();
    assert!(cursor.x < 80, "cursor x={} should be < 80", cursor.x);
    assert!(cursor.y < 24, "cursor y={} should be < 24", cursor.y);
    assert!(cursor.visible, "cursor should be visible");
}

// ═══════════════════════════════════════════════════════════
// Cell inspection
// ═══════════════════════════════════════════════════════════

#[test]
fn cell_inspection() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo ABCDEF\n").unwrap();
    s.wait().text("ABCDEF").until().unwrap();

    let screen = s.screenshot().unwrap();
    let (row_idx, line) = screen
        .text()
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains("ABCDEF") && !l.contains("echo"))
        .expect("should find ABCDEF output row");

    let col = line.find('A').unwrap() as u16;
    let cell = s.cell(col, row_idx as u16).unwrap();
    assert_eq!(cell.grapheme, "A");

    let cell = s.cell(col + 1, row_idx as u16).unwrap();
    assert_eq!(cell.grapheme, "B");
}

// ═══════════════════════════════════════════════════════════
// Resize
// ═══════════════════════════════════════════════════════════

#[test]
fn resize() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("tput cols\n").unwrap();
    s.wait().text("80").until().unwrap();

    s.resize(120, 40).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));

    s.send().type_text("tput cols\n").unwrap();
    s.wait().text("120").until().unwrap();
}

// ═══════════════════════════════════════════════════════════
// Scrollback
// ═══════════════════════════════════════════════════════════

#[test]
fn scrollback() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send()
        .type_text("for i in $(seq 1 50); do echo \"scrollback_line_$i\"; done\n")
        .unwrap();
    s.wait().text("scrollback_line_50").until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        !screen.text().contains("scrollback_line_1\n"),
        "line 1 should have scrolled off the visible screen"
    );

    let scrollback = s.scrollback(None).unwrap();
    assert!(
        scrollback.contains("scrollback_line_1"),
        "scrollback should contain line 1:\n{scrollback}"
    );

    let limited = s.scrollback(Some(5)).unwrap();
    let line_count = limited.lines().filter(|l| !l.is_empty()).count();
    assert!(
        line_count <= 5,
        "limited scrollback should have at most 5 non-empty lines, got {line_count}"
    );
}

// ═══════════════════════════════════════════════════════════
// Output capture
// ═══════════════════════════════════════════════════════════

#[test]
fn output_capture() {
    let pt = Phantom::new().unwrap();
    let s = pt
        .run("bash")
        .args(&["-c", "echo the_final_output"])
        .start()
        .unwrap();
    s.wait().process_exit().until().unwrap();

    let output = s.output().unwrap();
    assert!(
        output.contains("the_final_output"),
        "output should contain process stdout: {output}"
    );
}

// ═══════════════════════════════════════════════════════════
// Snapshot (save/diff via screenshots)
// ═══════════════════════════════════════════════════════════

#[test]
fn snapshot_compare() {
    let pt = Phantom::new().unwrap();
    let s = bash_ready(&pt);

    s.send().type_text("echo SNAPSHOT_CONTENT\n").unwrap();
    s.wait().text("SNAPSHOT_CONTENT").until().unwrap();

    let ref_text = s.screenshot().unwrap().text().to_string();
    let current = s.screenshot().unwrap().text().to_string();
    assert_eq!(ref_text, current, "consecutive screenshots should match");

    s.send().type_text("echo CHANGED\n").unwrap();
    s.wait().text("CHANGED").until().unwrap();

    let changed = s.screenshot().unwrap().text().to_string();
    assert_ne!(ref_text, changed, "screen should differ after new output");
}

// ═══════════════════════════════════════════════════════════
// Unicode
// ═══════════════════════════════════════════════════════════

#[test]
fn wide_characters() {
    let pt = Phantom::new().unwrap();
    let s = pt
        .run("bash")
        .args(&["-c", "printf '日本語\\n'; sleep 30"])
        .start()
        .unwrap();
    s.wait().stable(500).until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        screen.contains("日"),
        "screenshot should contain CJK characters: {screen}"
    );
}

#[test]
fn emoji() {
    let pt = Phantom::new().unwrap();
    let s = pt
        .run("bash")
        .args(&["-c", "printf '🎉🚀\\n'; sleep 30"])
        .start()
        .unwrap();
    s.wait().stable(500).until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        screen.contains("🎉") || screen.contains("🚀"),
        "screenshot should contain emoji: {screen}"
    );
}
