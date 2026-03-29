use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::Sender;
use mio::Waker;
#[allow(unused_imports)]
use phantom_core::exit_codes;
use phantom_core::protocol::{Response, ResponseData};
use phantom_core::types::{
    CellData, CursorInfo, InputAction, ScreenContent, ScreenFormat, SessionInfo, SessionStatus,
    WaitCondition,
};
use phantom_daemon::engine::{Engine, EngineCommand};

/// Test harness that manages an Engine on a dedicated thread.
pub struct TestHarness {
    cmd_tx: Sender<EngineCommand>,
    waker: Arc<Waker>,
    engine_thread: Option<JoinHandle<()>>,
}

impl TestHarness {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (waker_tx, waker_rx) = crossbeam_channel::bounded::<Arc<Waker>>(1);

        let engine_thread = std::thread::Builder::new()
            .name("test-engine".into())
            .spawn(move || {
                let (mut engine, waker) = Engine::new(cmd_rx).expect("failed to create engine");
                let _ = waker_tx.send(Arc::new(waker));
                if let Err(e) = engine.run() {
                    eprintln!("Engine error: {e}");
                }
            })
            .expect("failed to spawn engine thread");

        let waker = waker_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("engine thread failed to start");

        Self {
            cmd_tx,
            waker,
            engine_thread: Some(engine_thread),
        }
    }

    /// Send a command to the engine and wait for its response (public for custom commands).
    pub fn send_command_raw(
        &self,
        make_cmd: impl FnOnce(Sender<Response>) -> EngineCommand,
    ) -> Response {
        self.send_command(make_cmd)
    }

    /// Send a command to the engine and wait for its response.
    fn send_command(
        &self,
        make_cmd: impl FnOnce(Sender<Response>) -> EngineCommand,
    ) -> Response {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        let cmd = make_cmd(reply_tx);
        self.cmd_tx.send(cmd).unwrap();
        self.waker.wake().unwrap();
        reply_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("engine did not respond within 30s")
    }

    /// Send a wait command — these can take longer since the engine holds the reply
    /// until the condition is met or the wait's own timeout expires.
    fn send_wait_command(
        &self,
        make_cmd: impl FnOnce(Sender<Response>) -> EngineCommand,
        timeout: Duration,
    ) -> Response {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        let cmd = make_cmd(reply_tx);
        self.cmd_tx.send(cmd).unwrap();
        self.waker.wake().unwrap();
        reply_rx
            .recv_timeout(timeout + Duration::from_secs(5))
            .expect("engine did not respond for wait command")
    }

    pub fn create_session(
        &self,
        name: &str,
        command: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
    ) -> Response {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.send_command(|reply| EngineCommand::CreateSession {
            name: name.to_string(),
            command: command.to_string(),
            args,
            env: vec![
                ("LANG".into(), "C".into()),
                ("LC_ALL".into(), "C".into()),
            ],
            cwd: None,
            cols,
            rows,
            scrollback: 1000,
            reply,
        })
    }

    pub fn send_type(&self, session: &str, text: &str) -> Response {
        self.send_command(|reply| EngineCommand::SendInput {
            session: session.to_string(),
            action: InputAction::Type {
                text: text.to_string(),
                delay_ms: None,
            },
            reply,
        })
    }

    pub fn send_keys(&self, session: &str, keys: &[&str]) -> Response {
        self.send_command(|reply| EngineCommand::SendInput {
            session: session.to_string(),
            action: InputAction::Key {
                keys: keys.iter().map(|s| s.to_string()).collect(),
            },
            reply,
        })
    }

    pub fn send_paste(&self, session: &str, text: &str) -> Response {
        self.send_command(|reply| EngineCommand::SendInput {
            session: session.to_string(),
            action: InputAction::Paste {
                text: text.to_string(),
            },
            reply,
        })
    }

    pub fn screenshot(&self, session: &str, format: ScreenFormat) -> Response {
        self.send_command(|reply| EngineCommand::Screenshot {
            session: session.to_string(),
            format,
            region: None,
            reply,
        })
    }

    /// Take a text screenshot and return the screen text content.
    pub fn screenshot_text(&self, session: &str) -> String {
        let resp = self.screenshot(session, ScreenFormat::Text);
        match resp {
            Response::Ok {
                data: Some(ResponseData::Screen(screen)),
            } => screen
                .screen
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => panic!("screenshot failed: {resp:?}"),
        }
    }

    /// Take a JSON screenshot with full cell data.
    pub fn screenshot_json(&self, session: &str) -> ScreenContent {
        let resp = self.screenshot(session, ScreenFormat::Json);
        match resp {
            Response::Ok {
                data: Some(ResponseData::Screen(screen)),
            } => screen,
            _ => panic!("screenshot failed: {resp:?}"),
        }
    }

    pub fn wait_for_text(&self, session: &str, text: &str, timeout_ms: u64) -> Response {
        self.send_wait_command(
            |reply| EngineCommand::Wait {
                session: session.to_string(),
                conditions: vec![WaitCondition::TextPresent(text.to_string())],
                timeout_ms,
                poll_ms: 50,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn wait_for_text_absent(&self, session: &str, text: &str, timeout_ms: u64) -> Response {
        self.send_wait_command(
            |reply| EngineCommand::Wait {
                session: session.to_string(),
                conditions: vec![WaitCondition::TextAbsent(text.to_string())],
                timeout_ms,
                poll_ms: 50,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn wait_for_stable(&self, session: &str, duration_ms: u64, timeout_ms: u64) -> Response {
        self.send_wait_command(
            |reply| EngineCommand::Wait {
                session: session.to_string(),
                conditions: vec![WaitCondition::ScreenStable { duration_ms }],
                timeout_ms,
                poll_ms: 50,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn wait_for_exit(&self, session: &str, timeout_ms: u64) -> Response {
        self.send_wait_command(
            |reply| EngineCommand::Wait {
                session: session.to_string(),
                conditions: vec![WaitCondition::ProcessExited { exit_code: None }],
                timeout_ms,
                poll_ms: 50,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn get_cursor(&self, session: &str) -> CursorInfo {
        let resp = self.send_command(|reply| EngineCommand::GetCursor {
            session: session.to_string(),
            reply,
        });
        match resp {
            Response::Ok {
                data: Some(ResponseData::Cursor(c)),
            } => c,
            _ => panic!("get_cursor failed: {resp:?}"),
        }
    }

    pub fn get_status(&self, session: &str) -> Response {
        self.send_command(|reply| EngineCommand::GetStatus {
            session: session.to_string(),
            reply,
        })
    }

    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        let resp = self.send_command(|reply| EngineCommand::ListSessions { reply });
        match resp {
            Response::Ok {
                data: Some(ResponseData::Sessions(s)),
            } => s,
            _ => panic!("list_sessions failed: {resp:?}"),
        }
    }

    pub fn resize(&self, session: &str, cols: u16, rows: u16) -> Response {
        self.send_command(|reply| EngineCommand::Resize {
            session: session.to_string(),
            cols,
            rows,
            reply,
        })
    }

    pub fn send_mouse(&self, session: &str, spec: &str) -> Response {
        self.send_command(|reply| EngineCommand::SendInput {
            session: session.to_string(),
            action: InputAction::Mouse {
                spec: spec.to_string(),
            },
            reply,
        })
    }

    pub fn get_scrollback(&self, session: &str, lines: Option<u32>) -> String {
        let resp = self.send_command(|reply| EngineCommand::GetScrollback {
            session: session.to_string(),
            lines,
            reply,
        });
        match resp {
            Response::Ok {
                data: Some(ResponseData::Text(text)),
            } => text,
            _ => panic!("get_scrollback failed: {resp:?}"),
        }
    }

    pub fn screenshot_region(
        &self,
        session: &str,
        top: u16,
        left: u16,
        bottom: u16,
        right: u16,
    ) -> ScreenContent {
        let resp = self.send_command(|reply| EngineCommand::Screenshot {
            session: session.to_string(),
            format: ScreenFormat::Text,
            region: Some((top, left, bottom, right)),
            reply,
        });
        match resp {
            Response::Ok {
                data: Some(ResponseData::Screen(screen)),
            } => screen,
            _ => panic!("screenshot_region failed: {resp:?}"),
        }
    }

    pub fn get_output(&self, session: &str) -> String {
        let resp = self.send_command(|reply| EngineCommand::GetOutput {
            session: session.to_string(),
            reply,
        });
        match resp {
            Response::Ok {
                data: Some(ResponseData::Text(text)),
            } => text,
            _ => panic!("get_output failed: {resp:?}"),
        }
    }

    pub fn get_cell(&self, session: &str, x: u16, y: u16) -> CellData {
        let resp = self.send_command(|reply| EngineCommand::GetCell {
            session: session.to_string(),
            x,
            y,
            reply,
        });
        match resp {
            Response::Ok {
                data: Some(ResponseData::Cell(cell)),
            } => cell,
            _ => panic!("get_cell failed: {resp:?}"),
        }
    }

    pub fn wait_for_changed(&self, session: &str, timeout_ms: u64) -> Response {
        self.send_wait_command(
            |reply| EngineCommand::Wait {
                session: session.to_string(),
                conditions: vec![WaitCondition::ScreenChanged],
                timeout_ms,
                poll_ms: 50,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn wait_for_regex(&self, session: &str, pattern: &str, timeout_ms: u64) -> Response {
        self.send_wait_command(
            |reply| EngineCommand::Wait {
                session: session.to_string(),
                conditions: vec![WaitCondition::Regex(regex::Regex::new(pattern).unwrap())],
                timeout_ms,
                poll_ms: 50,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn wait_with_conditions(
        &self,
        session: &str,
        conditions: Vec<WaitCondition>,
        timeout_ms: u64,
    ) -> Response {
        self.send_wait_command(
            |reply| EngineCommand::Wait {
                session: session.to_string(),
                conditions,
                timeout_ms,
                poll_ms: 50,
                reply,
            },
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn kill_session(&self, session: &str) -> Response {
        self.send_command(|reply| EngineCommand::KillSession {
            session: session.to_string(),
            signal: None,
            reply,
        })
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(EngineCommand::Shutdown);
        let _ = self.waker.wake();
        if let Some(handle) = self.engine_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Check if a command is available on the system.
pub fn has_command(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Assert that a response is Ok.
pub fn assert_ok(resp: &Response) {
    match resp {
        Response::Ok { .. } => {}
        Response::Error { code, message } => {
            panic!("expected Ok, got Error(code={code}): {message}")
        }
    }
}

/// Assert that a response is an error with a specific code.
pub fn assert_error(resp: &Response, expected_code: i32) {
    match resp {
        Response::Error { code, .. } => {
            assert_eq!(*code, expected_code, "expected error code {expected_code}, got {code}");
        }
        Response::Ok { .. } => {
            panic!("expected Error(code={expected_code}), got Ok")
        }
    }
}
