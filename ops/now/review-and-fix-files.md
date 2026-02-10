# Review and Fix Non-Task Targets

**Date**: 2026-02-06
**Status**: Idea
**Priority**: P3
**Depends on**: `ops/done/fix-original-task.md`, `ops/done/review-scope-refactor.md`, `ops/now/task-summary.md`

**Related Documents**:
- [Fix Targets Original Task](../done/fix-original-task.md) - Task-targeted fix (prerequisite)
- [Composable Task Templates](../done/composable-task-templates.md) - Template composition with `{% subtask %}` (prerequisite)
- [Template Conditionals](../done/template-conditionals.md) - Conditional logic for templates (implemented)
- [Task Templates](../done/task-templates.md) - Template system (implemented)
- [ReviewScope Refactor](../done/review-scope-refactor.md) - Typed `ReviewScope` struct for review/fix routing (implemented)
- [Task Summary](task-summary.md) - Dedicated `summary` field on tasks, replacing `--comment` on close (prerequisite)

---

## Problem

`aiki review` currently only targets tasks. We want it to also handle files and specs, with `aiki fix` adapting accordingly. The `ReviewScope` struct and fix routing via `ReviewScope::from_data()` are implemented (see [ReviewScope Refactor](../done/review-scope-refactor.md)), but only `Task` and `Session` kinds are wired up — `Spec` and `Implementation` need to be added.

This plan extends both `review` and `fix` to support non-task targets, building on the existing `ReviewScope` infrastructure.

---

## Prerequisites Status

- ✅ Template conditionals (implemented in `cli/src/tasks/templates/conditionals.rs`)
- ✅ Composable task templates (implemented in `cli/src/tasks/templates/`)
- ✅ Fix targets original task (implemented in `cli/src/commands/fix.rs`)
- ✅ [ReviewScope Refactor](../done/review-scope-refactor.md) — typed `ReviewScope` struct, scope as task data, fix routing via data instead of source prefixes
- ⬜ [Task Summary](task-summary.md) — dedicated `summary` field on tasks, `--summary` flag on `task close`

The `ReviewScope` struct and `ReviewScopeKind` enum are implemented in `cli/src/commands/review.rs`. Fix routing in `cli/src/commands/fix.rs` uses `ReviewScope::from_data()` to read scope from task data. Currently only `ReviewScopeKind::Task` and `ReviewScopeKind::Session` are fully wired — `Spec` and `Implementation` are the new targets added by this plan.

---

## Part 1: Spec-Aware Review

### Command Syntax

```bash
aiki review [<target>] [options]
```

**Options:**
- `--implementation` - When reviewing a spec file, review the current codebase implementation instead of the spec itself

**Target detection:**
```bash
# Review code changes for a task (already works)
aiki review xqrmnpst

# Review a spec/document file (new)
aiki review ops/now/feature.md

# Review the current codebase implementation described in a spec (new)
aiki review ops/now/feature.md --implementation

# Review all closed tasks in session (already works)
aiki review
```

### Target Detection

Extends the existing `detect_target()` to return `ReviewScope` for file targets. The `--implementation` flag determines whether a file target produces `ReviewScopeKind::Spec` or `ReviewScopeKind::Implementation`:

```rust
fn detect_target(arg: Option<&str>, implementation: bool) -> Result<ReviewScope> {
    match arg {
        None => Ok(ReviewScope {
            kind: ReviewScopeKind::Session,
            id: "session".into(),
            task_ids: collect_session_task_ids()?,
        }),

        Some(s) if s.ends_with(".md") && Path::new(s).exists() => {
            let kind = if implementation {
                ReviewScopeKind::Implementation
            } else {
                ReviewScopeKind::Spec
            };
            Ok(ReviewScope {
                kind,
                id: s.into(),
                task_ids: vec![],
            })
        }

        Some(s) if s.ends_with(".md") => {
            bail!("File not found: {}", s)
        }

        Some(s) if is_task_id(s) => {
            if implementation {
                bail!("--implementation flag only applies to file targets")
            }
            Ok(ReviewScope {
                kind: ReviewScopeKind::Task,
                id: s.into(),
                task_ids: vec![],
            })
        }

        Some(s) if Path::new(s).exists() => {
            bail!("File review only supports .md files currently")
        }

        Some(s) => {
            bail!("Target not found: {}", s)
        }
    }
}
```

### Scope Data

**Implemented via [ReviewScope Refactor](../done/review-scope-refactor.md).** The `ReviewScope` struct serializes scope metadata to task `data` fields via `scope.to_data()`. These are persisted on the review task and available to both templates (as `{{data.scope.type}}`, etc.) and downstream consumers like `aiki fix` (via `ReviewScope::from_data(&review_task.data)`).

The review command builds a `ReviewScope` from target detection, then calls `scope.to_data()` to produce:

**For task review:**
| Key | Value |
|-----|-------|
| `scope.type` | `task` |
| `scope.id` | `xqrmnpst` |
| `scope.name` | `Task (xqrmnpst)` |

**For spec document review:**
| Key | Value |
|-----|-------|
| `scope.type` | `spec` |
| `scope.id` | `ops/now/feature.md` |
| `scope.name` | `Spec (feature.md)` |

**For implementation review (with --implementation flag):**
| Key | Value |
|-----|-------|
| `scope.type` | `implementation` |
| `scope.id` | `ops/now/feature.md` |
| `scope.name` | `Implementation (feature.md)` |

**For session review:**
| Key | Value |
|-----|-------|
| `scope.type` | `session` |
| `scope.id` | `session` |
| `scope.name` | `Session` |
| `scope.task_ids` | `abc123,def456` |

The `scope.name` field is computed by `ReviewScope::name()` from kind and id, keeping templates simple.

**Why task data instead of sources:** Scope fields are structured metadata about the review target. Storing them as task data means they persist on the task, are readable by `aiki fix` via `ReviewScope::from_data()`, and automatically flow into templates. Sources (`task:`, `file:`, `prompt:`) remain purely for lineage/provenance.

### Unified Review Template with Composable Subtasks

The `aiki/review` template uses **composable task templates** (via `{% subtask %}`) to delegate to specialized review templates. The `data.scope.type` field directly maps to the template name:

```markdown
---
version: 2.0.0
type: review
---

# Review: {{data.scope.name}}

Review the target for quality and readiness.

When all subtasks are complete, close this task with a summary:

```bash
aiki task close {{id}} --summary "Review complete (n issues found)"
```

# Subtasks

{% subtask aiki/review/{{data.scope.type}} %}

## Report findings

Review the findings from the specialized review subtask above. For each issue found, add a comment to this parent task ({{parent.id}}) using:

```bash
aiki task comment {{parent.id}} "<description of issue>"
```

Once all findings are recorded as comments on {{parent.id}}, close this review task with a summary:

```bash
aiki task close {{id}} --summary "Review complete (n issues found)"
```
```

**How it works:**
- `data.scope.type` is set based on what you're reviewing (task, spec, implementation, session)
- Template interpolation resolves `aiki/review/{{data.scope.type}}` to the appropriate specialized template
- Each specialized template brings its own subtasks with domain-specific evaluation criteria
- Templates inherit data variables from parent context (`data.scope.*`, `source.*`, etc.)
- Agent sees a nested task tree with appropriate review steps for the target type
- Final "Report findings" subtask translates findings into comments on the parent review task for `aiki fix` to consume

**Specialized review templates (mapped from data.scope.type):**

1. **`aiki/review/task`** - Review code changes from a task
2. **`aiki/review/spec`** - Review spec documents for completeness, clarity, implementability
3. **`aiki/review/implementation`** - Review current codebase implementation against a spec
4. **`aiki/review/session`** - Review all closed tasks in a session

Each specialized template is a complete template with its own parent name and subtasks, which get composed into the parent review task tree.

### Contract Between Wrapper and Specialized Templates

**Wrapper template responsibilities:**
- Set up review context (`data.scope.*` fields on the review task)
- Delegate to specialized template via `{% subtask aiki/review/{{data.scope.type}} %}`
- Collect findings from specialized review subtask
- Record findings as comments on the parent review task
- Close review with `--summary "Review complete (n issues found)"`

**Specialized template responsibilities:**
- Perform domain-specific review (code, spec, implementation, session)
- Identify issues during review subtasks
- Specialized templates should NOT directly comment on the parent - they work within their subtask scope
- Findings are collected by the wrapper's "Report findings" subtask

**Contract for `aiki fix`:**
- Review task is closed with `--summary` describing the overall result (e.g., "Review complete (3 issues found)")
- Each issue is a comment on the review task (added during review, before closing)
- `aiki fix` reads comments from closed review task and creates fix tasks
- Comment text becomes fix task description
- The review task's `summary` field provides a quick overview; individual comments provide the actionable detail

This separation allows specialized templates to focus on domain-specific review logic without knowing about the fix workflow.

### How Composable Templates Work for Reviews

**Subtask Resolution:**

When the template engine encounters `{% subtask aiki/review/{{data.scope.type}} %}`:

1. Interpolate `{{data.scope.type}}` to get the template name (e.g., `data.scope.type: spec` → `aiki/review/spec`)
2. Load the referenced template
3. Child template inherits parent's full variable context (`data.scope.*`, `source.*`, etc.)
4. Resolve the child template's parent name (becomes the composed subtask name)
5. Create the composed subtask in the task tree
6. Recursively create the child template's subtasks as sub-subtasks

**Task Tree Example:**

For `aiki review ops/now/feature.md` (spec document), where `aiki/review/spec` has subtasks "Read document" and "Evaluate completeness":

```
Review: ops/now/feature.md           (parent)
├── Understand what you're reviewing (static subtask .0)
├── Review Spec Document             (composed subtask .1, from {% subtask aiki/review/spec %})
│   ├── Read document                (spec review subtask .1.1)
│   └── Evaluate completeness        (spec review subtask .1.2)
└── Provide feedback                 (static subtask .2)
```

**Variable Inheritance:**

Child templates inherit all variables from the parent review template:

| Variable | Available in child template |
|----------|----------------------------|
| `data.scope.type` | Type of review (task, spec, implementation, session) |
| `data.scope.id` | Path to file being reviewed (or task ID) |
| `data.scope.name` | Display name of review target |
| `data.scope.task_ids` | Task IDs (for session reviews) |
| `parent.*` | Points to composed subtask (not top-level review) |

This allows specialized templates to reference context like `{{data.scope.id}}` in their instructions without re-passing variables.

**Benefits:**
- **Modularity**: Each review type is a separate, reusable template
- **Clarity**: Main review template is concise (just conditional includes)
- **Maintainability**: Update spec review logic in one place (`aiki/review/spec`)
- **Composability**: Can nest reviews or reuse templates in other contexts

### Review Criteria by Target Type

**Code Review (task/session):**

| Category | What to Check |
|----------|---------------|
| Correctness | Logic errors, edge cases, incorrect assumptions |
| Quality | Error handling, resource leaks, code clarity |
| Security | Injection, auth, data exposure, crypto misuse |
| Performance | Algorithms, unnecessary operations, resources |

**Document Review (file without --implementation):**

| Category | What to Check |
|----------|---------------|
| Completeness | All sections filled, no TODOs, open questions documented |
| Clarity | Unambiguous requirements, clear acceptance criteria |
| Implementability | Decomposable into tasks, sufficient technical detail |
| UX | User experience considered, intuitive design |

**Implementation Review (file with --implementation):**

| Category | What to Check |
|----------|---------------|
| Spec Coverage | All requirements in codebase, no missing features, no scope creep, completeness |
| Code Quality | Logic errors, error handling, resource management, clarity |
| Security | Injection vulnerabilities, auth issues, data exposure |
| Spec Alignment | UX matches spec, commands as designed, criteria met, architecture followed |

---

## Implementation Review (`--implementation` flag)

### Overview

The `--implementation` flag changes the review target from the **spec document** to the **current codebase implementation** described in the spec.

```bash
# Review the spec document itself
aiki review ops/now/feature.md

# Review the current codebase implementation described in the spec
aiki review ops/now/feature.md --implementation
```

### How It Works

When `--implementation` is used, the agent explores the current codebase:

1. **Read the spec** - Understand what should be implemented
   - What features are described
   - What files/modules should exist
   - What the architecture should look like

2. **Explore the codebase** - Find relevant implementation in current state
   - Search for files and functions mentioned in the spec
   - Identify modules and components related to the feature
   - Read the actual code to understand current state

3. **Compare against spec** - Verify the current implementation matches requirements
   - Check if all requirements exist in the codebase
   - Verify UX matches the design
   - Confirm acceptance criteria are met
   - Identify gaps or deviations from the spec

### Template Behavior

The review command sets `data.scope.type` based on the target and flags:

- No flag (default) → `data.scope.type: spec` → uses `aiki/review/spec` template
- With `--implementation` → `data.scope.type: implementation` → uses `aiki/review/implementation` template

### Use Cases

**Pre-implementation spec review:**
```bash
# Before building, review the spec itself
aiki review ops/now/feature.md
```

**Post-implementation review:**
```bash
# Review the current implementation against the spec
aiki review ops/now/feature.md --implementation
```

**Full workflow:**
```bash
# 1. Write and review the spec
aiki spec "Add authentication"
aiki review ops/now/add-authentication.md

# 2. Build the implementation
aiki build ops/now/add-authentication.md

# 3. Review the current implementation against the spec
aiki review ops/now/add-authentication.md --implementation

# 4. Fix any issues found
aiki fix <review-id>
```

### What Gets Reviewed

**Document review (no flag):**
- The spec markdown file itself
- Structure, completeness, clarity
- Requirements are well-defined
- Can be decomposed into tasks

**Implementation review (`--implementation` flag):**
- Current codebase state (not task diffs)
- Agent explores to find relevant code
- Implementation matches requirements
- UX matches the design
- Acceptance criteria are met
- Code quality and security

---

## Part 2: Fix for File-Targeted Reviews

### Flow

```
aiki review ops/now/feature.md → close review Y → fix Y → new standalone fix task Z
```

- **Fix** creates a new standalone task (no parent task to attach to)
- Fix task sources the review: `source: task:<review-id>`
- Agent handles fixes independently

**Examples:**
- `aiki review ops/now/plan.md` → `aiki fix xqrmnpst` → new standalone task
- `aiki review mvslrsp` → `aiki fix ytnzwklm` → subtask on original (handled by `fix-original-task.md`)

### Fix Routing via Task Data

**Implemented via [ReviewScope Refactor](../done/review-scope-refactor.md).** Fix routing uses `ReviewScope::from_data(&review_task.data)` to deserialize scope from the review task's data fields, then matches on `scope.kind`:

```rust
let scope = ReviewScope::from_data(&review_task.data)?;
match scope.kind {
    ReviewScopeKind::Task => {
        // Subtask on original task (existing behavior)
    }
    ReviewScopeKind::Spec | ReviewScopeKind::Implementation => {
        // NEW: Standalone fix task (no parent task to attach to)
    }
    ReviewScopeKind::Session => {
        // Session fix (existing behavior)
    }
}
```

For file targets (`Spec` or `Implementation`), fix creates a standalone task (no parent task to attach to). The fix task's template can read `scope.kind` to know whether to fix the spec or the implementation.

### Fix Template for File Reviews

File reviews need a different template path since there's no parent task. The template should:
- Reference the review task to read findings
- Not assume a parent task exists
- Create subtasks under the standalone fix task itself

### Pipe Flow

```bash
aiki review ops/now/feature.md | aiki fix   # review outputs review ID (e.g., xqrmnpst), fix creates standalone task
```

---

## Use Cases

### Pre-Plan Spec Review

```bash
# Review the spec document before building
aiki review ops/now/feature.md
# If issues found, fix the spec
aiki fix xqrmnpst
```

### Post-Implementation Review

```bash
# Review the current implementation against the spec
aiki review ops/now/feature.md --implementation
# If issues found, fix the implementation
aiki fix ytnzwklm
```

### Full Pipeline

```bash
# Spec → Review spec → Fix spec → Build → Review implementation → Fix implementation
aiki spec "Add authentication"
aiki review ops/now/add-authentication.md | aiki fix              # Review the spec itself
aiki build ops/now/add-authentication.md
aiki review ops/now/add-authentication.md --implementation | aiki fix   # Review the current implementation
```

### Iterative Development

```bash
# Write spec
aiki spec "User authentication"

# Review spec quality
aiki review ops/now/user-authentication.md

# Build implementation
aiki build ops/now/user-authentication.md

# Review current implementation against spec
aiki review ops/now/user-authentication.md --implementation

# Fix any issues found
aiki fix xqrmnpst

# Re-review after fixes
aiki review ops/now/user-authentication.md --implementation
```

---

## Output Format

Output format follows the simpler-markdown-output.md spec. Since review completes and closes a review task, it's a state-transition command that includes a context footer.

**stdout (for piping):**
```
xqrmnpst
```

**stderr (file review without --implementation):**
```
Review: xqrmnpst
Type: spec
Scope: ops/now/feature.md

Review complete: 3 issues found
- Incomplete requirements section
- Missing acceptance criteria
- Unclear technical approach

---
Run `aiki fix xqrmnpst` to remediate.

---
Tasks (2 ready)
Run `aiki task` to view - OR - `aiki task start` to begin work.
```

**stderr (file review with --implementation):**
```
Review: ytnzwkl
Type: implementation
Scope: ops/now/feature.md

Review complete: 2 issues found
- Missing error handling in auth module
- UX doesn't match spec requirements

---
Run `aiki fix ytnzwkl` to remediate.

---
Tasks (2 ready)
Run `aiki task` to view - OR - `aiki task start` to begin work.
```

**stderr (task review):**
```
Review: zqrmnps
Type: task
Scope: mvslrsp - Fix auth bug

Review complete: 1 issue found
- Potential SQL injection vulnerability

---
Run `aiki fix zqrmnps` to remediate.

---
Tasks (2 ready)
Run `aiki task` to view - OR - `aiki task start` to begin work.
```

**Note:** Review completes automatically and closes the review task with `--summary` (see [Task Summary](task-summary.md)), so the output matches the pattern for `task close` (state-transition command with context footer). The task ID is written to stdout for piping to `aiki fix`. The review summary (e.g., "Review complete: 3 issues found") is stored as the task's `summary` field, while individual issues are stored as comments.

---

## Implementation Order

**Prerequisites (done):**
- ✅ [ReviewScope Refactor](../done/review-scope-refactor.md) — `ReviewScope` struct, scope as task data, fix routing via `from_data()`

**Prerequisites (pending):**
- ⬜ [Task Summary](task-summary.md) — `--summary` flag on `task close`, templates updated to use `--summary`

### Phase 1: Target Detection
- Extend `detect_target()` to return `ReviewScope` for file targets (currently only handles task and session)
- Add `--implementation` flag: when used with a file target, returns `ReviewScopeKind::Implementation`
- Validate `--implementation` is only used with file targets (error otherwise)

### Phase 2: Specialized Review Templates
- Create `.aiki/templates/aiki/review/spec.md` for spec document reviews
- Create `.aiki/templates/aiki/review/implementation.md` for implementation vs spec reviews
- Create `.aiki/templates/aiki/review/task.md` for task code reviews (extract existing logic)
- Create `.aiki/templates/aiki/review/session.md` for session reviews (extract existing logic)

**Template structure:**

Each specialized template should be a complete, standalone template with:
- Frontmatter (`version`, `type: review`)
- Parent task name (e.g., `# Review Spec Document`)
- Subtasks specific to that review type
- Clear instructions that reference inherited data variables (`{{data.scope.id}}`, `{{data.scope.name}}`, etc.)

### Phase 2b: Composable Review Template
- Update `aiki/review` template to use `{% subtask aiki/review/{{data.scope.type}} %}`
- Single line interpolates to the appropriate specialized template based on `data.scope.type`
- Test with all target types: task, spec, implementation, session

### Phase 3: Fix for File Targets
- Handle `ReviewScopeKind::Spec` and `ReviewScopeKind::Implementation` in existing fix routing (standalone task creation)
- Fix template for file-targeted reviews
- `ReviewScopeKind` naturally distinguishes "fix the spec" (`Spec`) from "fix the code" (`Implementation`)
- Fix reads review task's `summary` field for quick overview, comments for individual issues

### Phase 4: Integration & Testing
- Wire target detection into review command
- **Output formatting**: Review completion closes the task with `--summary` and emits a state-transition footer on stderr (matching `task close` pattern) with review summary, `aiki fix` hint, and task context block. Review task ID written to stdout for piping. See Output Format section above.
- Tests for detection, template rendering, implementation flag, and fix flow
- Verify review templates use `--summary` (not `--comment`) for closing

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| File doesn't exist | Error: `"File not found: <path>"` |
| File exists but not .md | Error: `"File review only supports .md files currently"` |
| `--implementation` flag on task target | Error: `"--implementation flag only applies to file targets"` |
| `--implementation` flag on session target | Error: `"--implementation flag only applies to file targets"` |
| Task ID doesn't exist | Error: `"Task not found: <id>"` |
| Empty session (no closed tasks) | Success with "nothing to review" message |

---

## Edge Cases

- **Review targets multiple files:** Fix task covers all files from the review.
- **File no longer exists at fix time:** Agent should note this and skip or adapt.
- **Missing scope data:** If a review task has no `data.scope.type` (e.g., created before the ReviewScope refactor), `ReviewScope::from_data()` returns an error: "Missing scope.type in review task data".

---

## Future Enhancements

- **Smart file type detection** - Auto-detect specs vs docs vs readmes from path/content
- **Non-markdown files** - Review code, config, scripts directly (new specialized templates)
- **Related file detection** - Auto-include tests/docs when reviewing code
- **Review history tracking** - Quality trends and common issues over time
- **Batch reviews** - Review multiple files with glob patterns
- **Per-directory templates** - Different review criteria per codebase section (composable templates make this easy)
- **Spec quality gate hook** - Block `aiki plan` until spec review passes
- **Custom review templates** - Users can define their own specialized review templates for domain-specific needs
- **Review template composition** - Chain multiple specialized reviews (e.g., `{% subtask aiki/review/security %}` + `{% subtask aiki/review/performance %}`)
