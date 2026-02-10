# Simpler Markdown Output for Task Commands

**Date**: 2026-02-08
**Status**: Draft
**Purpose**: Reduce token waste in task command output by eliminating echo-back of known data and removing the context footer from action commands.

**Related Documents**:
- [Markdown Output for Tasks](../next/markdown-output-for-tasks.md) - Original spec (Phase 1, now live)
- [Task System](../done/task-system.md) - Core task management

---

## Executive Summary

Task command output currently echoes back data the caller already knows and appends a context footer ("In Progress" / "Ready") to every command. This wastes tokens on every invocation. We simplify by:

1. **Action commands return only new information** — data the caller couldn't already know
2. **Context footer only on read commands** — `task` (bare/list) and `task show`
3. **Compressed formatting** — drop bold markers, timestamps, comment IDs, and redundant sections from `show`

---

## Problem

### Echo-back waste

```bash
aiki task add "Fix auth bug"
```
Current output:
```markdown
## Added
- **qotyswor...** — Fix auth bug (p2)

### In Progress
- **abcdefgh...** — Some other task
### Ready (3)
- **qotyswor...** [p2] Fix auth bug
- ...
```

The caller typed "Fix auth bug" — they know the name. The only new info is the **task ID**.

```bash
aiki task close abcdefgh... --comment "Done"
```
Current output:
```markdown
## Closed (done)
- **abcdefgh...** — Fix auth bug

### In Progress
(none)
### Ready (2)
- ...
```

The caller knows the ID (they typed it), the name (they already looked it up), and the outcome (they chose it). **There is zero new information**, plus a full context footer.

### Context footer on every command

Every command — `add`, `start`, `stop`, `close`, `comment` — appends the full "In Progress" / "Ready" context footer. This is useful on `task list` (where you're asking "what's the state?") but redundant on action commands where you just want confirmation.

---

## Design Principles

1. **Only return what the caller doesn't already know**
2. **Confirmation should be one line** — enough to verify the action succeeded
3. **Context footer on state-transition commands** — `stop` and `close` change what's active, so the agent needs to know what's next without a separate `aiki task` call
4. **No context footer on additive commands** — `add`, `start`, `comment` don't need it (you already know what you're working on)
5. **Keep `show` informative but compressed** — drop metadata that isn't actionable

---

## Proposed Output Formats

### Action Commands (write operations)

These commands mutate state. The caller already knows what they asked for — they just need confirmation + any new data.

#### `task add`

New information: the generated task ID.

```
Added: qotysworupowzkxyknzkworuwlyksmls — Fix auth bug
```

When adding a subtask (`--parent`), the subtask ID suffix is server-generated:
```
Added: qotysworupowzkxyknzkworuwlyksmls.3 — Update docs
```

#### `task start` (existing task)

New information: instructions (if present). ID and name are already known.

Without instructions:
```
Started: qotysworupowzkxyknzkworuwlyksmls
```

With instructions:
```
Started: qotysworupowzkxyknzkworuwlyksmls

### Instructions
{instructions content}
```

#### `task start` (quick-start: create + start)

New information: task ID + instructions.

```
Started: qotysworupowzkxyknzkworuwlyksmls — Fix auth bug

### Instructions
{instructions content, if any}
```

Auto-stopped tasks (if any were running):
```
Stopped: abcdefghijklmnopqrstuvwxyzabcdef
Started: qotysworupowzkxyknzkworuwlyksmls — Fix auth bug
```

#### `task stop`

No new information about the stopped task, but the agent needs to know what's next.

```
Stopped: qotysworupowzkxyknzkworuwlyksmls

In Progress:
(none)

Ready (2):
- anothertwentycharsofidpadding01 [p0] Urgent fix
- anothertwentycharsofidpadding02 [p2] Add tests
```

With reason:
```
Stopped: qotysworupowzkxyknzkworuwlyksmls (reason: Blocked on API access)

In Progress:
(none)

Ready (2):
- ...
```

#### `task close`

No new information about the closed task, but the agent needs to know what's next.

```
Closed: qotysworupowzkxyknzkworuwlyksmls

In Progress:
(none)

Ready (2):
- anothertwentycharsofidpadding01 [p0] Urgent fix
- anothertwentycharsofidpadding02 [p2] Add tests
```

When auto-starting next subtask (new information: which subtask was auto-started):
```
Closed: qotysworupowzkxyknzkworuwlyksmls.2
Started: qotysworupowzkxyknzkworuwlyksmls.3 — Update docs

In Progress:
- qotysworupowzkxyknzkworuwlyksmls.3 — Update docs

Ready (1):
- qotysworupowzkxyknzkworuwlyksmls.4 [p2] Integration tests
```

When all subtasks done and parent auto-started:
```
Closed: qotysworupowzkxyknzkworuwlyksmls.5

> All subtasks complete. Parent auto-started for review.
Started: qotysworupowzkxyknzkworuwlyksmls

In Progress:
- qotysworupowzkxyknzkworuwlyksmls — Fix auth bug

Ready (0):
```

#### `task comment`

No new information.

```
Commented: qotysworupowzkxyknzkworuwlyksmls
```

### Read Commands (query operations)

These commands ask "what's the state?" — they should return full context.

#### `task` (bare) / `task list`

This is the primary "what should I do?" command. Keep the context format.

```
In Progress:
- qotysworupowzkxyknzkworuwlyksmls — Fix auth bug

Ready (3):
- anothertwentycharsofidpadding01 [p0] Urgent fix
- anothertwentycharsofidpadding02 [p2] Add tests
- anothertwentycharsofidpadding03 [p2] Update docs
```

When nothing is happening:
```
In Progress:
(none)

Ready (0):
```

With filters (`--closed`, `--all`, etc.), show the filtered list:
```
Tasks (5):
- qotyswor... [p2] Fix auth bug (closed)
- abcdefgh... [p0] Urgent fix (in_progress)
...

In Progress:
- abcdefgh... — Urgent fix

Ready (2):
- ...
```

#### `task show`

This is the detailed view. Keep it informative but compressed.

Current → Proposed changes:
- Drop `**bold**` markers on field labels
- Drop RFC3339 timestamps on comments (not actionable)
- Drop comment IDs (rarely referenced)
- Drop "Files Changed" section (use `task diff` for that)
- Drop "Changes" section from default (use `task diff --show-changes`)
- Use checklist format for subtasks instead of table (shorter)
- Use relative subtask IDs (`.1`, `.2`) since parent ID is already shown

```
Task: Fix auth bug
ID: qotysworupowzkxyknzkworuwlyksmls
Status: in_progress
Priority: p1

Subtasks (2/5):
- [x] .1 Update auth validation
- [>] .2 Add unit tests
- [ ] .3 Update docs
- [ ] .4 Integration tests
- [ ] .5 Deploy

Comments:
- Found the issue in auth handler
- Implementing fix now

In Progress:
- qotysworupowzkxyknzkworuwlyksmls — Fix auth bug

Ready (2):
- nextaskidpaddingblahblahblah01 [p2] Another task
- nextaskidpaddingblahblahblah02 [p3] Low pri task
```

Sources are shown only when present:
```
Task: Fix auth bug
ID: qotysworupowzkxyknzkworuwlyksmls
Status: in_progress
Priority: p1
Sources: file:ops/now/auth-fix.md
```

---

## What's Removed vs Kept

| Element | Current | Proposed | Rationale |
|---------|---------|----------|-----------|
| Context footer on add/start/comment | Always present | Removed | Caller knows what they're working on |
| Context footer on stop/close | Always present | Kept (simplified) | Agent needs to know what's next |
| Task name echo on close/stop/comment | Always present | Removed | Caller already knows |
| `**bold**` markers | All field labels | Removed | Token overhead, no info value |
| `## Section` headers on actions | `## Added`, `## Started`, etc. | Single line prefix | Less noise |
| RFC3339 timestamps on comments | Always present | Removed | Not actionable for agents |
| Comment IDs | Always present | Removed | Rarely referenced |
| Files Changed on show | Present for closed tasks | Removed | Use `task diff` |
| Changes list on show | Present always | Removed | Use `task diff` |
| Subtask table format | `| ID | Status | Name |` | Checklist `- [x] .1 Name` | Shorter, scannable |
| Full subtask IDs | `parent.1`, `parent.2` | `.1`, `.2` | Parent ID already shown |
| Priority on show | Always shown | Kept | Useful context |
| Type suffix on show | Always shown | Kept | Useful context |
| Instructions on start | Present when available | Kept | Essential for work |
| Notices (auto-start) | Verbose sentences | Compact lines | Still informative |

---

## Implementation Plan

### Phase 1: Slim action command output

**Goal**: Action commands return single-line confirmations, no context footer.

**Changes to `md.rs`**:
- Add new format functions: `format_action_added`, `format_action_started`, `format_action_stopped`, `format_action_closed`, `format_action_commented`
- Each returns a single line (or line + instructions for start)
- No `MdBuilder::build()` call (which appends context)

**Changes to `task.rs`**:
- `run_add`: use `format_action_added`, print directly, skip `MdBuilder`
- `run_start`: use `format_action_started`, include instructions if present, skip `MdBuilder` for context
- `run_stop`: use `format_action_stopped`, keep context footer (state transition)
- `run_close`: use `format_action_closed`, include auto-start notices, keep context footer (state transition)
- `run_comment`: use `format_action_commented`, print directly

### Phase 2: Simplify `task show`

**Goal**: Compressed show output, drop non-actionable metadata.

**Changes to `task.rs` (`run_show`)**:
- Drop `**bold**` from field labels
- Replace subtask table with checklist format using relative IDs
- Drop RFC3339 timestamps and IDs from comments
- Drop "Files Changed" section
- Drop "Changes" section
- Keep context footer (this is a read command)

### Phase 3: Clean up `task list`

**Goal**: Simplify list format to match new style.

**Changes to `md.rs`**:
- `format_task_list`: drop `**bold**` markers
- `build_context`: drop `**bold**` markers, simplify format

**Changes to `task.rs` (`run_list`)**:
- Keep context footer (this is a read command)

### Phase 4: Update CLAUDE.md examples

**Goal**: Update the task output format examples in CLAUDE.md to match the new output.

---

## Migration Notes

- No breaking changes to the event system or data model
- Output format changes are purely cosmetic/structural
- `MdBuilder` can be simplified or methods deprecated as action commands stop using `build()`
- Tests in `md.rs` need updating for new format functions
- The `--diff` and `--with-source` flags on `show` continue to work (they add optional sections)

---

## Examples

### Before (current)

```bash
$ aiki task start "Fix login bug" --source prompt
```
```markdown
## Added
- **qotyswor...** — Fix login bug (p2)

## Started
- **qotyswor...** [p2] Fix login bug
### In Progress
- **qotyswor...** — Fix login bug
### Ready (0)
```

### After (proposed)

```bash
$ aiki task start "Fix login bug" --source prompt
```
```
Started: qotysworupowzkxyknzkworuwlyksmls — Fix login bug
```

**Token savings**: ~180 tokens → ~60 tokens (67% reduction on this command).

---

## Open Questions

1. **Error responses** — Keep `**Error** (cmd): message` format, or simplify to `Error: message`?
   - Recommendation: Simplify to `Error: message` (the cmd name is visible from context)

2. **Should `task start` with auto-stop show the stopped task?** — Currently shows `## Stopped` section.
   - Recommendation: Yes, as a single line `Stopped: <id>` before `Started: <id>` — the auto-stop is new info

3. **Should `task show` keep the context footer?** — It's a read command, but the context is already available via bare `task`.
   - Recommendation: Keep it — `show` is often the only command run, and knowing what's next is valuable
