# phantom (`pt`)

[![CI](https://github.com/alexpasmantier/phantom/actions/workflows/ci.yml/badge.svg)](https://github.com/alexpasmantier/phantom/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/phantom-test.svg)](https://crates.io/crates/phantom-test)

Phantom is a headless TUI interaction tool. Spawn any terminal application, send input, read the screen, and wait for conditions — all programmatically. Built on [libghostty-vt](https://github.com/ghostty-org/ghostty) (Ghostty's terminal emulation core).

## Use Cases

- **AI agent tool use** — let an LLM interact with TUI applications (navigate, read screen contents, send commands)
- **Integration testing** — write deterministic tests for TUI apps without a real terminal
- **Automation** — script interactions with any terminal application

## Installation

### From source

Requires [Rust](https://rustup.rs/) nightly and [Zig](https://ziglang.org/download/) 0.15.x.

```bash
git clone https://github.com/alexpasmantier/phantom.git
cd phantom
cargo build --release --workspace
# Binaries are in target/release/phantom and target/release/phantom-daemon
```

### From GitHub releases

Download pre-built binaries for your platform from the [Releases](https://github.com/alexpasmantier/phantom/releases) page.

### Shell Completions

```bash
# Bash
phantom completions bash > ~/.local/share/bash-completion/completions/phantom

# Zsh
phantom completions zsh > ~/.zfunc/_phantom

# Fish
phantom completions fish > ~/.config/fish/completions/phantom.fish
```

## Quick Start

```bash
# Start a session running vim
phantom run -s editor -- vim

# Wait for vim to be ready
phantom wait -s editor --text "~" --timeout 5000

# Type some text
phantom send -s editor --type "iHello, world!"
phantom send -s editor --key escape

# Take a screenshot
phantom screenshot -s editor

# Clean up
phantom send -s editor --type ":q!\n"
phantom wait -s editor --process-exit
```

The daemon starts automatically on the first command and runs in the background.

## CLI Reference

`phantom` (or its short alias `pt`) is a stateless CLI that talks to a background daemon over a Unix socket. The daemon manages terminal sessions using libghostty-vt.

### Session Management

```bash
phantom run -s NAME -- COMMAND [ARGS...]     # Spawn a TUI in a new session
phantom run -s app --cols 120 --rows 40 \    # Custom terminal size
  --env TERM=xterm-256color -- ./my-tui

phantom list                                 # List all sessions
phantom status -s NAME                       # Session status (running/exited)
phantom kill -s NAME                         # Terminate (SIGTERM)
phantom kill -s NAME --signal 9              # SIGKILL
```

### Sending Input

```bash
phantom send -s NAME --type "hello"          # Type text character by character
phantom send -s NAME --type "hello" --delay 50  # Type with 50ms delay between chars
phantom send -s NAME --key enter             # Send a key
phantom send -s NAME --key ctrl-c            # Key combo
phantom send -s NAME --key f1                # Function key
phantom send -s NAME --paste "block of text" # Bracketed paste
phantom send -s NAME --mouse click:10,5      # Mouse click at column 10, row 5
```

### Reading the Screen

```bash
phantom screenshot -s NAME                   # Plain text screenshot
phantom screenshot -s NAME --format json     # Full cell data (grapheme, fg, bg, attrs)
phantom screenshot -s NAME --region 0,0,5,40 # Region: top,left,bottom,right

phantom cursor -s NAME                       # Cursor position and style
phantom cell -s NAME --x 0 --y 0            # Inspect a single cell
phantom scrollback -s NAME --lines 100       # Scrollback buffer
phantom output -s NAME                       # Captured stdout (after process exits)
```

### Waiting for Conditions

```bash
phantom wait -s NAME --text "Ready"          # Wait for text on screen
phantom wait -s NAME --regex "v\d+\.\d+"     # Wait for regex match
phantom wait -s NAME --stable                # Wait for screen to stop changing
phantom wait -s NAME --changed               # Wait for any screen change
phantom wait -s NAME --process-exit          # Wait for process to exit
phantom wait -s NAME --exit-code 0           # Wait for specific exit code
phantom wait -s NAME --cursor 0,0            # Wait for cursor at position
phantom wait -s NAME --text-disappear "Loading"  # Wait for text to disappear
phantom wait -s NAME --timeout 5000          # Custom timeout (default: 10s)
```

Multiple conditions can be combined and all must be satisfied:

```bash
phantom wait -s NAME --text "Done" --stable --timeout 5000
```

### Snapshots

```bash
phantom snapshot save -s NAME -f baseline.txt   # Save screen to file
phantom snapshot diff -s NAME -f baseline.txt   # Compare against saved (exit 1 if different)
```

### Live Monitoring

```bash
phantom monitor -s NAME              # Real-time view of a session
phantom monitor -s NAME --fps 60     # Higher refresh rate
```

### Batch Commands

```bash
phantom batch commands.txt           # Run commands from a file
```

Batch file format (one command per line, `#` for comments):

```
run -s app -- vim
wait -s app --text "~" --timeout 5000
send -s app --type "iHello"
screenshot -s app
send -s app --type ":q!\n"
wait -s app --process-exit
```

### Daemon Management

```bash
phantom daemon start                 # Start daemon (auto-starts on first command)
phantom daemon start --foreground    # Run in foreground (for debugging)
phantom daemon status                # Check daemon status and session count
phantom daemon stop                  # Stop daemon and all sessions
```

### Global Options

| Flag | Description |
|------|-------------|
| `--json` | Force JSON output (default when stdout is not a TTY) |
| `--human` | Force human-readable output |
| `--socket PATH` | Custom daemon socket path |
| `--version` | Show version |

## Rust Library (`phantom-test`)

Write TUI integration tests in Rust without a daemon process — the terminal emulation engine runs in-process.

```rust
use phantom_test::Phantom;

let pt = Phantom::new()?;
let s = pt.run("vim").args(&["--clean", "-u", "NONE"]).start()?;

s.wait().text("~").timeout_ms(5000).until()?;
s.send().type_text("iHello")?;
s.send().key("escape")?;

let screen = s.screenshot()?;
assert!(screen.contains("Hello"));

s.send().type_text(":q!\n")?;
s.wait().process_exit().until()?;
```

### TestRunner

For structured test suites with an optional live TUI monitor:

```rust
use phantom_test::TestRunner;

TestRunner::new()
    .test("vim insert and quit", |pt| {
        let s = pt.run("vim").args(&["--clean", "-u", "NONE"]).start()?;
        s.wait().text("~").timeout_ms(10000).until()?;
        s.send().type_text("iHello")?;
        s.wait().text("Hello").until()?;
        s.send().key("escape")?;
        s.send().type_text(":q!\n")?;
        s.wait().process_exit().until()?;
        Ok(())
    })
    .run(); // pass --monitor for live TUI
```

Add `phantom-test` to your `Cargo.toml`:

```toml
[dev-dependencies]
phantom-test = "0.1"

# Enable the live TUI monitor (optional)
# phantom-test = { version = "0.1", features = ["monitor"] }
```

## Exit Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 0 | `SUCCESS` | Command completed successfully |
| 1 | `ERROR` | General error |
| 2 | `SESSION_NOT_FOUND` | Named session does not exist |
| 3 | `WAIT_TIMEOUT` | Wait condition was not met before timeout |
| 4 | `PROCESS_EXITED` | Process exited (used with status/wait) |
| 5 | `SESSION_COLLISION` | Session name already in use |

## Architecture

```
phantom CLI  --[unix socket]-->  phantom-daemon  --[PTY]-->  child process
                                      |
                                 libghostty-vt
                              (terminal emulation)
```

Phantom uses a daemon + stateless CLI architecture. The daemon owns all terminal state on a dedicated engine thread (required because libghostty-vt types are `!Send`). The CLI connects over a Unix socket, sends a JSON request, and gets a JSON response.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full architecture guide.

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, architecture details, and guidelines.

## License

[MIT](LICENSE)
