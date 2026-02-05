---
version: 1.0.0
---

# Followup: {{source.name}}

Review task `{{source.id}}` found issues that need to be addressed.

## Instructions

1. Read the review comments to understand what issues were found:
   ```bash
   aiki task show {{source.id}}
   ```
🛑 Do NOT edit code before reading following:

2. Create a subtask for EACH issue found (use your current task ID as parent):
   ```bash
   aiki task add --parent {{id}} "Fix: <brief description of issue>"
   ```

3. Start and work through each subtask, closing as you go:
   ```bash
   aiki task start {{id}}.1
   # ... do the work to fix the issue ...
   aiki task close {{id}}.1 --comment "Fixed by doing X"
   ```
   - **You MUST start each subtask before working on it**
   - Close with `--comment` when fixed
   - Close with `--wont-do --comment` if out of scope or adds too much complexity
   - Continue until all subtasks are completed

4. Return to the parent task and close it:
   ```bash
   aiki task close {{id}} --comment "Fixed all issues"
   ```

Important: Do NOT return without closing all subtasks and the parent task
