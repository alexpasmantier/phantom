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

## Commands

```
phantom run -s <name> -- <cmd> [args]     Spawn a TUI in a session
phantom screenshot -s <name>              Capture screen (text or --format json)
phantom send -s <name> --type "text"      Type characters
phantom send -s <name> --key ctrl-c       Send key (ctrl-c, enter, escape, up, f1, etc.)
phantom send -s <name> --paste "text"     Bracketed paste
phantom send -s <name> --mouse click:x,y  Mouse event
phantom wait -s <name> --text "pattern"   Wait for text (also: --regex, --stable, --process-exit)
phantom cursor -s <name>                  Cursor position
phantom scrollback -s <name>              Scrollback buffer
phantom resize -s <name> --cols N --rows N
phantom monitor -s <name>                 Live view (30fps)
phantom status -s <name>
phantom list
phantom kill -s <name>
```

Output is human-readable on TTY, JSON when piped. Override with `--json` / `--human`.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error |
| 2 | Session not found |
| 3 | Wait timeout |
| 4 | Process exited |
| 5 | Session name collision |

## Building

Requires Rust nightly and [Zig](https://ziglang.org/) 0.15.x.

```bash
cargo build --workspace
cargo test -p phantom-daemon -- --test-threads=1
```

## License

MIT
