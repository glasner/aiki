---
version: 1.0.0
type: review
---

# Review: {{data.scope.name}}

Review the code changes from task `{{data.scope.id}}`.

## Understand the changes

Run these commands to see what was modified:

```bash
aiki task show {{data.scope.id}} --with-source  # Understand intent
aiki task diff {{data.scope.id}}                 # View all code changes
```

## Review for issues and report findings

Examine the changes for:
- **Bugs**: Logic errors, edge cases, correctness
- **Quality**: Error handling, resource leaks, null checks, code clarity
- **Security**: SQL injection, XSS, auth issues, data exposure, crypto misuse
- **Performance**: Inefficient algorithms, unnecessary operations, resource usage
- **User Experience**: UI/UX, accessibility, usability

For each issue found, add a structured comment to THIS task that includes the following custom --data fields:

- file: <path> of file where issue is located
- line: <line number> of file where issue is located
- severity: one of the following: high|medium|low
- category: one of the following: bug|quality|security|performance|ux

**Important:** All issues reported must include this information.

Example Syntax:

```bash
aiki task comment "<description of issue, impact, and suggested fix>" \
  --data file=<path> --data line=<line number> \
  --data severity=<severity> \
  --data category=<category>
```

*Add comments as you find issues - don't wait until the end.*

## Close the review

When done, close this task with a comment of "Review complete (n issues found)"
