---
description: General code quality, functionality, basic security
type: review
---

# Review: {data.scope}

Code review orchestration task.

This task coordinates review steps.

# Subtasks

## Digest code changes

Examine the code changes to understand what was modified.

Commands to use:
- `jj diff --revision {data.scope}` - Show full diff
- `jj show {data.scope}` - Show change description and summary
- `jj log -r {data.scope}` - Show change in log context

Summarize:
- What files were changed
- What functionality was added/modified
- The scope and intent of the changes

## Review code

Review the code changes for functionality, quality, security, and performance.

Focus on:
- **Functionality**: Logic errors, edge cases, correctness
- **Quality**: Error handling, resource leaks, null checks, code clarity
- **Security**: SQL injection, XSS, auth issues, data exposure, crypto misuse
- **Performance**: Inefficient algorithms, unnecessary operations, resource usage

For each issue found, add a comment using `aiki task comment` with:
**File**: <path>:<line>
**Severity**: error|warning|info
**Category**: functionality|quality|security|performance

<description of issue>

**Impact**: <what could go wrong>

**Suggested Fix**:
<how to fix it>

Add comments as you find issues, don't wait until the end.
