# CLAUDE.md

## Project Overview

Phantom is a headless TUI interaction CLI tool built on libghostty-vt (Ghostty's terminal emulation core). It uses a daemon + stateless CLI architecture over Unix sockets.

## Common Commands

```bash
cargo build --workspace          # Build everything
cargo test -p phantom-daemon -- --test-threads=1  # Run tests (sequential — avoids resource contention)
cargo check --workspace          # Quick check
```

Requires Rust nightly + Zig 0.15.x (for vendored libghostty-vt build).

## Architecture

### Crate Layout

- `crates/phantom-core/` — Shared types, JSON protocol, exit codes. No libghostty dependency.
- `crates/phantom-daemon/` — Daemon binary + library. Owns all libghostty-vt state.
- `crates/phantom-cli/` — Stateless CLI binary (`phantom`). Connects to daemon via Unix socket.

### Threading Model

All libghostty-vt types are `!Send + !Sync`. The daemon uses a dedicated **engine thread** (std::thread) that owns all terminal state. Tokio async handlers communicate with it via crossbeam channels + mio Waker.

```
Main thread → Tokio runtime (socket listener, connection handlers)
Engine thread → mio event loop (PTY I/O, session management, wait evaluation)
```

### Key Data Flows

- **Input**: CLI → socket → handler → crossbeam channel → engine → key/mouse encoder → PTY write
- **Screenshot**: CLI → socket → engine → RenderState.update() → RowIterator/CellIterator → JSON response
- **Wait**: Engine stores PendingWait, re-evaluates after each PTY read cycle, replies when satisfied or timed out
- **PTY responses**: Terminal's `on_pty_write` callback buffers bytes during `vt_write()`, flushed to PTY after

### Important Files

- `crates/phantom-daemon/src/engine.rs` — Core event loop, all EngineCommand variants
- `crates/phantom-daemon/src/session.rs` — Session struct wrapping Terminal + PTY + encoders
- `crates/phantom-daemon/src/capture.rs` — Screen capture via RenderState/Snapshot iteration
- `crates/phantom-daemon/src/input.rs` — Key spec parsing, key/mouse encoding
- `crates/phantom-daemon/src/wait.rs` — Wait condition evaluation
- `crates/phantom-daemon/build.rs` — Embeds rpath to libghostty-vt dylib

## Code Conventions

- Empty terminal cells (no graphemes) must emit a space character, not nothing
- Use `Result` returns from all libghostty-vt methods (they all return `Result<T, Error>`)
- `Terminal<'static, 'static>` lifetime — callbacks must be `'static` (use `Rc<RefCell<>>` for shared state)
- Key encoding: use `send_type` for plain character input, `send_key` for special keys/modifiers
- vim tests must use `--clean -u NONE` (not just `--clean`, which loads defaults.vim and can trigger libghostty-vt crashes)

## Testing

Integration tests live in `crates/phantom-daemon/tests/`. They use a `TestHarness` that spawns an engine on a dedicated thread and communicates via channels (same as production, without the socket layer).

Tests require `--test-threads=1` to avoid resource contention between concurrent engine instances.
