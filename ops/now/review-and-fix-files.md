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

`aiki review` currently only targets tasks. We want it to also handle files and specs, with `aiki fix` adapting accordingly. The `ReviewScope` struct and fix routing via `ReviewScope::from_data()` are implemented (see [ReviewScope Refactor](../done/review-scope-refactor.md)), but only `Task` kind is fully wired for both review and fix. `Session` review works but `aiki fix` hard-errors for session reviews (out of scope for this plan). `Spec` and `Implementation` need to be added.

This plan extends both `review` and `fix` to support non-task targets, building on the existing `ReviewScope` infrastructure.

---

## Prerequisites Status

- ✅ Template conditionals (implemented in `cli/src/tasks/templates/conditionals.rs`)
- ✅ Composable task templates (implemented in `cli/src/tasks/templates/`)
- ✅ Fix targets original task (implemented in `cli/src/commands/fix.rs`)
- ✅ [ReviewScope Refactor](../done/review-scope-refactor.md) — typed `ReviewScope` struct, scope as task data, fix routing via data instead of source prefixes
- ✅ [Task Summary](task-summary.md) — dedicated `summary` field on tasks, `--summary` flag on `task close`

The `ReviewScope` struct and `ReviewScopeKind` enum are implemented in `cli/src/commands/review.rs`. Fix routing in `cli/src/commands/fix.rs` uses `ReviewScope::from_data()` to read scope from task data. Currently only `ReviewScopeKind::Task` is fully wired for both review and fix. `Session` review works but fix hard-errors (out of scope for this plan). `Spec` and `Implementation` are the new targets added by this plan.

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

**Refactor:** Extract scope resolution from `create_review()` into a new `detect_target()` function. Currently, `create_review()` resolves scope inline via `match params.task_id` (`Some(id)` → Task, `None` → Session). This refactor moves that logic to the CLI layer and extends it with file target support. The `--implementation` flag determines whether a file target produces `ReviewScopeKind::Spec` or `ReviewScopeKind::Implementation`:

**`CreateReviewParams` change:** Replace `task_id: Option<String>` with `scope: ReviewScope`. The caller (CLI or flow action) pre-resolves the scope and passes it in, keeping `create_review()` focused on task creation.

```rust
fn detect_target(arg: Option<&str>, implementation: bool) -> Result<ReviewScope> {
    match arg {
        None => {
            if implementation {
                bail!("--implementation flag only applies to file targets")
            }
            Ok(ReviewScope {
                kind: ReviewScopeKind::Session,
                id: "session".into(),
                task_ids: collect_session_task_ids()?,
            })
        }

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

        Some(s) if looks_like_task_id(s) => {
            // Looks like a task ID or prefix — try resolving it.
            // Uses find_task() which handles both full IDs and unique prefixes.
            if implementation {
                bail!("--implementation flag only applies to file targets")
            }
            let task = find_task(&tasks, s)?; // resolves prefixes, errors on ambiguity
            Ok(ReviewScope {
                kind: ReviewScopeKind::Task,
                id: task.id.clone(),
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

**Implemented via [ReviewScope Refactor](../done/review-scope-refactor.md).** The `ReviewScope` struct serializes scope metadata to task `data` fields via `scope.to_data()`. These are persisted on the review task and available to both templates (as `{{data.scope.kind}}`, etc.) and downstream consumers like `aiki fix` (via `ReviewScope::from_data(&review_task.data)`).

The review command builds a `ReviewScope` from target detection, then calls `scope.to_data()` to produce:

**For task review:**
| Key | Value |
|-----|-------|
| `scope.kind` | `task` |
| `scope.id` | `xqrmnpst` |
| `scope.name` | `Task (xqrmnpst)` |

**For spec document review:**
| Key | Value |
|-----|-------|
| `scope.kind` | `spec` |
| `scope.id` | `ops/now/feature.md` |
| `scope.name` | `Spec (feature.md)` |

**For implementation review (with --implementation flag):**
| Key | Value |
|-----|-------|
| `scope.kind` | `implementation` |
| `scope.id` | `ops/now/feature.md` |
| `scope.name` | `Implementation (feature.md)` |

**For session review:**
| Key | Value |
|-----|-------|
| `scope.kind` | `session` |
| `scope.id` | `session` |
| `scope.name` | `Session` |
| `scope.task_ids` | `abc123,def456` |

The `scope.name` field is computed by `ReviewScope::name()` from kind and id, keeping templates simple.

**Why task data instead of sources:** Scope fields are structured metadata about the review target. Storing them as task data means they persist on the task, are readable by `aiki fix` via `ReviewScope::from_data()`, and automatically flow into templates. Sources (`task:`, `file:`, `prompt:`) remain purely for lineage/provenance.

### Unified Review Template (`.aiki/templates/aiki/review.md`)

The wrapper template already exists and works. Its structure:

1. `{% subtask aiki/review/{{data.scope.kind}} %}` — delegates to specialized template based on scope
2. `## Review` — agent reviews work and leaves comments on `{{parent.id}}` for each finding
3. `{% subtask aiki/fix/loop if data.options.fix %}` — conditional fix loop when `data.options.fix` is set

**No structural changes needed for this plan.** The existing template already routes via `data.scope.kind` and will work for new scope kinds (`spec`, `implementation`) once `detect_target()` produces them and the specialized templates handle them.

**How it works:**
- `data.scope.kind` is set based on what you're reviewing (task, spec, implementation, session)
- Template interpolation resolves `aiki/review/{{data.scope.kind}}` to the appropriate specialized template
- Each specialized template brings its own subtasks with domain-specific evaluation criteria
- Templates inherit data variables from parent context (`data.scope.*`, `source.*`, etc.)
- Agent sees a nested task tree with appropriate review steps for the target type
- The "Review" subtask collects findings as comments on the parent review task for `aiki fix` to consume

**Specialized review templates (mapped from data.scope.kind):**

1. **`aiki/review/task`** - Review code changes from a task
2. **`aiki/review/spec`** - Review spec documents for completeness, clarity, implementability
3. **`aiki/review/implementation`** - Review current codebase implementation against a spec
4. **`aiki/review/session`** - Review all closed tasks in a session

Each specialized template is a complete template with its own parent name and subtasks, which get composed into the parent review task tree.

### Contract Between Wrapper and Specialized Templates

**Wrapper template responsibilities:**
- Set up review context (`data.scope.*` fields on the review task)
- Delegate to specialized template via `{% subtask aiki/review/{{data.scope.kind}} %}`
- Collect findings from specialized review subtask
- Record findings as comments on the parent review task
- Close review with `--summary "Review complete (n issues found)"`

**Specialized template responsibilities:**
- Perform domain-specific review (code, spec, implementation, session)
- Identify issues during review subtasks
- Specialized templates should NOT directly comment on the parent - they work within their subtask scope
- Findings are collected by the wrapper's "Review" subtask

**Contract for `aiki fix`:**
- Review task is closed with `--summary` describing the overall result (e.g., "Review complete (3 issues found)")
- Each issue is a comment on the review task (added during review, before closing)
- `aiki fix` reads comments from closed review task and creates fix tasks
- Comment text becomes fix task description
- The review task's `summary` field provides a quick overview; individual comments provide the actionable detail

This separation allows specialized templates to focus on domain-specific review logic without knowing about the fix workflow.

### The `data.options.fix` Conditional

The wrapper template includes `{% subtask aiki/fix/loop if data.options.fix %}` for inline fix loops (review finds issues, then immediately fixes them without a separate `aiki fix` invocation). This is wired via `--fix` on `aiki review`:

```bash
aiki review <task-id> --fix          # Review + auto-fix in one command
aiki review ops/now/spec.md --fix    # Review spec + auto-fix in one command
```

**Wiring `data.options.fix` for all scope kinds:**

The `--fix` flag must be added to `ReviewArgs` and passed through to `create_review_task_from_template` as a data field (`options.fix: "true"`). This is independent of scope kind — the fix loop should work for task, spec, and implementation reviews alike.

**Phase 1 addition:** Add `--fix` flag to `ReviewArgs`, wire it into `data.options` map alongside `scope.*` data when calling `create_review_task_from_template`. The template's conditional `{% subtask aiki/fix/loop if data.options.fix %}` then activates for any scope kind when the flag is present.

### How Composable Templates Work for Reviews

**Subtask Resolution:**

When the template engine encounters `{% subtask aiki/review/{{data.scope.kind}} %}`:

1. Interpolate `{{data.scope.kind}}` to get the template name (e.g., `data.scope.kind: spec` → `aiki/review/spec`)
2. Load the referenced template
3. Child template inherits parent's full variable context (`data.scope.*`, `source.*`, etc.)
4. Resolve the child template's parent name (becomes the composed subtask name)
5. Create the composed subtask in the task tree
6. Recursively create the child template's subtasks as sub-subtasks

**Task Tree Example:**

For `aiki review ops/now/feature.md` (spec document), where `aiki/review/spec` has a subtask "Understand the plan described in spec":

```
Review: Spec (feature.md)                        (parent)
├── Review all subtasks and start first batch     (.0, auto-created)
├── Understand the plan described in spec          (.1, from {% subtask aiki/review/spec %})
├── Review                                         (.2, leave comments on parent for findings)
└── Fix Loop                                       (.3, conditional: only if data.options.fix)
```

**Variable Inheritance:**

Child templates inherit all variables from the parent review template:

| Variable | Available in child template |
|----------|----------------------------|
| `data.scope.kind` | Type of review (task, spec, implementation, session) |
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

The review command sets `data.scope.kind` based on the target and flags:

- No flag (default) → `data.scope.kind: spec` → uses `aiki/review/spec` template
- With `--implementation` → `data.scope.kind: implementation` → uses `aiki/review/implementation` template

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
        // Out of scope — session fix remains a hard error for now.
        // Session reviews span multiple tasks with no single parent,
        // which needs a different fix strategy (future work).
    }
}
```

For file targets (`Spec` or `Implementation`), fix creates a standalone task (no parent task to attach to). The fix task's template can read `scope.kind` to know whether to fix the spec or the implementation.

### Unified Fix Task Creation

**Principle:** Always pass scope data to fix templates. One codepath handles both parent-based and standalone fix tasks.

**Refactor `create_fix_subtask_on_original` → `create_fix_task`** with `parent: Option<&Task>`:

```rust
fn create_fix_task(
    cwd: &Path,
    review_task: &Task,
    scope: &ReviewScope,
    parent: Option<&Task>,
    assignee: &Option<String>,
    template_name: &str,
) -> Result<String> {
    if let Some(p) = parent {
        let events = read_events(cwd)?;
        let current_tasks = materialize_graph(&events).tasks;
        reopen_if_closed(cwd, &p.id, &current_tasks, "Subtasks added")?;
    }

    // Always pass scope data — templates use {{data.scope.name}} for framing
    let mut source_data = scope.to_data();
    source_data.insert("name".into(), review_task.name.clone());
    source_data.insert("id".into(), review_task.id.clone());

    let params = TemplateTaskParams {
        template_name: template_name.to_string(),
        sources: vec![format!("task:{}", review_task.id)],
        assignee: assignee.clone(),
        priority: parent.map(|p| Some(p.priority)).unwrap_or(None),
        parent_id: parent.map(|p| p.id.clone()),
        parent_name: parent.map(|p| p.name.clone()),
        source_data,
        ..Default::default()
    };

    create_from_template(cwd, params)
}
```

**Callers in fix routing:**

```rust
let scope = ReviewScope::from_data(&review_task.data)?;
match scope.kind {
    ReviewScopeKind::Task => {
        let original_task = find_task(&tasks, &scope.id)?;
        let assignee = determine_followup_assignee(agent_type, Some(original_task));
        let template = template_name.as_deref().unwrap_or("aiki/fix");
        create_fix_task(cwd, review_task, &scope, Some(original_task), &assignee, template)?
    }
    ReviewScopeKind::Spec | ReviewScopeKind::Implementation => {
        let assignee = determine_followup_assignee(agent_type, None);
        let template = template_name.as_deref().unwrap_or("aiki/fix");
        create_fix_task(cwd, review_task, &scope, None, &assignee, template)?
    }
    ReviewScopeKind::Session => {
        return Err(...) // remains hard error
    }
}
```

**Update existing `aiki/fix` template** to use `{{data.scope.name}}` instead of `{{parent.id}}`:

```diff
- Review task `{{source.id}}` found issues in the original task `{{parent.id}}`.
+ Review task `{{source.id}}` found issues in {{data.scope.name}}.
```

Since scope data is always passed, `{{data.scope.name}}` resolves to "Task (xqrmnpst)" for task reviews and "Spec (feature.md)" for file reviews. One template works for both. **No separate `aiki/fix/file` template needed.**

**fix.rs output functions:** Thread `scope` through and use scope-aware copy:
- Task scope: "Created fix followup subtask under original task"
- File scope: "Created standalone fix task for {scope.name}"

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

### Common Output Formatter

Both `review.rs` and `fix.rs` have multiple output functions (started/completed/async/approved) that each hand-build markdown strings. These should share a common formatter so presentation stays in sync.

```rust
/// Structured output data for review/fix commands.
/// All output functions build one of these, then call format_command_output().
struct CommandOutput<'a> {
    /// Heading: "Review Started", "Fix Completed", "Approved", etc.
    heading: &'a str,
    /// Task ID of the review or fix task
    task_id: &'a str,
    /// Review scope (provides Type + Scope lines)
    scope: &'a ReviewScope,
    /// Execution status: "started", "completed", "async"
    status: &'a str,
    /// Issue list (when available, e.g., after review completion)
    issues: Option<&'a [TaskComment]>,
    /// Action hint: "Run `aiki fix ...` to remediate.", etc.
    hint: Option<String>,
}

/// Produces the canonical output block used by all review/fix output functions.
fn format_command_output(output: &CommandOutput) -> String {
    // heading, task id, type (scope.kind), scope (scope.name),
    // optional issue list, optional hint
}
```

Each existing output function becomes a thin wrapper:

```rust
fn output_review_completed(review_id: &str, scope: &ReviewScope, comments: &[TaskComment]) -> Result<()> {
    let hint = if comments.is_empty() { None } else {
        Some(format!("Run `aiki fix {}` to remediate.", review_id))
    };
    let output = CommandOutput {
        heading: "Review Completed",
        task_id: review_id,
        scope,
        status: "completed",
        issues: Some(comments),
        hint,
    };
    let content = format_command_output(&output);
    let md = MdBuilder::new("review").build(&content, &[], &[]);
    eprintln!("{}", md);
    Ok(())
}
```

**Benefits:**
- One place to change layout (heading order, separator style, issue formatting)
- review.rs and fix.rs stay in sync automatically
- New output modes (e.g., JSON) would only need one new formatter

---

## Implementation Order

**Prerequisites (done):**
- ✅ [ReviewScope Refactor](../done/review-scope-refactor.md) — `ReviewScope` struct, scope as task data, fix routing via `from_data()`

**Prerequisites (pending):**
- ✅ [Task Summary](task-summary.md) — `--summary` flag on `task close`, templates updated to use `--summary`

### Phase 1: Target Detection & CreateReviewParams Refactor
- Extract scope resolution from `create_review()` into a new `detect_target()` function at the CLI layer
- Replace `CreateReviewParams.task_id: Option<String>` with `scope: ReviewScope` (callers pre-resolve scope)
- `detect_target()` handles: no arg → Session, task ID → Task, `.md` file → Spec, `.md` file + `--implementation` → Implementation
- Add `--implementation` flag: when used with a file target, returns `ReviewScopeKind::Implementation`
- Validate `--implementation` is only used with file targets (error otherwise)
- Wire `file:` source for file targets: for `Spec` and `Implementation` scopes, emit `sources: vec![format!("file:{}", scope.id)]` (matching the `task:` source pattern for task scope)
- Add `--fix` flag to `ReviewArgs`: when present, set `data.options.fix: "true"` alongside scope data in `create_review_task_from_template`. This activates the conditional `{% subtask aiki/fix/loop if data.options.fix %}` in the wrapper template for any scope kind

### Phase 2: Specialized Review Templates
These templates already exist but currently contain only minimal instructions (read the thing, close with summary). They need to be enriched with the structured review criteria from the "Review Criteria by Target Type" section above.

**Templates to update:**

**`.aiki/templates/aiki/review/spec.md`** — Currently: reads spec, checks for source task context. **Add:** structured checklist for Completeness, Clarity, Implementability, and UX criteria. The agent should explicitly check each category and report findings.

**`.aiki/templates/aiki/review/implementation.md`** — Currently: reads spec via nested `{% subtask aiki/review/spec %}`, then explores codebase. **Add:** structured checklist for Spec Coverage, Code Quality, Security, and Spec Alignment criteria. The agent should compare each spec requirement against the codebase.

**`.aiki/templates/aiki/review/task.md`** — Currently: shows task intent + diff. Already adequate for code review, but could reference the Correctness, Quality, Security, Performance categories.

**`.aiki/templates/aiki/review/session.md`** — Currently: iterates `data.scope.task_ids`. Already adequate (delegates per-task review).

**Template structure:**

Existing specialized templates are subtask content (no frontmatter) with:
- A heading that becomes the subtask name (e.g., `# Understand the plan described in spec`)
- Instructions referencing inherited data variables (`{{data.scope.id}}`, `{{data.scope.name}}`, etc.)
- Optional nested `{% subtask %}` directives (e.g., `implementation.md` nests `aiki/review/spec`)

### Phase 2b: Verify Wrapper Template
- The wrapper template (`aiki/review.md`) already routes via `{% subtask aiki/review/{{data.scope.kind}} %}` — no structural changes needed
- Verify it works with new scope kinds: test `spec` and `implementation` targets end-to-end
- Verify the conditional fix loop (`{% subtask aiki/fix/loop if data.options.fix %}`) still works for all scope kinds

### Phase 3: Fix for File Targets
- Refactor `create_fix_subtask_on_original` → `create_fix_task` with `parent: Option<&Task>` (see "Unified Fix Task Creation" section above)
- Always pass scope data to fix templates via `source_data` — both task and file fix paths use the same codepath
- Replace hard-error for `ReviewScopeKind::Spec | Implementation` in `fix.rs` with `create_fix_task(cwd, review_task, &scope, None, ...)` (no parent)
- Update existing `aiki/fix` template: replace `{{parent.id}}` with `{{data.scope.name}}` — one template works for all scope kinds
- Update fix output functions to be scope-aware (thread `scope` through): "standalone fix task for {scope.name}" vs "fix followup subtask under original task"
- `ReviewScopeKind` naturally distinguishes "fix the spec" (`Spec`) from "fix the code" (`Implementation`)
- Fix reads review task's `summary` field for quick overview, comments for individual issues
- ✅ **Already fixed:** Off-by-one in fix output issue count (`comments.len().saturating_sub(1)` → `comments.len()`) — summary is no longer a comment, so all comments are issues

### Phase 4: Integration & Testing
- Wire target detection into review command
- **Output formatting**: Introduce common `CommandOutput` struct and `format_command_output()` formatter (see "Common Output Formatter" section above). Refactor all output functions in both `review.rs` and `fix.rs` to use it:
  - `review.rs`: `output_review_started`, `output_review_completed`, `output_review_async`, `output_nothing_to_review`
  - `fix.rs`: `output_followup_started`, `output_followup_completed`, `output_followup_async`, `output_approved`
  - All functions thread `ReviewScope` through for Type/Scope lines
  - Completed variants include issue list and `aiki fix` hint
  - Review task ID written to stdout for piping (already implemented)
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
| Target not found (not a file, not a valid task ID/prefix) | Error: `"Target not found: <input>"` |
| Empty session (no closed tasks) | Success with "nothing to review" message |

---

## Edge Cases

- **Review targets multiple files:** Not currently supported. Each review targets a single file. See [Multiple File Review](../next/multiple-file-review.md) for future design.
- **File no longer exists at fix time:** Agent should note this and skip or adapt.
- **Missing scope data:** If a review task has no `data.scope.kind` (e.g., created before the ReviewScope refactor), `ReviewScope::from_data()` returns an error: "Missing scope.kind in review task data".
- **Flow review actions remain task-id-centric:** The flow engine's review action (`cli/src/flows/engine.rs`, `cli/src/flows/types.rs`) accepts only task IDs and won't automatically gain file-target behavior. This is out of scope for this plan — flow schema/engine updates are future work. File-targeted reviews are only available via the CLI `aiki review` command.

---

## Future Enhancements

- **Smart file type detection** - Auto-detect specs vs docs vs readmes from path/content
- **Non-markdown files** - Review code, config, scripts directly (new specialized templates)
- **Related file detection** - Auto-include tests/docs when reviewing code
- **Review history tracking** - Quality trends and common issues over time
- **[Batch reviews](../next/multiple-file-review.md)** - Review multiple files with glob patterns
- **Per-directory templates** - Different review criteria per codebase section (composable templates make this easy)
- **Spec quality gate hook** - Block `aiki plan` until spec review passes
- **Custom review templates** - Users can define their own specialized review templates for domain-specific needs
- **Review template composition** - Chain multiple specialized reviews (e.g., `{% subtask aiki/review/security %}` + `{% subtask aiki/review/performance %}`)
