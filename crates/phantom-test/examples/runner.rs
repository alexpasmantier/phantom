use phantom_test::{Phantom, TestRunner, has_command};

fn main() {
    TestRunner::new()
        // ── Session lifecycle ───────────────────────────────
        .test("echo and screenshot", |pt| {
            let s = bash(pt);
            s.send().type_text("echo hello_phantom\n")?;
            s.wait().text("hello_phantom").until()?;
            let screen = s.screenshot()?;
            assert!(screen.contains("hello_phantom"));
            Ok(())
        })
        .test("session collision", |pt| {
            pt.run("bash")
                .args(&["--norc", "--noprofile"])
                .name("dup")
                .start()?;
            let result = pt
                .run("bash")
                .args(&["--norc", "--noprofile"])
                .name("dup")
                .start();
            assert!(result.is_err());
            Ok(())
        })
        .test("list sessions", |pt| {
            let _a = pt
                .run("bash")
                .args(&["--norc", "--noprofile"])
                .name("list_a")
                .start()?;
            let _b = pt
                .run("bash")
                .args(&["--norc", "--noprofile"])
                .name("list_b")
                .start()?;
            let sessions = pt.sessions()?;
            let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
            assert!(names.contains(&"list_a"));
            assert!(names.contains(&"list_b"));
            Ok(())
        })
        .test("kill session", |pt| {
            let s = pt.run("sleep").args(&["999"]).start()?;
            s.kill()?;
            s.wait().process_exit().until()?;
            Ok(())
        })
        // ── Input ───────────────────────────────────────────
        .test("paste input", |pt| {
            let s = bash(pt);
            s.send().paste("echo pasted_ok\n")?;
            s.wait().text("pasted_ok").until()?;
            Ok(())
        })
        .test("send key ctrl-c", |pt| {
            let s = bash(pt);
            s.send().type_text("sleep 999\n")?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            s.send().key("ctrl-c")?;
            s.wait().stable(500).until()?;
            s.send().type_text("echo back\n")?;
            s.wait().text("back").until()?;
            Ok(())
        })
        // ── Wait conditions ─────────────────────────────────
        .test("wait text delayed", |pt| {
            let s = bash(pt);
            s.send().type_text("sleep 1 && echo MARKER\n")?;
            s.wait().text("MARKER").timeout_ms(5000).until()?;
            Ok(())
        })
        .test("wait text absent", |pt| {
            let s = bash(pt);
            s.send().type_text("echo TEMP\n")?;
            s.wait().text("TEMP").until()?;
            s.send().type_text("clear\n")?;
            s.wait().text_absent("TEMP").until()?;
            Ok(())
        })
        .test("wait regex", |pt| {
            let s = bash(pt);
            s.send().type_text("echo 'count: 42 items'\n")?;
            s.wait().regex(r"count: \d+ items").until()?;
            Ok(())
        })
        .test("wait stable", |pt| {
            let s = pt.run("bash").args(&["--norc", "--noprofile"]).start()?;
            s.wait().stable(500).until()?;
            Ok(())
        })
        .test("wait screen changed", |pt| {
            let s = bash(pt);
            s.send().type_text("echo CHANGE\n")?;
            s.wait().screen_changed().until()?;
            Ok(())
        })
        .test("combined wait", |pt| {
            let s = bash(pt);
            s.send().type_text("echo A; echo B\n")?;
            s.wait().text("A").text("B").until()?;
            Ok(())
        })
        // ── Screen capture ──────────────────────────────────
        .test("json screenshot", |pt| {
            let s = bash(pt);
            s.send().type_text("echo hello\n")?;
            s.wait().text("hello").until()?;
            let screen = s.screenshot_json()?;
            assert_eq!(screen.cols, 80);
            assert_eq!(screen.rows, 24);
            let row = screen.screen.iter().find(|r| !r.text.trim().is_empty());
            assert!(row.is_some());
            assert!(!row.unwrap().cells.is_empty());
            Ok(())
        })
        .test("region screenshot", |pt| {
            let s = bash(pt);
            s.send().type_text("echo LINE_ONE\n")?;
            s.wait().text("LINE_ONE").until()?;
            let narrow = s.screenshot_region(0, 0, 23, 9)?;
            for row in &narrow.raw().screen {
                assert!(row.text.len() <= 10);
            }
            Ok(())
        })
        // ── Cursor & cells ──────────────────────────────────
        .test("cursor position", |pt| {
            let s = bash(pt);
            let c = s.cursor()?;
            assert!(c.x < 80);
            assert!(c.y < 24);
            assert!(c.visible);
            Ok(())
        })
        .test("cell inspection", |pt| {
            let s = bash(pt);
            s.send().type_text("echo ABCDEF\n")?;
            s.wait().text("ABCDEF").until()?;
            let screen = s.screenshot()?;
            let (row, line) = screen
                .text()
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains("ABCDEF") && !l.contains("echo"))
                .unwrap();
            let col = line.find('A').unwrap() as u16;
            assert_eq!(s.cell(col, row as u16)?.grapheme, "A");
            assert_eq!(s.cell(col + 1, row as u16)?.grapheme, "B");
            Ok(())
        })
        // ── Resize ──────────────────────────────────────────
        .test("resize terminal", |pt| {
            let s = bash(pt);
            s.send().type_text("tput cols\n")?;
            s.wait().text("80").until()?;
            s.resize(120, 40)?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            s.send().type_text("tput cols\n")?;
            s.wait().text("120").until()?;
            Ok(())
        })
        // ── Scrollback ──────────────────────────────────────
        .test("scrollback", |pt| {
            let s = bash(pt);
            s.send()
                .type_text("for i in $(seq 1 50); do echo sb_$i; done\n")?;
            s.wait().text("sb_50").until()?;
            let sb = s.scrollback(None)?;
            assert!(sb.contains("sb_1"));
            Ok(())
        })
        // ── Output capture ──────────────────────────────────
        .test("output capture", |pt| {
            let s = pt.run("bash").args(&["-c", "echo final_output"]).start()?;
            s.wait().process_exit().until()?;
            assert!(s.output()?.contains("final_output"));
            Ok(())
        })
        // ── Exit codes ──────────────────────────────────────
        .test("exit code", |pt| {
            let s = pt.run("bash").args(&["-c", "exit 42"]).start()?;
            s.wait().exit_code(42).until()?;
            Ok(())
        })
        // ── Unicode ─────────────────────────────────────────
        .test("wide characters", |pt| {
            let s = pt
                .run("bash")
                .args(&["-c", "printf '日本語\\n'; sleep 30"])
                .start()?;
            s.wait().stable(500).until()?;
            assert!(s.screenshot()?.contains("日"));
            Ok(())
        })
        // ── vim ─────────────────────────────────────────────
        .test("vim startup", |pt| {
            if !has_command("vim") {
                return Ok(());
            }
            let s = pt.run("vim").args(&["--clean", "-u", "NONE"]).start()?;
            s.wait().text("~").timeout_ms(10000).until()?;
            let screen = s.screenshot()?;
            assert!(screen.text().lines().filter(|l| l.starts_with('~')).count() > 5);
            Ok(())
        })
        .test("vim insert and quit", |pt| {
            if !has_command("vim") {
                return Ok(());
            }
            let s = pt.run("vim").args(&["--clean", "-u", "NONE"]).start()?;
            s.wait().text("~").timeout_ms(10000).until()?;
            s.send().type_text("iHello")?;
            s.wait().text("Hello").until()?;
            s.send().key("escape")?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            s.send().type_text(":q!\n")?;
            s.wait().process_exit().until()?;
            Ok(())
        })
        // ── less ────────────────────────────────────────────
        .test("less view and quit", |pt| {
            if !has_command("less") {
                return Ok(());
            }
            let mut f = tempfile::NamedTempFile::new().unwrap();
            use std::io::Write;
            for i in 1..=100 {
                writeln!(f, "Line {i}: content").unwrap();
            }
            f.flush().unwrap();
            let path = f.path().to_str().unwrap().to_string();
            let s = pt.run("less").args(&[&path]).start()?;
            s.wait().text("Line 1:").timeout_ms(10000).until()?;
            s.send().key("space")?;
            s.wait().text_absent("Line 1:").until()?;
            s.send().key("q")?;
            s.wait().process_exit().until()?;
            Ok(())
        })
        .run();
}

fn bash(pt: &Phantom) -> phantom_test::Session {
    let s = pt
        .run("bash")
        .args(&["--norc", "--noprofile"])
        .start()
        .unwrap();
    s.wait().stable(300).until().unwrap();
    s
}
