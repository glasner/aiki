# Step 1d: Never Delete Unabsorbed Files

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P0
**Phase**: 1 — Stop the bleeding (prevent data loss now)
**Source**: "Always Absorb" design (2026-03-19) — Rules 2-4
**Depends on**: Nothing (but most valuable after 1a reduces failure frequency)

---

## Problem

Even after fixing stale locks, absorptions can still fail (JJ errors, corrupted `.jj/repo`, snapshot failures). Today, every failure path ends with `cleanup_workspace` deleting files — causing silent data loss.

### Current Data Loss Paths

#### Path 1: `AbsorbResult::Skipped` → cleanup deletes files

**File:** `functions.rs:1163-1164`

```rust
Ok(isolation::AbsorbResult::Skipped) => {
    let _ = isolation::cleanup_workspace(&repo_root, &workspace);
}
```

`Skipped` is returned when:
- Workspace not found in `jj workspace list` (isolation.rs:354-361)
- Workspace head is root/zero change ID (isolation.rs:381-384)
- Idempotency guard: ws_head already in `::@` (isolation.rs:468-478)

The idempotency case is genuinely safe (already absorbed). The other two cases may have unabsorbed files on disk that get deleted.

#### Path 2: `absorb_workspace` errors → cleanup deletes files

**File:** `functions.rs:1166-1171`

```rust
Err(e) => {
    eprintln!("[aiki] Warning: failed to absorb workspace '{}': {}", ...);
    let _ = isolation::cleanup_workspace(&repo_root, &workspace);
}
```

Rebase failures, JJ errors, lock issues — any error means files are deleted with only an eprintln warning.

#### Path 3: Can't find repo root → `rm -rf` the workspace

**File:** `functions.rs:1058-1067`

```rust
None => {
    // Clean up directory even if we can't absorb
    let _ = std::fs::remove_dir_all(&session_ws_dir);
    continue;
}
```

If `find_repo_root_from_workspace` fails (e.g., `.jj/repo` file is corrupted), the entire workspace directory is deleted.

#### Path 4: `jj status` snapshot failure is ignored

**File:** `isolation.rs:369-372`

```rust
let _ = jj_cmd()
    .current_dir(&workspace.path)
    .args(["status"])
    .output();
```

The snapshot that captures pending file writes is fire-and-forget. If it fails, absorption proceeds with stale data.

---

## Fixes

### Rule 2: Check for files before cleanup on skip/error

Before calling `cleanup_workspace`, check if the workspace has any modified files:

```rust
fn workspace_has_changes(workspace_path: &Path) -> bool {
    let output = jj_cmd()
        .current_dir(workspace_path)
        .args(["diff", "--summary", "-r", "@"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            !String::from_utf8_lossy(&o.stdout).trim().is_empty()
        }
        _ => {
            // Can't determine — assume there are changes (safe default)
            true
        }
    }
}
```

### Rule 3: Fallback copy on failed absorption

When absorption fails but workspace has files, copy them directly into the main workspace as a last resort:

```rust
fn fallback_copy_files(workspace_path: &Path, target_dir: &Path) -> Result<()> {
    let output = jj_cmd()
        .current_dir(workspace_path)
        .args(["diff", "--summary", "-r", "@"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Lines look like: "M path/to/file" or "A path/to/file"
        let path = line.split_whitespace().nth(1);
        if let Some(rel_path) = path {
            let src = workspace_path.join(rel_path);
            let dst = target_dir.join(rel_path);
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::copy(&src, &dst);
            }
        }
    }
    Ok(())
}
```

This is a brute-force fallback — no JJ provenance, no metadata — but the files survive. Better than silent deletion.

### Rule 4: Don't ignore snapshot failures

Replace the fire-and-forget snapshot with an error check:

```rust
let snapshot_output = jj_cmd()
    .current_dir(&workspace.path)
    .args(["status"])
    .output();

match snapshot_output {
    Ok(o) if !o.status.success() => {
        let stderr = String::from_utf8_lossy(&o.stderr);
        eprintln!(
            "[aiki] WARNING: workspace snapshot failed, files may not be captured: {}",
            stderr.trim()
        );
    }
    Err(e) => {
        eprintln!(
            "[aiki] WARNING: workspace snapshot failed to run: {}",
            e
        );
    }
    _ => {}
}
```

Still proceed with absorption (the snapshot might have partially worked), but at least log the failure.

---

## Updated `workspace_absorb_all` Flow

```
for each workspace:
  1. Find repo root
     └─ FAIL → log warning, DO NOT delete workspace, continue

  2. Snapshot workspace (jj status)
     └─ FAIL → log warning, continue anyway (partial snapshot may work)

  3. absorb_workspace()
     ├─ Absorbed → cleanup workspace ✓
     ├─ Empty → cleanup workspace ✓
     ├─ Skipped(reason) → check for files on disk
     │   ├─ No files → cleanup workspace ✓
     │   └─ Has files → fallback_copy_files(), then cleanup
     └─ Error → check for files on disk
         ├─ No files → cleanup workspace ✓
         └─ Has files → fallback_copy_files(), then cleanup
```

**Key invariant: `cleanup_workspace` is ONLY called after confirming no unabsorbed files exist on disk.**

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/session/isolation.rs` | Add `workspace_has_changes()` function; update snapshot to check errors |
| `cli/src/flows/core/functions.rs` | Add `fallback_copy_files()`; add `workspace_has_changes` guard before every `cleanup_workspace` call; update `find_repo_root` failure path to not delete |

---

## Implementation Steps

1. Add `workspace_has_changes()` to `isolation.rs` (~15 lines)
2. Update the `jj status` snapshot in `isolation.rs` to check for failures (Rule 4)
3. Add `fallback_copy_files()` to `functions.rs` (~25 lines)
4. In `functions.rs`, wrap every `cleanup_workspace` call with a `workspace_has_changes` check
5. In the `find_repo_root` failure path, log a warning instead of `remove_dir_all`
6. Run `cargo test` to verify no regressions
7. **Run the full isolation test:** Execute the test plan at `cli/tests/prompts/test_session_isolation.md`
