---
version: 1.0.0
---

# Followup: Review {{source.id}}

Review task `{{source.id}}` found issues in **{{data.scope.name}}**.

## Instructions

1. Read the review comments to understand what issues were found:
   ```bash
   aiki task show {{source.id}} --with-source
   ```
🛑 Do NOT edit code before reading above.

2. Create a nested subtask for EACH issue found (use your current task ID as parent):
   ```bash
   aiki task add --subtask-of {{id}} "Fix: <brief description of issue>"
   ```

3. Work through each nested subtask using `--next-subtask`:
   ```bash
   aiki task run {{id}} --next-subtask
   ```
   - This automatically starts the next ready subtask and delegates it
   - The subagent will do the work and close the subtask with a summary
   - Repeat `aiki task run --next-subtask` until all subtasks are completed
   - If a subtask should be skipped, manually close it with `--wont-do --summary`

4. Return to this fix subtask and close it:
   ```bash
   aiki task close {{id}} --summary "<summary of fix>"
   ```

Important: Do NOT return without closing all nested subtasks and this fix subtask.
