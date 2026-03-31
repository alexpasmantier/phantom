# phantom (`pt`)

Phantom is a CLI tool that enables you to programmatically drive a TUI. It uses [libghostty-vt](https://github.com/ghostty-org/ghostty) for terminal emulation.

## Usage

### CLI

```bash
# Spawn a TUI app in a named session
pt run -s myapp -- vim

# See what's on screen
pt screenshot -s myapp

# Send input
pt send -s myapp --type "iHello world"
pt send -s myapp --key escape
pt send -s myapp --key ctrl-c

# Wait for conditions
pt wait -s myapp --text "Ready" --timeout 5000
pt wait -s myapp --process-exit

# Watch a session live
pt monitor -s myapp

# Clean up
pt kill -s myapp
```

### Rust library (`phantom-test`)

Write TUI integration tests in Rust with an ergonomic builder API. No daemon process needed — the terminal emulation engine runs in-process.

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

Use the `TestRunner` for structured test suites with an optional live TUI monitor:

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

## What can this be used for?

- **AI agent tool use**: let an LLM-based agent interact with TUI applications (search, navigate, read screen contents, etc.)
- **Integration testing**: write deterministic tests for TUI apps without needing a real terminal
- **Automation**: script interactions with any terminal application that doesn't have a non-interactive mode

## Building

Requires Rust nightly and [Zig](https://ziglang.org/) 0.15.x.

```bash
cargo build --workspace
```

## Testing

```bash
# Rust integration tests (phantom-test)
cargo test -p phantom-test -- --test-threads=1

# Test runner (headless)
cargo run -p phantom-test --features monitor --example runner

# Test runner with live TUI monitor
cargo run -p phantom-test --features monitor --example runner -- --monitor

# Or via just
just test-rust           # headless
just test-rust-monitor   # with TUI monitor
```

## License

MIT
