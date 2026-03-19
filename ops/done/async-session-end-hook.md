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
                ├─ Read stdin as raw bytes (no parsing needed)
                ├─ spawn_aiki_background_with_stdin(
                │     "hooks", "stdin", "--event", "SessionEnd",
                │     "--_continue-async",
                │     stdin_payload = original JSON
                │  )
                ├─ Print JSON response to stdout
                └─ exit(0)  ← Claude Code sees success immediately

Background process:
  aiki hooks stdin --event SessionEnd --_continue-async
    │
    ├─ Read payload from stdin (same JSON, piped by parent)
    ├─ Run full session.ended flow synchronously (same code path as sync)
    │     └─ workspace_absorb_all()
    │     └─ cleanup_orphaned_workspaces()
    │     └─ session.end()
    │     └─ emit Absorbed task events
    └─ exit(0)
```

### Why pipe stdin (not a temp file or CLI arg)

The background process receives the exact same JSON payload on stdin that the hook would have processed synchronously. This means:

- **Identical code path** — the child reads stdin the same way the sync path does. No separate "background handler", no file I/O, no cleanup.
- **No argument reconstruction** — no need to extract session-id, cwd, reason, etc. and thread them through CLI args. The payload already has everything.
- **No temp file** — no file to write, read, or clean up. No risk of orphaned temp files. No size limits (unlike CLI args which cap at ~256KB on Linux).
- **Future-proof** — if `session.ended` handlers grow (new flow actions, new event fields), the background process automatically gets the full context.

### Implementation

Follows the pattern from `commands/async_spawn.rs` + `--_continue-async` hidden args used by `review.rs`, `fix.rs`, and `build.rs`, with a new stdin-piping variant:

1. **Add `spawn_aiki_background_with_stdin`** to `commands/async_spawn.rs`
   ```rust
   pub fn spawn_aiki_background_with_stdin(cwd: &Path, args: &[&str], stdin_payload: &[u8]) -> Result<()> {
       let binary = crate::config::get_aiki_binary_path();

       let mut child = Command::new(&binary)
           .current_dir(cwd)
           .args(args)
           .stdin(Stdio::piped())
           .stdout(Stdio::null())
           .stderr(Stdio::null())
           .spawn()
           .with_context(|| format!("failed to spawn background aiki process: {binary}"))?;

       // Write payload to child's stdin, then drop to send EOF
       if let Some(mut stdin) = child.stdin.take() {
           use std::io::Write;
           let _ = stdin.write_all(stdin_payload); // best-effort
       }
       // Child is detached — don't wait

       Ok(())
   }
   ```

2. **Add `--_continue-async` hidden flag to the hooks stdin command** (`commands/hooks.rs`)
   ```rust
   #[arg(long = "_continue-async", hide = true)]
   continue_async: bool,  // flag only, no value needed
   ```

3. **Normal hook call** (no `--_continue-async`): when event is `SessionEnd`:
   - Call `spawn_aiki_background_with_stdin(cwd, &["hooks", "stdin", "--event", "SessionEnd", "--_continue-async"], &stdin_json)`
   - Return `HookCommandOutput::new(None, 0)` immediately

4. **Background call** (`--_continue-async` is set):
   - Read payload from stdin (normal stdin read — identical to sync path)
   - Run the full `session.ended` flow synchronously

5. **Add `"SessionEnd"` arm to `build_command_output`** in `output.rs`
   - Returns `HookCommandOutput::new(None, 0)` — no JSON needed
   - Removes the spurious "Unknown event type" warning

### Error handling

- If `spawn_aiki_background_with_stdin()` fails, **fall back to synchronous execution** (better slow than lost work)
- Background process logs to stderr (which is `/dev/null` via the spawn, but could optionally append to a debug log)
- No temp files to clean up

### Process lifecycle

The new `spawn_aiki_background_with_stdin()` mirrors the existing `spawn_aiki_background()` detachment pattern but uses `Stdio::piped()` for stdin instead of `Stdio::null()`. The parent writes the payload, drops the stdin handle (sending EOF), and returns without waiting.

This works for `review`/`fix`/`build` where the parent exits and the child continues. For SessionEnd, Claude Code may kill the process group on exit. If this proves to be an issue, add `setsid()` via `pre_exec` — but try without it first since the existing pattern works.

## Steps

1. Add `"SessionEnd"` arm to `build_command_output` in `output.rs` (trivial, do first)
2. Add `spawn_aiki_background_with_stdin()` to `commands/async_spawn.rs`
3. Add `--_continue-async` hidden flag to `hooks stdin` command
4. In SessionEnd handler: pipe payload to background via stdin, return immediately
5. In background path (`--_continue-async`): read stdin, run session.ended flow (same code as sync)
6. Add fallback: if spawn fails, run synchronously
7. Test: verify hook exits fast and background cleanup completes

## Risks

- **Lost work if background process dies** — mitigate with fallback-to-sync
- **Race with next session** — unlikely since session IDs are unique, and lock file in `absorb_workspace` already handles concurrent access
- **Claude Code kills process group** — if detachment isn't enough, add `setsid()`. Existing `--_continue-async` pattern hasn't needed this.
- **Stdin write failure** — if the parent can't write the full payload before being killed, the child gets a truncated/empty stdin and fails to parse. Mitigated by: the write is fast (small payload), and the fallback-to-sync path handles spawn failures
