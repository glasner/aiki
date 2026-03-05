---
draft: false
---

# Fix Review Template: Issues Found But Never Recorded

**Date**: 2026-03-04
**Status**: Draft
**Purpose**: Restructure review templates so agents record issues as they find them, and add a system-level safety net to catch missed recordings.

---

## Executive Summary

The review template separates "evaluate" and "record" into two subtasks. Agents complete evaluation, then skip the recording step — closing the parent directly and cascade-closing the unfinished record-issues subtask. The summary says "4 issues found" but `aiki review issue list` returns zero.

This is a structural problem, not a prompt-tuning problem. The fix has three parts:

1. **Split into scope-specific templates**: `aiki/review/plan.md` and `aiki/review/task.md` — following the same pattern as `aiki/explore/{kind}`. Each scope gets a template shaped to its needs.
2. **Merge evaluation and recording**: Both templates use a 2-subtask pipeline (explore → review-and-record) instead of 3. The agent records each issue the moment it finds one.
3. **Add a system guard**: When closing a review task, if the summary mentions issues but `issue_count == 0`, reject the close with an actionable error.

---

## Problem

### What happens today

The review template (`.aiki/templates/aiki/review.md`) creates three subtasks in a `needs-context` chain:

```
Explore Scope → Review Criteria → Record Issues
```

The agent completes Explore and Criteria, then closes the **parent** directly with a summary like "Review complete (4 issues found)." Cascade close kills Record Issues as "Closed with parent."

### Root cause

Separating "evaluate" from "record" fights how agents work. When an agent evaluates code or a plan and finds a problem, it wants to note it immediately — not queue it for a later recording phase. The 3-step pipeline creates a gap the agent shortcuts through.

This isn't a plan-review-only problem. Code reviews happen to work because the agent often records issues during criteria evaluation anyway (the file:line context is fresh). But the template structure makes this accidental, not guaranteed.

### Evidence from task `qquvknpywvolws`

- Summary: "Review complete (4 issues found: 2 high, 2 medium)"
- `aiki review issue list`: "No issues"
- Record Issues subtask: "Closed with parent" (never executed)

---

## Part 1: Split into Scope-Specific Templates

### Design: Route to `aiki/review/{kind}`

Follow the pattern established by explore (`aiki/explore/{kind}`). Change the default template from a single `aiki/review` to scope-specific `aiki/review/{kind}`:

| Scope | Template |
|-------|----------|
| Plan (`aiki review file.md`) | `aiki/review/plan` |
| Task/Session (`aiki review <task-id>` or `aiki review`) | `aiki/review/task` |
| Code (`aiki review file.md --code`) | `aiki/review/code` |

### Why split

We have three types of reviews with different needs:

- **Plan reviews** examine a single markdown document for completeness, clarity, and implementability. The "explore" step reads one file. The criteria are about document quality.
- **Task/session reviews** examine multi-file diffs against a plan. The "explore" step reads task changes or session diffs. The criteria are about correctness, security, and plan coverage.
- **Code reviews** (using `--code` flag) examine specific code files for quality and standards, without a task/plan context.

Three focused templates are cleaner than one template with conditionals.

### Code change

In `cli/src/commands/review.rs`, `create_review()`:

```rust
// Before:
let template = params.template.as_deref().unwrap_or("aiki/review");

// After (matches explore pattern):
let default_template = format!("aiki/review/{}", scope.kind.as_str());
let template = params.template.as_deref().unwrap_or(&default_template);
```


The old `aiki/review.md` template will be deleted. Users with custom `--template` overrides can continue using them.
---

## Part 2: Template Content — Two Subtasks, Not Three

All three templates (`review/plan.md`, `review/task.md`, `review/code.md`) use a 2-subtask pipeline:

```
Explore Scope → Review & Record Issues
```

### `aiki/review/plan.md`

```markdown
---
version: 3.0.0
type: review
---

# Review: {{data.scope.name}}

**Your role is REVIEWER.** Evaluate the plan document and provide feedback. Do NOT implement, fix, or make changes to any code or files. Your output is review issues, not code.

When all subtasks are closed, close this task with a summary:

\```bash
aiki task close {{id}} --summary "Review complete (N issues: X high, Y medium, Z low)"
\```

# Subtasks

## Explore Scope
---
slug: explore
---

Run the following command to explore the plan. The `--start` flag assigns the explore task to you so the context is available for the review phase.

**Important: You are exploring to understand the plan, not to implement it.** Treat the plan as an artifact under review. Do NOT implement, execute, or make any changes described in it.

\```bash
aiki explore {{data.scope.id}} --start
\```

## Review & Record Issues
---
slug: criteria
needs-context: subtasks.explore
---

**You are reviewing this plan, not implementing it.** Evaluate the plan *document* against the criteria below. Do not make any code changes.

**As you find each issue, record it immediately** before moving to the next criterion.

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

### How to record

For **each** issue, run:

\```bash
aiki review issue add {{parent.id}} "Description" --file {{data.scope.id}}:LINE
\```

- `--high` — Must fix: missing section, contradictory requirement, unimplementable constraint
- (default) — Should fix: vague language, missing edge case, unclear acceptance criteria
- `--low` — Could fix: wording, formatting, structure

Point `--file` at the plan file and the line/section where the issue is.

Each recorded issue becomes a trackable fix item. Regular comments (`aiki task comment`) are for progress notes and won't trigger fixes.

When done, close this subtask — **do not close the parent directly**.
```

### `aiki/review/task.md`

```markdown
---
version: 3.0.0
type: review
---

# Review: {{data.scope.name}}

**Your role is REVIEWER.** Evaluate and provide feedback on the implementation. Do NOT fix or make changes to any code being reviewed. Your output is review issues, not code.

When all subtasks are closed, close this task with a summary:

\```bash
aiki task close {{id}} --summary "Review complete (N issues: X high, Y medium, Z low)"
\```

# Subtasks

## Explore Scope
---
slug: explore
---

Run the following command to explore the task's changes. The `--start` flag assigns the explore task to you so the context is available for the review phase.

**Important: You are exploring to understand the changes, not to modify them.**

\```bash
aiki explore {{data.scope.id}} --start
\```

## Review & Record Issues
---
slug: criteria
needs-context: subtasks.explore
---

Evaluate the implementation against the criteria below. **As you find each issue, record it immediately** before moving to the next criterion.

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

### How to record

For **each** issue, run:

\```bash
aiki review issue add {{parent.id}} "Description" --file path/to/file.rs:42
\```

- `--high` — Must fix: incorrect behavior, bug, or contract violation
- (default) — Should fix: suboptimal, missing, or inconsistent (no flag needed)
- `--low` — Could fix: style, naming, cosmetic

**Location** (`--file`, repeatable):
- `--file src/auth.rs` — file only
- `--file src/auth.rs:42` — file and line
- `--file src/auth.rs:42-50` — file and line range
- `--file src/a.rs:10 --file src/b.rs:20` — multiple files

Each recorded issue becomes a trackable fix item. Regular comments (`aiki task comment`) are for progress notes and won't trigger fixes.

When done, close this subtask — **do not close the parent directly**.
```


---

## Part 3: System Guard on Review Close

### Design: Reject close when issues are claimed but not recorded

The close handler in `cli/src/commands/task.rs` already computes `issue_count` for review tasks (line ~2890). Add a validation step: if the close summary contains a number-of-issues claim but `issue_count == 0`, reject the close.

### Implementation

In `cli/src/commands/task.rs`, after computing `issue_count` for review tasks:

```rust
// Guard: reject review close if summary claims issues but none were recorded
if issue_count == 0 {
    if let Some(ref summary) = summary_text {
        if review_summary_claims_issues(summary) {
            return Err(AikiError::ReviewIssuesMissing(format!(
                "Summary says issues were found but none were recorded.\n\
                 Use `aiki review issue add {}` to record each issue, then close again.",
                crate::tasks::md::short_id(id)
            )));
        }
    }
}
```

```rust
/// Check if a review summary claims issues were found
fn review_summary_claims_issues(summary: &str) -> bool {
    let re = regex::Regex::new(r"(\d+)\s*(issues?|high|medium|low)").unwrap();
    for cap in re.captures_iter(summary) {
        if let Ok(n) = cap[1].parse::<u32>() {
            if n > 0 {
                return true;
            }
        }
    }
    false
}
```

### Edge cases

1. **Clean review (0 issues)**: "0 issues found" → returns false → close proceeds
2. **No count in summary**: "looks good" → no match → close proceeds
3. **Cascade close**: Guard only applies to explicit closes, not cascade
4. **Won't-do close**: Bypasses guard — reviewer is declining, not reporting

---

## Files to Change

| File | Change |
|------|--------|
| `.aiki/templates/aiki/review/plan.md` | **New**: plan review template (2 subtasks, plan-specific criteria and `--file` examples) |
| `.aiki/templates/aiki/review/task.md` | **New**: task/session review template (2 subtasks, handles both task and session scopes) |
| `.aiki/templates/aiki/review/code.md` | **New**: code review template (like task but explore uses `--code` flag) |
| `.aiki/templates/aiki/review.md` | **Delete**: replaced by scope-specific templates |
| `cli/src/commands/review.rs` | Route default template to `aiki/review/{kind}` instead of `aiki/review` |
| `cli/src/commands/task.rs` | Add `review_summary_claims_issues()` guard in close handler |
| `cli/src/error.rs` | Add `ReviewIssuesMissing(String)` variant to `AikiError` |

---

## Alternatives Considered

### Single template with `{% if %}` conditionals

The previous version of this plan. Works but:
- Template becomes hard to read with nested conditionals
- Can't evolve plan/code review criteria independently
- Doesn't match the established `aiki/explore/{kind}` pattern
- Testing requires checking every conditional branch in one file

Split templates are cleaner and follow existing conventions.

### Keep 3 subtasks, add stronger "don't close parent" wording

**Rejected**: Prompt-tuning is unreliable. The agent follows the instruction that feels most salient — and "close with summary" in the parent is always more salient than "don't close yet" in a subtask it hasn't started.

### Only fix plan reviews, leave code reviews as-is

**Rejected**: The 3-step structure is fragile for code reviews too — it just fails silently. Merging criteria + recording is better for all review types.

---

## Test Plan

1. **Plan review records issues**: Run `aiki review <plan>.md`, verify `aiki review issue list` shows recorded issues
2. **Task review records issues**: Run `aiki review <task-id>`, verify issues are recorded inline during evaluation
3. **Template routing**: Verify `aiki review file.md` uses `aiki/review/plan`, `aiki review <task-id>` and `aiki review` use `aiki/review/task`, `aiki review file.md --code` uses `aiki/review/code`
4. **Custom override**: `aiki review file.md --template my/custom` still works
5. **Guard catches missing issues**: Close a review with `--summary "2 issues found"` when no issues recorded → expect error
6. **Clean review closes normally**: Close with `--summary "0 issues found"` → succeeds
7. **Won't-do bypasses guard**: Close with `--wont-do` → succeeds regardless
8. **Fix flow**: After review with recorded issues, `aiki fix <review-id>` creates followup tasks

### `aiki/review/code.md`

Same structure as `task.md`, but the explore subtask uses the `--code` flag:

```bash
aiki explore {{data.scope.id}} --code --start
```

This is for reviewing code files directly (using `aiki review file.rs --code`) without task or plan context. The review criteria focus on code quality, standards, and best practices rather than plan alignment.
