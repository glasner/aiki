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

3. Work through each subtask, closing as you go:
   - Close with `--comment` when fixed
   - Close with `--wont-do --comment` if out of scope or adds too much complexity
