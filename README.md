# phantom (`pt`)

Phantom is a CLI tool that enables you to programmatically drive a TUI. It uses [libghostty-vt](https://github.com/ghostty-org/ghostty) for terminal emulation.

## Usage

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

## What can this be used for?

- **AI agent tool use**: let an LLM-based agent interact with TUI applications (search, navigate, read screen contents, etc.)
- **Integration testing**: write deterministic tests for TUI apps without needing a real terminal
- **Automation**: script interactions with any terminal application that doesn't have a non-interactive mode

## Building

Requires Rust nightly and [Zig](https://ziglang.org/) 0.15.x.

```bash
cargo build --workspace
cargo test -p pt-daemon -- --test-threads=1
```

## License

MIT
