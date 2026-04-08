mod commands;
mod connection;
mod daemon_ctl;
mod output;

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "phantom",
    about = "Headless TUI interaction for AI agents and integration tests",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Force JSON output
    #[arg(long, global = true)]
    json: bool,

    /// Force human-readable output
    #[arg(long, global = true)]
    human: bool,

    /// Custom daemon socket path
    #[arg(long, global = true)]
    socket: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Spawn a TUI application in a new session
    Run {
        /// Session name (auto-generated if omitted)
        #[arg(short, long)]
        session: Option<String>,
        /// Terminal columns
        #[arg(long, default_value = "80")]
        cols: u16,
        /// Terminal rows
        #[arg(long, default_value = "24")]
        rows: u16,
        /// Max scrollback lines
        #[arg(long, default_value = "1000")]
        scrollback: u32,
        /// Environment variables (KEY=VALUE)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        envs: Vec<String>,
        /// Working directory
        #[arg(long)]
        cwd: Option<String>,
        /// Command and arguments
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Send input to a session
    Send {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Type text character by character
        #[arg(long = "type", value_name = "TEXT")]
        type_text: Option<String>,
        /// Send key sequences (e.g. ctrl-c, enter, f1)
        #[arg(long, value_name = "KEY")]
        key: Vec<String>,
        /// Send bracketed paste
        #[arg(long)]
        paste: Option<String>,
        /// Send mouse event (e.g. click:10,5)
        #[arg(long)]
        mouse: Option<String>,
        /// Delay between typed characters in ms
        #[arg(long, default_value = "0")]
        delay: u64,
    },
    /// Capture screen content
    Screenshot {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
        /// Region to capture: top,left,bottom,right (0-indexed)
        #[arg(long)]
        region: Option<String>,
    },
    /// Wait for a condition
    Wait {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Wait for text to appear
        #[arg(long)]
        text: Option<String>,
        /// Wait for regex match
        #[arg(long)]
        regex: Option<String>,
        /// Wait for screen to stabilize
        #[arg(long)]
        stable: bool,
        /// Duration screen must be stable (ms)
        #[arg(long, default_value = "500")]
        stable_duration: u64,
        /// Wait for cursor at position (x,y)
        #[arg(long)]
        cursor: Option<String>,
        /// Wait for cursor to be visible
        #[arg(long)]
        cursor_visible: bool,
        /// Wait for cursor to be hidden
        #[arg(long)]
        cursor_hidden: bool,
        /// Wait for process to exit
        #[arg(long)]
        process_exit: bool,
        /// Expected exit code
        #[arg(long)]
        exit_code: Option<i32>,
        /// Wait for text to disappear
        #[arg(long)]
        text_disappear: Option<String>,
        /// Wait for screen to change from current state
        #[arg(long)]
        changed: bool,
        /// Timeout in ms
        #[arg(long, default_value = "10000")]
        timeout: u64,
        /// Poll interval in ms
        #[arg(long, default_value = "50")]
        poll: u64,
    },
    /// Query cursor position and style
    Cursor {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
    },
    /// Inspect a single cell's content and attributes
    Cell {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Column (0-indexed)
        #[arg(long)]
        x: u16,
        /// Row (0-indexed)
        #[arg(long)]
        y: u16,
    },
    /// Get process output (what was written to stdout after TUI exit)
    Output {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
    },
    /// Dump scrollback buffer
    Scrollback {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Number of lines
        #[arg(long)]
        lines: Option<u32>,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Resize a session's terminal
    Resize {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// New column count
        #[arg(long)]
        cols: u16,
        /// New row count
        #[arg(long)]
        rows: u16,
    },
    /// Query session status
    Status {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
    },
    /// Live view of a session (updates in real-time)
    Monitor {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Refresh rate in fps
        #[arg(long, default_value = "30")]
        fps: u64,
    },
    /// Save or compare screen snapshots
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },
    /// Run commands from a file
    Batch {
        /// Path to command file
        file: String,
    },
    /// List all active sessions
    List,
    /// Terminate a session
    Kill {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Signal number (default: SIGTERM)
        #[arg(long)]
        signal: Option<i32>,
    },
    /// Manage the daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum SnapshotAction {
    /// Save current screen to a file
    Save {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Output file path
        #[arg(short, long, required = true)]
        file: String,
    },
    /// Compare current screen against a saved snapshot
    Diff {
        /// Session name
        #[arg(short, long, required = true)]
        session: String,
        /// Reference file to compare against
        #[arg(short, long, required = true)]
        file: String,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon
    Start {
        /// Run in foreground
        #[arg(long)]
        foreground: bool,
        /// Socket path
        #[arg(long)]
        socket: Option<String>,
    },
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let output_mode = output::OutputMode::detect(cli.json, cli.human);

    if let Some(socket) = &cli.socket {
        daemon_ctl::set_socket_path(socket);
    }

    let result = match cli.command {
        Commands::Run {
            session,
            cols,
            rows,
            scrollback,
            envs,
            cwd,
            command,
        } => {
            commands::run::execute(
                session,
                cols,
                rows,
                scrollback,
                envs,
                cwd,
                command,
                output_mode,
            )
            .await
        }
        Commands::Send {
            session,
            type_text,
            key,
            paste,
            mouse,
            delay,
        } => commands::send::execute(session, type_text, key, paste, mouse, delay).await,
        Commands::Screenshot {
            session,
            format,
            region,
        } => commands::screenshot::execute(session, format, region, output_mode).await,
        Commands::Wait {
            session,
            text,
            regex,
            stable,
            stable_duration,
            cursor,
            cursor_visible,
            cursor_hidden,
            process_exit,
            exit_code,
            text_disappear,
            changed,
            timeout,
            poll,
        } => {
            commands::wait::execute(
                session,
                text,
                regex,
                stable,
                stable_duration,
                cursor,
                cursor_visible,
                cursor_hidden,
                process_exit,
                exit_code,
                text_disappear,
                changed,
                timeout,
                poll,
            )
            .await
        }
        Commands::Cursor { session } => commands::cursor::execute(session, output_mode).await,
        Commands::Cell { session, x, y } => {
            commands::cell::execute(session, x, y, output_mode).await
        }
        Commands::Output { session } => commands::output::execute(session).await,
        Commands::Scrollback {
            session,
            lines,
            format,
        } => commands::scrollback::execute(session, lines, format, output_mode).await,
        Commands::Resize {
            session,
            cols,
            rows,
        } => commands::resize::execute(session, cols, rows).await,
        Commands::Status { session } => commands::status::execute(session, output_mode).await,
        Commands::Monitor { session, fps } => commands::monitor::execute(session, fps).await,
        Commands::Snapshot { action } => match action {
            SnapshotAction::Save { session, file } => commands::snapshot::save(session, file).await,
            SnapshotAction::Diff { session, file } => commands::snapshot::diff(session, file).await,
        },
        Commands::Batch { file } => commands::batch::execute(file).await,
        Commands::List => commands::list::execute(output_mode).await,
        Commands::Kill { session, signal } => commands::kill::execute(session, signal).await,
        Commands::Daemon { action } => match action {
            DaemonAction::Start { foreground, socket } => {
                commands::daemon::start(foreground, socket).await
            }
            DaemonAction::Stop => commands::daemon::stop().await,
            DaemonAction::Status => commands::daemon::status().await,
        },
        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "phantom",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(phantom_core::exit_codes::ERROR);
    }
    Ok(())
}
