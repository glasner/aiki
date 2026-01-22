# Conditional Subtasks

**Date**: 2026-01-21
**Status**: Future Enhancement
**Related**: [Task Templates](../../now/task-templates.md)

---

## Overview

Allow subtasks to be conditionally included based on repository context (file existence, git status, etc.).

## Syntax

File: `.aiki/templates/myorg/multi-language-test.md`

```markdown
# Run Tests

Run tests for the appropriate language.

# Subtasks

## Run Rust tests
<!-- condition: file_exists("Cargo.toml") -->

```bash
cargo test
```

## Run JavaScript tests
<!-- condition: file_exists("package.json") -->

```bash
npm test
```

## Run Python tests
<!-- condition: file_exists("setup.py") || file_exists("pyproject.toml") -->

```bash
pytest
```
```

## Usage

```bash
# Template automatically detects language and creates appropriate subtasks
aiki task add --template myorg/multi-language-test
```

## Supported Conditions

### File System

- `file_exists(path)` - Check if file exists
- `dir_exists(path)` - Check if directory exists
- `glob_matches(pattern)` - Check if glob pattern matches files

### Git/JJ Status

- `has_changes()` - Working copy has changes
- `staged_files()` - Files in Git staging area
- `file_modified(path)` - Specific file was modified

### Data Variables

- `data.key == "value"` - Compare data variable
- `data.key != null` - Check if data variable is set

### Boolean Logic

- `A && B` - Both conditions true
- `A || B` - Either condition true
- `!A` - Condition is false

## Implementation Considerations

- **Evaluation timing**: When template is loaded (task creation time)
- **Error handling**: What if condition evaluation fails?
- **Security**: Restrict to safe operations (no arbitrary code execution)
- **Performance**: Cache file system checks during template evaluation

## Benefits

- **Smart workflows**: Adapt to project structure automatically
- **Language-agnostic**: Single template works across multiple languages
- **Reduced noise**: Only show relevant subtasks to agents

## Alternative: Multiple Templates

Instead of conditionals, users could create separate templates:
- `myorg/test-rust.md`
- `myorg/test-javascript.md`
- `myorg/test-python.md`

**Trade-off**: More files to maintain vs. more complex template syntax.
