# phantom-mcp

An MCP (Model Context Protocol) server that exposes [phantom](../../README.md)
to AI agents. It lets a language model spawn TUI programs (vim, less, fzf,
htop, lazygit, …) in a headless terminal, send input, and screenshot the
result — including as **PNG images** for vision-capable models.

It speaks JSON-RPC over **stdio**, so any MCP client (Claude Code, Claude
Desktop, Cursor, Zed, …) can use it directly.

## How it works

`phantom-mcp` embeds the phantom engine in-process (via the `phantom-test`
library) — there is no separate daemon to manage. Sessions live for the
lifetime of the server and are killed automatically when it shuts down.

## Build

```sh
cargo build -p phantom-mcp --release
```

The binary is placed at `target/release/phantom-mcp`. It needs to find
`libghostty-vt.dylib` at runtime; the build script embeds an rpath to the
vendored copy under `target/.../build/libghostty-vt-sys-*/`, so running it
straight from `target/` Just Works.

## Configure your MCP client

### Claude Code

```json
{
  "mcpServers": {
    "phantom": {
      "command": "/absolute/path/to/target/release/phantom-mcp"
    }
  }
}
```

### Claude Desktop / Cursor / Zed

Same shape — point `command` at the absolute path to the `phantom-mcp` binary.

## Tools

| Tool | Purpose |
|---|---|
| `phantom_run` | Spawn a TUI program in a new session (cols, rows, env, cwd configurable) |
| `phantom_show` | Open a live viewer pane next to the user's chat (tmux only; no-op elsewhere) |
| `phantom_send` | Send input — `text`, `key` (`enter`, `ctrl-c`, `f1`, …), `paste`, `mouse` |
| `phantom_wait` | Block until conditions hold: `text`, `text_absent`, `regex`, `stable_ms`, `process_exit`, `exit_code`, `cursor_at`, `cursor_visible`, `screen_changed` |
| `phantom_screenshot` | Capture the screen as `text` or as a rendered PNG `image`. Supports an optional `region` rectangle. |
| `phantom_cursor` | Get cursor position and visibility |
| `phantom_cell` | Inspect a single cell (grapheme + style attrs) |
| `phantom_scrollback` | Dump the scrollback buffer as text |
| `phantom_output` | Get the post-exit primary screen content (e.g. fzf's selection) |
| `phantom_status` | Session status: dimensions, title, cwd, running/exited |
| `phantom_list` | List all active sessions |
| `phantom_resize` | Resize a session's terminal |
| `phantom_kill` | Terminate a session (SIGTERM) |

## Live viewing inside tmux

When the user is running their MCP client (e.g. Claude Code) **inside tmux**,
phantom-mcp can open a live viewer pane right next to the chat — the user
sees what the agent is doing in real time, no separate terminal required.

How it works:

1. On startup, phantom-mcp opens an **observer Unix socket** at
   `$XDG_RUNTIME_DIR/phantom-mcp-<pid>.sock` (or `~/.phantom/phantom-mcp-<pid>.sock`).
   It speaks the same JSON wire protocol as the phantom daemon.
2. When the agent calls **`phantom_show`** for a session, phantom-mcp shells
   out to `tmux split-window -h -d 'phantom monitor -s <session> --socket <path>'`.
   The new pane runs the existing `phantom monitor` viewer against the
   observer socket and refreshes ~30 fps.
3. `-d` keeps focus on the original pane, so the user keeps typing to their
   chat client uninterrupted.
4. Outside tmux, `phantom_show` is a graceful no-op — it just tells the agent
   to use `phantom_screenshot` for in-chat viewing instead.

### Layout options

Set `PHANTOM_MCP_SPLIT` to control where the viewer goes:

- `horizontal` (default) — `tmux split-window -h`, side-by-side panes
- `vertical` — `tmux split-window -v`, stacked panes
- `popup` — `tmux display-popup -E -w 90% -h 90%`, modal floating viewer

### Socket path

By default the observer socket lives at
`$XDG_RUNTIME_DIR/phantom-mcp-<pid>.sock` (or `~/.phantom/phantom-mcp-<pid>.sock`
when `XDG_RUNTIME_DIR` is unset). Override it with **`PHANTOM_MCP_SOCKET`**
if you want a fixed, predictable path:

```sh
PHANTOM_MCP_SOCKET=/tmp/my-phantom.sock phantom-mcp
```

This is useful for development (always know where to attach) and for sharing
a single socket between multiple debugging tools.

### Manual attach

The observer socket also lets a curious user poke at the running engine
directly without going through the agent — useful for debugging:

```sh
phantom list --socket ~/.phantom/phantom-mcp-<pid>.sock
phantom screenshot -s <name> --socket ~/.phantom/phantom-mcp-<pid>.sock
phantom monitor -s <name> --socket ~/.phantom/phantom-mcp-<pid>.sock
```

The socket file is removed when phantom-mcp exits.

## Recommended workflow for agents

1. `phantom_run` — spawn the program
2. `phantom_wait` with `stable_ms: 300` — let the UI settle
3. `phantom_screenshot` with `format: "image"` — visual grounding
4. `phantom_send` — type or press keys
5. `phantom_wait` with `text:` or `stable_ms:` — let the redraw finish
6. Repeat 3–5
7. `phantom_kill` when done

The server's `instructions` field tells the model this same flow on
initialize, so well-behaved clients will follow it without prompting.

## Image rendering

Screenshots requested with `format: "image"` are rendered to PNG using
[fontdue](https://github.com/mooman219/fontdue) over a vendored copy of
**JetBrains Mono Regular** (SIL OFL 1.1; license at
`assets/JetBrainsMono-OFL.txt`). The renderer honors per-cell foreground,
background, bold, faint, italic, inverse, underline, and strikethrough.
Color formats supported: `#rrggbb` and `palette:N` (xterm 256-color).

## Development

```sh
cargo test -p phantom-mcp                    # unit + integration tests (--test-threads=1 not required)
cargo test -p phantom-mcp --lib              # renderer unit tests only
cargo test -p phantom-mcp --tests            # integration tests against bash
```

The integration tests require `bash` on `PATH` and gracefully skip if it's
absent.
