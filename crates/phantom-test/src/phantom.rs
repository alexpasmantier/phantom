use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::Sender;
use mio::Waker;
use phantom_core::protocol::{Response, ResponseData};
use phantom_core::types::SessionInfo;
use phantom_daemon::engine::{Engine, EngineCommand};

use crate::builder::SessionBuilder;
use crate::error::{PhantomError, response_to_result};

const DEFAULT_TIMEOUT_MS: u64 = 10_000;

pub(crate) struct PhantomInner {
    pub cmd_tx: Sender<EngineCommand>,
    pub waker: Arc<Waker>,
    pub default_timeout_ms: u64,
}

pub(crate) type SessionHook = Arc<dyn Fn(Arc<PhantomInner>, String) + Send + Sync>;

/// The phantom engine handle. Spawns a terminal emulation engine on a
/// background thread. Create sessions from it to drive TUI applications.
pub struct Phantom {
    pub(crate) inner: Arc<PhantomInner>,
    engine_thread: Option<JoinHandle<()>>,
    session_counter: AtomicU64,
    pub(crate) on_session_created: Option<SessionHook>,
}

impl Phantom {
    /// Create a new phantom engine.
    pub fn new() -> crate::Result<Self> {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (waker_tx, waker_rx) = crossbeam_channel::bounded::<Arc<Waker>>(1);

        let engine_thread = std::thread::Builder::new()
            .name("phantom-engine".into())
            .spawn(move || {
                let (mut engine, waker) = Engine::new(cmd_rx).expect("failed to create engine");
                let _ = waker_tx.send(Arc::new(waker));
                if let Err(e) = engine.run() {
                    eprintln!("Engine error: {e}");
                }
            })
            .map_err(|e| PhantomError::EngineStartFailed(e.to_string()))?;

        let waker = waker_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| PhantomError::EngineStartFailed("engine thread did not start".into()))?;

        Ok(Self {
            inner: Arc::new(PhantomInner {
                cmd_tx,
                waker,
                default_timeout_ms: DEFAULT_TIMEOUT_MS,
            }),
            engine_thread: Some(engine_thread),
            session_counter: AtomicU64::new(0),
            on_session_created: None,
        })
    }

    /// Start building a new session that will run the given command.
    pub fn run(&self, command: &str) -> SessionBuilder<'_> {
        SessionBuilder::new(self, command)
    }

    /// List all active sessions.
    pub fn sessions(&self) -> crate::Result<Vec<SessionInfo>> {
        let resp = self.inner.send_command(|reply| EngineCommand::ListSessions { reply })?;
        match response_to_result(resp)? {
            Some(ResponseData::Sessions(s)) => Ok(s),
            _ => Ok(Vec::new()),
        }
    }

    pub(crate) fn next_session_name(&self) -> String {
        let n = self.session_counter.fetch_add(1, Ordering::Relaxed);
        format!("session-{n}")
    }
}

impl PhantomInner {
    /// Send a command to the engine and wait for its response.
    pub fn send_command(
        &self,
        make_cmd: impl FnOnce(Sender<Response>) -> EngineCommand,
    ) -> crate::Result<Response> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        let cmd = make_cmd(reply_tx);
        self.cmd_tx
            .send(cmd)
            .map_err(|e| PhantomError::EngineStartFailed(e.to_string()))?;
        self.waker
            .wake()
            .map_err(|e| PhantomError::Internal(e.into()))?;
        reply_rx
            .recv_timeout(Duration::from_secs(30))
            .map_err(|_| PhantomError::EngineTimeout)
    }

    /// Send a wait command — uses a longer receive timeout since the engine
    /// holds the reply until the wait condition is met or its own timeout fires.
    pub fn send_wait_command(
        &self,
        make_cmd: impl FnOnce(Sender<Response>) -> EngineCommand,
        timeout: Duration,
    ) -> crate::Result<Response> {
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        let cmd = make_cmd(reply_tx);
        self.cmd_tx
            .send(cmd)
            .map_err(|e| PhantomError::EngineStartFailed(e.to_string()))?;
        self.waker
            .wake()
            .map_err(|e| PhantomError::Internal(e.into()))?;
        reply_rx
            .recv_timeout(timeout + Duration::from_secs(5))
            .map_err(|_| PhantomError::EngineTimeout)
    }
}

impl Drop for Phantom {
    fn drop(&mut self) {
        let _ = self.inner.cmd_tx.send(EngineCommand::Shutdown);
        let _ = self.inner.waker.wake();
        if let Some(handle) = self.engine_thread.take() {
            let _ = handle.join();
        }
    }
}
