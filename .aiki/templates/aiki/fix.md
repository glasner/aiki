---
version: 1.0.0
---

# Followup: Review {{source.id}}

Review task `{{source.id}}` found issues in **{{data.scope.name}}**.

## Instructions

1. List the issues from the review to understand what was found:
   ```bash
   aiki review issue list {{source.id}}
   ```
🛑 Do NOT edit code before reading above.

2. Create a nested subtask for EACH issue found (use your current task ID as parent).
   Issues include severity (`[high]`, `[medium]`, `[low]`) and file locations — use these to name subtasks and find code:
   ```bash
   aiki task add --subtask-of {{id}} "Fix: [high] Missing null check (src/auth.rs:42-50)"
   ```
   Prioritize high-severity issues first.

3. Work through each nested subtask using `--next-session`:
   ```bash
   aiki task run {{id}} --next-session
   ```
   - This automatically starts the next ready subtask and delegates it
   - The subagent will do the work and close the subtask with a summary
   - Repeat `aiki task run --next-session` until all subtasks are completed
   - If a subtask should be skipped, manually close it with `--wont-do --summary`

4. Return to this fix subtask and close it:
   ```bash
   aiki task close {{id}} --summary "<summary of fix>"
   ```

Important: Do NOT return without closing all nested subtasks and this fix subtask.
