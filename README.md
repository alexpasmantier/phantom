# phantom

A headless TUI interaction tool for AI agents and integration tests, powered by [libghostty-vt](https://github.com/ghostty-org/ghostty) (Ghostty's terminal emulation core).

Phantom lets you spawn TUI applications in headless sessions, interact with them via keyboard/mouse input, capture screen content, and wait for specific conditions — all through a simple CLI. It's designed to be driven by AI agents (like Claude Code) or used in deterministic integration tests.

## Quick Start

```bash
# Build (requires Rust nightly + Zig 0.15.x)
cargo build --workspace

# Spawn a TUI app
phantom run -s myapp -- vim

# See what's on screen
phantom screenshot -s myapp

# Type text and send keys
phantom send -s myapp --type "iHello world"
phantom send -s myapp --key escape
phantom send -s myapp --type ":wq"
phantom send -s myapp --key enter

# Wait for conditions
phantom wait -s myapp --text "Ready" --timeout 5000
phantom wait -s myapp --process-exit --timeout 5000

# Watch a session live from another terminal
phantom monitor -s myapp

# Clean up
phantom kill -s myapp
```

## Architecture

Phantom uses a **daemon + stateless CLI** architecture:

```
┌────────────┐  Unix socket   ┌──────────────────────────┐
│  phantom   │◄──────────────►│  phantom-daemon          │
│  (CLI)     │  JSON protocol │                          │
└────────────┘                │  Engine Thread            │
                              │  ├─ PTY sessions          │
                              │  ├─ libghostty-vt         │
                              │  │  (terminal emulation)  │
                              │  └─ mio event loop        │
                              └──────────────────────────┘
```

- The **daemon** manages PTY sessions with libghostty-vt terminals on a dedicated engine thread (required because libghostty-vt types are `!Send + !Sync`).
- The **CLI** is stateless — each invocation connects to the daemon via Unix socket, sends a request, and exits.
- The daemon **auto-starts** on first use. No manual setup needed.

## Commands

### Session Management

| Command | Description |
|---------|-------------|
| `phantom run -s <name> -- <cmd> [args]` | Spawn a TUI in a named session |
| `phantom list` | List active sessions |
| `phantom status -s <name>` | Session info (running/exited, PID, dimensions) |
| `phantom resize -s <name> --cols N --rows N` | Resize terminal |
| `phantom kill -s <name>` | Terminate session |

### Observation

| Command | Description |
|---------|-------------|
| `phantom screenshot -s <name>` | Capture screen as text |
| `phantom screenshot -s <name> --format json` | Capture with cell-level attributes (fg/bg color, bold, etc.) |
| `phantom cursor -s <name>` | Cursor position, visibility, style |
| `phantom scrollback -s <name>` | Dump scrollback buffer |
| `phantom monitor -s <name>` | Live-updating view (30fps, alternate screen) |

### Input

| Command | Description |
|---------|-------------|
| `phantom send -s <name> --type "text"` | Type characters |
| `phantom send -s <name> --key ctrl-c` | Send key sequence |
| `phantom send -s <name> --key enter` | Send special keys |
| `phantom send -s <name> --paste "text"` | Bracketed paste |
| `phantom send -s <name> --mouse "click:10,5"` | Mouse events |

Key specs: `ctrl-c`, `alt-x`, `shift-tab`, `enter`, `escape`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `f1`-`f12`, `backspace`, `delete`, or single characters.

Mouse specs: `click:x,y`, `right-click:x,y`, `middle-click:x,y`, `scroll-up:x,y`, `scroll-down:x,y`, `move:x,y`.

### Synchronization

| Command | Description |
|---------|-------------|
| `phantom wait -s <name> --text "Ready"` | Wait for text to appear |
| `phantom wait -s <name> --text-disappear "Loading"` | Wait for text to vanish |
| `phantom wait -s <name> --regex "Error.*failed"` | Wait for regex match |
| `phantom wait -s <name> --stable` | Wait for screen to stop changing |
| `phantom wait -s <name> --cursor 0,5` | Wait for cursor position |
| `phantom wait -s <name> --process-exit` | Wait for process to exit |

All wait commands support `--timeout <ms>` (default 10s) and `--poll <ms>` (default 50ms). Multiple conditions can be combined.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success / condition met |
| 1 | General error |
| 2 | Session not found |
| 3 | Wait timeout |
| 4 | Process already exited |
| 5 | Session name collision |

## Output Format

Output is **human-readable** on TTY, **JSON** when piped. Override with `--json` or `--human`.

## Building

Requirements:
- Rust nightly (edition 2024)
- [Zig](https://ziglang.org/) 0.15.x (for building libghostty-vt)

```bash
cargo build --workspace
```

The daemon binary embeds an rpath to the libghostty-vt dylib, so no `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` is needed.

## Testing

```bash
cargo test -p phantom-daemon -- --test-threads=1
```

25 integration tests covering bash, vim, and less interactions.

## Why libghostty-vt?

Phantom uses Ghostty's terminal emulation engine rather than simpler alternatives (like the `vt100` crate) because:

- **Accuracy**: Same VT parser that powers the Ghostty terminal, handling modern escape sequences correctly
- **SIMD-optimized**: Fast parsing for high-throughput scenarios
- **Full protocol support**: Kitty keyboard protocol, mouse tracking modes, OSC sequences, device attribute queries
- **Cell-level attributes**: True color, bold/italic/underline/strikethrough, palette colors

## License

MIT
