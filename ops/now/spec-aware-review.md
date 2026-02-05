# Spec-Aware Review

**Date**: 2026-02-04
**Status**: Ready
**Purpose**: Make `aiki review` aware of what it's reviewing (code vs specs) with a single adaptive template

**Related Documents**:
- [Review and Fix Commands](review-and-fix.md) - Base review system (implemented)
- [Template Conditionals](template-conditionals.md) - Conditional logic for templates (prerequisite)
- [Task Templates](../done/task-templates.md) - Template system (implemented)

**Prerequisites**:
- Template conditionals ✓ (implemented in `cli/src/tasks/templates/conditionals.rs`)

---

## Executive Summary

Extend `aiki review` to detect what it's reviewing based on the target argument:

| Target | Review Type | Default Criteria |
|--------|-------------|------------------|
| `<task-id>` | Code changes | Bugs, quality, security, performance |
| `<path.md>` | Document/spec | Completeness, clarity, implementability, UX |
| _(none)_ | Session | All closed tasks in session |

**One command, one template** - the template uses conditionals to adapt.

---

## User Experience

### Command Syntax

```bash
aiki review [<target>] [options]
```

**Target detection:**
```bash
# Review code changes for a task
aiki review xqrmnpst

# Review a spec/document file
aiki review ops/now/feature.md

# Review all closed tasks in session (default)
aiki review
```

**Options (unchanged from current):**
- `--template <name>` - Override template (default: `aiki/review`)
- `--agent <name>` - Reviewer agent (default: `codex`)
- `--async` - Run asynchronously
- `--start` - Calling agent takes over

### Examples

```bash
# Review a spec before planning
aiki review ops/now/add-auth.md
# → Creates review task with document criteria

# Review code after implementation
aiki review xqrmnpst
# → Creates review task with code criteria

# Full workflow
aiki review ops/now/feature.md | aiki fix
aiki build ops/now/feature.md | aiki review | aiki fix
```

---

## How It Works

### Target Detection

```rust
fn detect_target(arg: Option<&str>) -> ReviewTarget {
    match arg {
        None => ReviewTarget::Session,

        Some(s) if s.ends_with(".md") && Path::new(s).exists() => {
            ReviewTarget::File {
                path: s.into(),
                file_type: "spec",  // Could detect from content later
            }
        }

        Some(s) if is_task_id(s) => {
            ReviewTarget::Task { id: s.into() }
        }

        Some(s) if Path::new(s).exists() => {
            // Non-markdown file - could support in future
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

**For file review:**
```yaml
data:
  target_type: file
  target_name: "ops/now/feature.md"
  path: ops/now/feature.md
  file_type: spec
```

**For session review:**
```yaml
data:
  target_type: session
  target_name: "session changes"
  task_ids: [abc123, def456]  # All closed tasks
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

{% if data.target_type == "file" %}
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

{% if data.target_type == "file" %}
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

For each issue found, add a comment:

```bash
aiki task comment {{task.parent_id}} \
  --data severity=high|medium|low \
  --data category=<category> \
  "<description of issue and suggested fix>"
```

{% if data.target_type == "file" %}
Categories: completeness, clarity, implementability, ux
{% else %}
Categories: correctness, quality, security, performance
{% endif %}

Add comments as you find issues, don't wait until the end.
```

---

## Review Criteria by Target Type

### Code Review (task/session)

| Category | What to Check |
|----------|---------------|
| **Correctness** | Logic errors, edge cases, incorrect assumptions |
| **Quality** | Error handling, resource leaks, code clarity |
| **Security** | Injection, auth, data exposure, crypto misuse |
| **Performance** | Algorithms, unnecessary operations, resources |

### Document Review (file)

| Category | What to Check |
|----------|---------------|
| **Completeness** | All sections filled, no TODOs, open questions documented |
| **Clarity** | Unambiguous requirements, clear acceptance criteria |
| **Implementability** | Decomposable into tasks, sufficient technical detail |
| **UX** | User experience considered, intuitive design |

---

## Output Format

Output format is unchanged from current `aiki review`:

**stdout (piped):**
```
xqrmnpst
```

**stderr:**
```xml
<aiki_review cmd="review" status="ok">
  <completed task_id="xqrmnpst" target="ops/now/feature.md" target_type="file" comments="3">
    Review completed with 3 comments.
  </completed>
</aiki_review>
```

The `target` and `target_type` attributes are added to provide context.

---

## Use Cases

### 1. Pre-Plan Spec Review

```bash
# Review spec before creating implementation plan
aiki review ops/now/feature.md

# If issues found, fix interactively
aiki spec ops/now/feature.md

# Then plan
aiki plan ops/now/feature.md
```

### 2. Post-Build Code Review

```bash
# Build creates implementation task
aiki build ops/now/feature.md

# Review the implementation
aiki review <impl-task-id> | aiki fix
```

### 3. Full Pipeline

```bash
# Spec → Review → Plan → Build → Review → Fix
aiki spec "Add authentication"
aiki review ops/now/add-authentication.md | aiki fix
aiki build ops/now/add-authentication.md | aiki review | aiki fix
```

### 4. Spec Quality Gate (Hook)

```yaml
# .aiki/hooks/spec-quality.yml
name: "spec-quality"
version: "1"

plan.started:
  - if: $event.spec_path
    then:
      - run: aiki review $event.spec_path
      - if: $last.comments > 0
        then:
          - block: |
              Spec review found ${last.comments} issue(s).
              Run `aiki spec ${event.spec_path}` to address findings.
```

---

## Implementation Plan

### Phase 1: Target Detection

**Deliverables:**
- `ReviewTarget` enum with `Task`, `File`, `Session` variants
- Detection logic based on argument
- Populate `data.*` fields for template

**Files:**
- `cli/src/commands/review.rs` - Add target detection

```rust
pub enum ReviewTarget {
    Task { id: String },
    File { path: PathBuf, file_type: String },
    Session { task_ids: Vec<String> },
}

impl ReviewTarget {
    pub fn to_template_data(&self) -> HashMap<String, Value> {
        // Convert to data.* fields for template
    }
}
```

### Phase 2: Template Update

**Deliverables:**
- Update `aiki/review` template with conditionals
- Test with both code and file targets

**Files:**
- `.aiki/templates/aiki/review.md` - Add conditional sections

**Prerequisite:** Template conditionals ✓ (implemented).

### Phase 3: Integration

**Deliverables:**
- Wire target detection into review command
- Pass target data to template renderer
- Update XML output to include target info

**Files:**
- `cli/src/commands/review.rs` - Integration

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| File doesn't exist | Error: `"File not found: <path>"` |
| File exists but not .md | Error: `"File review only supports .md files currently"` |
| Task ID doesn't exist | Error: `"Task not found: <id>"` |
| Empty session (no closed tasks) | Success with "nothing to review" message |

---

## Future Enhancements

See [File Review Improvements](../future/file-review-improvements.md) for detailed future ideas, including:

- **Smart file type detection** - Auto-detect specs vs docs vs readmes from path/content
- **Non-markdown files** - Review code, config, scripts directly
- **Related file detection** - Auto-include tests/docs when reviewing code
- **Review history tracking** - See quality trends and common issues over time
- **Batch reviews** - Review multiple files with glob patterns
- **Interactive mode** - Agent asks clarifying questions during review
- **Per-directory templates** - Different review criteria per codebase section
- **AI-suggested fixes** - Concrete improvement recommendations with diffs


---

## Summary

**Spec-aware review** extends `aiki review` to handle both code and documents:

| Aspect | Before | After |
|--------|--------|-------|
| Command | `aiki review <task-id>` | `aiki review <target>` |
| Targets | Tasks only | Tasks, files, session |
| Templates | Separate per type | Single adaptive template |
| Detection | Manual | Automatic from argument |

**Key benefits:**
- **One command** for all review types
- **One template** that adapts via conditionals
- **Simpler UX** - no need to remember subcommands
- **Extensible** - add new target types without new commands

**Dependencies:**
- Template conditionals ✓ (implemented)
