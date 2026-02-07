# Review and Fix Non-Task Targets

**Date**: 2026-02-06
**Status**: Idea
**Priority**: P3
**Depends on**: `ops/now/fix-original-task.md`

**Related Documents**:
- [Fix Targets Original Task](../now/fix-original-task.md) - Task-targeted fix (prerequisite)
- [Template Conditionals](../now/template-conditionals.md) - Conditional logic for templates (implemented)
- [Task Templates](../done/task-templates.md) - Template system (implemented)

**Prerequisites**:
- Template conditionals (implemented in `cli/src/tasks/templates/conditionals.rs`)
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

### Unified Review Template

The `aiki/review` template uses conditionals to adapt:

```markdown
---
version: 2.0.0
type: review
---

# Review: {{data.target_name}}

Review the target for quality and readiness.

# Subtasks

## Understand what you're reviewing

{% if data.target_type == "file" and data.review_mode == "implementation" %}
You're reviewing the **current codebase implementation** described in spec `{{data.path}}`.

First, read the spec to understand what should be implemented:
1. Read `{{data.path}}` to understand the requirements and design
2. Identify what files/modules/functions should exist based on the spec
3. Explore the current codebase to find the relevant implementation
4. Read the actual code to understand what currently exists

Summarize:
- What the spec describes
- What you found in the codebase
- Whether the current implementation matches the spec
- What's missing or different from the spec
{% elif data.target_type == "file" %}
Read the file at `{{data.path}}` to understand:
- What is this document about?
- What problem does it solve?
- Who is the audience?

Summarize the document's purpose and scope.
{% elif data.target_type == "task" %}
Examine the code changes:
1. `aiki task show {{data.task_id}} --with-source` - Understand intent
2. `aiki task diff {{data.task_id}}` - View all changes

Summarize:
- What files were changed
- What functionality was added/modified
- The scope and intent of the changes
{% else %}
Review all completed work in the session:
1. List closed tasks: `aiki task list --status closed`
2. For each task, run `aiki task diff <id>`

Summarize the overall changes made in this session.
{% endif %}

## Evaluate quality

{% if data.target_type == "file" and data.review_mode == "implementation" %}
Review the **current implementation against the spec**:

**Spec Coverage:**
- Does the current codebase fulfill all requirements from the spec?
- Are there missing features or partial implementations?
- Are there extra features not in the spec (scope creep)?
- Is the implementation complete or still in progress?

**Code Quality:**
- Logic errors and edge cases
- Incorrect assumptions
- Missing error handling
- Resource leaks
- Code clarity and maintainability

**Security:**
- Injection vulnerabilities (SQL, command, XSS)
- Authentication/authorization issues
- Data exposure risks

**Spec Alignment:**
- Does the UX match what was specified?
- Are command syntaxes as designed?
- Are error messages as defined in the spec?
- Are acceptance criteria met?
- Does the implementation follow the spec's architecture?
{% elif data.target_type == "file" %}
Review the document for:

**Completeness:**
- Are all required sections present and filled out?
- Are there any TODOs or placeholder text?
- Are open questions documented?

**Clarity:**
- Are requirements unambiguous?
- Are there clear acceptance criteria?
- Could another engineer implement this without asking questions?

**Implementability:**
- Can this be decomposed into concrete tasks?
- Are technical details sufficient?
- Are dependencies identified?

**UX:**
- Is the user experience considered?
- Are command syntaxes intuitive?
- Are error scenarios and messages defined?
{% else %}
Review the code for:

**Correctness:**
- Logic errors and edge cases
- Incorrect assumptions
- Missing error handling

**Quality:**
- Resource leaks
- Null/undefined checks
- Code clarity and maintainability

**Security:**
- Injection vulnerabilities (SQL, command, XSS)
- Authentication/authorization issues
- Data exposure risks

**Performance:**
- Inefficient algorithms
- Unnecessary operations
- Resource usage concerns
{% endif %}

For each issue found, add a comment describing the problem and suggested fix:

```bash
aiki task comment {{task.parent_id}} "<description of issue and suggested fix>"
```

Add comments as you find issues, don't wait until the end.
```

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

### Phase 3: Adaptive Review Template
- Update `aiki/review` template with conditionals
- Add `review_mode` branches: `document` vs `implementation`
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
- **Non-markdown files** - Review code, config, scripts directly
- **Related file detection** - Auto-include tests/docs when reviewing code
- **Review history tracking** - Quality trends and common issues over time
- **Batch reviews** - Review multiple files with glob patterns
- **Per-directory templates** - Different review criteria per codebase section
- **Spec quality gate hook** - Block `aiki plan` until spec review passes
