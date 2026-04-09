#!/usr/bin/env python3
"""Demo runner for the phantom-mcp live-tmux-viewer recording.

Spawns phantom-mcp, opens a vim session, calls phantom_show (which splits
the surrounding tmux pane to display a live viewer), types into vim, and
cleans up. Designed to be invoked from a VHS tape inside a fresh tmux
session — see ``demo.tape`` for the recording script.
"""

import json
import os
import subprocess
import sys
import time

BINARY = "./target/debug/phantom-mcp"
SOCKET = os.environ.setdefault("PHANTOM_MCP_SOCKET", "/tmp/phantom-mcp-demo.sock")
os.environ.setdefault("PHANTOM_MCP_SPLIT", "vertical")

if not os.path.exists(BINARY):
    print(f"ERROR: {BINARY} not built — run `cargo build -p phantom-mcp` first.", file=sys.stderr)
    sys.exit(1)

proc = subprocess.Popen(
    [BINARY],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
    bufsize=0,
)


def call(rid, method, params=None):
    msg = {"jsonrpc": "2.0", "id": rid, "method": method}
    if params is not None:
        msg["params"] = params
    proc.stdin.write((json.dumps(msg) + "\n").encode())
    proc.stdin.flush()
    return json.loads(proc.stdout.readline())


def tool(rid, name, args):
    r = call(rid, "tools/call", {"name": name, "arguments": args})
    if "error" in r:
        return f"ERROR: {r['error']['message']}"
    return r["result"]["content"][0]["text"]


# MCP handshake
call(
    1,
    "initialize",
    {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "demo", "version": "0"},
    },
)
proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n')
proc.stdin.flush()


def step(label, result):
    print(f"  agent → {label}")
    if result and result != "ok":
        print(f"          {result}")


step(
    "phantom_run vim",
    tool(
        2,
        "phantom_run",
        {
            "command": "vim",
            "args": ["--clean", "-u", "NONE"],
            "name": "demo",
            "cols": 80,
            "rows": 14,
        },
    ),
)
time.sleep(0.3)

step("phantom_wait stable", tool(3, "phantom_wait", {"session": "demo", "stable_ms": 400, "timeout_ms": 5000}))
time.sleep(0.3)

step("phantom_show  (live viewer pane appears below ↓)", tool(4, "phantom_show", {"session": "demo"}))
time.sleep(2.0)

step("phantom_send  (typing into vim — watch the viewer)", "ok")
tool(5, "phantom_send", {"session": "demo", "kind": "key", "value": "i"})
time.sleep(0.4)
tool(6, "phantom_send", {"session": "demo", "kind": "text", "value": "Hello from phantom-mcp!"})
time.sleep(1.0)
tool(7, "phantom_send", {"session": "demo", "kind": "key", "value": "enter"})
tool(8, "phantom_send", {"session": "demo", "kind": "text", "value": "The agent is driving this vim session live."})
time.sleep(1.0)
tool(9, "phantom_send", {"session": "demo", "kind": "key", "value": "enter"})
tool(10, "phantom_send", {"session": "demo", "kind": "text", "value": "You are seeing every keystroke in real time."})
time.sleep(1.5)

step("cursor moves (escape, then h × 18)", "ok")
tool(11, "phantom_send", {"session": "demo", "kind": "key", "value": "escape"})
time.sleep(0.2)
for _ in range(18):
    tool(12, "phantom_send", {"session": "demo", "kind": "key", "value": "h"})
    time.sleep(0.04)
time.sleep(0.6)

step("visual line select", "ok")
tool(13, "phantom_send", {"session": "demo", "kind": "key", "value": "shift-v"})
time.sleep(0.3)
for _ in range(2):
    tool(14, "phantom_send", {"session": "demo", "kind": "key", "value": "j"})
    time.sleep(0.2)
time.sleep(0.5)

# Hold the final populated state on screen so the recording's last frame
# captures the rich vim+selection view rather than an empty cleanup state.
# We deliberately do NOT call phantom_kill here — phantom-mcp will tear down
# the session when we close its stdin below, which happens after VHS has
# already stopped recording. The hold is intentionally longer than the
# tape's remaining `Sleep` so the recording cuts off mid-hold rather than
# during cleanup.
print()
print("(holding final state…)")
time.sleep(10.0)

proc.stdin.close()
try:
    proc.wait(timeout=3)
except subprocess.TimeoutExpired:
    proc.terminate()
