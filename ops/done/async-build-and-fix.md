# Fix --async for build and fix commands

## Problem

Both `aiki build --async` and `aiki fix --async` are broken.

**build --async**: The async flag is only passed to `run_loop()`, which spawns the loop agent in the background via `task_run_async`. But all the work *before* the loop — `find_or_create_epic()` which runs the decompose agent via `task_run()` — still blocks the caller. So `aiki build --async` blocks on decompose, then only the loop runs in the background.

**fix --async**: The `run_async` parameter is renamed to `_run_async` and completely ignored. The entire quality loop (plan → decompose → loop → review × N iterations) always runs synchronously.

Both commands should return immediately when `--async` is given, with the entire pipeline running in a background process.

## Design

### 1. Shared pattern: spawn-self with `--_continue-async`

When `--async` is set, the command:

1. Creates the container task synchronously (fast — just event writes)
2. Spawns `aiki <command> --_continue-async <container-id> [other flags]` as a detached child process
3. Emits the container task ID to stdout and returns immediately

The background process picks up via `--_continue-async`, which skips container creation and goes straight to the pipeline work.

`--_continue-async` is internal plumbing — hidden from `--help` via `#[arg(hide = true)]`. The underscore prefix signals "not a public API."

#### Shared utility

```rust
// cli/src/commands/async_spawn.rs (or add to an existing shared module)

/// Spawn `aiki <args>` as a detached background process.
///
/// The child process inherits cwd but detaches stdin/stdout/stderr
/// so the parent can exit immediately.
pub fn spawn_aiki_background(cwd: &Path, args: &[&str]) -> Result<()>
```

Uses `get_aiki_binary_path()` + `std::process::Command` with `Stdio::null()` for all three streams. Same daemonization pattern as `AgentRuntime::spawn_background` in the agent runtimes.

### 2. Build changes

**New hidden CLI arg:**

```rust
#[arg(long = "_continue-async", hide = true)]
pub continue_async: Option<String>,
```

**Async path** (`run_build_plan` when `run_async == true`):

1. Validate plan (exists, not draft) — stays synchronous (fast)
2. Cleanup stale builds — stays synchronous (fast)
3. Find existing epic or create empty epic task via `create_epic_task()` — no decompose
4. `spawn_aiki_background(cwd, &["build", "--_continue-async", &epic_id, plan_path, ...])`
5. Emit epic ID, return

**`--_continue-async` path**:

1. Find the epic by ID
2. Check if it has subtasks:
   - No subtasks → run decompose (create decompose task, `task_run`)
   - Has subtasks → skip decompose
3. `run_loop()` (synchronous — we're already in the background process)
4. Post-build review/fix if requested

This is similar to the existing `run_build_epic()` path but handles the "no subtasks yet" case by running decompose first.

### 3. Fix changes

**New hidden CLI arg:**

```rust
#[arg(long = "_continue-async", hide = true)]
pub continue_async: Option<String>,
```

**Async path** (`run_fix` when `run_async == true`):

1. Validate review task, determine scope/assignee — stays synchronous (fast)
2. Create fix-parent task via `create_fix_parent()`
3. `spawn_aiki_background(cwd, &["fix", "--_continue-async", &fix_parent_id, &review_id, ...])`
4. Emit fix-parent ID, return

**`--_continue-async` path**:

1. Find the fix-parent by ID
2. Read the review ID from fix-parent's `data.review`
3. Enter the quality loop from step 3 onward (plan-fix → decompose → loop → review → repeat), skipping fix-parent creation (step 2)

### 4. Quality loop termination bug (fix.rs)

After `MAX_QUALITY_ITERATIONS`, the loop falls through to `Ok(())` without surfacing that issues remain. After the loop, check if the last review still has issues and return an error or emit a warning.

## Changes

| File | Change |
|------|--------|
| `cli/src/commands/mod.rs` | Add `async_spawn` module |
| `cli/src/commands/async_spawn.rs` | New: `spawn_aiki_background()` utility |
| `cli/src/main.rs` | Add `--_continue-async` (hidden) to Fix |
| `cli/src/commands/build.rs` | Add `--_continue-async` (hidden); split async/continue paths in `run_build_plan` and `run_build_epic` |
| `cli/src/commands/fix.rs` | Add `--_continue-async` (hidden); wire `run_async` via spawn-self, add continue path, fix loop termination |
| `cli/src/commands/epic.rs` | Make `create_epic_task()` public so build's async path can call it directly |

## Issues addressed

From review `poswnrlqsxnssp`:

- **Issue 1** — fix `--async` ignored: spawn-self pattern
- **Issue 2** — build `--async` incomplete: same spawn-self pattern
- **Issue 4** — quality loop termination: error/warning after max iterations

## Related

- `ops/now/build-and-fix-templates.md` — per-stage template overrides (issue 3)
