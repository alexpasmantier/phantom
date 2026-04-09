# phantom (`pt`)

[![CI](https://github.com/alexpasmantier/phantom/actions/workflows/ci.yml/badge.svg)](https://github.com/alexpasmantier/phantom/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/phantom-test.svg)](https://crates.io/crates/phantom-test)

Phantom lets you programmatically drive any terminal application — spawn it, send input, read the screen, wait for conditions. Built on [libghostty-vt](https://github.com/ghostty-org/ghostty) (Ghostty's terminal emulation core).

**Use cases:** AI agent tool use, TUI integration testing, terminal automation.

<p align="center">
  <img src="assets/demo.gif" alt="phantom test runner with live TUI monitor" width="800">
</p>

## Quick Start

```bash
# Spawn vim in a headless session
pt run -s editor -- vim

# Wait for it, type something, read the screen
pt wait -s editor --text "~" --timeout 5000
pt send -s editor --type "iHello, world!"
pt send -s editor --key escape
pt screenshot -s editor

# Clean up
pt send -s editor --type ":q!\n"
pt wait -s editor --process-exit
```

The daemon starts automatically on the first command.

## Installation

Requires [Rust](https://rustup.rs/) nightly and [Zig](https://ziglang.org/download/) 0.15.x.

```bash
cargo build --release --workspace
```

Pre-built binaries are available on the [Releases](https://github.com/alexpasmantier/phantom/releases) page.

## CLI

```bash
# Sessions
pt run -s NAME -- COMMAND [ARGS...]     # Spawn a TUI
pt run -s app --cols 120 --rows 40 -- ./my-tui
pt list                                 # List sessions
pt status -s NAME                       # Running/exited?
pt kill -s NAME                         # Terminate
pt monitor -s NAME                      # Watch a session live

# Input
pt send -s NAME --type "hello"          # Type text
pt send -s NAME --key ctrl-c            # Keys (enter, escape, f1, alt-x, ...)
pt send -s NAME --paste "text"          # Bracketed paste
pt send -s NAME --mouse click:10,5      # Mouse events

# Screen
pt screenshot -s NAME                   # Plain text
pt screenshot -s NAME --format json     # Full cell data (grapheme, colors, attrs)
pt screenshot -s NAME --region 0,0,5,40 # Capture a region
pt cursor -s NAME                       # Cursor position and style
pt cell -s NAME --x 0 --y 0            # Single cell
pt scrollback -s NAME --lines 100       # Scrollback buffer

# Wait (conditions are AND-ed, default timeout 10s)
pt wait -s NAME --text "Ready"          # Text on screen
pt wait -s NAME --regex "v\d+\.\d+"     # Regex match
pt wait -s NAME --stable                # Screen stopped changing
pt wait -s NAME --process-exit          # Process exited
pt wait -s NAME --text "Done" --stable --timeout 5000

# Snapshots
pt snapshot save -s NAME -f ref.txt     # Save screen
pt snapshot diff -s NAME -f ref.txt     # Compare (exit 1 on diff)

# Batch
pt batch commands.txt                   # One command per line, # for comments
```

Global flags: `--json`, `--human`, `--socket PATH`, `--version`.

## Rust Library (`phantom-test`)

Write TUI integration tests in Rust — no daemon needed, the engine runs in-process.

```toml
[dev-dependencies]
phantom-test = { version = "0.1", features = ["monitor"] }
```

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

### Test Runner with Live Monitor

`TestRunner` runs structured test suites with an optional TUI monitor that shows test progress alongside a live view of the terminal session being tested:

```rust
use phantom_test::TestRunner;

TestRunner::new()
    .test("vim insert mode", |pt| {
        let s = pt.run("vim").args(&["--clean", "-u", "NONE"]).start()?;
        s.wait().text("~").timeout_ms(5000).until()?;
        s.send().type_text("iHello from phantom")?;
        s.wait().text("Hello from phantom").until()?;
        s.send().key("escape")?;
        s.send().type_text(":q!\n")?;
        s.wait().process_exit().until()?;
        Ok(())
    })
    .test("bash echo", |pt| {
        let s = pt.run("bash").args(&["--norc", "--noprofile"]).start()?;
        s.wait().stable(300).until()?;
        s.send().type_text("echo 'phantom works!'\n")?;
        s.wait().text("phantom works!").until()?;
        Ok(())
    })
    .run(); // pass --monitor for live TUI, or set PHANTOM_MONITOR=1
```

Run headless or with the live monitor:

```bash
# Headless — prints results to stdout
cargo test --example my_tests

# With live TUI monitor — shows a real-time view of each test's terminal
cargo test --example my_tests -- --monitor
```

Headless output:

```
phantom integration tests
────────────────────────────

  ✓ vim insert mode         1.2s
  ✓ bash echo               0.4s

────────────────────────────
2/2 passed
```

The `--monitor` flag opens a split-screen TUI: test list on the left with pass/fail status, and a live mirror of the terminal session on the right — so you can see exactly what your tests see as they run.

## MCP Server (`phantom-mcp`)

Phantom also ships as an [MCP](https://modelcontextprotocol.io) server, so AI agents (Claude Code, Claude Desktop, Cursor, Zed, …) can drive headless TUI programs over stdio. It embeds the engine in-process — no daemon to manage.

<p align="center">
  <img src="crates/phantom-mcp/docs/demo.gif" alt="phantom-mcp live in-tmux viewer demo" width="800">
</p>

When the user is running their MCP client inside **tmux**, the `phantom_show` tool splits the surrounding pane to show a live `phantom monitor` view of the agent's TUI session — the human watches what the agent is doing in real time, in the same terminal, without opening anything.

```bash
cargo build -p phantom-mcp --release
```

Then point your MCP client at the binary:

```json
{ "mcpServers": { "phantom": { "command": "/abs/path/to/target/release/phantom-mcp" } } }
```

Tools exposed: `phantom_run`, `phantom_send`, `phantom_wait`, `phantom_screenshot` (text or PNG image, with optional region), `phantom_show`, `phantom_cursor`, `phantom_cell`, `phantom_scrollback`, `phantom_output`, `phantom_status`, `phantom_list`, `phantom_resize`, `phantom_kill`. See [`crates/phantom-mcp/README.md`](crates/phantom-mcp/README.md) for the full reference.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error |
| 2 | Session not found |
| 3 | Wait timed out |
| 4 | Process exited |
| 5 | Session name collision |

## Architecture

```
pt CLI  ──unix socket──▶  phantom-daemon  ──pty──▶  child process
                               │
                          libghostty-vt
                       (terminal emulation)
```

The daemon owns all terminal state on a dedicated engine thread (libghostty-vt types are `!Send`). The CLI is stateless — connect, send JSON, get JSON back.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full architecture guide.

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
