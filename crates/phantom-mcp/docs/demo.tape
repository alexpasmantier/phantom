# phantom-mcp — live in-tmux viewer demo
#
# Records a GIF showing an agent driving a vim session via MCP, with the
# `phantom_show` tool splitting the surrounding tmux pane to display a live
# viewer of the session.
#
# Re-record with:   vhs crates/phantom-mcp/docs/demo.tape
# (must be run from the workspace root; requires `cargo build -p phantom-mcp`)

Output crates/phantom-mcp/docs/demo.gif

# Force a clean POSIX shell — the default user shell may have heavy rc files
# (oh-my-zsh, vi-mode plugins) that interfere with VHS's typing.
Set Shell "bash"

Set Width 1400
Set Height 900
Set FontSize 14
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 30ms

# Setup (hidden): suppress shell prompt clutter, clean any prior socket,
# kill any prior demo tmux server, then start a fresh isolated tmux server
# (named socket -L phantom-vhs) inside VHS's embedded terminal.
Hide
Type "export PS1='$ ' && unset PROMPT_COMMAND && clear"
Enter
Sleep 200ms
Type "rm -f /tmp/phantom-mcp-demo.sock"
Enter
Sleep 100ms
Type "tmux -L phantom-vhs kill-server 2>/dev/null; true"
Enter
Sleep 200ms
Type "tmux -L phantom-vhs new-session -s demo -x 160 -y 42 'bash --norc --noprofile'"
Enter
Sleep 1500ms
Type "export PS1='$ ' && unset PROMPT_COMMAND && clear"
Enter
Sleep 300ms
Show

# Inside the recorded tmux pane now. Show context to the viewer.
Type "# phantom-mcp -- live in-tmux viewer demo"
Sleep 300ms
Enter
Type "# An LLM agent drives a vim session via MCP."
Sleep 200ms
Enter
Type "# phantom_show splits this pane to render the session live."
Sleep 200ms
Enter
Sleep 600ms

# Run the demo. The runner spawns phantom-mcp, opens vim, calls phantom_show
# (which splits this very pane vertically), types into vim live, and holds
# the final populated state on-screen for the closing frame of the recording.
Type "python3 crates/phantom-mcp/docs/demo_runner.py"
Enter
Sleep 18s
