use phantom_test::{Phantom, has_command};

const VIM_ARGS: &[&str] = &["--clean", "-u", "NONE"];

fn vim(pt: &Phantom) -> phantom_test::Session {
    let s = pt.run("vim").args(VIM_ARGS).start().unwrap();
    s.wait().text("~").timeout_ms(10000).until().unwrap();
    s
}

#[test]
fn vim_startup() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let pt = Phantom::new().unwrap();
    let s = vim(&pt);

    let screen = s.screenshot().unwrap();
    let tilde_count = screen.text().lines().filter(|l| l.starts_with('~')).count();
    assert!(
        tilde_count > 5,
        "vim should show many ~ lines for empty buffer, got {tilde_count}:\n{screen}"
    );
}

#[test]
fn vim_insert_and_type() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let pt = Phantom::new().unwrap();
    let s = vim(&pt);

    s.send().type_text("i").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));

    s.send().type_text("Hello from phantom").unwrap();
    s.wait().text("Hello from phantom").until().unwrap();

    // Escape back to normal mode
    s.send().key("escape").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));

    // dd to delete the line — proves we're in normal mode
    s.send().type_text("dd").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));

    let screen = s.screenshot().unwrap();
    assert!(
        !screen.contains("Hello from phantom"),
        "dd should have deleted the line:\n{screen}"
    );
}

#[test]
fn vim_navigation() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let pt = Phantom::new().unwrap();
    let s = vim(&pt);

    s.send()
        .type_text("iLine one\nLine two\nLine three")
        .unwrap();
    s.wait().text("Line three").until().unwrap();
    s.send().key("escape").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));

    // gg goes to line 1
    s.send().type_text("gg").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));

    let cursor = s.cursor().unwrap();
    assert_eq!(cursor.y, 0, "gg should put cursor on first line");
}

#[test]
fn vim_command_mode() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let pt = Phantom::new().unwrap();
    let s = vim(&pt);

    s.send().type_text(":set number\n").unwrap();
    s.wait().stable(500).until().unwrap();

    let screen = s.screenshot().unwrap();
    assert!(
        screen.contains("  1 "),
        "screen should show line numbers:\n{screen}"
    );
}

#[test]
fn vim_quit() {
    if !has_command("vim") {
        eprintln!("SKIP: vim not found");
        return;
    }

    let pt = Phantom::new().unwrap();
    let s = vim(&pt);

    s.send().type_text(":q!\n").unwrap();
    s.wait().process_exit().until().unwrap();
}
