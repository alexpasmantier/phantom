# Contributing to Phantom

Thanks for your interest in contributing to Phantom! This guide will help you get set up and productive quickly.

## Development Environment

### Prerequisites

- **Rust nightly** -- pinned in `rust-toolchain.toml`, so `rustup` will pick it up automatically.
- **Zig 0.15.x** -- required for the vendored libghostty-vt build. Install via your package manager or from [ziglang.org](https://ziglang.org/download/).

### Getting Started

```bash
git clone https://github.com/alexpasmantier/phantom.git
cd phantom
cargo build --workspace
```

If the build succeeds, your environment is ready.

## Building and Testing

```bash
# Build the entire workspace
cargo build --workspace

# Run daemon integration tests (must be sequential to avoid resource contention)
cargo test -p phantom-daemon -- --test-threads=1

# Run phantom-test library tests
cargo test -p phantom-test -- --test-threads=1

# Run all tests via just
just test-all

# Lint
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --all
```

**Important:** daemon and library tests must run with `--test-threads=1` because concurrent engine instances contend over shared resources.

## Architecture Overview

Phantom uses a daemon + stateless CLI architecture communicating over Unix sockets.

```
phantom CLI  --[unix socket]-->  phantom-daemon  --[PTY]-->  child process
```

### Crate Layout

| Crate | Path | Purpose |
|-------|------|---------|
| **phantom-core** | `crates/phantom-core/` | Shared types, JSON protocol, exit codes. No libghostty dependency. |
| **phantom-daemon** | `crates/phantom-daemon/` | Daemon binary and library. Owns all libghostty-vt terminal state. |
| **phantom-cli** | `crates/phantom-cli/` | Stateless CLI binary (`phantom` / `pt`). Connects to the daemon over a Unix socket. |
| **phantom-test** | `crates/phantom-test/` | In-process testing library with an ergonomic builder API. |

### Threading Model

All libghostty-vt types are `!Send + !Sync`. The daemon runs a dedicated **engine thread** (`std::thread`) that owns all terminal state. The Tokio async runtime handles socket I/O and communicates with the engine thread via crossbeam channels and an mio `Waker`.

Key files to read first:

- `crates/phantom-daemon/src/engine.rs` -- core event loop and command dispatch
- `crates/phantom-daemon/src/session.rs` -- session wrapper around Terminal + PTY
- `crates/phantom-daemon/src/capture.rs` -- screen capture via RenderState iteration
- `crates/phantom-daemon/src/input.rs` -- key spec parsing and encoding
- `crates/phantom-daemon/src/wait.rs` -- wait condition evaluation

## Code Style and Conventions

- Run `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` before submitting.
- All libghostty-vt method calls return `Result` -- always propagate errors, do not unwrap.
- Empty terminal cells (cells with no grapheme content) must emit a space character, not an empty string.
- Key encoding: use `send_type` for plain character input and `send_key` for special keys or modifier combinations.
- When writing tests that launch vim, always use `--clean -u NONE` (not just `--clean`) to avoid loading `defaults.vim`, which can trigger libghostty-vt crashes.
- Integration tests live in `crates/phantom-daemon/tests/` and use `TestHarness`, which spawns an engine on a dedicated thread and communicates via channels (same architecture as production, minus the socket layer).

## Pull Request Guidelines

1. **Keep PRs focused.** One logical change per PR makes review faster.
2. **Write tests.** If you are adding a feature or fixing a bug, include a test that covers it.
3. **Run the full check locally** before pushing:
   ```bash
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   cargo test -p phantom-daemon -- --test-threads=1
   cargo test -p phantom-test -- --test-threads=1
   ```
4. **Write a clear description.** Explain *what* changed and *why*. Link to any relevant issue.
5. **Keep commits clean.** Squash fixup commits before requesting review.

## Reporting Bugs

Please open an issue on the [GitHub issue tracker](https://github.com/alexpasmantier/phantom/issues) with:

- A clear description of the problem
- Steps to reproduce
- Expected vs. actual behavior
- Your OS, Rust version (`rustc --version`), and Zig version (`zig version`)
