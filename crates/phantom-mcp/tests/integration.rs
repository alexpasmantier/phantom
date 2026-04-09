//! Integration tests that drive a real bash session through PhantomMcpServer.
//!
//! These call the server's tool methods directly (the same code path the
//! MCP transport invokes) without going over JSON-RPC. They require `bash`
//! to be available on PATH and must be run with `--test-threads=1` to avoid
//! contention between concurrent phantom engines (matching phantom-daemon's
//! convention).

use phantom_mcp::server::{
    CellArgs, CursorPos, PhantomMcpServer, Region, ResizeArgs, RunArgs, ScreenshotArgs, SendArgs,
    SessionArgs, WaitArgs,
};
use rmcp::model::{CallToolResult, RawContent};

fn has_bash() -> bool {
    std::process::Command::new("which")
        .arg("bash")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn text_of(result: &CallToolResult) -> String {
    let content = result
        .content
        .first()
        .expect("at least one content block");
    match &content.raw {
        RawContent::Text(t) => t.text.clone(),
        other => panic!("expected text content, got {other:?}"),
    }
}

fn image_of(result: &CallToolResult) -> (String, Vec<u8>) {
    use base64::Engine as _;
    let content = result
        .content
        .first()
        .expect("at least one content block");
    match &content.raw {
        RawContent::Image(img) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&img.data)
                .expect("base64 decode");
            (img.mime_type.clone(), bytes)
        }
        other => panic!("expected image content, got {other:?}"),
    }
}

fn make_run_args(name: &str) -> RunArgs {
    RunArgs {
        command: "bash".into(),
        args: vec!["--norc".into(), "--noprofile".into()],
        name: Some(name.into()),
        cols: 60,
        rows: 12,
        cwd: None,
        env: vec![],
    }
}

#[tokio::test]
async fn run_send_screenshot_kill_roundtrip() {
    if !has_bash() {
        eprintln!("skipping: bash not available");
        return;
    }
    let server = PhantomMcpServer::new().expect("server");

    // Spawn a bash session.
    let r = server
        .phantom_run(rmcp::handler::server::wrapper::Parameters(make_run_args(
            "rt1",
        )))
        .await
        .expect("run");
    assert!(text_of(&r).contains("rt1"));

    // Wait for the prompt to settle.
    let r = server
        .phantom_wait(rmcp::handler::server::wrapper::Parameters(WaitArgs {
            session: "rt1".into(),
            text: None,
            text_absent: None,
            regex: None,
            stable_ms: Some(300),
            process_exit: None,
            exit_code: None,
            cursor_at: None,
            cursor_visible: None,
            screen_changed: None,
            timeout_ms: 3_000,
        }))
        .await
        .expect("wait stable");
    assert_eq!(text_of(&r), "condition met");

    // Type a command.
    let r = server
        .phantom_send(rmcp::handler::server::wrapper::Parameters(SendArgs {
            session: "rt1".into(),
            kind: "text".into(),
            value: "echo phantom-mcp-works\n".into(),
        }))
        .await
        .expect("send");
    assert_eq!(text_of(&r), "ok");

    // Wait for the output to appear.
    let r = server
        .phantom_wait(rmcp::handler::server::wrapper::Parameters(WaitArgs {
            session: "rt1".into(),
            text: Some("phantom-mcp-works".into()),
            text_absent: None,
            regex: None,
            stable_ms: None,
            process_exit: None,
            exit_code: None,
            cursor_at: None,
            cursor_visible: None,
            screen_changed: None,
            timeout_ms: 3_000,
        }))
        .await
        .expect("wait text");
    assert_eq!(text_of(&r), "condition met");

    // Text screenshot — must contain our marker.
    let r = server
        .phantom_screenshot(rmcp::handler::server::wrapper::Parameters(ScreenshotArgs {
            session: "rt1".into(),
            format: "text".into(),
            region: None,
        }))
        .await
        .expect("screenshot");
    assert!(text_of(&r).contains("phantom-mcp-works"));

    // Image screenshot — must be a valid PNG.
    let r = server
        .phantom_screenshot(rmcp::handler::server::wrapper::Parameters(ScreenshotArgs {
            session: "rt1".into(),
            format: "image".into(),
            region: None,
        }))
        .await
        .expect("screenshot image");
    let (mime, bytes) = image_of(&r);
    assert_eq!(mime, "image/png");
    assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n");

    // Kill cleanup.
    let r = server
        .phantom_kill(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "rt1".into(),
        }))
        .await
        .expect("kill");
    assert!(text_of(&r).contains("rt1"));
}

#[tokio::test]
async fn screenshot_region_returns_smaller_image() {
    if !has_bash() {
        eprintln!("skipping: bash not available");
        return;
    }
    let server = PhantomMcpServer::new().expect("server");

    server
        .phantom_run(rmcp::handler::server::wrapper::Parameters(make_run_args(
            "rg1",
        )))
        .await
        .expect("run");
    server
        .phantom_wait(rmcp::handler::server::wrapper::Parameters(WaitArgs {
            session: "rg1".into(),
            text: None,
            text_absent: None,
            regex: None,
            stable_ms: Some(300),
            process_exit: None,
            exit_code: None,
            cursor_at: None,
            cursor_visible: None,
            screen_changed: None,
            timeout_ms: 3_000,
        }))
        .await
        .expect("wait");

    // Capture full image and a 10×3 region; the region image must be smaller.
    let full = server
        .phantom_screenshot(rmcp::handler::server::wrapper::Parameters(ScreenshotArgs {
            session: "rg1".into(),
            format: "image".into(),
            region: None,
        }))
        .await
        .expect("full");
    let region = server
        .phantom_screenshot(rmcp::handler::server::wrapper::Parameters(ScreenshotArgs {
            session: "rg1".into(),
            format: "image".into(),
            region: Some(Region {
                top: 0,
                left: 0,
                bottom: 2,
                right: 9,
            }),
        }))
        .await
        .expect("region");

    let (_, full_bytes) = image_of(&full);
    let (_, region_bytes) = image_of(&region);
    let full_img = image::load_from_memory(&full_bytes).unwrap();
    let region_img = image::load_from_memory(&region_bytes).unwrap();
    assert!(region_img.width() < full_img.width());
    assert!(region_img.height() < full_img.height());

    server
        .phantom_kill(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "rg1".into(),
        }))
        .await
        .expect("kill");
}

#[tokio::test]
async fn wait_regex_matches_screen() {
    if !has_bash() {
        eprintln!("skipping: bash not available");
        return;
    }
    let server = PhantomMcpServer::new().expect("server");

    server
        .phantom_run(rmcp::handler::server::wrapper::Parameters(make_run_args(
            "rgx",
        )))
        .await
        .expect("run");
    server
        .phantom_wait(rmcp::handler::server::wrapper::Parameters(WaitArgs {
            session: "rgx".into(),
            text: None,
            text_absent: None,
            regex: None,
            stable_ms: Some(300),
            process_exit: None,
            exit_code: None,
            cursor_at: None,
            cursor_visible: None,
            screen_changed: None,
            timeout_ms: 3_000,
        }))
        .await
        .expect("wait stable");

    server
        .phantom_send(rmcp::handler::server::wrapper::Parameters(SendArgs {
            session: "rgx".into(),
            kind: "text".into(),
            value: "echo abc-12345-xyz\n".into(),
        }))
        .await
        .expect("send");

    let r = server
        .phantom_wait(rmcp::handler::server::wrapper::Parameters(WaitArgs {
            session: "rgx".into(),
            text: None,
            text_absent: None,
            regex: Some(r"abc-\d+-xyz".into()),
            stable_ms: None,
            process_exit: None,
            exit_code: None,
            cursor_at: None,
            cursor_visible: None,
            screen_changed: None,
            timeout_ms: 3_000,
        }))
        .await
        .expect("wait regex");
    assert_eq!(text_of(&r), "condition met");

    server
        .phantom_kill(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "rgx".into(),
        }))
        .await
        .expect("kill");
}

#[tokio::test]
async fn wait_invalid_regex_returns_clean_error() {
    if !has_bash() {
        eprintln!("skipping: bash not available");
        return;
    }
    let server = PhantomMcpServer::new().expect("server");

    server
        .phantom_run(rmcp::handler::server::wrapper::Parameters(make_run_args(
            "rge",
        )))
        .await
        .expect("run");

    let err = server
        .phantom_wait(rmcp::handler::server::wrapper::Parameters(WaitArgs {
            session: "rge".into(),
            text: None,
            text_absent: None,
            regex: Some("(unclosed".into()),
            stable_ms: None,
            process_exit: None,
            exit_code: None,
            cursor_at: None,
            cursor_visible: None,
            screen_changed: None,
            timeout_ms: 1_000,
        }))
        .await
        .expect_err("invalid regex should error");
    assert!(err.message.contains("invalid regex"));

    server
        .phantom_kill(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "rge".into(),
        }))
        .await
        .expect("kill");
}

#[tokio::test]
async fn cursor_and_cell_inspection() {
    if !has_bash() {
        eprintln!("skipping: bash not available");
        return;
    }
    let server = PhantomMcpServer::new().expect("server");

    server
        .phantom_run(rmcp::handler::server::wrapper::Parameters(make_run_args(
            "ins",
        )))
        .await
        .expect("run");
    server
        .phantom_wait(rmcp::handler::server::wrapper::Parameters(WaitArgs {
            session: "ins".into(),
            text: None,
            text_absent: None,
            regex: None,
            stable_ms: Some(300),
            process_exit: None,
            exit_code: None,
            cursor_at: None,
            cursor_visible: None,
            screen_changed: None,
            timeout_ms: 3_000,
        }))
        .await
        .expect("wait");

    // Cursor should be readable.
    let r = server
        .phantom_cursor(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "ins".into(),
        }))
        .await
        .expect("cursor");
    let cursor_text = text_of(&r);
    assert!(cursor_text.contains("("));
    assert!(cursor_text.contains("visible") || cursor_text.contains("hidden"));

    // Cell inspection.
    let r = server
        .phantom_cell(rmcp::handler::server::wrapper::Parameters(CellArgs {
            session: "ins".into(),
            x: 0,
            y: 0,
        }))
        .await
        .expect("cell");
    let cell_json: serde_json::Value = serde_json::from_str(&text_of(&r)).expect("cell json");
    assert!(cell_json.get("grapheme").is_some());

    server
        .phantom_kill(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "ins".into(),
        }))
        .await
        .expect("kill");
}

#[tokio::test]
async fn list_status_resize_lifecycle() {
    if !has_bash() {
        eprintln!("skipping: bash not available");
        return;
    }
    let server = PhantomMcpServer::new().expect("server");

    server
        .phantom_run(rmcp::handler::server::wrapper::Parameters(make_run_args(
            "lcc",
        )))
        .await
        .expect("run");

    // List should contain our session.
    let r = server.phantom_list().await.expect("list");
    assert!(text_of(&r).contains("lcc"));

    // Status should be JSON parseable.
    let r = server
        .phantom_status(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "lcc".into(),
        }))
        .await
        .expect("status");
    let status: serde_json::Value = serde_json::from_str(&text_of(&r)).expect("status json");
    assert_eq!(status.get("name").and_then(|v| v.as_str()), Some("lcc"));
    assert_eq!(status.get("cols").and_then(|v| v.as_u64()), Some(60));

    // Resize and re-check via status.
    let r = server
        .phantom_resize(rmcp::handler::server::wrapper::Parameters(ResizeArgs {
            session: "lcc".into(),
            cols: 100,
            rows: 30,
        }))
        .await
        .expect("resize");
    assert!(text_of(&r).contains("100"));

    let r = server
        .phantom_status(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "lcc".into(),
        }))
        .await
        .expect("status after resize");
    let status: serde_json::Value = serde_json::from_str(&text_of(&r)).expect("status json");
    assert_eq!(status.get("cols").and_then(|v| v.as_u64()), Some(100));
    assert_eq!(status.get("rows").and_then(|v| v.as_u64()), Some(30));

    server
        .phantom_kill(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "lcc".into(),
        }))
        .await
        .expect("kill");
}

#[tokio::test]
async fn unknown_session_returns_error() {
    let server = PhantomMcpServer::new().expect("server");
    let err = server
        .phantom_cursor(rmcp::handler::server::wrapper::Parameters(SessionArgs {
            session: "no-such-session".into(),
        }))
        .await
        .expect_err("missing session should error");
    assert!(err.message.contains("no session named"));
}

#[tokio::test]
async fn cursor_at_unused_field_compiles() {
    // Smoke compile check that CursorPos is constructible from Rust code.
    let _ = CursorPos { x: 0, y: 0 };
}
