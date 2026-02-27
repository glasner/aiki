# Fix

`aiki fix` reads issues from a review task and creates followup tasks to resolve them. It's designed to be piped from `aiki review` for autonomous review-fix loops.

## Usage

```bash
# Fix issues from a review
aiki fix <review-task-id>

# Pipeline: review then fix
aiki review <task-id> | aiki fix

# Hand off fix to calling agent
aiki fix <review-task-id> --start

# Run fix in the background
aiki fix <review-task-id> --async
```

## How It Works

1. **Read review issues** — Fix reads the review task's issue comments (added via `aiki review issue add`). Regular comments are ignored.

2. **Create followup tasks** — A parent fix task is created with a subtask for each issue found. Each subtask contains enough context for an agent to resolve the issue independently.

3. **Execute fixes** — Subtasks are run sequentially using `aiki task run --next-session`. The agent that wrote the original code is assigned to fix it.

4. **Link back** — Fix tasks emit `remediates` links back to the review and `fixes` links to the reviewed targets, creating a traceable remediation chain.

If no issues are found, fix outputs "Approved" and exits successfully — the review passed.

## Quality Loop

When review and fix are chained (via `--fix` flag or piping), Aiki runs a quality loop:

```
Review ──▶ Fix ──▶ Re-review ──▶ Fix ──▶ ... ──▶ Approved
```

The loop continues until the re-review finds no issues or a maximum of 10 iterations is reached. This ensures fixes don't introduce new problems.

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
| `--start` | Assign fix to calling agent (hand off) |
| `--once` | Single-pass fix (no quality loop) |
| `--autorun` | Auto-start fix when review closes |
| `--template <name>` | Use a custom fix template (default: `aiki/fix`) |
| `--agent <type>` | Override fix agent |

## Stdin Piping

`aiki fix` reads a task ID from stdin when no argument is provided, enabling pipeline composition:

```bash
aiki review <task-id> | aiki fix
```

The review command outputs its task ID to stdout when piped, which fix reads as input.
