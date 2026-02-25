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
   aiki task add --parent {{id}} "Fix: <brief description of issue>"
   ```

3. Start and work through each nested subtask, closing as you go:
   ```bash
   aiki task start {{id}}.1
   # ... do the work to fix the issue ...
   aiki task close {{id}}.1 --summary "Fixed by doing X"
   ```
   - **You MUST start each subtask before working on it**
   - Close with `--summary` when fixed
   - Close with `--wont-do --summary` if out of scope or adds too much complexity
   - Continue until all nested subtasks are completed

4. Return to this fix subtask and close it:
   ```bash
   aiki task close {{id}} --summary "<summary of fix>"
   ```

Important: Do NOT return without closing all nested subtasks and this fix subtask.
