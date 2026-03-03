# Fix

`aiki fix` reads issues from a review task and creates followup tasks to resolve them. It uses a pipeline architecture (plan → decompose → loop) with an optional Rust-driven quality loop.

## Usage

```bash
# Fix issues from a review
aiki fix <review-task-id>

# Pipeline: review then fix
aiki review <task-id> | aiki fix

# Single-pass fix (no quality loop)
aiki fix <review-task-id> --once

# Run fix in the background
aiki fix <review-task-id> --async
```

## How It Works

1. **Read review issues** — Fix reads the review task's issue comments (added via `aiki review issue add`). Regular comments are ignored.

2. **Plan** — A fix plan is created from the `aiki/plan/fix` template, describing what needs to change to address each issue.

3. **Decompose** — The plan is decomposed into subtasks, one per issue, each with enough context for an agent to resolve independently.

4. **Loop** — Subtasks are executed via `aiki/loop`, which runs each fix through an agent session.

5. **Link back** — Fix tasks emit `remediates` links back to the review and `fixes` links to the reviewed targets, creating a traceable remediation chain.

If no issues are found, fix outputs "Approved" and exits successfully — the review passed.

## Quality Loop

The Rust-driven quality loop chains review and fix automatically:

```
Review ──▶ Fix ──▶ Re-review ──▶ Fix ──▶ ... ──▶ Approved
```

The loop continues until the re-review finds no issues or a maximum of 10 iterations is reached. This ensures fixes don't introduce new problems. Use `--once` to disable the loop and run a single fix pass.

## Fix Targets

Where the fix task gets attached depends on the review scope:

| Review scope | Fix behavior |
|-------------|--------------|
| **Task** | Fix added as subtask of the original task (reopened if closed) |
| **Plan / Code** | Standalone fix task (file-targeted) |
| **Conflict** | Merge-conflict resolution task |

## Options

| Flag | Effect |
|------|--------|
| `--async` | Run in the background |
| `--once` | Single-pass fix (no quality loop) |
| `--autorun` | Auto-start fix when review closes |
| `--template <name>` | Use a custom fix template |
| `--agent <type>` | Override fix agent |

## Stdin Piping

`aiki fix` reads a task ID from stdin when no argument is provided, enabling pipeline composition:

```bash
aiki review <task-id> | aiki fix
```

The review command outputs its task ID to stdout when piped, which fix reads as input.
