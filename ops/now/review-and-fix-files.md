# Review and Fix Non-Task Targets

**Date**: 2026-02-06
**Status**: Idea
**Priority**: P3
**Depends on**: `ops/now/fix-original-task.md`

**Related Documents**:
- [Fix Targets Original Task](../now/fix-original-task.md) - Task-targeted fix (prerequisite)
- [Composable Task Templates](composable-task-templates.md) - Template composition with `{% subtask %}` (prerequisite)
- [Template Conditionals](../now/template-conditionals.md) - Conditional logic for templates (implemented)
- [Task Templates](../done/task-templates.md) - Template system (implemented)

**Prerequisites**:
- Template conditionals (implemented in `cli/src/tasks/templates/conditionals.rs`)
- Composable task templates (in progress - needed for specialized review templates)
- Fix targets original task (in progress)

---

## Problem

`aiki review` currently only targets tasks. We want it to also handle files and specs, with `aiki fix` adapting accordingly. After `fix-original-task.md` lands, the `ReviewTarget` enum and `get_review_target` function exist but `File` returns an error.

This plan extends both `review` and `fix` to support non-task targets.

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

```rust
fn detect_target(arg: Option<&str>) -> ReviewTarget {
    match arg {
        None => ReviewTarget::Session,

        Some(s) if s.ends_with(".md") && Path::new(s).exists() => {
            ReviewTarget::File {
                path: s.into(),
                file_type: "spec",
            }
        }

        Some(s) if is_task_id(s) => {
            ReviewTarget::Task { id: s.into() }
        }

        Some(s) if Path::new(s).exists() => {
            anyhow::bail!("File review only supports .md files currently")
        }

        Some(s) => {
            anyhow::bail!("Target not found: {}", s)
        }
    }
}
```

### Template Data

The review command populates `data.*` fields based on target type:

**For task review:**
```yaml
data:
  target_type: task
  target_name: "task xqrmnpst"
  task_id: xqrmnpst
```

**For file review (spec document):**
```yaml
data:
  target_type: file
  target_name: "ops/now/feature.md"
  path: ops/now/feature.md
  file_type: spec
  review_mode: document  # default when no --implementation flag
```

**For file review with --implementation (current codebase):**
```yaml
data:
  target_type: file
  target_name: "ops/now/feature.md"
  path: ops/now/feature.md
  file_type: spec
  review_mode: implementation  # when --implementation flag is used
```

**For session review:**
```yaml
data:
  target_type: session
  target_name: "session changes"
  task_ids: [abc123, def456]
```

### Unified Review Template with Composable Subtasks

The `aiki/review` template uses **composable task templates** (via `{% subtask %}`) to delegate to specialized review templates based on target type:

```markdown
---
version: 2.0.0
type: review
---

# Review: {{data.target_name}}

Review the target for quality and readiness.

# Subtasks

## Understand what you're reviewing
Identify the target type and review approach.

{% subtask aiki/review/spec if data.file_type == "spec" and data.review_mode == "document" %}
{% subtask aiki/review/implementation if data.file_type == "spec" and data.review_mode == "implementation" %}
{% subtask aiki/review/code if data.target_type == "task" %}
{% subtask aiki/review/session if data.target_type == "session" %}

## Provide feedback
Leave comments on issues found using `aiki task comment`.
```

**How it works:**
- The unified template conditionally includes specialized review templates
- Each specialized template brings its own subtasks with domain-specific evaluation criteria
- Templates inherit variables from parent context (`data.*`, `source.*`, etc.)
- Agent sees a nested task tree with appropriate review steps for the target type

**Specialized review templates:**

1. **`aiki/review/spec`** - Review spec documents for completeness, clarity, implementability
2. **`aiki/review/implementation`** - Review current codebase implementation against a spec
3. **`aiki/review/code`** - Review code changes from a task (existing behavior)
4. **`aiki/review/session`** - Review all closed tasks in a session (existing behavior)

Each specialized template is a complete template with its own parent name and subtasks, which get composed into the parent review task tree.

### How Composable Templates Work for Reviews

**Subtask Resolution:**

When the template engine encounters `{% subtask aiki/review/spec if data.file_type == "spec" %}`:

1. Evaluate the condition (`if data.file_type == "spec"`)
2. If true, load the referenced template (`aiki/review/spec`)
3. Child template inherits parent's full variable context (`data.*`, `source.*`, etc.)
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
| `data.path` | Path to file being reviewed |
| `data.file_type` | Type of file (e.g., "spec") |
| `data.review_mode` | "document" or "implementation" |
| `data.target_name` | Display name of review target |
| `parent.*` | Points to composed subtask (not top-level review) |

This allows specialized templates to reference context like `{{data.path}}` in their instructions without re-passing variables.

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

The review template adapts based on `data.review_mode`:

- `review_mode: document` (default) → Review the spec document for completeness, clarity, implementability
- `review_mode: implementation` (with `--implementation`) → Review the current codebase against the spec for coverage, quality, alignment

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
- `aiki review ops/now/plan.md` → `aiki fix <review-id>` → new standalone task
- `aiki review <task-id>` → `aiki fix <review-id>` → subtask on original (handled by `fix-original-task.md`)

### Handle `ReviewTarget::File` in fix.rs

The `get_review_target` function and `ReviewTarget::File` variant already exist (added by `fix-original-task.md`). This work adds the handler:

```rust
ReviewTarget::File(_) => {
    // Create standalone fix task (current behavior)
    // Source the fix task to the review: task:<review-id>
    // Agent handles fixes independently
}
```

### Fix Template for File Reviews

File reviews need a different template path since there's no parent task. The template should:
- Reference the review task to read findings
- Not assume a parent task exists
- Create subtasks under the standalone fix task itself

### Pipe Flow

```bash
aiki review ops/now/feature.md | aiki fix   # review outputs review ID, fix creates standalone task
```

---

## Use Cases

### Pre-Plan Spec Review

```bash
# Review the spec document before building
aiki review ops/now/feature.md
# If issues found, fix the spec
aiki fix <review-id>
```

### Post-Implementation Review

```bash
# Review the current implementation against the spec
aiki review ops/now/feature.md --implementation
# If issues found, fix the implementation
aiki fix <review-id>
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
aiki fix <review-id>

# Re-review after fixes
aiki review ops/now/user-authentication.md --implementation
```

---

## Output Format

Output format follows current `aiki review` pattern, with added target info:

**stdout (piped):**
```
xqrmnpst
```

**stderr (file review without --build):**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" target="ops/now/feature.md" target_type="file" review_mode="document" comments="3">
    Review completed with 3 comments.
  </completed>
</aiki_review>
```

**stderr (file review with --implementation):**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" target="ops/now/feature.md" target_type="file" review_mode="implementation" comments="5">
    Review completed with 5 comments.
  </completed>
</aiki_review>
```

**stderr (task review):**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" target="task" target_type="task" comments="3">
    Review completed with 3 comments.
  </completed>
</aiki_review>
```

---

## Implementation Order

### Phase 1: Target Detection (review side)
- `ReviewTarget` enum with `Task`, `File`, `Session` variants
- Detection logic in `cli/src/commands/review.rs`
- Populate `data.*` fields for template

### Phase 2: Implementation Flag Support
- Add `--implementation` flag to review command
- When `--implementation` is used with a file target, set `data.review_mode = "implementation"`
- Agent will explore codebase based on spec content (no task lookup needed)

### Phase 3: Specialized Review Templates
- Create `.aiki/templates/aiki/review/spec.md` for spec document reviews
- Create `.aiki/templates/aiki/review/implementation.md` for implementation vs spec reviews
- Create `.aiki/templates/aiki/review/code.md` for task code reviews (extract existing logic)
- Create `.aiki/templates/aiki/review/session.md` for session reviews (extract existing logic)

**Template structure:**

Each specialized template should be a complete, standalone template with:
- Frontmatter (`version`, `type: review`)
- Parent task name (e.g., `# Review Spec Document`)
- Subtasks specific to that review type
- Clear instructions that reference inherited variables (`{{data.path}}`, `{{data.task_id}}`, etc.)

### Phase 3b: Composable Review Template
- Update `aiki/review` template to use `{% subtask %}` with conditionals
- Include appropriate specialized template based on `data.file_type` and `data.review_mode`
- Test with both code and file targets, with and without `--implementation`

### Phase 4: Fix for File Targets
- Handle `ReviewTarget::File` in fix.rs (remove error, add standalone task creation)
- Fix template for file-targeted reviews

### Phase 5: Integration & Testing
- Wire target detection into review command
- Pass target data to template renderer
- Update XML output with `target`, `target_type`, and `review_mode` attributes
- Tests for detection, template rendering, implementation flag, and fix flow

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
- **Mixed sources (task + file):** `task:` takes priority per source resolution in `fix-original-task.md`.

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
