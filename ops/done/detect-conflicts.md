# Detect and Reject Conflicts During Workspace Absorption

**Date**: 2026-02-24
**Status**: Draft
**Purpose**: Prevent JJ conflicts from silently entering the main workspace during absorption

## Problem

When an isolated workspace is absorbed back into the main workspace (or parent workspace), `jj rebase` can produce conflicts without failing. JJ exits with status 0 and prints a warning to stderr like:

```
New conflicts appeared in N commits
```

The current absorption code (`cli/src/session/isolation.rs:absorb_workspace`) only checks `output.status.success()`, so conflicts pass through silently. The user (or their next agent session) inherits a conflicted working copy with no warning.

**How this manifests:** Two sessions edit the same file (e.g., a template or design doc). The second session to finish absorbs its workspace, rebasing the main workspace on top. The rebase succeeds (exit 0) but creates conflict markers in the affected files. The next session starts on a conflicted commit and has to manually resolve.

## Summary

1. After a successful `jj rebase`, check stderr for JJ's conflict warning
2. If conflicts are detected, **don't fail** — the rebase already happened and the changes are valuable
3. Instead, surface the conflict clearly so it gets resolved before the next session starts

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Fail vs warn | Warn + surface to agent | The rebase already happened — failing would leave things in a worse state (absorbed but not cleaned up). Better to surface and let the agent resolve. |
| Detection method | Parse stderr for `"conflicts"` keyword | JJ consistently prints `"New conflicts appeared"` or `"unresolved conflicts"` to stderr. Simpler than a post-rebase `jj log` query. |
| Where to resolve | In the absorbing session, before cleanup | The session that caused the conflict has the most context to resolve it. After cleanup, the workspace is gone and the conflict is orphaned. |
| Conflict list | `jj status` in target after rebase | Shows exactly which files are conflicted in the target workspace. |

## Changes

### 1. Detect conflicts after rebase in `absorb_workspace`

**File:** `cli/src/session/isolation.rs`

After the successful rebase (line 252), before the debug log, add conflict detection:

```rust
// Check for conflicts in rebase output
let stderr = String::from_utf8_lossy(&output.stderr);
let has_conflicts = stderr.to_lowercase().contains("conflict");

if has_conflicts {
    // Get list of conflicted files from target workspace
    let status_output = jj_cmd()
        .current_dir(&target_dir)
        .args(["status"])
        .output();

    let conflicted_files = status_output
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    return Err(AikiError::WorkspaceAbsorbConflict {
        workspace: workspace.name.clone(),
        stderr: stderr.trim().to_string(),
        conflicted_files,
    });
}

if !stderr.is_empty() {
    debug_log(|| format!("jj rebase stderr: {}", stderr.trim()));
}
```

### 2. Add new error variant

**File:** `cli/src/error.rs`

```rust
#[error("Workspace absorption created conflicts in {workspace}:\n{stderr}\n\nConflicted files:\n{conflicted_files}")]
WorkspaceAbsorbConflict {
    workspace: String,
    stderr: String,
    conflicted_files: String,
},
```

### 3. Handle conflict error in the caller

**File:** `cli/src/flows/core/functions.rs` (around line 1262)

The caller currently treats all absorption errors as warnings. For conflicts specifically, we should attempt resolution:

```rust
match isolation::absorb_workspace(
    &repo_root,
    &workspace,
    parent_session_uuid.as_deref(),
) {
    Ok(()) => absorbed += 1,
    Err(AikiError::WorkspaceAbsorbConflict { workspace: ws, stderr, conflicted_files }) => {
        eprintln!(
            "[aiki] Warning: absorption of '{}' created conflicts:\n{}\n{}",
            ws, stderr, conflicted_files
        );
        // Still count as absorbed — changes landed, just conflicted
        absorbed += 1;
        // Store conflict info for hook context on next session start
        store_pending_conflicts(&repo_root, &conflicted_files);
    }
    Err(e) => {
        eprintln!(
            "[aiki] Warning: failed to absorb workspace '{}': {}",
            workspace.name, e
        );
    }
}
```

### 4. Surface conflicts to the next session via hook context

**File:** `cli/src/flows/core/functions.rs` (session start hook)

On session start, check if the working copy has unresolved conflicts and inject a `CONFLICT RESOLUTION REQUIRED` message into the hook context (the same mechanism that injects `WORKSPACE ISOLATION`):

```rust
// Check for conflicts in working copy
let status = jj_cmd()
    .current_dir(&repo_root)
    .args(["status"])
    .output();

if let Ok(output) = status {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("conflict") {
        // Extract conflicted file paths
        let conflict_lines: Vec<&str> = stdout
            .lines()
            .filter(|l| l.contains("conflict"))
            .collect();

        context.push(format!(
            "CONFLICT RESOLUTION REQUIRED: The following files have unresolved JJ conflicts:\n{}\n\nResolve these before starting new work.",
            conflict_lines.join("\n")
        ));
    }
}
```

### 5. Also check at workspace creation time

**File:** `cli/src/session/isolation.rs` — `create_isolated_workspace`

Before forking a new workspace, check if the fork point (`@-`) is already conflicted. If so, warn — the new workspace will inherit those conflicts.

```rust
// After resolving parent_change_id, before jj workspace add:
let conflict_check = jj_cmd()
    .current_dir(repo_root)
    .args([
        "log", "-r", &parent_change_id,
        "-T", "conflict", "--no-graph",
        "--ignore-working-copy",
    ])
    .output();

if let Ok(output) = conflict_check {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout == "true" {
        eprintln!(
            "[aiki] Warning: forking from a conflicted commit ({}). New workspace will inherit conflicts.",
            parent_change_id
        );
    }
}
```

---

## Implementation Order

1. Add `WorkspaceAbsorbConflict` error variant
2. Add conflict detection in `absorb_workspace` (the core fix)
3. Add conflict surfacing in session start hook context
4. Add pre-fork conflict check in `create_isolated_workspace`
5. Tests

## Tests

- Absorption of workspace that creates conflicts → returns `WorkspaceAbsorbConflict`
- Absorption of workspace with no conflicts → returns `Ok(())`
- Conflict detection parses JJ stderr correctly (test various JJ output formats)
- Session start hook surfaces conflicts when working copy is conflicted
- Fork from conflicted commit emits warning

## Files Changed

| File | Change |
|------|--------|
| `cli/src/error.rs` | Add `WorkspaceAbsorbConflict` variant |
| `cli/src/session/isolation.rs` | Detect conflicts post-rebase; check fork point for conflicts |
| `cli/src/flows/core/functions.rs` | Handle conflict error; surface in session start hook |
