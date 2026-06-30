# Crash report: SIGSEGV on terminal-query responses (move-unsafe FFI callback)

**Status:** root cause confirmed — **fixed** by upgrading to libghostty-vt 0.2.0
(branch `apasmantier/-/libghostty-vt-0.2.0`)
**Severity:** high — segfaults the daemon/engine process (unrecoverable)
**Discovered:** 2026-07-01, dogfooding phantom-mcp to drive an editor

## TL;DR

The `libghostty-vt` 0.1.1 binding registers terminal callbacks
(`on_pty_write`, etc.) by handing libghostty a pointer to a struct field that
lives **inline inside the `Terminal`** (`&self.vtable`). That pointer is **not
stable across moves**. phantom moves the `Terminal` (inside `Session`) several
times after registering the callback, so libghostty is left holding a dangling
pointer. The callback fires only when the guest program asks the terminal for
something (a DSR / DA / DECRQM query that needs a reply). When it finally fires,
it dereferences the stale pointer and the process **SIGSEGVs**.

- bash / `less` / trivial editors never query the terminal → callback never
  invoked → no crash.
- **neovim** queries aggressively on startup → crashes immediately.
- **vim + syntax on a large file** eventually emits a cursor-position request
  (DSR) → crashes. "Colored output volume" is a *correlation*, not the cause.

The old `CLAUDE.md` note ("`--clean -u NONE` … `defaults.vim` triggers
libghostty-vt crashes") is a misdiagnosis. Config isn't the variable; whether
the guest issues a **terminal query** is.

## Confirmed root cause

### Crash backtrace (lldb, thread `phantom-engine`)

```
EXC_BAD_ACCESS (code=1, address=0xa9454ff4a9467c0d)   # stale-stack garbage
 #0 phantom_daemon::session::Session::new::{closure#0} + 20     # our on_pty_write closure
 #1 libghostty_vt::terminal::Terminal::on_pty_write::callback   # binding trampoline
 #2 libghostty-vt.dylib terminal.c.terminal.Effects.writePtyTrampoline
 #3 libghostty-vt.dylib terminal.stream_terminal.Handler.deviceStatus   # <-- DSR/DA reply
 #4 libghostty-vt.dylib terminal.stream_terminal.Handler.vt__anon_46008
 #5 libghostty-vt.dylib terminal.stream.Stream(...).nextNonUtf8
 #6 libghostty-vt.dylib ghostty_terminal_vt_write
 #7 phantom_daemon::session::Session::process_pty_output
 #8 phantom_daemon::engine::Engine::run
```

`deviceStatus` = the guest sent a Device Status Report request; libghostty is
trying to write the *reply* back through the registered write callback.

### The buggy binding

`libghostty-vt-0.1.1/src/terminal.rs`, `handlers!` macro:

```rust
self.vtable.$name = Some(Box::new(f));
self.set(
    GHOSTTY_TERMINAL_OPT_USERDATA,
    &self.vtable          // pointer INTO the Terminal struct — not move-stable
)?;
```

```rust
// SAFETY: We own the vtable, so it should never become invalid.   <-- WRONG
let vtable = unsafe { &mut *ud.cast::<VTable<'_, '_>>() };
```

Owning the data keeps it from being *dropped*; it does **not** pin its
*address*. `&self.vtable` is invalidated by any move of the `Terminal`.

### How phantom trips it

`crates/phantom-daemon/src/session.rs`, `Session::new`:

```rust
let mut terminal = Terminal::new(...)?;        // (A) terminal on the stack
terminal.on_pty_write(move |_t, data| { ... })?;   // registers &terminal.vtable @ (A)
...
Ok(Self { terminal, ... })                     // (B) terminal moved into Session — addr changes
// caller then moves Session into the engine's session map — addr changes again
```

After (B) and the later move into the session map, libghostty still points at
(A) — a dead stack slot. First DSR reply → deref garbage → crash.

## Reproduction

```bash
PT=./target/release/pt
R=$(pwd)/crates/phantom-mcp

RUST_BACKTRACE=1 ./target/release/phantom-daemon &     # foreground daemon (does NOT daemonize)

# Crashes (vim emits a DSR during a full syntax-highlighted redraw):
$PT run -s repro --cols 110 --rows 36 -- vim --clean -n $R/src/server.rs

# Crashes immediately (nvim queries on startup):
$PT run -s repro -- nvim --clean -u NONE $R/src/server.rs
```

Native backtrace on demand:

```bash
lldb -b -o 'process handle SIGSEGV --stop true --pass false' -o run \
     -k 'bt all' -k 'quit' -- ./target/release/phantom-daemon
# then trigger the crash from another shell
```

## Test matrix

| Program | File / behavior            | Issues terminal query? | Result |
|---------|----------------------------|------------------------|--------|
| bash    | interactive                | no                     | OK     |
| less    | any source                 | no                     | OK     |
| synthetic | 33k fg+bg 256-color cells | no                     | OK     |
| vim     | build.rs, syntax on/off    | no (small redraw)      | OK     |
| vim     | server.rs, syntax **off**  | no                     | OK     |
| vim     | server.rs, syntax **on**   | **yes (DSR)**          | CRASH  |
| nvim    | any                        | **yes (startup)**      | CRASH  |

The discriminator is the last column, not color or file size.

## Fix options

1. **Upgrade to `libghostty-vt` 0.2.0 — applied & verified.** 0.2.0 changes
   `Terminal.vtable` from an inline field to `Box<VTable>` and registers the
   boxed *pointee's* heap address as USERDATA
   (`std::ptr::from_mut(self.vtable.as_mut())`), which survives moves of the
   `Terminal`. The `on_pty_write` closure signature is unchanged, so phantom's
   call site in `Session::new` ports as-is. `libghostty-vt-sys` follows to
   0.2.0 transitively.

   **Applied:** `libghostty-vt = "0.1"` → `"0.2"` in
   `crates/phantom-daemon/Cargo.toml`, plus the only API break — `ffi::Ghostty`
   prefix dropped: `ffi::GhosttyPointCoordinate` → `ffi::PointCoordinate`
   (2 spots in `session.rs`). 3-line change total.

   **Verified:** previously-crashing `vim --clean -n server.rs` and
   `nvim --clean` both now run and render with the daemon staying alive (no
   SIGSEGV in its log); nvim is interactive (cursor/DSR responses flow). Full
   `phantom-daemon` test suite: 53 passed, 0 failed.
   Minor follow-up: cursor `style` now reports `"unknown"` (cosmetic 0.2.0
   enum change) — worth a separate look.

2. **phantom-side workaround (no binding change).** Keep the `Terminal` at a
   stable address from registration onward and register the callback only once
   it's there:
   - Heap-allocate the `Session` (`Box<Session>`) / pin it, and call
     `on_pty_write` **after** it's in its final location, **or**
   - Re-install the callbacks after the `Session` is inserted into the engine
     map (re-registration rewrites the user-data to the current address).

   Either keeps `&self.vtable` valid when the first DSR arrives.

3. **Stopgaps if neither is ready:** drive editors that don't query the
   terminal (or run with queries disabled). Not viable for nvim. Don't rely on
   `-u NONE` as a "fix" — it only reduces the chance of a query.

## Secondary findings (separate, lower priority)

1. **Dead-daemon hang.** After a crash the socket is stale; the next `pt`
   command auto-spawns a *foreground* daemon and blocks indefinitely instead of
   failing fast. (`phantom-daemon` run directly does not daemonize.)
2. **Orphan observer sockets.** phantom-mcp's Ctrl-C/EOF cleanup in `main.rs`
   doesn't run on a hard crash → `~/.phantom/phantom-mcp-*.sock` leak.
3. **`pt kill` tombstones.** A killed session stays in the registry as `exited`,
   so its name can't be reused by `run -s <name>`.

## Environment

- darwin 25.5.0 (arm64); phantom @ `main` (0.1.0)
- vim 9.1 (`/usr/bin/vim`); nvim (`/opt/homebrew/bin/nvim`)
- libghostty-vt 0.1.1 + libghostty-vt-sys 0.1.1 (Zig 0.15.x vendored build)
- lldb-1703.0.236.21
</content>
