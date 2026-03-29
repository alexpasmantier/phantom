use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token, Waker};
use nix::sys::signal::Signal;
use phantom_core::exit_codes;
use phantom_core::protocol::{Response, ResponseData};
use phantom_core::types::{InputAction, ScreenFormat, WaitCondition};

use crate::capture;
use crate::input;
use crate::session::Session;
use crate::wait::{PendingWait, evaluate_conditions};

const WAKER_TOKEN: Token = Token(0);
const PTY_TOKEN_BASE: usize = 1000;

pub enum EngineCommand {
    CreateSession {
        name: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        scrollback: u32,
        reply: Sender<Response>,
    },
    SendInput {
        session: String,
        action: InputAction,
        reply: Sender<Response>,
    },
    Screenshot {
        session: String,
        format: ScreenFormat,
        reply: Sender<Response>,
    },
    Wait {
        session: String,
        conditions: Vec<WaitCondition>,
        timeout_ms: u64,
        poll_ms: u64,
        reply: Sender<Response>,
    },
    GetCursor {
        session: String,
        reply: Sender<Response>,
    },
    Resize {
        session: String,
        cols: u16,
        rows: u16,
        reply: Sender<Response>,
    },
    GetStatus {
        session: String,
        reply: Sender<Response>,
    },
    ListSessions {
        reply: Sender<Response>,
    },
    GetScrollback {
        session: String,
        lines: Option<u32>,
        reply: Sender<Response>,
    },
    KillSession {
        session: String,
        signal: Option<i32>,
        reply: Sender<Response>,
    },
    Shutdown,
}

pub struct Engine {
    poll: Poll,
    cmd_rx: Receiver<EngineCommand>,
    sessions: HashMap<String, Session>,
    token_to_session: HashMap<Token, String>,
    next_token: usize,
    pending_waits: Vec<PendingWait>,
}

impl Engine {
    pub fn new(cmd_rx: Receiver<EngineCommand>) -> Result<(Self, Waker)> {
        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), WAKER_TOKEN)?;

        Ok((
            Self {
                poll,
                cmd_rx,
                sessions: HashMap::new(),
                token_to_session: HashMap::new(),
                next_token: PTY_TOKEN_BASE,
                pending_waits: Vec::new(),
            },
            waker,
        ))
    }

    pub fn run(&mut self) -> Result<()> {
        let mut events = Events::with_capacity(256);
        let mut read_buf = [0u8; 8192];

        loop {
            let timeout = if self.pending_waits.is_empty() {
                None
            } else {
                Some(Duration::from_millis(50))
            };

            self.poll.poll(&mut events, timeout)?;

            for event in events.iter() {
                match event.token() {
                    WAKER_TOKEN => {
                        while let Ok(cmd) = self.cmd_rx.try_recv() {
                            if matches!(cmd, EngineCommand::Shutdown) {
                                return Ok(());
                            }
                            self.handle_command(cmd);
                        }
                    }
                    token => {
                        if let Some(session_name) = self.token_to_session.get(&token).cloned() {
                            if let Some(session) = self.sessions.get_mut(&session_name) {
                                loop {
                                    match session.pty.read(&mut read_buf) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            session.process_pty_output(&read_buf[..n]);
                                        }
                                        Err(_) => break,
                                    }
                                }
                            }
                        }
                    }
                }
            }

            self.evaluate_waits();
        }
    }

    fn handle_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::CreateSession {
                name,
                command,
                args,
                env,
                cwd,
                cols,
                rows,
                scrollback,
                reply,
            } => {
                let resp =
                    self.create_session(name, &command, &args, &env, cwd.as_deref(), cols, rows, scrollback);
                let _ = reply.send(resp);
            }
            EngineCommand::SendInput {
                session,
                action,
                reply,
            } => {
                let resp = self.send_input(&session, action);
                let _ = reply.send(resp);
            }
            EngineCommand::Screenshot {
                session,
                format,
                reply,
            } => {
                let resp = self.screenshot(&session, &format);
                let _ = reply.send(resp);
            }
            EngineCommand::Wait {
                session,
                conditions,
                timeout_ms,
                poll_ms,
                reply,
            } => {
                self.add_wait(session, conditions, timeout_ms, poll_ms, reply);
            }
            EngineCommand::GetCursor { session, reply } => {
                let resp = self.get_cursor(&session);
                let _ = reply.send(resp);
            }
            EngineCommand::Resize {
                session,
                cols,
                rows,
                reply,
            } => {
                let resp = self.resize(&session, cols, rows);
                let _ = reply.send(resp);
            }
            EngineCommand::GetStatus { session, reply } => {
                let resp = self.get_status(&session);
                let _ = reply.send(resp);
            }
            EngineCommand::ListSessions { reply } => {
                let resp = self.list_sessions();
                let _ = reply.send(resp);
            }
            EngineCommand::GetScrollback {
                session,
                lines,
                reply,
            } => {
                let resp = self.get_scrollback(&session, lines);
                let _ = reply.send(resp);
            }
            EngineCommand::KillSession {
                session,
                signal,
                reply,
            } => {
                let resp = self.kill_session(&session, signal);
                let _ = reply.send(resp);
            }
            EngineCommand::Shutdown => unreachable!(),
        }
    }

    fn create_session(
        &mut self,
        name: String,
        command: &str,
        args: &[String],
        env: &[(String, String)],
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
        scrollback: u32,
    ) -> Response {
        if self.sessions.contains_key(&name) {
            return Response::error(
                exit_codes::SESSION_COLLISION,
                format!("Session '{name}' already exists"),
            );
        }

        match Session::new(name.clone(), command, args, env, cwd, cols, rows, scrollback) {
            Ok(session) => {
                let token = Token(self.next_token);
                self.next_token += 1;

                let raw_fd = session.pty.raw_fd();
                let _ = self.poll.registry().register(
                    &mut SourceFd(&raw_fd),
                    token,
                    Interest::READABLE,
                );

                self.token_to_session.insert(token, name.clone());

                let info = phantom_core::types::SessionInfo {
                    name: name.clone(),
                    pid: session.pty.child_pid.as_raw() as u32,
                    cols,
                    rows,
                    title: None,
                    pwd: None,
                    status: phantom_core::types::SessionStatus::Running,
                };

                self.sessions.insert(name, session);
                Response::ok_with(ResponseData::Session(info))
            }
            Err(e) => Response::error(exit_codes::ERROR, format!("Failed to create session: {e}")),
        }
    }

    fn send_input(&mut self, session_name: &str, action: InputAction) -> Response {
        let Some(session) = self.sessions.get_mut(session_name) else {
            return Response::session_not_found(session_name);
        };

        let result = match action {
            InputAction::Type { text, delay_ms } => input::type_text(session, &text, delay_ms),
            InputAction::Key { keys } => {
                let mut r = Ok(());
                for key_spec in &keys {
                    r = input::send_key(session, key_spec);
                    if r.is_err() {
                        break;
                    }
                }
                r
            }
            InputAction::Paste { text } => input::paste(session, &text),
            InputAction::Mouse { spec } => input::send_mouse(session, &spec),
        };

        match result {
            Ok(()) => Response::ok(),
            Err(e) => Response::error(exit_codes::ERROR, format!("Input error: {e}")),
        }
    }

    fn screenshot(&mut self, session_name: &str, format: &ScreenFormat) -> Response {
        let Some(session) = self.sessions.get_mut(session_name) else {
            return Response::session_not_found(session_name);
        };

        match capture::capture_screen(session, format) {
            Ok(screen) => Response::ok_with(ResponseData::Screen(screen)),
            Err(e) => Response::error(exit_codes::ERROR, format!("Capture error: {e}")),
        }
    }

    fn add_wait(
        &mut self,
        session_name: String,
        conditions: Vec<WaitCondition>,
        timeout_ms: u64,
        poll_ms: u64,
        reply: Sender<Response>,
    ) {
        if !self.sessions.contains_key(&session_name) {
            let _ = reply.send(Response::session_not_found(&session_name));
            return;
        }

        // Check immediately
        if let Some(session) = self.sessions.get_mut(&session_name) {
            if evaluate_conditions(session, &conditions) {
                let _ = reply.send(Response::ok());
                return;
            }
        }

        self.pending_waits.push(PendingWait {
            session_name,
            conditions,
            deadline: Instant::now() + Duration::from_millis(timeout_ms),
            poll_interval: Duration::from_millis(poll_ms),
            last_check: Instant::now(),
            reply,
        });
    }

    fn evaluate_waits(&mut self) {
        let mut completed = Vec::new();

        for (i, wait) in self.pending_waits.iter_mut().enumerate() {
            if wait.is_expired() {
                let _ = wait.reply.send(Response::error(
                    exit_codes::WAIT_TIMEOUT,
                    "Wait condition timed out",
                ));
                completed.push(i);
                continue;
            }

            if !wait.should_poll() {
                continue;
            }
            wait.last_check = Instant::now();

            if let Some(session) = self.sessions.get_mut(&wait.session_name) {
                if evaluate_conditions(session, &wait.conditions) {
                    let _ = wait.reply.send(Response::ok());
                    completed.push(i);
                }
            } else {
                let _ = wait
                    .reply
                    .send(Response::session_not_found(&wait.session_name));
                completed.push(i);
            }
        }

        for i in completed.into_iter().rev() {
            self.pending_waits.swap_remove(i);
        }
    }

    fn get_cursor(&mut self, session_name: &str) -> Response {
        let Some(session) = self.sessions.get_mut(session_name) else {
            return Response::session_not_found(session_name);
        };
        Response::ok_with(ResponseData::Cursor(session.cursor_info()))
    }

    fn resize(&mut self, session_name: &str, cols: u16, rows: u16) -> Response {
        let Some(session) = self.sessions.get_mut(session_name) else {
            return Response::session_not_found(session_name);
        };
        match session.resize(cols, rows) {
            Ok(()) => Response::ok(),
            Err(e) => Response::error(exit_codes::ERROR, format!("Resize error: {e}")),
        }
    }

    fn get_status(&mut self, session_name: &str) -> Response {
        let Some(session) = self.sessions.get_mut(session_name) else {
            return Response::session_not_found(session_name);
        };
        Response::ok_with(ResponseData::Session(session.info()))
    }

    fn list_sessions(&mut self) -> Response {
        let sessions: Vec<_> = self.sessions.values_mut().map(|s| s.info()).collect();
        Response::ok_with(ResponseData::Sessions(sessions))
    }

    fn get_scrollback(&mut self, session_name: &str, lines: Option<u32>) -> Response {
        let Some(session) = self.sessions.get_mut(session_name) else {
            return Response::session_not_found(session_name);
        };
        match session.scrollback_text(lines) {
            Ok(scrollback_lines) => {
                let text = scrollback_lines.join("\n");
                Response::ok_with(ResponseData::Text(text))
            }
            Err(e) => Response::error(exit_codes::ERROR, format!("Scrollback error: {e}")),
        }
    }

    fn kill_session(&mut self, session_name: &str, signal: Option<i32>) -> Response {
        let Some(session) = self.sessions.get_mut(session_name) else {
            return Response::session_not_found(session_name);
        };
        let sig = signal
            .and_then(|s| Signal::try_from(s).ok())
            .unwrap_or(Signal::SIGTERM);
        match session.pty.kill_child(sig) {
            Ok(()) => Response::ok(),
            Err(e) => Response::error(exit_codes::ERROR, format!("Kill error: {e}")),
        }
    }
}
