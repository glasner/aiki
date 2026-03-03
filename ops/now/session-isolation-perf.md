# Session Isolation Performance: Reduce JJ Subprocess Overhead

## Problem

Each workspace absorption cycle invokes ~8-10 jj subprocesses. Several of these are redundant or can be deferred. Cutting 3 calls per turn reduces latency on every agent turn without changing correctness.

### Current subprocess count per absorption

| Step | Commands | Count |
|------|----------|-------|
| Find workspace change ID | `jj workspace list` + `jj log` (resolve short→full) | 2 |
| Snapshot workspace | `jj status` | 1 |
| Snapshot target (inside lock) | `jj status` | 1 |
| Two-phase rebase | `jj rebase -b` + `jj rebase -s` | 2 |
| Sync filesystem | `jj workspace update-stale` | 1 |
| Conflict detection | `jj log -r "conflicts() & @"` + `jj resolve --list` | 1-2 |
| Orphan cleanup | `jj workspace list` | 1 |
| **Total** | | **9-10** |

---

## Changes

### 1. One-shot change ID resolution via `jj workspace list -T`

**File:** `cli/src/session/isolation.rs` — `find_workspace_change_id()`

**Current** (2 subprocesses):
```
jj workspace list --ignore-working-copy
→ parse "aiki-<uuid>: <short_id> ..."
→ jj log -r <short_id> -T change_id --no-graph --limit 1 --ignore-working-copy
```

**After** (1 subprocess):
```
jj workspace list --ignore-working-copy -T 'name ++ " " ++ target.change_id() ++ "\n"'
```

Parse the output as `<name> <full_change_id>` lines. The template emits full change IDs directly, eliminating the second `jj log` resolution.

**Saves:** 1 subprocess per absorption.

**Risk:** Low. Verify the template keyword `target` exists in our minimum supported jj version. Fallback: use `commit_id` and convert, or keep the two-step path behind a version check.

**Validation:** ✅ Tested with jj 0.38.0 — Template works correctly, outputs full 32-character change IDs for all workspaces.

---

### 2. Combine conflict detection into single `jj resolve --list`

**File:** `cli/src/flows/core/functions.rs` — `workspace_absorb_all()` (lines 1106-1161)

**Current** (2 subprocesses):
```rust
// Step 1: boolean check
jj log -r "conflicts() & @" --no-graph -T 'change_id ++ "\n"' --ignore-working-copy
// Step 2: if non-empty, get file list
jj resolve --list -r @
```

**After** (1 subprocess):
```rust
// Single call — check BOTH stdout and stderr for conflict output.
// JJ may emit conflict listings on either stream depending on version.
let output = jj resolve --list -r @;
let stdout = String::from_utf8_lossy(&output.stdout);
let stderr = String::from_utf8_lossy(&output.stderr);
// Only classify as clean if BOTH streams are empty after trimming.
let has_conflict_output = !stdout.trim().is_empty() || !stderr.trim().is_empty();
if has_conflict_output {
    // Use whichever stream has content as the file list
    let files = if !stdout.trim().is_empty() { stdout } else { stderr };
    // ... report conflicts with file list ...
}
```

<<<<<<< conflict 1 of 4
%%%%%%% diff from: ttsnqluz dca7ece8 (parents of rebased revision)
\\\\\\\        to: ttsnqluz fe5cae93 (rebase destination)
-If exit code is 0 and stdout is empty (or contains no conflict entries), @ is clean. If non-empty, we already have the file list for the autoreply. Skip the preliminary `jj log` check entirely.
+If exit code is 0, conflicts exist and stdout contains the file list for the autoreply. If exit code is 2, @ is clean. Skip the preliminary `jj log` check entirely.
+++++++ kvqvklpw d0337a7a "[aiki]" (rebased revision)
If exit code is 0 and **both stdout and stderr are empty** (after trimming), @ is clean. If either stream contains output, we have conflicts and already have the file list for the autoreply. Skip the preliminary `jj log` check entirely.

**Important:** Must check both stdout and stderr — not just stdout. The existing code (functions.rs:1122-1132) already handles this because JJ emits conflict listings on different streams across versions. The simplified single-call approach must preserve this dual-stream check.
%%%%%%% diff from: wnulxsyy 1301eb0b (parents of rebased revision)
\\\\\\\        to: kttyvoyy 5e152a0c (rebased revision)
 If exit code is 0, conflicts exist and stdout contains the file list for the autoreply. If exit code is 2, @ is clean. Skip the preliminary `jj log` check entirely.
>>>>>>> conflict 1 of 4 ends

**Saves:** 1 subprocess per absorption.

<<<<<<< conflict 2 of 4
%%%%%%% diff from: ttsnqluz dca7ece8 (parents of rebased revision)
\\\\\\\        to: ttsnqluz fe5cae93 (rebase destination)
-**Risk:** Low. Need to verify `jj resolve --list -r @` behavior when @ has no conflicts:
-- Expected: exits 0 with empty output (or a "No conflicts" message on stderr)
-- If it exits non-zero when clean, we'd need to distinguish "no conflicts" from "error"
-- Test this manually before implementing
-
-**Validation:** In a clean repo, run `jj resolve --list -r @` and confirm it exits 0 with empty/no-conflict output. Then introduce a conflict and confirm it lists files.
+**Risk:** Low. ✅ **TESTED — Behavior confirmed:**
+- **With conflicts:** Exit code 0, stdout contains `filename    conflict-type` entries
+- **No conflicts:** Exit code 2, stderr contains `Error: No conflicts found at this revision`
+
+**Implementation note:** Check exit code 0 (not stdout emptiness) to detect conflicts. This is inverted from typical Unix conventions — success means conflicts exist.
+
+**Validation:** ✅ Tested manually — `jj resolve --list -r @` exits 0 when conflicts exist, exits 2 when clean.
+++++++ kvqvklpw d0337a7a "[aiki]" (rebased revision)
**Risk:** Low-medium.
- Must preserve the existing dual-stream check from functions.rs:1128-1132. Checking only stdout would regress conflict detection for JJ versions that emit on stderr.
- Need to verify `jj resolve --list -r @` behavior when @ has no conflicts:
  - Expected: exits 0 with empty output on both streams (or a "No conflicts" message on stderr)
  - If it exits non-zero when clean, we'd need to distinguish "no conflicts" from "error"
  - Test this manually before implementing

**Validation:**
1. In a clean repo, run `jj resolve --list -r @` and confirm it exits 0 with empty/no-conflict output on **both** stdout and stderr.
2. Introduce a conflict and confirm it lists files — note **which stream** (stdout or stderr) receives the listing.
3. Test across JJ versions (at minimum: our oldest supported version and latest) to confirm which stream receives output in each, ensuring the dual-stream check covers all cases.
%%%%%%% diff from: wnulxsyy 1301eb0b (parents of rebased revision)
\\\\\\\        to: kttyvoyy 5e152a0c (rebased revision)
 **Risk:** Low. ✅ **TESTED — Behavior confirmed:**
 - **With conflicts:** Exit code 0, stdout contains `filename    conflict-type` entries
 - **No conflicts:** Exit code 2, stderr contains `Error: No conflicts found at this revision`
 
 **Implementation note:** Check exit code 0 (not stdout emptiness) to detect conflicts. This is inverted from typical Unix conventions — success means conflicts exist.
 
 **Validation:** ✅ Tested manually — `jj resolve --list -r @` exits 0 when conflicts exist, exits 2 when clean.
>>>>>>> conflict 2 of 4 ends

---

### 3. Defer orphan cleanup to session start and end

**File:** `cli/src/flows/core/functions.rs` — `workspace_absorb_all()` (lines 1176-1189)

**Current:** `cleanup_orphaned_workspaces()` runs after every `turn.completed` absorption. Scans `jj workspace list` for dead sessions on every turn.

**After:** Move the orphan cleanup call from `workspace_absorb_all()` to a new `workspace_cleanup_orphans()` native function, called from both `session.started` (fallback for crashed sessions) and `session.ended` in hooks.yaml.

```yaml
# hooks.yaml — session.started
session.started:
    # ... existing session setup ...
    - call: self.workspace_cleanup_orphans   # NEW — fallback: cleans up orphans from crashed sessions

# hooks.yaml — session.ended
session.ended:
    - let: absorb_result = self.workspace_absorb_all
    # ... existing conflict handling ...
    - call: self.workspace_cleanup_orphans   # NEW — cleans up this session's workspace
```

Orphans accumulate only from crashed sessions. Cleaning at session start and end (not per turn) is sufficient. The `session.started` trigger acts as a fallback: if a previous session crashed without firing `session.ended`, the next session to start will clean up the orphaned workspaces. This fully mitigates the crash-orphan scenario.

**Saves:** 1 subprocess per turn (except the first and last turns). Cleanup runs on session start and end, but not on every intermediate turn.

**Risk:** Very low. Orphans from crashed sessions are cleaned up when the next session starts, so they cannot accumulate indefinitely. In the worst case (no new sessions start), orphans are harmless — they're just entries in `jj workspace list` with no filesystem impact.

---

### 4. Remove `resolve_at_minus` wrapper

**File:** `cli/src/session/isolation.rs` (lines 878-880)

**Current:**
```rust
fn resolve_at_minus(repo_root: &Path) -> Result<String> {
    resolve_at_minus_in_path(repo_root)
}
```

One-line passthrough. Replace all calls to `resolve_at_minus()` with `resolve_at_minus_in_path()` directly, then delete the wrapper.

**Saves:** 0 subprocesses (code clarity only).

---

<<<<<<< conflict 3 of 4
%%%%%%% diff from: ttsnqluz dca7ece8 (parents of rebased revision)
\\\\\\\        to: ttsnqluz fe5cae93 (rebase destination)
 ### 5. Deduplicate workspace isolation context in hooks.yaml
 
 **File:** `cli/src/flows/core/hooks.yaml`
 
 **Current:** The identical WORKSPACE ISOLATION context block is copy-pasted in 5 event handlers: `session.resumed`, `session.compacted`, `session.cleared`, `turn.started`, `repo.changed`.
 
 **After:** Extract into a reusable anchor or callable block. Two options:
 
 **Option A — YAML anchor** (if the hook DSL supports it):
 ```yaml
 _workspace_context: &workspace_context
     - let: ws_path = self.workspace_ensure_isolated
     - if: ws_path
       then:
           - context: |
                 WORKSPACE ISOLATION: ...
 
 session.resumed:
     <<: *workspace_context
     # ... rest of handler
 
 turn.started:
     <<: *workspace_context
     # ... rest of handler
 ```
 
 **Option B — Named subroutine** (if the hook DSL supports `call:`):
 ```yaml
 # Define once
 _ensure_workspace:
     - let: ws_path = self.workspace_ensure_isolated
     - if: ws_path
       then:
           - context: |
                 WORKSPACE ISOLATION: ...
 
 # Use everywhere
 turn.started:
     - call: _ensure_workspace
     # ... rest
 ```
 
 **Saves:** 0 subprocesses (maintainability only — one place to update the context wording).
 
 **Risk:** Depends on whether the hook DSL supports YAML anchors or subroutines. If not, this becomes a feature request for the hook engine.
 
 ---
+++++++ kvqvklpw d0337a7a "[aiki]" (rebased revision)
### 5. Deduplicate workspace isolation context in hooks.yaml — DEFERRED

**File:** `cli/src/flows/core/hooks.yaml`

**Current:** The identical WORKSPACE ISOLATION context block is copy-pasted in 5 event handlers: `session.resumed`, `session.compacted`, `session.cleared`, `turn.started`, `repo.changed`.

**Findings:**

- **Option A — YAML anchors:** `serde_yaml 0.9` supports YAML anchors (`&name`) and aliases (`*name`) for individual scalar/sequence/mapping values, but **does not support `<<:` merge keys**. Merge key support was explicitly removed in serde_yaml 0.9. Without merge keys, anchors can't merge step arrays into handler arrays — you'd need `<<: *workspace_context` which won't parse.

- **Option B — Named subroutines via `call:`:** The hook DSL's `call:` action only supports native Rust functions registered as `self.function_name` or `aiki/module.function` (see `CallAction` in `cli/src/flows/types.rs`). It cannot invoke YAML-defined handler blocks. Adding support for YAML-level subroutines would be a new hook engine feature.

**Decision:** Deferred. Neither approach works without changes to the hook engine or serde_yaml configuration. This is purely a maintainability concern — it saves 0 subprocesses and has no runtime impact. The duplication is cosmetic (5 copies of a ~6-line block). If the context wording needs updating, a simple find-and-replace suffices. A proper fix would require either (a) adding YAML-level subroutine support to the hook engine, or (b) creating a native function that emits the workspace context block — both of which are disproportionate to the benefit.

**Saves:** 0 subprocesses (maintainability only).

---
%%%%%%% diff from: wnulxsyy 1301eb0b (parents of rebased revision)
\\\\\\\        to: kttyvoyy 5e152a0c (rebased revision)
-### 5. Deduplicate workspace isolation context in hooks.yaml
-
-**File:** `cli/src/flows/core/hooks.yaml`
-
-**Current:** The identical WORKSPACE ISOLATION context block is copy-pasted in 5 event handlers: `session.resumed`, `session.compacted`, `session.cleared`, `turn.started`, `repo.changed`.
-
-**After:** Extract into a reusable anchor or callable block. Two options:
-
-**Option A — YAML anchor** (if the hook DSL supports it):
-```yaml
-_workspace_context: &workspace_context
-    - let: ws_path = self.workspace_ensure_isolated
-    - if: ws_path
-      then:
-          - context: |
-                WORKSPACE ISOLATION: ...
-
-session.resumed:
-    <<: *workspace_context
-    # ... rest of handler
-
-turn.started:
-    <<: *workspace_context
-    # ... rest of handler
-```
-
-**Option B — Named subroutine** (if the hook DSL supports `call:`):
-```yaml
-# Define once
-_ensure_workspace:
-    - let: ws_path = self.workspace_ensure_isolated
-    - if: ws_path
-      then:
-          - context: |
-                WORKSPACE ISOLATION: ...
-
-# Use everywhere
-turn.started:
-    - call: _ensure_workspace
-    # ... rest
-```
-
-**Saves:** 0 subprocesses (maintainability only — one place to update the context wording).
-
-**Risk:** Depends on whether the hook DSL supports YAML anchors or subroutines. If not, this becomes a feature request for the hook engine.
-
----
>>>>>>> conflict 3 of 4 ends

## Impact Summary

| Change | Subprocess savings | Scope |
|--------|-------------------|-------|
| One-shot change ID | -1 per absorption | isolation.rs |
| Combined conflict detection | -1 per absorption | functions.rs |
| Deferred orphan cleanup | -1 per turn (except first and last) | functions.rs + hooks.yaml |
| Remove wrapper | 0 (clarity) | isolation.rs |
| Deduplicate context | 0 (maintainability) | hooks.yaml |
| **Total** | **~3 fewer jj calls per turn** | |

Current: ~9-10 subprocesses per absorption cycle → After: ~6-7.

---

## What NOT to Change

These were evaluated and rejected:

| Component | Why it stays |
|-----------|-------------|
| **Two-phase rebase** | Single rebase cascades rewrites to sibling workspaces. Essential for concurrent correctness. |
| **Absorb lock** | Without serialization, concurrent absorptions disconnect from each other. |
| **Lineage check on workspace reuse** | JJ rebases always "succeed" (recording conflicts as tree algebra), so we can't detect lineage divergence from rebase failure alone. The explicit ancestry check is needed. |
| **Target snapshot before rebase** | Captures user filesystem changes (e.g., files moved in Finder) that would otherwise be silently reverted during rebase. |
| **Crash recovery with bookmarks** | `jj op restore` can't replace this — it would undo operations from other sessions that happened after the crash. |
| **`jj status` for snapshots** | `jj debug snapshot` is an unstable API. `jj status` is stable and triggers snapshot as a side effect. The extra output formatting overhead is negligible vs. subprocess spawn cost. |

---

## Implementation Order

1. **Change 2** (combine conflict detection) — smallest diff, easiest to validate
2. **Change 1** (one-shot change ID) — requires template syntax validation
3. **Change 3** (defer orphan cleanup) — touches hooks.yaml, needs integration test
4. **Change 4** (remove wrapper) — trivial, do alongside any of the above
<<<<<<< conflict 4 of 4
%%%%%%% diff from: ttsnqluz dca7ece8 (parents of rebased revision)
\\\\\\\        to: ttsnqluz fe5cae93 (rebase destination)
 5. **Change 5** (deduplicate context) — depends on hook DSL capabilities
+++++++ kvqvklpw d0337a7a "[aiki]" (rebased revision)
5. **Change 5** (deduplicate context) — **deferred**: neither YAML anchors (no merge key support in serde_yaml 0.9) nor `call:` subroutines (only supports native Rust functions) work without hook engine changes
%%%%%%% diff from: wnulxsyy 1301eb0b (parents of rebased revision)
\\\\\\\        to: kttyvoyy 5e152a0c (rebased revision)
-5. **Change 5** (deduplicate context) — depends on hook DSL capabilities
>>>>>>> conflict 4 of 4 ends
