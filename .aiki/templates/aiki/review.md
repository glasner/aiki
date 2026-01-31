---
version: 1.0.0
type: review
---

# Review: Work from {scope.name} (task:{scope.id})

This task coordinates review steps as subtasks.

# Subtasks

## Digest code changes

Examine the code changes to understand what was modified.

Commands to use:
1. `aiki task show {scope} --with-source` - Understand the task and its intent
2. `aiki task diff {scope}` - View all code changes for the task

The `--with-source` flag expands source references to show why the task exists.
The `diff` command shows the net result of all task work (not individual changes).

## Review code

Review the code changes for bugs, quality, security, performance, and user experience.

If you haven't already, run:
- `aiki task diff {scope}` - View all code changes for the task

Focus on:
- **Bugs**: Logic errors, edge cases, correctness
- **Quality**: Error handling, resource leaks, null checks, code clarity
- **Security**: SQL injection, XSS, auth issues, data exposure, crypto misuse
- **Performance**: Inefficient algorithms, unnecessary operations, resource usage
- **User Experience**: UI/UX, accessibility, usability

If you find any issues, create a followup task with `aiki task` and leave a comment with structured data for each finding:

`aiki task comment --id <parent.id> \
  --data file=<path> --data line=<line> \
  --data severity=high|medium|low \
  --data category=bug|quality|security|performance|ux \
  "<description of issue, impact, and suggested fix>"`

Add comments as you find issues, don't wait until the end.

Your final response should just be:

<followup_task id={id}>
