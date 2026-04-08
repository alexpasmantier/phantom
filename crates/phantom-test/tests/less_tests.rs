use std::io::Write;

use phantom_test::{Phantom, has_command};

fn create_test_file(lines: usize) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("failed to create temp file");
    for i in 1..=lines {
        writeln!(f, "Line {i}: The quick brown fox jumps over the lazy dog").unwrap();
    }
    if lines >= 50 {
        writeln!(f, "FINDME_MARKER_HERE").unwrap();
    }
    f.flush().unwrap();
    f
}

#[test]
fn less_view_content() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(100);
    let path = file.path().to_str().unwrap();

    let pt = Phantom::new().unwrap();
    let s = pt.run("less").args(&[path]).start().unwrap();
    s.wait().text("Line 1:").timeout_ms(10000).until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        screen.contains("Line 1:"),
        "should show first line:\n{screen}"
    );
    assert!(
        screen.contains("Line 2:"),
        "should show second line:\n{screen}"
    );
    assert!(
        !screen.contains("Line 50:"),
        "should not show line 50 yet:\n{screen}"
    );
}

#[test]
fn less_scroll_down() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(100);
    let path = file.path().to_str().unwrap();

    let pt = Phantom::new().unwrap();
    let s = pt.run("less").args(&[path]).start().unwrap();
    s.wait().text("Line 1:").timeout_ms(10000).until().unwrap();

    s.send().key("space").unwrap();
    s.wait().text_absent("Line 1:").until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        !screen.contains("Line 1:"),
        "line 1 should have scrolled off:\n{screen}"
    );
}

#[test]
fn less_search() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(100);
    let path = file.path().to_str().unwrap();

    let pt = Phantom::new().unwrap();
    let s = pt.run("less").args(&[path]).start().unwrap();
    s.wait().text("Line 1:").timeout_ms(10000).until().unwrap();

    s.send().type_text("/FINDME_MARKER\n").unwrap();
    s.wait().text("FINDME_MARKER_HERE").until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        screen.contains("FINDME_MARKER_HERE"),
        "should have jumped to search result:\n{screen}"
    );
}

#[test]
fn less_quit() {
    if !has_command("less") {
        eprintln!("SKIP: less not found");
        return;
    }

    let file = create_test_file(10);
    let path = file.path().to_str().unwrap();

    let pt = Phantom::new().unwrap();
    let s = pt.run("less").args(&[path]).start().unwrap();
    s.wait().text("Line 1:").timeout_ms(10000).until().unwrap();

    s.send().key("q").unwrap();
    s.wait().process_exit().until().unwrap();
}
