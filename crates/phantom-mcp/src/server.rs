//! MCP server: exposes phantom-test as a set of tools an LLM agent can call.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use phantom_test::{Phantom, PhantomError, Session};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{render, tmux};

// ── Parameter structs ────────────────────────────────────────

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}
fn default_timeout_ms() -> u64 {
    10_000
}
fn default_screenshot_format() -> String {
    "text".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunArgs {
    /// The command to run (e.g., "vim", "bash", "less").
    pub command: String,
    /// Arguments passed to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional explicit session name. Auto-generated if omitted.
    #[serde(default)]
    pub name: Option<String>,
    /// Terminal column count.
    #[serde(default = "default_cols")]
    pub cols: u16,
    /// Terminal row count.
    #[serde(default = "default_rows")]
    pub rows: u16,
    /// Working directory for the spawned process.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Environment variables in `KEY=VALUE` form.
    #[serde(default)]
    pub env: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionArgs {
    /// Session name.
    pub session: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendArgs {
    /// Session name.
    pub session: String,
    /// One of: `text`, `key`, `paste`, `mouse`.
    pub kind: String,
    /// Payload: typed text, a key spec like `ctrl-c` / `enter` / `f1`,
    /// pasted text, or a mouse spec like `click:10,5`.
    pub value: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WaitArgs {
    /// Session name.
    pub session: String,
    /// Wait for this substring to appear on screen.
    #[serde(default)]
    pub text: Option<String>,
    /// Wait for this substring to disappear from screen.
    #[serde(default)]
    pub text_absent: Option<String>,
    /// Wait for a regex pattern to match the screen content.
    #[serde(default)]
    pub regex: Option<String>,
    /// Wait for the screen to be visually stable for this many milliseconds.
    #[serde(default)]
    pub stable_ms: Option<u64>,
    /// Wait for the process to exit (any exit code).
    #[serde(default)]
    pub process_exit: Option<bool>,
    /// Wait for the process to exit with this specific code (implies process_exit).
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Wait for the cursor to reach a specific (x, y) position.
    #[serde(default)]
    pub cursor_at: Option<CursorPos>,
    /// Wait for the cursor to become visible (true) or hidden (false).
    #[serde(default)]
    pub cursor_visible: Option<bool>,
    /// Wait for the screen content to change from its current state at the moment of this call.
    #[serde(default)]
    pub screen_changed: Option<bool>,
    /// Maximum wait duration in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CursorPos {
    /// Column (0-indexed).
    pub x: u16,
    /// Row (0-indexed).
    pub y: u16,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenshotArgs {
    /// Session name.
    pub session: String,
    /// `text` for plain text rows, `image` for a rendered PNG of the screen.
    #[serde(default = "default_screenshot_format")]
    pub format: String,
    /// Optional region to capture (top/left/bottom/right, 0-indexed, inclusive).
    /// If omitted the full screen is returned.
    #[serde(default)]
    pub region: Option<Region>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Region {
    /// Top row (0-indexed, inclusive).
    pub top: u16,
    /// Left column (0-indexed, inclusive).
    pub left: u16,
    /// Bottom row (0-indexed, inclusive).
    pub bottom: u16,
    /// Right column (0-indexed, inclusive).
    pub right: u16,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CellArgs {
    /// Session name.
    pub session: String,
    /// Column (0-indexed).
    pub x: u16,
    /// Row (0-indexed).
    pub y: u16,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResizeArgs {
    /// Session name.
    pub session: String,
    /// New column count.
    pub cols: u16,
    /// New row count.
    pub rows: u16,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowArgs {
    /// Session name to display.
    pub session: String,
}

// ── Server ───────────────────────────────────────────────────

#[derive(Clone)]
pub struct PhantomMcpServer {
    phantom: Arc<Phantom>,
    sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
    /// Path to the observer Unix socket, if one is bound. Used by
    /// `phantom_show` to point `phantom monitor` at the right socket.
    observer_socket: Option<PathBuf>,
    tool_router: ToolRouter<Self>,
}

impl PhantomMcpServer {
    pub fn new() -> anyhow::Result<Self> {
        let phantom = Phantom::new()
            .map_err(|e| anyhow::anyhow!("failed to start phantom engine: {e}"))?;
        Ok(Self {
            phantom: Arc::new(phantom),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            observer_socket: None,
            tool_router: Self::tool_router(),
        })
    }

    /// Builder-style: record where the observer socket lives so the
    /// `phantom_show` tool can hand the path to `phantom monitor`.
    pub fn with_observer_socket(mut self, path: PathBuf) -> Self {
        self.observer_socket = Some(path);
        self
    }

    /// Engine command sender + waker, for callers that want to bind their
    /// own observer listener (the binary does this in `main.rs`).
    pub fn engine_handle(
        &self,
    ) -> (
        crossbeam_channel::Sender<phantom_daemon::engine::EngineCommand>,
        Arc<mio::Waker>,
    ) {
        self.phantom.engine_handle()
    }

    async fn get_session(&self, name: &str) -> Result<Arc<Session>, McpError> {
        let map = self.sessions.lock().await;
        map.get(name).cloned().ok_or_else(|| {
            McpError::invalid_params(format!("no session named '{name}'"), None)
        })
    }
}

#[tool_router]
impl PhantomMcpServer {
    #[tool(
        description = "Spawn a TUI program in a new headless terminal session. \
                       Returns the session name to use in subsequent tool calls. \
                       The terminal defaults to 80x24 unless overridden."
    )]
    pub async fn phantom_run(
        &self,
        Parameters(args): Parameters<RunArgs>,
    ) -> Result<CallToolResult, McpError> {
        let arg_refs: Vec<&str> = args.args.iter().map(|s| s.as_str()).collect();
        let mut builder = self
            .phantom
            .run(&args.command)
            .args(&arg_refs)
            .size(args.cols, args.rows);
        if let Some(name) = &args.name {
            builder = builder.name(name);
        }
        if let Some(cwd) = &args.cwd {
            builder = builder.cwd(cwd);
        }
        for entry in &args.env {
            if let Some((k, v)) = entry.split_once('=') {
                builder = builder.env(k, v);
            }
        }

        let session = builder.start().map_err(to_mcp_err)?;
        let name = session.name().to_string();
        self.sessions
            .lock()
            .await
            .insert(name.clone(), Arc::new(session));

        Ok(CallToolResult::success(vec![Content::text(format!(
            "session '{name}' started ({}x{})",
            args.cols, args.rows
        ))]))
    }

    #[tool(
        description = "Send input to a session. `kind`: \
                       `text` (typed characters), \
                       `key` (named key like `enter`, `escape`, `ctrl-c`, `f1`, `up`), \
                       `paste` (bracketed paste), \
                       `mouse` (e.g. `click:10,5`, `right-click:20,10`, `scroll-up:0,0`)."
    )]
    pub async fn phantom_send(
        &self,
        Parameters(args): Parameters<SendArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        let result = match args.kind.as_str() {
            "text" => session.send().type_text(&args.value),
            "key" => session.send().key(&args.value),
            "paste" => session.send().paste(&args.value),
            "mouse" => session.send().mouse(&args.value),
            other => {
                return Err(McpError::invalid_params(
                    format!("unknown send kind '{other}' (expected text/key/paste/mouse)"),
                    None,
                ));
            }
        };
        result.map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text("ok")]))
    }

    #[tool(
        description = "Block until one or more conditions are met on a session. \
                       Conditions are AND-ed: all must hold simultaneously. Available conditions: \
                       `text` (substring appears), \
                       `text_absent` (substring disappears), \
                       `regex` (pattern matches screen), \
                       `stable_ms` (screen unchanged for N ms), \
                       `process_exit` / `exit_code` (process terminates), \
                       `cursor_at` (cursor reaches x/y), \
                       `cursor_visible` (cursor visibility), \
                       `screen_changed` (screen differs from now). \
                       At least one condition is required. Returns an error on timeout."
    )]
    pub async fn phantom_wait(
        &self,
        Parameters(args): Parameters<WaitArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        let WaitArgs {
            text,
            text_absent,
            regex,
            stable_ms,
            process_exit,
            exit_code,
            cursor_at,
            cursor_visible,
            screen_changed,
            timeout_ms,
            ..
        } = args;

        // Validate the regex up-front so we can return a clean error rather
        // than panicking inside the wait builder.
        if let Some(ref pattern) = regex {
            ::regex::Regex::new(pattern).map_err(|e| {
                McpError::invalid_params(format!("invalid regex: {e}"), None)
            })?;
        }

        // wait blocks on the engine; run it on a blocking worker so we don't
        // hold up an async runtime thread.
        let outcome = tokio::task::spawn_blocking(move || {
            let mut wait = session.wait().timeout_ms(timeout_ms);
            let mut has_condition = false;
            if let Some(ref t) = text {
                wait = wait.text(t);
                has_condition = true;
            }
            if let Some(ref t) = text_absent {
                wait = wait.text_absent(t);
                has_condition = true;
            }
            if let Some(ref pattern) = regex {
                wait = wait.regex(pattern);
                has_condition = true;
            }
            if let Some(ms) = stable_ms {
                wait = wait.stable(ms);
                has_condition = true;
            }
            // exit_code implies process_exit; honor whichever is more specific.
            if let Some(code) = exit_code {
                wait = wait.exit_code(code);
                has_condition = true;
            } else if process_exit.unwrap_or(false) {
                wait = wait.process_exit();
                has_condition = true;
            }
            if let Some(pos) = cursor_at {
                wait = wait.cursor_at(pos.x, pos.y);
                has_condition = true;
            }
            match cursor_visible {
                Some(true) => {
                    wait = wait.cursor_visible();
                    has_condition = true;
                }
                Some(false) => {
                    wait = wait.cursor_hidden();
                    has_condition = true;
                }
                None => {}
            }
            if screen_changed.unwrap_or(false) {
                wait = wait.screen_changed();
                has_condition = true;
            }
            if !has_condition {
                return Err(PhantomError::Internal(anyhow::anyhow!(
                    "phantom_wait requires at least one condition (text, text_absent, regex, \
                     stable_ms, process_exit, exit_code, cursor_at, cursor_visible, screen_changed)"
                )));
            }
            wait.until()
        })
        .await
        .map_err(|e| McpError::internal_error(format!("wait join failed: {e}"), None))?;

        outcome.map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text("condition met")]))
    }

    #[tool(
        description = "Capture the current screen of a session. \
                       `format=text` returns plain text rows joined by newlines. \
                       `format=image` returns a PNG rendering of the terminal — preferred \
                       when you need to visually understand layout, colors, or cursor placement. \
                       `region` (top/left/bottom/right, 0-indexed inclusive) restricts the capture \
                       to a sub-rectangle of the screen."
    )]
    pub async fn phantom_screenshot(
        &self,
        Parameters(args): Parameters<ScreenshotArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        if let Some(ref r) = args.region
            && (r.top > r.bottom || r.left > r.right)
        {
            return Err(McpError::invalid_params(
                format!(
                    "invalid region: top={} left={} bottom={} right={} (top<=bottom and left<=right required)",
                    r.top, r.left, r.bottom, r.right
                ),
                None,
            ));
        }

        match args.format.as_str() {
            "text" => {
                let screen = if let Some(r) = args.region {
                    session
                        .screenshot_region(r.top, r.left, r.bottom, r.right)
                        .map_err(to_mcp_err)?
                } else {
                    session.screenshot().map_err(to_mcp_err)?
                };
                Ok(CallToolResult::success(vec![Content::text(
                    screen.text().to_string(),
                )]))
            }
            "image" => {
                let region_tuple = args.region.as_ref().map(|r| (r.top, r.left, r.bottom, r.right));
                let screen = if let Some((top, left, bottom, right)) = region_tuple {
                    session
                        .screenshot_region_json(top, left, bottom, right)
                        .map_err(to_mcp_err)?
                } else {
                    session.screenshot_json().map_err(to_mcp_err)?
                };
                let png = render::render_png(&screen, region_tuple).map_err(|e| {
                    McpError::internal_error(format!("png render failed: {e}"), None)
                })?;
                use base64::Engine as _;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
                Ok(CallToolResult::success(vec![Content::image(b64, "image/png")]))
            }
            other => Err(McpError::invalid_params(
                format!("unknown screenshot format '{other}' (expected text or image)"),
                None,
            )),
        }
    }

    #[tool(description = "Get the cursor position and visibility of a session.")]
    pub async fn phantom_cursor(
        &self,
        Parameters(args): Parameters<SessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        let cursor = session.cursor().map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "({}, {}) {}",
            cursor.x,
            cursor.y,
            if cursor.visible { "visible" } else { "hidden" }
        ))]))
    }

    #[tool(
        description = "Inspect a single cell at column `x`, row `y`. \
                       Returns its grapheme and style attributes (fg/bg/bold/italic/...) as JSON."
    )]
    pub async fn phantom_cell(
        &self,
        Parameters(args): Parameters<CellArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        let cell = session.cell(args.x, args.y).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&cell).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Get the scrollback buffer (lines that have scrolled off the visible viewport) \
                       of a session as plain text."
    )]
    pub async fn phantom_scrollback(
        &self,
        Parameters(args): Parameters<SessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        let text = session.scrollback(None).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "Get the process output — what a TUI like fzf or fzy wrote to stdout \
                       after exiting alternate-screen mode. Useful for capturing a selection."
    )]
    pub async fn phantom_output(
        &self,
        Parameters(args): Parameters<SessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        let text = session.output().map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "Get session status as JSON: running/exited, dimensions, title, working directory."
    )]
    pub async fn phantom_status(
        &self,
        Parameters(args): Parameters<SessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        let info = session.status().map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&info).unwrap_or_default(),
        )]))
    }

    #[tool(description = "List all active sessions as a JSON array.")]
    pub async fn phantom_list(&self) -> Result<CallToolResult, McpError> {
        let sessions = self.phantom.sessions().map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&sessions).unwrap_or_default(),
        )]))
    }

    #[tool(description = "Terminate a session (sends SIGTERM to its process).")]
    pub async fn phantom_kill(
        &self,
        Parameters(args): Parameters<SessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        session.kill().map_err(to_mcp_err)?;
        self.sessions.lock().await.remove(&args.session);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "killed '{}'",
            args.session
        ))]))
    }

    #[tool(description = "Resize a session's terminal to the given column and row count.")]
    pub async fn phantom_resize(
        &self,
        Parameters(args): Parameters<ResizeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.get_session(&args.session).await?;
        session.resize(args.cols, args.rows).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "resized to {}x{}",
            args.cols, args.rows
        ))]))
    }

    #[tool(
        description = "Open a live viewer for a session in a tmux pane next to the user's chat. \
                       Only works when the user is running Claude Code inside tmux. \
                       If tmux isn't available this is a graceful no-op — you should still call \
                       it after `phantom_run` so tmux users get a live view automatically. \
                       The split direction can be set via the PHANTOM_MCP_SPLIT env var \
                       (`horizontal` (default), `vertical`, or `popup`)."
    )]
    pub async fn phantom_show(
        &self,
        Parameters(args): Parameters<ShowArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Confirm the session exists before opening a viewer for it.
        let _ = self.get_session(&args.session).await?;

        if !tmux::in_tmux() {
            return Ok(CallToolResult::success(vec![Content::text(
                "skipped: not running inside tmux. The user can still see what you're \
                 doing by calling phantom_screenshot — text screenshots render inline in chat.",
            )]));
        }

        let socket = self.observer_socket.as_deref().ok_or_else(|| {
            McpError::internal_error(
                "no observer socket configured (phantom-mcp was started without one)",
                None,
            )
        })?;

        let mode = tmux::SplitMode::from_env();
        match tmux::open_viewer(&args.session, socket, mode) {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Err(McpError::internal_error(
                format!("failed to open viewer: {e}"),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for PhantomMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(
                "Drive headless TUI programs (vim, less, fzf, htop, lazygit, …) from an agent. \
                 \n\nTypical workflow: \
                 \n  1. phantom_run to spawn the program \
                 \n  2. phantom_show right after phantom_run, so tmux users get a live viewer \
                 \n     pane; this is a no-op for non-tmux users so always call it \
                 \n  3. phantom_wait (stable_ms or text) until the UI is ready \
                 \n  4. phantom_screenshot (use format='image' for visual grounding) \
                 \n  5. phantom_send to type text or press keys \
                 \n  6. repeat 3–5 \
                 \n  7. phantom_kill when done \
                 \n\nAlways wait for the screen to settle before screenshotting after a send — \
                 TUIs redraw asynchronously. The user only sees what you screenshot \
                 (image content blocks may be hidden in chat) and what phantom_show streams \
                 to their tmux pane, so screenshot generously and call phantom_show every \
                 time you start a new session."
                    .to_string(),
            )
    }
}

fn to_mcp_err(e: PhantomError) -> McpError {
    match e {
        PhantomError::SessionNotFound(name) => {
            McpError::invalid_params(format!("session '{name}' not found"), None)
        }
        PhantomError::SessionCollision(name) => {
            McpError::invalid_params(format!("session '{name}' already exists"), None)
        }
        PhantomError::WaitTimeout => {
            McpError::invalid_params("wait condition timed out".to_string(), None)
        }
        PhantomError::ProcessExited => {
            McpError::invalid_params("process has already exited".to_string(), None)
        }
        PhantomError::Engine { code, message } => {
            McpError::invalid_params(format!("engine error ({code}): {message}"), None)
        }
        PhantomError::EngineStartFailed(msg) => {
            McpError::internal_error(format!("engine start failed: {msg}"), None)
        }
        PhantomError::EngineTimeout => {
            McpError::internal_error("engine did not respond".to_string(), None)
        }
        PhantomError::Internal(e) => {
            McpError::internal_error(format!("internal: {e}"), None)
        }
    }
}
