# Cleanup Review Templates

## Summary

The review templates in `.aiki/templates/aiki/review/` have accumulated issues from copy-paste, and the template structure conflates two distinct concerns: **exploring** (understanding what you're reviewing) and **reviewing** (judging it against criteria). This spec describes a restructuring that separates explore from review, plus mechanical cleanup of stale artifacts.

A new `aiki explore` CLI command is introduced as a standalone entry point for the exploration phase. The review template instructs the agent to run `aiki explore --start`, which assigns the explore task to the current agent so the context gained is retained for the review phase.

## Problems

### 1. Explore and review are conflated

The current `review/` subtemplates mix exploration (read the spec, browse code, look at diffs) with review criteria. For example, `code.md` asks you to "explore the codebase" _and_ has review criteria inline, while the parent `review.md` also has an inline "Review" section for leaving comments. There's no clean boundary.

### 2. `code.md` — wrong wording and redundant nesting

**File:** `.aiki/templates/aiki/review/code.md`

- Header says "Understand the implementation of spec" — leftover from rename
- Body says "understand the **current implementation** of plan in ..." — "plan" is wrong
- Contains `{% subtask aiki/review/spec %}` — creates a redundant chain: review → code → spec

### 3. Stale `# View all code changes` comments

Copy-paste artifacts appear as trailing comments in bash code blocks:

- `review.md:11` — `aiki task close ... # View all code changes`
- `session.md:9` — `aiki task close ... # View all code changes`
- `task.md:8` — `aiki task close ... # View all code changes`

### 4. Only two review modes, but four scope kinds

There are four scope kinds (`spec`, `code`, `task`, `session`) but only two meaningful types of review: reviewing a **spec** or reviewing **code** (implementation). Task and session scopes produce code changes — they should use code review criteria, not have their own review templates.

## Design

### Introduce `aiki explore`

`aiki explore` is a new CLI command that follows the same pattern as `aiki review` and `aiki build`: it resolves a scope from its argument, picks the matching template from `explore/`, and runs it as a task.

```
aiki explore spec.md          # scope: spec  → explore/spec template
aiki explore spec.md --code   # scope: code  → explore/code template
aiki explore <task-id>        # scope: task  → explore/task template  (32 lowercase letters)
aiki explore <session-id>     # scope: session → explore/session template  (UUID format)
```

Scope detection autodetects by argument format: `.md` file → spec/code, 32-lowercase-letter string → task, UUID format → session. The `--code` flag works identically to `aiki review --code`. No no-argument default — a target is always required.

Run modes: blocking (default), `--async`, `--start`. The `--start` flag is important: it assigns the explore task to the current agent rather than spawning a subagent, preserving context for the subsequent review phase. No `--fix` equivalent.

### Separate explore from review

Split the current `review/` subtemplates into two template directories:

- **`explore/`** — scope-specific templates for understanding what you're looking at
- **`review/`** — two review templates with evaluation criteria

### Explore templates (scope-specific)

Each scope kind needs its own explore approach:

| Template | Purpose |
|----------|---------|
| `explore/spec.md` | Read the spec file, check for linked tasks |
| `explore/code.md` | Search codebase for implementation of a spec |
| `explore/task.md` | `aiki task show` + `aiki task diff` for a single task |
| `explore/session.md` | Iterate task IDs, show + diff each |

These templates are run by both `aiki explore` (standalone) and `aiki review` (via `--start`, inline in the current agent).

### Review templates (two types)

| Template | Purpose | Used by scope kinds |
|----------|---------|-------------------|
| `review/spec.md` | Evaluate a spec: completeness, clarity, implementability, UX | `spec` |
| `review/code.md` | Evaluate code: spec coverage, quality, security, spec alignment | `code`, `task`, `session` |

### Parent template dispatch

The parent `review.md` instructs the agent to run explore inline, then dispatches to the appropriate review template:

```markdown
## Explore Scope

Run the following command to explore the scope. The `--start` flag assigns
the explore task to you so the context is available for the review phase:

​```bash
aiki explore {{data.scope.id}} --start
​```

{% subtask aiki/review/spec if data.scope.kind == "spec" %}
{% subtask aiki/review/code if data.scope.kind != "spec" %}
```

The explore step is plain markdown instructions rather than a `{% subtask %}` or `{% run %}` directive. The agent runs `aiki explore --start`, which creates and starts the explore task assigned to the current agent. Once the explore task is closed, the agent proceeds to the review subtask with full context intact.

### Scope kind → template mapping

| Command | scope.kind | Explore template | Review template |
|---------|-----------|-----------------|----------------|
| `aiki review spec.md` | spec | explore/spec | review/spec |
| `aiki review spec.md --code` | code | explore/code | review/code |
| `aiki review <task-id>` | task | explore/task | review/code |
| `aiki review` (session) | session | explore/session | review/code |

For session scope, `review.md` passes `{{data.scope.id}}` (the session UUID) as a positional argument. Since session IDs are UUID format and task IDs are 32 lowercase letters, `aiki explore` autodetects the scope from the argument — no special flag needed.

## Requirements

### R1: Add `aiki explore` CLI command
- Follows the same pattern as `aiki review` and `aiki build`
- Accepts a required target argument: file path, task ID (32 lowercase letters), or session ID (UUID)
- Autodetects scope from argument format — no `--session` flag needed
- No no-argument default (unlike `aiki review`)
- Accepts `--code` flag for file targets (same semantics as `aiki review --code`)
- Resolves scope kind and runs the matching `explore/<kind>` template as a task
- Supports `--async` and `--start` run modes; `--start` assigns the task to the current agent
- Lives in `cli/src/commands/explore.rs`
- Scope detection logic shared with or mirrored from `review.rs`

### R2: Create explore templates
- Move exploration logic from current `review/` subtemplates into `explore/` templates
- `explore/spec.md` — read the spec, check linked tasks (from current `review/spec.md`)
- `explore/code.md` — search codebase for spec implementation (from current `review/code.md`, minus the review criteria)
- `explore/task.md` — show + diff a task (from current `review/task.md`)
- `explore/session.md` — iterate session tasks, show + diff each (from current `review/session.md`)

### R3: Restructure review templates
- `review/spec.md` — spec evaluation criteria (completeness, clarity, implementability, UX)
- `review/code.md` — code evaluation criteria (spec coverage, quality, security, alignment)
- Remove `review/task.md` and `review/session.md` — those scope kinds now use `review/code.md`

### R4: Update parent `review.md`
- First step: plain markdown instructions to run `aiki explore {{data.scope.id}} --start` (for session scope, `data.scope.id` is the session UUID, autodetected by format)
- Second step: conditional dispatch to `review/spec` or `review/code` based on `data.scope.kind`
- Keep the existing inline "Review" section (leave comments, close subtask)
- **Note:** `review.rs` currently stores `"session"` (literal string) in `scope.id` for session reviews. This must be changed to store the actual session UUID so `aiki explore {{data.scope.id}}` passes a detectable ID.

### R5: Remove stale comments
- Strip `# View all code changes` from bash blocks in all templates

### R6: Fix wording
- All templates should have accurate headers and descriptions
- No references to "plan" when meaning "spec"

## Non-Goals

- Adding new scope kinds
- Changing how `aiki review` CLI dispatches (scope.kind resolution stays the same)
- Modifying the fix loop (`{% subtask aiki/fix/loop %}`)
- Adding `list` / `show` subcommands to `aiki explore` (not needed initially)
- Defaulting `aiki explore` to any scope when no argument is given
- A `--session` flag (autodetection by UUID format is sufficient)

## Acceptance Criteria

- [ ] `aiki explore` command exists and works standalone
- [ ] `aiki explore spec.md` runs the `explore/spec` template
- [ ] `aiki explore spec.md --code` runs the `explore/code` template
- [ ] `aiki explore <task-id>` runs the `explore/task` template
- [ ] `aiki explore <session-uuid>` runs the `explore/session` template
- [ ] `aiki explore` with no argument errors
- [ ] `aiki explore --start` assigns the explore task to the current agent
- [ ] Explore templates exist: `explore/spec.md`, `explore/code.md`, `explore/task.md`, `explore/session.md`
- [ ] Review templates are exactly two: `review/spec.md`, `review/code.md`
- [ ] `review/task.md` and `review/session.md` are removed
- [ ] Parent `review.md` instructs agent to run `aiki explore ... --start` (no `{% subtask %}` or `{% run %}` for explore)
- [ ] No `# View all code changes` stale comments in any template
- [ ] No broken template variables
- [ ] `aiki review spec.md` works end-to-end (explore → spec review)
- [ ] `aiki review spec.md --code` works end-to-end (explore → code review)
- [ ] `aiki review <task-id>` works end-to-end (explore → code review)
