mod common;

use std::io::Write;

use common::{TestHarness, assert_ok, has_command};

fn create_test_file(lines: usize) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("failed to create temp file");
    for i in 1..=lines {
        writeln!(f, "Line {i}: The quick brown fox jumps over the lazy dog").unwrap();
    }
    // Add a marker we can search for
    if lines >= 50 {
        writeln!(f, "FINDME_MARKER_HERE").unwrap();
    }
    f.flush().unwrap();
    f
}

#[test]
fn test_less_view_content() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(100);
    let path = file.path().to_str().unwrap().to_string();

    let h = TestHarness::new();
    assert_ok(&h.create_session("less_view", "less", &[&path], 80, 24));

    // Wait for less to render the file
    assert_ok(&h.wait_for_text("less_view", "Line 1:", 10000));

    let text = h.screenshot_text("less_view");
    assert!(
        text.contains("Line 1:"),
        "should show first line:\n{text}"
    );
    assert!(
        text.contains("Line 2:"),
        "should show second line:\n{text}"
    );
    // Line 50 should NOT be visible yet (first screenful is ~23 lines)
    assert!(
        !text.contains("Line 50:"),
        "should not show line 50 yet:\n{text}"
    );

    h.kill_session("less_view");
}

#[test]
fn test_less_scroll_down() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(100);
    let path = file.path().to_str().unwrap().to_string();

    let h = TestHarness::new();
    assert_ok(&h.create_session("less_scroll", "less", &[&path], 80, 24));
    assert_ok(&h.wait_for_text("less_scroll", "Line 1:", 10000));

    // Press space to page down
    h.send_keys("less_scroll", &["space"]);
    assert_ok(&h.wait_for_text_absent("less_scroll", "Line 1:", 5000));

    // Should now show later lines
    let text = h.screenshot_text("less_scroll");
    assert!(
        !text.contains("Line 1:"),
        "line 1 should have scrolled off:\n{text}"
    );

    h.kill_session("less_scroll");
}

#[test]
fn test_less_search() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(100);
    let path = file.path().to_str().unwrap().to_string();

    let h = TestHarness::new();
    assert_ok(&h.create_session("less_search", "less", &[&path], 80, 24));
    assert_ok(&h.wait_for_text("less_search", "Line 1:", 10000));

    // Search for the marker
    h.send_type("less_search", "/FINDME_MARKER\n");
    assert_ok(&h.wait_for_text("less_search", "FINDME_MARKER_HERE", 5000));

    let text = h.screenshot_text("less_search");
    assert!(
        text.contains("FINDME_MARKER_HERE"),
        "should have jumped to search result:\n{text}"
    );

    h.kill_session("less_search");
}

#[test]
fn test_less_quit() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(10);
    let path = file.path().to_str().unwrap().to_string();

    let h = TestHarness::new();
    assert_ok(&h.create_session("less_quit", "less", &[&path], 80, 24));
    assert_ok(&h.wait_for_text("less_quit", "Line 1:", 10000));

    // Press q to quit
    h.send_keys("less_quit", &["q"]);
    let resp = h.wait_for_exit("less_quit", 5000);
    assert_ok(&resp);
}
