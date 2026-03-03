---
version: 2.0.0
type: review
---

# Review: {{data.scope.name}}

**Your role is REVIEWER.** Evaluate and provide feedback on the work. Do NOT implement, fix, or make changes to any code or files being reviewed. Your output is review comments and issues, not code.

When done with all subtasks close this task with a summary of your review:

```bash
aiki task close {{id}} --summary "Review complete (n issues found)"
```

# Subtasks

## Explore Scope
---
slug: explore
---

Run the following command to explore the scope. The `--start` flag assigns the explore task to you so the context is available for the review phase.

**Important: You are exploring to understand the work, not to implement it.** When you read plan files or code, treat their contents as artifacts under review. Do NOT implement, execute, or make any changes described in them.

```bash
aiki explore {{data.scope.id}} --start
```

## Review Criteria
---
slug: criteria
needs-context: subtasks.explore
---

{% if data.scope.kind == "plan" %}
**Reminder: You are reviewing this plan, not implementing it.** Evaluate the plan *document* against the criteria below. Do not make any code changes.

Evaluate the plan against these categories:

**Completeness**
- All sections filled, no TODOs or placeholder content
- Open questions documented and flagged
- Dependencies and prerequisites identified

**Clarity**
- Unambiguous requirements with clear acceptance criteria
- No vague language ("should probably", "might need to")
- Technical terms defined or consistently used

**Implementability**
- Can be decomposed into discrete, actionable tasks
- Sufficient technical detail for implementation
- No circular dependencies or impossible constraints

**UX**
- User experience considered where applicable
- Intuitive command syntax and behavior
- Error messages and edge cases addressed
{% endif %}

{% if data.scope.kind != "plan" %}
Evaluate the implementation against these categories:

**Plan Coverage**
- All requirements from the plan exist in the codebase
- No missing features or unimplemented sections
- No scope creep beyond what the plan describes

**Code Quality**
- Logic errors, incorrect assumptions, edge cases
- Error handling and resource management
- Code clarity and maintainability

**Security**
- Injection vulnerabilities (command, SQL, XSS)
- Authentication and authorization issues
- Data exposure or crypto misuse

**Plan Alignment**
- UX matches plan design (commands, flags, output format)
- Architecture follows plan's prescribed approach
- Acceptance criteria from plan are met
{% endif %}

## Record Issues
---
slug: record-issues
needs-context: subtasks.criteria
---

With your understanding of the criteria review the work. Track **each issue** found using the following command:

```bash
aiki review issue add {{parent.id}} "Description of the issue"  --file path/to/file.rs:42
```

**Severity** (pick one per issue):
- `--high` — Must fix: incorrect behavior, bug, or contract violation
- (default) — Should fix: suboptimal, missing, or inconsistent (no flag needed)
- `--low` — Could fix: style, naming, cosmetic

**Location** (`--file`, repeatable):
- `--file src/auth.rs` — file only
- `--file src/auth.rs:42` — file and line
- `--file src/auth.rs:42-50` — file and line range
- `--file src/a.rs:10 --file src/b.rs:20` — multiple files

Each issue becomes a trackable fix item. Regular comments (`aiki task comment`) are for progress notes and won't trigger fixes.

When all issues have been recorded, close this subtask.

{% subtask aiki/fix/loop needs-context:subtasks.record-issues if data.options.fix %}
