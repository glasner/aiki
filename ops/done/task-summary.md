# Task Summary Field

**Status**: 🔴 Design Phase
**Priority**: High
**Goal**: Add a dedicated `summary` field to tasks for structured final output, moving away from loose comment-based approach

---

## Problem

Currently, when closing a task, agents use `--comment` to document what was done:

```bash
aiki task close <id> --comment "Fixed auth bug by adding null check"
```

**Issues with this approach:**

1. **No semantic distinction**: Comments are a general-purpose log. The "closing comment" is just convention, not a distinct field.
2. **Hard to parse**: Comments are freeform text mixed with progress updates. Extracting "what was accomplished" requires parsing.
3. **Not queryable**: Can't easily filter/display task results without reading all comments.
4. **Ambiguous for templates**: Templates like `aiki/review` want structured output ("Review complete (3 issues found)"), but it's just a comment.
5. **Poor for delegation**: When `aiki task run` finishes, parent tasks can't easily extract the agent's final result.

---

## Solution: Add `summary` field

Add a dedicated `summary` field to tasks that captures the final output/result when closing.

### Design

**1. Data Model Changes**

```rust
// cli/src/tasks/types.rs

pub struct Task {
    pub id: String,
    pub name: String,
    // ... existing fields ...
    pub summary: Option<String>,  // NEW: final output when closed
    pub comments: Vec<Comment>,
}

pub enum TaskEvent {
    // ... existing variants ...
    Closed {
        task_ids: Vec<String>,
        outcome: TaskOutcome,
        summary: Option<String>,  // NEW: replaces closing comment pattern
        timestamp: DateTime<Utc>,
    },
}
```

**2. Close Event Semantics**

Each `Closed` event carries one summary that applies to all `task_ids` in that event. The close path emits separate events for explicitly-closed tasks vs cascade-closed subtasks:

1. Explicitly-closed tasks: `Closed { task_ids: [explicit IDs], summary: user's summary }`
2. Cascade-closed subtasks: `Closed { task_ids: [cascade IDs], summary: "Closed with parent" }`

This replaces the current separate `CommentAdded` event for cascade-closed subtasks. The summary field becomes the single source of truth for what happened at close time.

**3. CLI Interface**

```bash
# Close with summary (required when your session started the task)
aiki task close <id> --summary "Fixed auth bug by adding null check"

# Use comment command for progress updates (before closing)
aiki task comment <id> "Tried approach A, failed. Switching to B."
aiki task comment <id> "Switching to approach B"
aiki task close <id> --summary "Fixed via approach B: added null check"

# Summary from stdin (for long outputs)
aiki task close <id> --summary -

# Close without summary (only allowed if current session didn't start the task AND not --wont-do)
aiki task close <id> 
```

**Breaking changes**:
- `--comment` flag removed from `aiki task close`. Use `aiki task comment` before closing instead.
- **Summary required if current session started the task** (error if missing)
- **Summary always required with `--wont-do`** — declining a task requires a rationale regardless of session ownership
- Summary optional if closing a task started by another session (you may not know what was done), unless `--wont-do` is used

**4. Batch Close Validation**

When closing multiple tasks, each task is evaluated independently for summary requirement. If **any** task in the batch requires a summary (current session started it) and none was provided, the command errors before closing any of them:

```bash
$ aiki task close <id1> <id2>
Error: Summary required for in-progress tasks started by this session: <id1>

  aiki task close <id1> <id2> --summary "What you accomplished"
```

This is atomic — no tasks are closed if validation fails.

**5. Display Changes**

```bash
# aiki task show <id>
Task: Fix auth bug
Status: Closed (done)
Summary: Fixed auth bug by adding null check in token validation

Comments:
  [2025-02-09 10:23] Tried approach A, didn't work
  [2025-02-09 10:45] Switching to approach B

# aiki task list --closed
Closed (3):
- abc123 Fix auth bug
  ↳ Fixed auth bug by adding null check in token validation
- def456 Add tests
  ↳ Added 5 unit tests covering edge cases
- ghi789 Refactor module
  ↳ Split into 3 smaller modules for better separation
```

**6. Error Output**

When trying to close without summary when current session started the task:

```bash
$ aiki task close abc123
Error: Summary required when closing an in progress task.

Instead close with a summary of your work:
  aiki task close abc123 --summary "What you accomplished"
```

When closing another session's task (no summary required):

```bash
$ aiki task close def456
Closed def456

# (Works without error - you didn't start this task)
```

When using `--wont-do` without summary (always errors, regardless of session):

```bash
$ aiki task close def456 --wont-do
Error: Summary required when closing as won't-do. Explain why:
  aiki task close def456 --wont-do --summary "Already handled by existing code"
```

**7. Template Integration**

Templates can now specify expected summary format in instructions:

```markdown
# .aiki/templates/aiki/review.md

When all subtasks are complete, close this task with a summary:

```bash
aiki task close {{id}} --summary "Review complete (N issues found)"
```
```

**8. Delegation Integration**

When `aiki task run` completes, the summary is immediately available:

```rust
// Pseudocode
let result = run_task_with_output(task_id, options)?;
let task = materialize_task(task_id)?;
if let Some(summary) = task.summary {
    println!("Task completed: {}", summary);
}
```

---

## Migration Strategy

### Phase 1: Core implementation + templates + docs (single cutover)

All changes ship together to avoid a break window where existing templates reference `--comment` but the flag no longer exists.

- Add `summary: Option<String>` to `Task` and `TaskEvent::Closed`
- Add `--summary` flag to `aiki task close`
- **Remove `--comment` flag from `aiki task close` command**
- Add summary serialization/parsing in `storage.rs`
- Display summary in `aiki task show` and `aiki task list --closed`
- Update all `.aiki/templates/**/*.md` to use `--summary` instead of `--comment`
- Update `AGENTS.md` and `agents_template.rs` to use `--summary` pattern
- Migrate all code paths that read last comment as completion result

### Phase 2: Codebase-wide sweep

- Search all remaining references to `task close --comment` in:
  - All markdown files (`ops/`, docs, etc.)
  - Code comments and examples
  - Test files
- Replace with either:
  - `aiki task comment` (if it's progress updates)
  - `aiki task close --summary` (if it's final result)

---

## Implementation Tasks

### 1. Core Data Model
- [ ] Add `summary: Option<String>` to `Task` struct in `cli/src/tasks/types.rs`
- [ ] Add `summary: Option<String>` to `TaskEvent::Closed` in `cli/src/tasks/types.rs`
- [ ] Update `materialize_tasks()` to populate `summary` from `Closed` events
- [ ] Add tests for summary field persistence and retrieval

### 2. Storage Serialization
- [ ] Add `summary` to `TaskEvent::Closed` serialization in `cli/src/tasks/storage.rs` (~line 434)
- [ ] Add `summary` to `TaskEvent::Closed` parsing in `cli/src/tasks/storage.rs` (~line 650)

### 3. CLI Interface
- [ ] Add `--summary` flag to `close` subcommand in `cli/src/commands/task.rs`
- [ ] Support `--summary -` for stdin input
- [ ] **Remove `--comment` flag from `close` subcommand**
- [ ] Update `run_close()` to write summary to `Closed` event
- [ ] Update `run_close()` to remove comment-writing logic
- [ ] Replace cascade-close `CommentAdded` event with separate `Closed` event carrying `summary: "Closed with parent"`
- [ ] Add validation: require `--summary` if current session started the task
- [ ] Check task's `last_session_id` (NOT `claimed_by_session`) against current session to determine if summary is required. `claimed_by_session` is cleared on stop/close (manager.rs:109,120), so it would always be `None` at validation time. `last_session_id` persists across state transitions.
- [ ] Batch validation: error before closing any task if any required summary is missing

### 4. Fix Auto-Start Session Propagation
- [ ] Update parent auto-start in `run_close()` (~line 1932) to populate `session_id` from current session instead of `None`
- [ ] Current session is already available via `find_active_session(cwd)` (used at line 1977)

### 5. Migrate Result-Reading Code Paths
Code paths that currently treat the last comment as the completion result must read `task.summary` instead, **with fallback to last comment for historical tasks** (pre-summary tasks stored completion text as close comments):

- [ ] `cli/src/tasks/runner.rs:241` — `AgentSessionResult::Completed` reads last comment as summary
- [ ] `cli/src/tasks/runner.rs:286` — similar path for other exit condition
- [ ] `cli/src/commands/task.rs:3586` — `aiki task wait` table shows "Last Comment" column
- [ ] `cli/src/tasks/status_monitor.rs:273` — tree view shows last comment under subtasks (prefer summary for closed tasks)
- [ ] `cli/src/tasks/status_monitor.rs:316` — similar tree display path

**Backward compatibility:** All these paths should use a helper like `task.effective_summary()` that returns `task.summary.as_deref().or_else(|| task.comments.last().map(|c| c.text.as_str()))`. This ensures old tasks that were closed with `--comment` (which stored completion text as the last comment) still display their completion output. No data migration needed — the fallback handles it at read time.

### 6. Display & Output
- [ ] Update `format_task_details()` in `cli/src/tasks/md.rs` to show summary field
- [ ] Update `format_task_list()` to show summaries for closed tasks
- [ ] Update `aiki task show` to display summary prominently
- [ ] Add summary to `aiki task list --closed` output

### 7. Template Updates
- [ ] Update `.aiki/templates/aiki/review.md` to use `--summary`
- [ ] Update `.aiki/templates/aiki/fix.md` to use `--summary`
- [ ] Update `.aiki/templates/aiki/plan.md` to use `--summary`
- [ ] Update `.aiki/templates/aiki/build.md` to use `--summary`

### 8. Documentation & Runtime Instructions
- [ ] Update `cli/src/commands/agents_template.rs` (`AIKI_BLOCK_TEMPLATE`) to use `--summary`
- [ ] Bump `AIKI_BLOCK_VERSION` to `"1.13"` in `agents_template.rs`
- [ ] Update `AGENTS.md` (root) to use `--summary` instead of `--comment` for closing
- [ ] Add examples showing `aiki task comment` for progress, `--summary` for results
- [ ] Update task workflow section with new pattern
- [ ] **Update `cli/src/agents/runtime/mod.rs:209`** — `task_prompt()` hardcodes `--comment` in delegated-agent runtime instructions. Must change to `--summary` or `aiki task run` flows will fail post-cutover.
- [ ] Search and replace all `task close --comment` references in `ops/` directory

### 9. Testing
- [ ] Add unit tests for `--summary` flag parsing
- [ ] Add integration tests for close with summary
- [ ] Test closing without summary when session started task (should error)
- [ ] Test closing without summary when session didn't start task (should work)
- [ ] Test `--wont-do` without summary always errors (even for other session's task)
- [ ] Test batch close validation (mixed required/optional fails atomically)
- [ ] Test cascade close produces separate Closed event with "Closed with parent" summary
- [ ] Test auto-started parent has correct session_id
- [ ] Test `effective_summary()` falls back to last comment when summary is `None` (backward compat)
- [ ] Test `effective_summary()` returns summary over last comment when both exist
- [ ] Update existing tests that use `task close --comment`

---

## Open Questions

1. **Structured data in summary?**
   - Start with plain string
   - Future: could add `--summary-data key=value` for structured results
   - Example: `--summary-data issues_found=3 --summary-data files_reviewed=5`

2. **Length limits?**
   - No hard limit initially
   - Recommend 1-3 sentences in documentation
   - For long outputs, use stdin: `aiki task close <id> --summary -`

---

## Success Criteria

- ✅ `aiki task close <id> --summary "..."` works and persists summary
- ✅ `aiki task close <id>` without summary:
  - ✅ Errors if current session started the task (requires summary)
  - ✅ Works if another session started the task (summary optional)
  - ✅ Errors with `--wont-do` regardless of session ownership (rationale always required)
- ✅ Batch close: errors atomically if any task requires summary and none provided
- ✅ Cascade close: subtasks get separate `Closed` event with `summary: "Closed with parent"`
- ✅ Auto-started parents have correct `session_id` (not `None`)
- ✅ All result-reading code paths (`runner.rs`, `task.rs wait`, `status_monitor.rs`) use `task.effective_summary()` (summary with fallback to last comment for historical tasks)
- ✅ `cli/src/agents/runtime/mod.rs` `task_prompt()` updated to use `--summary`
- ✅ Storage: summary serialized/parsed correctly in `storage.rs`
- ✅ `aiki task show <id>` displays summary prominently
- ✅ `aiki task list --closed` shows summaries for closed tasks
- ✅ All templates updated to use `--summary`
- ✅ AGENTS.md and agents_template.rs updated with new pattern
- ✅ `--comment` flag removed from `aiki task close`
- ✅ All codebase references to `task close --comment` updated
- ✅ Tests pass for all new behavior

---

## Related

- `ops/next/task-perf.md` - Performance optimization (may benefit from summary as single field)
- `.aiki/templates/` - All templates need updates
- `AGENTS.md` - Core agent instructions need updates
