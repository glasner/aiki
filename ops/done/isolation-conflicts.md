# Resolve Conflicts Before Ending Turn

## Problem

When multiple agents work concurrently in isolated workspaces, their changes are absorbed back into the main workspace via `turn.completed → workspace_absorb_all`. If two agents touch the same file, the rebase during absorption creates JJ conflict markers.

After the 10-agent test, `shared-sections.txt` and `shared-conflict.txt` both had `<<<<<<<` conflict markers. The user must manually resolve them.

**The fix**: Before absorbing, rebase the agent's workspace onto the current target @- to pull in other agents' changes. If this creates conflicts, don't absorb — keep the workspace alive and autoreply telling the agent to resolve the conflicts *in their own workspace*. Once resolved, absorb cleanly.

## Why resolve in the workspace, not in main

If we absorb first and then detect conflicts in main, multiple agents could see the same conflicts and race to fix them. Each agent must resolve conflicts in **its own isolated workspace** — that's the whole point of isolation.

## Design

### Core idea: rebase-before-absorb

Currently `absorb_workspace` does:
1. Rebase workspace chain onto target @- (step 1)
2. Rebase target @ onto workspace head (step 2)

The new approach adds a **pre-check** before step 1:

1. **Rebase workspace onto current target @-** (in the workspace, not in main)
   - This pulls in all changes that other agents have already absorbed
   - If conflicts arise, they appear in the workspace's working copy
2. **Check for conflicts in the workspace**
3. If conflicts → don't absorb, autoreply, agent resolves in workspace
4. If clean → proceed with normal absorption

### Why this works

- Each agent's workspace is independent — no two agents touch the same workspace
- The absorb lock is NOT held during resolution (only during the actual rebase steps)
- Multiple agents can resolve their own conflicts simultaneously
- After resolution, the workspace is already rebased onto @-, so absorption is a fast-forward

### Hook flow

```yaml
turn.completed:
    # Try to absorb — includes pre-rebase conflict detection.
    # Retry counting is handled inside absorb_workspace via a .conflict_retries
    # file in the workspace dir. After 3 failed attempts, it force-absorbs and
    # returns "ok" (conflicts land in main for human resolution).
    - let: absorb_result = self.workspace_absorb_all  # returns "conflicts" or "ok" or "0"

    - if: absorb_result == "conflicts"
      then:
          # Workspace was NOT absorbed — conflicts detected during pre-rebase.
          # Workspace is still alive with conflict markers in files.
          # Autoreply to have agent resolve in their workspace.
          - let: conflicts = self.detect_workspace_conflicts
          - autoreply: |
                CONFLICT RESOLUTION REQUIRED: Your changes conflict with changes from
                other concurrent agents. The conflicts are in your workspace files:

                {{conflicts}}

                Fix each file by editing it to remove the JJ conflict markers and
                produce the correct merged content. The markers look like:

                <<<<<<< conflict N of M
                %%%%%%% (diff from one side)
                +++++++ (content from other side)
                >>>>>>> conflict N of M ends

                Replace the entire conflict block with the correct merged content.
                Once all conflicts are resolved, your changes will be absorbed automatically.
```

### What changes in `absorb_workspace`

The current two-step rebase becomes a three-step process:

```
Step 0 (NEW): Pre-rebase workspace onto target @-
  - Run in the WORKSPACE directory (not target)
  - jj rebase -d <target_at_minus> -s <ws_fork_point>
  - This pulls target's absorbed changes into the workspace
  - Check: jj resolve --list (in workspace)
  - If conflicts → return early with "conflicts" status
  - If clean → continue to step 1

Step 1: Rebase workspace chain onto target @- (existing, now a fast-forward)
Step 2: Rebase target @ onto workspace head (existing)
Step 3: Cleanup workspace
```

After step 0, the workspace's parent chain includes all previously-absorbed changes. If there's a conflict, the agent resolves it in the workspace. On the next `turn.completed`, step 0 runs again — this time the workspace is already rebased and conflict-free, so it's a no-op, and steps 1-2 proceed normally.

### Agent stays in their workspace

The autoreply triggers another turn. On `turn.started`:

- `workspace_create_if_concurrent` is idempotent — the workspace already exists, so it returns the existing path
- The agent is still directed to work in their workspace
- No change needed to `turn.started`

On the next `turn.completed`:

- `workspace_absorb_all` runs again
- Step 0: workspace is already rebased onto @- and conflicts are resolved → no-op
- Steps 1-2: normal absorption → clean
- Step 3: cleanup

### Loop safety

If the agent fails to resolve conflicts after 3 autoreply attempts, give up and absorb with conflicts (existing behavior — conflicts land in main for human resolution).

#### Persisting conflict_retry_count

The retry count is stored as a file inside the workspace directory:

```
<workspace_path>/.conflict_retries    # contains a single integer (e.g., "1", "2", "3")
```

**Why workspace-local:** `AikiState` is per-event (ephemeral) and `TurnState` is computed from JJ history — neither supports arbitrary session-level persistent state. Storing the counter in the workspace directory is the simplest approach because:
- The workspace is the entity being retried — the counter is naturally scoped to it
- When the workspace is cleaned up after successful absorption, the counter file disappears automatically
- No new session-level state infrastructure is needed
- Multiple workspaces (from different agents) each track their own retry count independently

**Lifecycle:**
1. **Created** by `absorb_workspace` when conflicts are first detected (initialized to `1`)
2. **Incremented** on each subsequent conflict detection during `absorb_workspace`
3. **Read** by `absorb_workspace` to decide whether to force-absorb or return `Conflicts`
4. **Deleted** automatically when the workspace is cleaned up after successful absorption

```rust
let retries_path = workspace.path.join(".conflict_retries");
let conflict_retry_count = std::fs::read_to_string(&retries_path)
    .ok()
    .and_then(|s| s.trim().parse::<u32>().ok())
    .unwrap_or(0);

if conflict_retry_count >= 3 {
    // Force absorb with conflicts — human will resolve
    proceed_with_absorption();
} else {
    std::fs::write(&retries_path, (conflict_retry_count + 1).to_string())?;
    return AbsorbResult::Conflicts { retry: conflict_retry_count + 1 };
}
```

## Implementation

### Step 1: Modify `absorb_workspace` to pre-rebase and detect conflicts

**File**: `cli/src/session/isolation.rs`

Before the existing two-step rebase, add step 0:

```rust
pub fn absorb_workspace(
    repo_root: &Path,
    workspace: &IsolatedWorkspace,
    parent_session_uuid: Option<&str>,
) -> Result<AbsorbResult> {  // Changed return type from () to AbsorbResult
    // ... existing setup (get ws_change_id, etc.) ...

    let target_dir = /* ... existing target resolution ... */;

    // Step 0: Rebase workspace onto target's @- to detect conflicts early
    // Get target's @- change ID (use full change_id to avoid ambiguity in large repos)
    let target_at_minus = jj_cmd()
        .current_dir(&target_dir)
        .args(["log", "-r", "@-", "--no-graph", "-T", "change_id", "--limit", "1", "--ignore-working-copy"])
        .output()?;
    let target_at_minus = String::from_utf8_lossy(&target_at_minus.stdout).trim().to_string();

    // Rebase workspace onto target's @- (pulls in other agents' absorbed changes)
    let output = jj_cmd()
        .current_dir(&workspace.path)
        .args(["rebase", "-d", &target_at_minus, "--ignore-working-copy"])
        .output()?;

    // Snapshot to materialize conflict markers in working copy
    let _ = jj_cmd()
        .current_dir(&workspace.path)
        .args(["debug", "snapshot"])
        .output();

    // Check for conflicts in workspace
    let conflict_check = jj_cmd()
        .current_dir(&workspace.path)
        .args(["resolve", "--list"])
        .output()?;
    let conflicts = String::from_utf8_lossy(&conflict_check.stdout);

    if !conflicts.trim().is_empty() {
        // Conflicts detected — don't absorb, return early
        return Ok(AbsorbResult::Conflicts);
    }

    // No conflicts — proceed with absorption (steps 1 + 2)
    let _lock = acquire_absorb_lock(&lock_path)?;
    // ... existing step 1 + step 2 ...

    Ok(AbsorbResult::Absorbed)
}
```

### Step 2: New `AbsorbResult` enum

**File**: `cli/src/session/isolation.rs`

```rust
pub enum AbsorbResult {
    /// Workspace absorbed successfully
    Absorbed,
    /// Conflicts detected — workspace kept alive for agent resolution
    Conflicts,
    /// Nothing to absorb (no workspace, or workspace was empty)
    Skipped,
}
```

### Step 3: Update `workspace_absorb_all` to return conflict status

**File**: `cli/src/flows/core/functions.rs`

Instead of always cleaning up after absorption, check the result:

```rust
match isolation::absorb_workspace(&repo_root, &workspace, parent_session_uuid.as_deref()) {
    Ok(AbsorbResult::Absorbed) => {
        absorbed += 1;
        let _ = isolation::cleanup_workspace(&repo_root, &workspace);
        // Unregister session...
    }
    Ok(AbsorbResult::Conflicts) => {
        // Don't cleanup — workspace stays alive for conflict resolution
        has_conflicts = true;
    }
    Ok(AbsorbResult::Skipped) => {
        let _ = isolation::cleanup_workspace(&repo_root, &workspace);
    }
    Err(e) => { /* existing error handling */ }
}
```

Return `"conflicts"` in stdout if conflicts were detected, so the hook can check.

### Step 4: Add `detect_workspace_conflicts` function

**File**: `cli/src/flows/core/functions.rs`

Reads conflicted files from the workspace (not main):

1. Find the agent's workspace path (same as `workspace_create_if_concurrent` returns)
2. Run `jj resolve --list` in the workspace
3. For each conflicted file, read its content
4. Return formatted string

### Step 5: Update `turn.completed` hook

**File**: `cli/src/flows/core/hooks.yaml`

The retry count is now managed inside `absorb_workspace` itself (see "Persisting conflict_retry_count" above). When retries are exhausted, `absorb_workspace` force-absorbs and returns `Absorbed` instead of `Conflicts`. This means the hook logic doesn't need to track retries — it only needs to handle the two outcomes:

```yaml
turn.completed:
    - let: absorb_result = self.workspace_absorb_all  # returns "conflicts", "ok", or "0"
    # retry counting and force-absorb after 3 attempts are handled inside
    # workspace_absorb_all, using a .conflict_retries file in the workspace dir

    - if: absorb_result == "conflicts"
      then:
          # Workspace was NOT absorbed — conflicts detected, retries remaining
          - let: conflicts = self.detect_workspace_conflicts
          - autoreply: |
                CONFLICT RESOLUTION REQUIRED:
                Your changes conflict with other concurrent agents' changes.
                The conflicts are in your workspace files:

                {{conflicts}}

                Fix each file by removing the JJ conflict markers and producing
                the correct merged content. Once resolved, your changes will be
                absorbed automatically on the next turn.
```

### Step 6: Update CLAUDE.md

Add a short section explaining that agents may receive `CONFLICT RESOLUTION REQUIRED` autoreplies and should:

- Read the conflict content in their workspace files
- Edit each file to produce correct merged content
- Remove all conflict markers (`<<<<<<<`, `%%%%%%%`, `+++++++`, `>>>>>>>`)

### Step 7: Add conflict detection to `aiki task wait` output

When `aiki task wait` returns, check for conflicts in the parent's workspace and include them in the output so the parent agent is aware.

## Flow diagram

```
Agent writes files (in isolated workspace)
    ↓
turn.completed fires
    ↓
workspace_absorb_all:
    Step 0: rebase workspace onto target @-
            (pulls in other agents' changes)
    ↓
┌─── No conflicts in workspace? ──→ Steps 1+2: absorb into target → cleanup → Done.
│
└─── Conflicts in workspace?
         ↓
    Return "conflicts" (workspace kept alive)
         ↓
    autoreply with conflict details
         ↓
    turn.started fires
    workspace_create_if_concurrent returns EXISTING workspace path
    Agent stays in their workspace
         ↓
    Agent edits conflicted files (removes markers)
         ↓
    turn.completed fires
         ↓
    workspace_absorb_all:
        Step 0: already rebased, no new conflicts → continue
        Steps 1+2: absorb cleanly → cleanup → Done.
```

## Key properties

1. **No race conditions**: Each agent resolves in its own workspace — no shared state during resolution
2. **Absorb lock not held during resolution**: Lock only held for the fast rebase steps, not while agent thinks
3. **Idempotent**: `workspace_create_if_concurrent` returns existing workspace, no special-casing needed in `turn.started`
4. **Clean absorption**: After resolution, the workspace is already rebased onto @-, so absorption is essentially a fast-forward

## Implementation order

1. `AbsorbResult` enum (Step 2)
2. Modify `absorb_workspace` with step 0 pre-rebase (Step 1)
3. Update `workspace_absorb_all` to handle conflicts result (Step 3)
4. `detect_workspace_conflicts` function (Step 4)
5. `turn.completed` autoreply hook (Step 5)
6. CLAUDE.md guidance (Step 6)
7. `aiki task wait` conflict output (Step 7)
8. Test with 10-agent scenario

## Open questions

1. **Large files**: If a conflicted file is huge, including its full content in the autoreply could bloat the context. Consider truncating or showing only the conflict sections.

2. **Step 0 rebase target**: Should we rebase onto @- or onto the latest absorbed change? @- is simpler and always correct — it's where absorption would insert the workspace chain anyway.

3. **Force-absorb fallback**: After 3 failed resolution attempts, should we absorb with conflicts (letting them propagate to main) or leave the workspace alive? Absorbing preserves the changes at least; leaving the workspace risks orphaning them.
