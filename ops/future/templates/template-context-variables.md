# Template Variables from Context

**Date**: 2026-01-21
**Status**: Future Enhancement
**Related**: [Task Templates](../../now/task-templates.md)

---

## Overview

Automatically populate template variables from repository context (git/jj status, file statistics, etc.).

## Syntax

File: `.aiki/tasks/contextual-review.md`

```markdown
# Review by {assignee}

Reviewing changes in {data.scope}

**Change Summary:**
- Files changed: {context.files_count}
- Lines added: {context.lines_added}
- Lines removed: {context.lines_removed}
- Change ID: {context.change_id}

# Subtasks

## Review changes

Analyze the {context.files_count} modified files.

Focus areas:
- {context.primary_language} code quality
- Changes in {context.modified_dirs}

## Test coverage

Ensure tests cover the {context.lines_added} lines added.
```

## Context Variables

### JJ/Git Status

- `{context.change_id}` - Current JJ change ID
- `{context.files_count}` - Number of modified files
- `{context.modified_files}` - List of modified files (comma-separated)
- `{context.modified_dirs}` - Directories containing changes
- `{context.lines_added}` - Total lines added
- `{context.lines_removed}` - Total lines removed

### File Analysis

- `{context.primary_language}` - Most common language in changes (rust, typescript, etc.)
- `{context.languages}` - All languages in changes (rust, javascript, markdown)
- `{context.has_tests}` - Boolean: changes include test files

### Repository Info

- `{context.repo_name}` - Repository name
- `{context.repo_path}` - Absolute path to repository
- `{context.branch}` - Current Git branch (if colocated)

## Usage

```bash
# Template auto-detects context
aiki task add --template myorg/contextual-review

# Context variables populated automatically:
# - context.files_count: 5
# - context.lines_added: 234
# - context.primary_language: rust
```

## Benefits

- **Automatic context**: No need to manually specify change details
- **Richer prompts**: Agents get more context about what changed
- **Smart workflows**: Templates adapt based on actual changes

## Considerations

- **Performance**: Computing context should be fast (< 100ms)
- **Caching**: Cache context during task creation to avoid recomputation
- **Privacy**: Be careful with sensitive repository information
- **Error handling**: What if context can't be determined? (e.g., no changes)

## Interaction with Data Variables

Command-line `--data` flags override context variables:

```bash
# Override auto-detected file count
aiki task add --template myorg/review \
  --data files_count="3"
# Uses data.files_count, not context.files_count
```

Priority: `--data` > `context.*` > template defaults
