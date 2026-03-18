# Async Session-End Hook

## Problem

Claude Code fires `SessionEnd` hook on exit and kills the process before it finishes → `Hook cancelled` error.

The `session.ended` handler runs `workspace_absorb_all` which spawns 6-8+ JJ subprocesses sequentially (snapshot, rebase, conflict-check, orphan cleanup, etc.). This takes seconds; Claude Code doesn't wait that long.

Secondary: `build_command_output` has no `"SessionEnd"` arm — falls to `_` default, prints a spurious warning to stderr.

## Design

**Use the existing `--_continue-async` pattern (same as `review`, `fix`, `build`) to fork the heavy work into a background process and exit the hook immediately.**

### Architecture

```
Claude Code → aiki hooks stdin --event SessionEnd
                │
                ├─ Parse stdin JSON (fast)
                ├─ Write full payload to temp file
                ├─ spawn_aiki_background("hooks", "stdin", "--event", "SessionEnd",
                │                        "--_continue-async", <payload-path>)
                ├─ Print JSON response to stdout
                └─ exit(0)  ← Claude Code sees success immediately

Background process:
  aiki hooks stdin --event SessionEnd --_continue-async /tmp/aiki-session-end-<uuid>.json
    │
    ├─ Read payload from temp file (same JSON that came on stdin)
    ├─ Delete temp file
    ├─ Run full session.ended flow synchronously (same code path as sync)
    │     └─ workspace_absorb_all()
    │     └─ cleanup_orphaned_workspaces()
    │     └─ session.end()
    │     └─ emit Absorbed task events
    └─ exit(0)
```

### Why pass the full payload

The background process reads the exact same JSON payload that the hook would have processed synchronously. This means:

- **Same code path** — the `--_continue-async` handler calls the exact same `dispatch_event()` / flow engine logic as the sync path. No separate "background handler" to maintain.
- **No argument reconstruction** — no need to extract session-id, cwd, reason, etc. and thread them through CLI args. The payload already has everything.
- **Future-proof** — if `session.ended` handlers grow (new flow actions, new event fields), the background process automatically gets the full context.

### Why a temp file (not CLI arg or env var)

The stdin JSON payload can be large (especially with tool results). CLI arg length limits vary by OS (~256KB on Linux, ~1MB on macOS). A temp file has no size limit and is simple:

```rust
// Parent: write payload, spawn background
let payload_path = std::env::temp_dir()
    .join(format!("aiki-session-end-{}.json", uuid::Uuid::new_v4()));
std::fs::write(&payload_path, &stdin_json)?;

spawn_aiki_background(cwd, &[
    "hooks", "stdin",
    "--event", "SessionEnd",
    "--_continue-async", payload_path.to_str().unwrap(),
])?;
```

```rust
// Child: read payload, delete file, process normally
let payload = std::fs::read_to_string(&payload_path)?;
let _ = std::fs::remove_file(&payload_path); // best-effort cleanup
// Feed `payload` into the same dispatch path as stdin
```

### Implementation

Follows the exact pattern from `commands/async_spawn.rs` + `--_continue-async` hidden args used by `review.rs`, `fix.rs`, and `build.rs`:

1. **Add `--_continue-async` hidden arg to the hooks stdin command** (`commands/hooks.rs`)
   ```rust
   #[arg(long = "_continue-async", hide = true)]
   continue_async: Option<String>,  // path to payload temp file
   ```

2. **Normal hook call** (no `--_continue-async`): when event is `SessionEnd`:
   - Write stdin payload to temp file
   - Call `spawn_aiki_background()` (from `commands/async_spawn.rs`)
   - Return `HookCommandOutput::new(None, 0)` immediately

3. **Background call** (`--_continue-async` is set):
   - Read payload from the file path
   - Delete the temp file
   - Run the full `session.ended` flow synchronously — same code path as today, just fed from file instead of stdin

4. **Add `"SessionEnd"` arm to `build_command_output`** in `output.rs`
   - Returns `HookCommandOutput::new(None, 0)` — no JSON needed
   - Removes the spurious "Unknown event type" warning

### Error handling

- If `spawn_aiki_background()` fails, **fall back to synchronous execution** (better slow than lost work)
- Background process logs to stderr (which is `/dev/null` via `spawn_aiki_background`, but could optionally append to a debug log)
- Temp file cleanup: deleted by background process on read. If background dies, temp dir is pruned by OS.

### Process lifecycle

`spawn_aiki_background()` (in `commands/async_spawn.rs`) already handles detachment:
```rust
Command::new(&binary)
    .current_dir(cwd)
    .args(args)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()?;
```

This works for `review`/`fix`/`build` where the parent exits and the child continues. For SessionEnd, Claude Code may kill the process group on exit. If this proves to be an issue, add `setsid()` via `pre_exec` — but try without it first since the existing pattern works.

## Steps

1. Add `"SessionEnd"` arm to `build_command_output` in `output.rs` (trivial, do first)
2. Add `--_continue-async` hidden arg to `hooks stdin` command
3. In SessionEnd handler: write payload to temp file, spawn background, return immediately
4. In background path: read payload from file, delete file, run session.ended flow (same code as sync)
5. Add fallback: if spawn fails, run synchronously
6. Test: verify hook exits fast and background cleanup completes

## Risks

- **Lost work if background process dies** — mitigate with fallback-to-sync; temp file survives for manual retry
- **Race with next session** — unlikely since session IDs are unique, and lock file in `absorb_workspace` already handles concurrent access
- **Claude Code kills process group** — if `Stdio::null()` detachment isn't enough, add `setsid()`. Existing `--_continue-async` pattern hasn't needed this.
- **Temp file left behind** — harmless, OS cleans temp dir periodically. Could add cleanup in `session.started` if desired.
