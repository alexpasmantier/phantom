# phantom

Phantom is a CLI tool that enables you to programmatically drive a TUI. It uses [libghostty-vt](https://github.com/ghostty-org/ghostty) for terminal emulation.

## Usage

```bash
# Spawn a TUI app in a named session
phantom run -s myapp -- vim

# See what's on screen
phantom screenshot -s myapp

# Send input
phantom send -s myapp --type "iHello world"
phantom send -s myapp --key escape
phantom send -s myapp --key ctrl-c

# Wait for conditions
phantom wait -s myapp --text "Ready" --timeout 5000
phantom wait -s myapp --process-exit

# Watch a session live
phantom monitor -s myapp

# Clean up
phantom kill -s myapp
```

## Building

Requires Rust nightly and [Zig](https://ziglang.org/) 0.15.x.

```bash
cargo build --workspace
cargo test -p phantom-daemon -- --test-threads=1
```

## License

MIT
