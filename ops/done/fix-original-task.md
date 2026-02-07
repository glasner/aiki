# Fix Targets Original Task

**Date**: 2026-02-05
**Status**: Spec
**Priority**: P2

---

## Problem

The current `aiki fix` workflow creates a brand new standalone parent task from review findings. This breaks the mental model when fixing tasks — fixes should live under the original task, not as disconnected standalone tasks.

---

## Proposed Model

### When Fixing a Task

```
Proposed: build X → close → review X → close review Y → fix Y → ONE fix subtask added to X
```

- **Review** stays its own task (tracks who reviewed, what was found)
- **Fix** reads the review task ID and determines what was reviewed
- If the review target is a task, **one fix subtask** is added to the original task
- The fix subtask contains nested subtasks for each issue found in the review
- The original task **reopens** when the fix subtask is added (it wasn't really done)
- Original task doesn't auto-close until all subtasks complete

```
Task X: "Implement feature Y"
  X.1: "Fix issues from review Y"          ← ONE fix subtask added by aiki fix Y
    X.1.1: "Fix: error handling missing"   ← agent creates these based on review
    X.1.2: "Fix: rename variable"          ← agent creates these based on review
```

**Multiple review rounds:**
```
Task X: "Implement feature Y"
  X.1: "Fix issues from review Y"          ← first review
    X.1.1: "Fix: error handling missing"
    X.1.2: "Fix: rename variable"
  X.2: "Fix issues from review Z"          ← second review
    X.2.1: "Fix: add missing tests"
```

### Scope

This plan covers **task-targeted reviews only** (`aiki review <task-id>`). File-targeted reviews (`aiki review src/auth.rs`) are planned separately — see `ops/next/review-and-fix-files.md`.

The code should use a `ReviewTarget` enum so file support can be added later without restructuring.

---

## Changes Required

### 1. `cli/src/commands/fix.rs` — Determine review target and adapt behavior

**Current behavior:** Takes a review task ID, always creates a new standalone fix task.

**New behavior:** Takes a review task ID, determines what was reviewed, and adapts:
- If reviewing a task → add fix subtask to the original task
- Other target types → error for now (file support planned in `ops/next/review-and-fix-files.md`)

#### a. Input changes

The command signature stays the same: `aiki fix <review-id>`

The fix command receives the review task ID. It does **not** accept a task-id directly — you must always pass the review task ID. The fix command then inspects the review's sources to determine what was reviewed and adapts behavior accordingly.

#### b. Determine what was reviewed

Check the review task's `source` field to determine the target.

**Source resolution priority:** A review task may have multiple sources (e.g., `task:`, `file:`, `prompt:`). Resolve in this order:
1. `task:` — the review targeted a task (highest priority)
2. `file:` — the review targeted a file (future — see `ops/next/review-and-fix-files.md`)
3. All other source types (`prompt:`, `comment:`, etc.) are ignored for target detection

```rust
enum ReviewTarget {
    Task(String),
    File(String),  // future: ops/next/review-and-fix-files.md
    Unknown,
}

fn get_review_target(review_task: &Task) -> ReviewTarget {
    // Priority: task > file
    for source in &review_task.sources {
        if source.starts_with("task:") {
            return ReviewTarget::Task(source.strip_prefix("task:").unwrap());
        }
    }
    for source in &review_task.sources {
        if source.starts_with("file:") {
            return ReviewTarget::File(source.strip_prefix("file:").unwrap());
        }
    }
    ReviewTarget::Unknown
}
```

**Note:** The enum includes `File` for forward-compatibility, but the current implementation only handles `Task`. Both `File` and `Unknown` produce an error for now.

#### c. Branch on target type

**If target is a task:**
1. Find the original task by ID (from `source: task:<id>`)
2. Gather all comments from the review task
3. If original task is closed, it will be implicitly reopened when the fix subtask is added (see section 3)
4. Create ONE fix subtask on the original task:
   - Use `get_next_subtask_number(original_id, ...)` to find the next available slot (e.g., X.1)
   - Emit `TaskEvent::Created` with ID `original_id.N` and name like "Fix issues from review <review-id>"
   - Source the fix subtask to the review: `task:<review-id>`
   - The agent will then create nested subtasks (X.1.1, X.1.2, etc.) for each issue

**If target is a file or Unknown:**
1. Error with message: "Fixing file-targeted reviews is not yet supported. Only task-targeted reviews can be fixed."
2. File support is planned in `ops/next/review-and-fix-files.md`.

#### d. Agent-driven subtask creation

The fix command creates one fix subtask (e.g., "Fix issues from review Y") on the original task. The agent is started with this subtask and reads review comments to create nested subtasks for each issue.

The fix subtask's template instructions reference the review task so the agent knows what to fix. The agent can intelligently group, split, or organize the individual fix items as nested subtasks (X.1.1, X.1.2, etc.).

### 2. Fix template update (`.aiki/templates/aiki/fix.md`)

The template is used for the fix subtask that gets created under the original task. It instructs the agent to create nested subtasks for each issue.

Template variables:
- `{{id}}` = the fix subtask ID (e.g., X.1)
- `{{parent.id}}` = the original task ID (e.g., X)  
- `{{source.id}}` = the review task ID (where to read comments)

New template:

```markdown
---
version: 1.0.0
---

# Followup: Review {{source.id}}

Review task `{{source.id}}` found issues in the original task `{{parent.id}}`.

## Instructions

1. Read the review comments to understand what issues were found:
   ```bash
   aiki task show {{source.id}} --with-source
   ```
🛑 Do NOT edit code before reading above.

2. Create a nested subtask for EACH issue found (use your current task ID as parent):
   ```bash
   aiki task add --parent {{id}} "Fix: <brief description of issue>"
   ```

3. Start and work through each nested subtask, closing as you go:
   ```bash
   aiki task start {{id}}.1
   # ... do the work to fix the issue ...
   aiki task close {{id}}.1 --comment "Fixed by doing X"
   ```
   - **You MUST start each subtask before working on it**
   - Close with `--comment` when fixed
   - Close with `--wont-do --comment` if out of scope or adds too much complexity
   - Continue until all nested subtasks are completed

4. Return to this fix subtask and close it:
   ```bash
   aiki task close {{id}} --comment <summary of fix>
   ```

Important: Do NOT return without closing all nested subtasks and this fix subtask.
```

### 3. Allow adding subtasks to closed tasks (implicit reopening)

**File:** `cli/src/commands/task.rs` line 989-991

Currently, `aiki task add --parent X` fails if X is closed. Remove this guard and allow subtask addition to implicitly reopen the parent task.

**Implementation:**
- When adding a subtask to a closed parent, automatically emit `TaskEvent::Reopened` with reason "Subtasks added"
- Update parent status back to `Open` and clear `closed_outcome`
- Log the reopening in the event stream for auditability

**Benefits:**
- Simpler fix command implementation (no explicit reopen step needed)
- Natural semantics: "adding work to a task reopens it"
- Still creates a reopened event for audit trail

### 4. Pipe flow (unchanged)

The pipe flow remains the same:

```bash
aiki review X | aiki fix   # review outputs review task ID, fix reads it
```

**Behavior:**
- `aiki review` outputs the review task ID
- `aiki fix` receives the review task ID
- `aiki fix` determines what was reviewed and adapts behavior accordingly

### 5. `aiki task show` — display fix lifecycle (future scope)

The existing subtask display should already show fix subtasks under the original task. No changes needed for this scope. A richer "Reviewed by: Y (3 issues)" display line can be added later.

---

## Implementation Order

1. **Allow adding subtasks to closed tasks** — Implicit reopen via `task add --parent` (section 3)
2. **Add `get_review_target`** — Target detection with source priority in fix.rs (section 1b)
3. **Branch fix behavior on target type** — Subtask-on-original vs standalone (section 1c)
4. **Update fix template** — New variable context (original task as parent, review as source) (section 2)
5. **Tests** — Unit tests for target detection, subtask creation, and implicit reopen

---

## Edge Cases

- **Review target is a file or unknown:** Error directing user to task-targeted reviews only (file support deferred).
- **Original task already has subtasks:** Fix subtasks are appended (X.3, X.4, etc. if X.1 and X.2 already exist). The existing subtask numbering handles this via `get_next_subtask_number`.
- **Original task is already open:** Implicit reopen is a no-op; subtask is just added.
- **Review has no issues (approved):** Output "approved" message, do NOT create subtasks or reopen anything. This check happens before any subtask creation or reopen logic.
- **Nested fix rounds:** Second review adds more subtasks (X.3, X.4...). Each round appends, never conflicts.
- **Input is not a review task:** Error with message: "Task X is not a review task." Detect by checking task type or absence of review-related metadata.
