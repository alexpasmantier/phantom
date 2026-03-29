#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PT="$REPO_DIR/target/debug/pt"
PTD="$REPO_DIR/target/debug/phantom-daemon"
SOCK="/tmp/phantom-integration-test.sock"
MONITOR_PID=""
DAEMON_PID=""
SESSION=""
PASSED=0
FAILED=0
TOTAL=0
ERRORS=""

# ─── Colors ──────────────────────────────────────────────
GREEN='\033[0;32m'
RED='\033[0;31m'
DIM='\033[2m'
BOLD='\033[1m'
NC='\033[0m'

# ─── Helpers ─────────────────────────────────────────────

cleanup() {
    if [ -n "$SESSION" ]; then
        "$PT" --socket "$SOCK" kill -s "$SESSION" 2>/dev/null || true
    fi
    if [ -n "$MONITOR_PID" ]; then
        kill "$MONITOR_PID" 2>/dev/null || true
        wait "$MONITOR_PID" 2>/dev/null || true
    fi
    if [ -n "$DAEMON_PID" ]; then
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    rm -f "$SOCK"
}
trap cleanup EXIT

die() { echo -e "${RED}FATAL: $1${NC}" >&2; exit 1; }

pass() {
    PASSED=$((PASSED + 1)); TOTAL=$((TOTAL + 1))
    echo -e "  ${GREEN}✓${NC} $1"
}

fail() {
    FAILED=$((FAILED + 1)); TOTAL=$((TOTAL + 1))
    echo -e "  ${RED}✗${NC} $1"
    ERRORS="$ERRORS\n  - $1"
}

check() {
    local desc="$1"; shift
    local rc=0
    "$@" >/dev/null 2>&1 || rc=$?
    if [ $rc -eq 0 ]; then pass "$desc"; else fail "$desc"; fi
}

pt() { "$PT" --socket "$SOCK" "$@" 2>/dev/null; }

new_session() {
    _new_session_inner "$@" >/dev/null
}

send_type()  { pt send -s "$SESSION" --type "$1" || true; }
send_key()   { pt send -s "$SESSION" --key "$1" || true; }
# Type text then press enter
run_cmd()    { send_type "$1"; send_key "enter"; }
wait_text()  { pt wait -s "$SESSION" --text "$1" --timeout "${2:-5000}"; }
wait_gone()  { pt wait -s "$SESSION" --text-disappear "$1" --timeout "${2:-5000}"; }
wait_stable(){ pt wait -s "$SESSION" --stable --stable-duration "${1:-500}" --timeout "${2:-5000}"; }
wait_exit()  { pt wait -s "$SESSION" --process-exit --timeout "${1:-5000}"; }
screenshot() { pt screenshot -s "$SESSION"; }
contains()   { screenshot | grep -q "$1"; }

# Suppress output from new_session
_new_session_inner() {
    local name="$1"; shift
    [ -n "$SESSION" ] && { pt kill -s "$SESSION" || true; sleep 0.2; }
    SESSION="$name"
    pt run -s "$name" "$@"
}

start_monitor() {
    if [ "${MONITOR_MODE:-}" = "--monitor" ] && [ -n "${TMUX:-}" ] && [ -z "$MONITOR_PID" ]; then
        tmux popup -d '#{pane_current_path}' -w 80% -h 80% -E \
            "$PT --socket $SOCK monitor -s $SESSION" &
        MONITOR_PID=$!
        sleep 0.3
    fi
}

stop_monitor() {
    if [ -n "$MONITOR_PID" ]; then
        kill "$MONITOR_PID" 2>/dev/null || true
        wait "$MONITOR_PID" 2>/dev/null || true
        MONITOR_PID=""
    fi
}

# ─── Preflight ───────────────────────────────────────────

[ -f "$PT" ] || die "not built — run: cargo build --workspace"
[ -f "$PTD" ] || die "not built — run: cargo build --workspace"

MONITOR_MODE="${1:-}"

rm -f "$SOCK"
"$PTD" --foreground --socket "$SOCK" &>/dev/null &
DAEMON_PID=$!
sleep 0.5

# ─── Header ─────────────────────────────────────────────

echo ""
echo -e "${BOLD}  phantom integration tests${NC}"
echo -e "  ${DIM}────────────────────────────${NC}"

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Session lifecycle${NC}"
# ═════════════════════════════════════════════════════════

new_session "s1" -- bash -c "sleep 60"
check "create session" pt status -s s1
check "list shows session" bash -c "$PT --socket $SOCK list --json | grep -q s1"

new_session "s2" -- bash -c "sleep 60"
check "multiple sessions" bash -c "$PT --socket $SOCK list --json | grep -q s2"

check "session collision" bash -c "! $PT --socket $SOCK run -s s1 -- echo hi"
check "session not found" bash -c "! $PT --socket $SOCK status -s nonexistent"

pt kill -s s2; sleep 0.3
check "kill session" pt wait -s s2 --process-exit --timeout 3000
pt kill -s s1 || true
SESSION=""

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Screen capture${NC}"
# ═════════════════════════════════════════════════════════

new_session "cap" --cols 80 --rows 24 -- bash --norc --noprofile
start_monitor
wait_stable 300 5000

run_cmd "echo HELLO_WORLD"
wait_text "HELLO_WORLD"
sleep 0.3

check "text screenshot" contains "HELLO_WORLD"
check "json has cells" bash -c "$PT --socket $SOCK screenshot -s cap --format json | grep -q grapheme"
check "region screenshot" bash -c "test \$($PT --socket $SOCK screenshot -s cap --region 0,0,2,79 | wc -l) -eq 3"

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Input${NC}"
# ═════════════════════════════════════════════════════════

run_cmd "echo TYPED"
wait_text "TYPED"
check "type text" contains "TYPED"

pt send -s "$SESSION" --paste $'echo PASTED\n'
wait_text "PASTED"
check "paste input" contains "PASTED"

run_cmd "sleep 60"
sleep 1
send_key "ctrl-c"
sleep 1
run_cmd "echo AFTER_CTRLC"
check "send key (ctrl-c)" wait_text "AFTER_CTRLC" 5000

check "mouse (no crash)" pt send -s "$SESSION" --mouse "click:5,5"

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Wait conditions${NC}"
# ═════════════════════════════════════════════════════════

run_cmd "sleep 1 && echo DELAYED"
check "wait text present" wait_text "DELAYED" 5000

# Fresh session for this test — screen clearing is unreliable across sessions
pt kill -s "$SESSION" || true; sleep 0.2
new_session "absent" --cols 80 --rows 24 -- bash --norc --noprofile
wait_stable 300 5000
run_cmd "echo TEMP_XYZ"
wait_text "TEMP_XYZ"
sleep 0.3
run_cmd "printf '\\033[2J\\033[H'"
check "wait text absent" wait_gone "TEMP_XYZ" 5000

check "wait timeout" bash -c "! $PT --socket $SOCK wait -s $SESSION --text NEVER --timeout 500"
check "wait stable" wait_stable 500 5000

run_cmd "echo CHANGE"
check "wait changed" pt wait -s "$SESSION" --changed --timeout 5000

run_cmd "echo 'val: 42 ok'"
check "wait regex" pt wait -s "$SESSION" --regex "val: [0-9]+ ok" --timeout 5000
check "wait regex miss" bash -c "! $PT --socket $SOCK wait -s $SESSION --regex 'XYZZY_[0-9]+' --timeout 500"

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Cursor & cells${NC}"
# ═════════════════════════════════════════════════════════

check "cursor visible" bash -c "$PT --socket $SOCK cursor -s $SESSION --json | grep -q '\"visible\":true'"
check "cell inspect" bash -c "$PT --socket $SOCK cell -s $SESSION --x 0 --y 0 --json | grep -q grapheme"

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Resize${NC}"
# ═════════════════════════════════════════════════════════

# Fresh session for resize test
pt kill -s "$SESSION" || true; sleep 0.2
new_session "resz" --cols 80 --rows 24 -- bash --norc --noprofile
wait_stable 300 5000
pt resize -s "$SESSION" --cols 120 --rows 40
sleep 1
run_cmd "tput cols"
check "resize terminal" wait_text "120" 5000

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Scrollback${NC}"
# ═════════════════════════════════════════════════════════

stop_monitor
pt kill -s "$SESSION" || true; sleep 0.2
new_session "scrl" --cols 80 --rows 24 -- bash --norc --noprofile
start_monitor
wait_stable 300 5000
run_cmd "for i in \$(seq 1 60); do echo sb_\$i; done"
wait_text "sb_60" 10000
wait_stable 500 5000
check "scrollback has old lines" bash -c "$PT --socket $SOCK scrollback -s $SESSION 2>/dev/null | grep -q sb_1"
check "scrollback --lines" bash -c "test \$($PT --socket $SOCK scrollback -s $SESSION --lines 5 2>/dev/null | grep -c sb_) -le 5"

stop_monitor
pt kill -s "$SESSION" || true
SESSION=""

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Snapshots${NC}"
# ═════════════════════════════════════════════════════════

SNAP=$(mktemp /tmp/phantom-snap-XXXXXX.txt)
new_session "snap" --cols 80 --rows 24 -- bash --norc --noprofile
start_monitor
wait_stable 300 5000
run_cmd "echo SNAPSHOT_REF"
wait_text "SNAPSHOT_REF"

pt snapshot save -s "$SESSION" -f "$SNAP"
check "snapshot save" test -s "$SNAP"
check "snapshot match" pt snapshot diff -s "$SESSION" -f "$SNAP"

run_cmd "echo CHANGED"
wait_text "CHANGED"
check "snapshot differs" bash -c "! $PT --socket $SOCK snapshot diff -s $SESSION -f $SNAP"

rm -f "$SNAP"
stop_monitor
pt kill -s "$SESSION" || true
SESSION=""

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Output capture${NC}"
# ═════════════════════════════════════════════════════════

new_session "out" -- bash -c "echo FINAL_OUTPUT"
wait_exit 5000
check "output capture" bash -c "$PT --socket $SOCK output -s out | grep -q FINAL_OUTPUT"
SESSION=""

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  Exit codes${NC}"
# ═════════════════════════════════════════════════════════

new_session "ex0" -- bash -c "exit 0"
wait_exit 5000
check "exit code 0" bash -c "$PT --socket $SOCK status -s ex0 --json | grep -q '\"code\":0'"
SESSION=""

new_session "ex42" -- bash -c "exit 42"
wait_exit 5000
check "exit code 42" bash -c "$PT --socket $SOCK status -s ex42 --json | grep -q '\"code\":42'"
SESSION=""

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  vim${NC}"
# ═════════════════════════════════════════════════════════

if command -v vim &>/dev/null; then
    new_session "vim" --cols 80 --rows 24 -- vim --clean -u NONE
    start_monitor
    wait_stable 1000 10000

    check "vim startup" bash -c "$PT --socket $SOCK screenshot -s vim | grep -c '^~' | grep -q '[5-9]\|[1-9][0-9]'"

    send_type "i"; sleep 0.5
    send_type "Hello vim"
    sleep 0.5
    check "vim insert + type" contains "Hello vim"

    send_key "escape"; sleep 0.5
    send_type "dd"; sleep 0.5
    check "vim dd deletes" bash -c "! $PT --socket $SOCK screenshot -s vim 2>/dev/null | grep -q 'Hello vim'"

    # Use raw carriage return for Enter in vim (raw mode)
    pt send -s "$SESSION" --type ":q!"
    sleep 0.3
    send_key "enter"
    check "vim quit" wait_exit 5000

    stop_monitor
    SESSION=""
else
    echo -e "  ${DIM}(skipped — vim not found)${NC}"
fi

# ═════════════════════════════════════════════════════════
echo -e "\n${BOLD}  less${NC}"
# ═════════════════════════════════════════════════════════

if command -v less &>/dev/null; then
    LESS_FILE=$(mktemp /tmp/phantom-less-XXXXXX.txt)
    for i in $(seq 1 100); do echo "Line $i: content" >> "$LESS_FILE"; done

    new_session "less" --cols 80 --rows 24 -- less "$LESS_FILE"
    start_monitor
    sleep 2
    wait_stable 500 10000

    check "less view" contains "Line 1:"

    send_key "space"
    check "less scroll" bash -c "$PT --socket $SOCK wait -s less --text-disappear 'Line 1:' --timeout 5000"

    send_key "q"
    check "less quit" wait_exit 5000

    stop_monitor
    rm -f "$LESS_FILE"
    SESSION=""
else
    echo -e "  ${DIM}(skipped — less not found)${NC}"
fi

# ═════════════════════════════════════════════════════════

echo ""
echo -e "  ${DIM}────────────────────────────${NC}"
if [ $FAILED -eq 0 ]; then
    echo -e "  ${GREEN}${BOLD}$PASSED/$TOTAL passed${NC}"
else
    echo -e "  ${GREEN}$PASSED passed${NC}, ${RED}${BOLD}$FAILED failed${NC} / $TOTAL total"
    echo -e "${RED}$ERRORS${NC}"
fi
echo ""

[ $FAILED -eq 0 ]
