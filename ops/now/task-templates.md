# Task Templates: Generic Workflow System

**Date**: 2026-01-20
**Status**: Design Proposal
**Purpose**: Replace review-specific `--prompt` flag with generic `--template` system

**Related Documents**:
- [Code Review System: Task-Based Design](code-review-task-native.md)
- [Task Execution: aiki task run](../done/run-task.md)

---

## Executive Summary

This design replaces the review-specific `--prompt` flag with a generic `--template` system that can define **any task workflow** for agents, not just code reviews.

**Key Changes:**
1. **`--prompt` → `--template`** - More generic naming
2. **Prompts inside tasks** - All instructions live in task templates
3. **Reusable workflows** - Templates define parent + subtask structure
4. **Not just reviews** - Can create any custom agent workflow

**User Experience:**
```bash
# Review with template (new syntax)
aiki review --template security

# Same power as old --prompt system
# But now templates can define any workflow:
aiki task create --template refactor-cleanup
aiki task create --template integration-test
aiki task create --template documentation-audit
```

---

## Table of Contents

1. [Motivation](#motivation)
2. [Design Overview](#design-overview)
3. [Template Format](#template-format)
4. [Built-in Templates](#built-in-templates)
5. [Custom Templates](#custom-templates)
6. [CLI Changes](#cli-changes)
7. [Implementation](#implementation)
8. [Migration Path](#migration-path)

---

## Motivation

### Current Limitations of `--prompt`

The current `--prompt` flag is tied specifically to code reviews:

```bash
# Only works with aiki review
aiki review --prompt security
aiki review --prompt performance
```

**Problems:**
1. **Review-specific** - Can't be used for other agent workflows
2. **Prompts scattered** - Instructions duplicated across different review types
3. **Not reusable** - Can't create custom workflows beyond reviews
4. **Naming confusion** - "Prompt" suggests one-off instructions, not structured workflows

### Vision: Generic Task Workflows

With templates, we can define **any repeatable agent workflow**:

```bash
# Code reviews
aiki review --template security

# Refactoring workflows
aiki task create --template refactor-cleanup

# Testing workflows
aiki task create --template integration-test

# Documentation workflows
aiki task create --template api-docs
```

**Benefits:**
1. **Generic** - Works for any task type, not just reviews
2. **Reusable** - Define once, use everywhere
3. **Structured** - Templates define parent + subtasks + instructions
4. **Composable** - Templates can reference other templates
5. **Customizable** - Users create their own workflows

---

## Design Overview

### What is a Task Template?

A **task template** is a YAML file that defines:

1. **Task structure** - Parent task + subtasks
2. **Instructions** - What the agent should do in each subtask
3. **Metadata** - Type, assignee, scope patterns
4. **Variables** - Placeholders filled at runtime

### Template Structure

Templates are markdown files with YAML frontmatter:

```markdown
<!-- .aiki/templates/aiki/review.md -->
---
description: General code quality, functionality, basic security
type: review
assignee: codex
---

# Review: {scope}

Code review orchestration task.

This task coordinates review steps.

# Subtasks

## Digest code changes

Examine the code changes to understand what was modified.

Commands to use:
- `jj diff --revision {scope}` - Show full diff
- `jj show {scope}` - Show change description and summary
- `jj log -r {scope}` - Show change in log context

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
```

**Template Structure:**
- **Filename** - Template name inferred from filename (e.g., `review.md` → name is `review`)
- **Frontmatter** (YAML between `---`) - Optional metadata like type, assignee, description (no need for `name` field!)
- **First `# Heading`** - Task name, supports variables like `{scope}`
- **Content before `# Subtasks`** - Parent task instructions
- **`# Subtasks`** - Marks beginning of subtasks section
- **`## Subheadings`** under `# Subtasks` - Each h2 defines a subtask (heading text = subtask name)
- **Content under each `##`** - Subtask instructions

### Template Variables

Templates support runtime variable substitution. Variables can be:
1. **Built-in** - Provided automatically by commands
2. **Custom** - Passed via CLI flags

#### Built-in Variables (aiki review)

| Variable | Description | Example |
|----------|-------------|---------|
| `{scope}` | Review scope (change ID, revset) | `@`, `xqrmnpst`, `main..@` |
| `{files}` | Comma-separated file list | `src/auth.ts, src/crypto.rs` |
| `{reviewer}` | Assigned reviewer agent | `codex`, `claude-code` |
| `{timestamp}` | Current timestamp | `2026-01-20T10:00:00Z` |

**Example:**
```markdown
# Review: {scope}

Review changes in {scope} (files: {files})
```

When used with `aiki review @`, this becomes:
```markdown
# Review: @

Review changes in @ (files: src/auth.ts, src/middleware.ts)
```

#### Custom Variables (aiki task create)

When creating tasks from templates, you can pass custom variables:

```bash
# Template at .aiki/templates/custom/feature.md uses {feature_name} and {module}
aiki task create --template feature --var feature_name="user auth" --var module="auth"
```

**Template:**
```markdown
# Implement {feature_name}

Add {feature_name} functionality to the {module} module.

# Subtasks

## Design {feature_name}

Create design doc for {feature_name}.

## Implement in {module}

Write code in {module} module.
```

**Result:**
```markdown
# Implement user auth

Add user auth functionality to the auth module.

# Subtasks

## Design user auth

Create design doc for user auth.

## Implement in auth

Write code in auth module.
```

---

## Template Format

### Full Template Specification

```markdown
---
# Frontmatter - Metadata (all optional, inferred from filename/defaults)
version: 1                    # Optional: Schema version (default: 1)
description: Human-readable description  # Optional

# Task defaults
type: review                  # Optional: Task type (default: generic)
assignee: codex               # Optional: Default assignee (can be overridden)
priority: p2                  # Optional: Default priority

# Optional: Custom metadata
metadata:
  custom_field: value
---

# Task name with {variables}

What the agent should know about this workflow.

Can include context, goals, constraints.

# Subtasks

## Subtask name

What the agent should do in this step.

## Another subtask

Next step instructions.
```

### Minimal Template

The simplest possible template (at `.aiki/templates/custom/minimal.md`):

```markdown
# Minimal task

Do something.

# Subtasks

## Do the work

Instructions here.
```

**Note**: No frontmatter needed! Template name is inferred from filename (`minimal.md` → `minimal`).

### Template with Variables

File: `.aiki/templates/custom/custom-review.md`

```markdown
# Review {scope} by {reviewer}

Reviewing: {scope}
Reviewer: {reviewer}
Files: {files}

Analyze the changes and provide feedback.

# Subtasks

## Analyze {scope}

Run: jj diff --revision {scope}
```

---

## Built-in Templates

Aiki ships with these built-in templates:

### 1. `review` - General Code Review (Default)

**Location**: `.aiki/templates/aiki/review.md`

**Purpose**: General code quality, functionality, basic security, and performance

**Subtasks:**
1. **Digest code changes** - Examine what was modified
2. **Review code** - Check functionality, quality, security, performance

**Usage:**
```bash
aiki review                    # Uses default review template
aiki review --template review  # Explicit
```

### Future Templates

Additional specialized templates can be added:

- **`review-security.md`** - Deep security analysis (SQL injection, XSS, auth, crypto)
- **`review-performance.md`** - Performance bottlenecks, algorithm efficiency  
- **`review-style.md`** - Code style, naming conventions, documentation

### Template Location Strategy

```
.aiki/
├── templates/
│   ├── aiki/                  # Shipped with aiki (read-only)
│   │   └── review.md          # Default review template
│   └── custom/                # User-defined templates
│       ├── refactor-cleanup.md
│       ├── api-docs.md
│       └── integration-test.md
```

**Resolution order:**
1. Check `.aiki/templates/custom/{name}.md`
2. Check `.aiki/templates/aiki/{name}.md`
3. Error: Template not found

---

## Custom Templates

Users can create custom templates for any workflow.

### Example: Refactoring Template

**File**: `.aiki/templates/custom/refactor-cleanup.md`

```markdown
---
description: Systematic code cleanup and refactoring
type: refactor
assignee: claude-code
---

# Refactor: {scope}

Systematic refactoring and cleanup of {scope}.

Focus on code quality without changing behavior.

# Subtasks

## Identify refactoring opportunities

Scan the code for:
- Duplicate code blocks
- Long functions (>50 lines)
- Complex conditionals
- Magic numbers
- Poor naming

List all findings with file:line references.

## Extract functions

Extract reusable logic into well-named functions.

For each extraction:
- Choose clear, descriptive name
- Add docstring
- Keep functions focused (single responsibility)

## Simplify conditionals

Refactor complex if/else chains:
- Extract guard clauses
- Use early returns
- Replace nested ifs with polymorphism where appropriate

## Remove duplication

Consolidate duplicate code:
- Extract common logic to shared functions
- Use data structures to eliminate repetition
- Document why code is shared

## Run tests

Verify refactoring didn't break functionality:

```bash
cargo test
```

If tests fail, fix issues before closing.
```

### Example: Documentation Template

**File**: `.aiki/templates/custom/api-docs.md`

```markdown
---
description: Generate comprehensive API documentation
type: documentation
assignee: claude-code
---

# Document API: {scope}

Create comprehensive API documentation for {scope}.

# Subtasks

## Inventory public APIs

List all public functions, types, and modules in {scope}.

For Rust:
```bash
rg "^pub " {files}
```

Create a checklist of items needing documentation.

## Write function documentation

For each public function, write:

```rust
/// One-line summary of what the function does.
///
/// More detailed explanation if needed.
///
/// # Arguments
///
/// * `param1` - Description
/// * `param2` - Description
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// When this function returns an error
///
/// # Examples
///
/// ```
/// let result = my_function("test");
/// assert_eq!(result, expected);
/// ```
pub fn my_function(param1: &str) -> Result<String> {
    // ...
}
```

## Write module documentation

Add module-level docs explaining:
- Purpose of the module
- Key concepts
- Usage examples
- Related modules

## Review completeness

Check documentation coverage:

```bash
cargo doc --no-deps
```

Verify all public APIs are documented.
```

---

## CLI Changes

### Before (Review-Specific)

```bash
# Old: --prompt flag (review-specific)
aiki review --prompt default
aiki review --prompt security
aiki review --prompt performance
aiki review --prompt style
```

### After (Generic)

```bash
# New: --template flag (generic)
aiki review --template default
aiki review --template security
aiki review --template performance
aiki review --template style

# And now works for any task type:
aiki task create --template refactor-cleanup
aiki task create --template api-docs
aiki task create --template integration-test
```

### Backward Compatibility

For a transition period, support both flags:

```bash
# New (recommended)
aiki review --template security

# Old (deprecated, still works)
aiki review --prompt security
# Warning: --prompt is deprecated, use --template instead
```

### New `aiki task create` Command

```bash
# Create task from template
aiki task create --template <name> [options]

# Options:
#   --var <key>=<value>  - Set custom template variable (can be used multiple times)
#   --assignee <agent>   - Override template assignee
#   --priority <level>   - Override template priority
#   --start              - Start the task immediately after creation

# Examples:

# Simple template (no variables)
aiki task create --template refactor-cleanup

# With custom variables
aiki task create --template feature \
  --var feature_name="user auth" \
  --var module="auth"

# Override assignee
aiki task create --template api-docs --assignee claude-code

# Multiple variables + start immediately
aiki task create --template implement \
  --var component="dashboard" \
  --var language="rust" \
  --start
```

**Behavior:**
1. Load template from `.aiki/templates/`
2. Substitute variables (built-in + custom via `--var`)
3. Create parent task + all subtasks atomically
4. Optionally start the task with `--start` flag

**Variable Precedence:**
1. Command-line `--var` flags (highest priority)
2. Built-in variables from context
3. Template frontmatter defaults

---

## Implementation

### Phase 1: Template Loading Infrastructure

**Deliverables:**
- Template parser (YAML → TaskTemplate struct)
- Template resolution (custom → built-in → error)
- Variable substitution engine
- Template validation

**Files:**
- `cli/src/templates/mod.rs` - Template module
- `cli/src/templates/parser.rs` - Markdown + frontmatter parsing
- `cli/src/templates/resolver.rs` - Template resolution
- `cli/src/templates/types.rs` - TaskTemplate struct
- `cli/src/templates/variables.rs` - Variable substitution

**Types:**
```rust
pub struct TaskTemplate {
    pub name: String,
    pub version: u32,
    pub description: Option<String>,
    pub task_defaults: TaskDefaults,
    pub parent: TaskDefinition,
    pub subtasks: Vec<TaskDefinition>,
}

pub struct TaskDefaults {
    pub task_type: Option<String>,
    pub assignee: Option<String>,
    pub priority: Option<String>,
}

pub struct TaskDefinition {
    pub name: String,
    pub instructions: String,
    pub metadata: HashMap<String, String>,
}
```

### Phase 2: Aiki Templates

**Deliverables:**
- Create `.aiki/templates/aiki/` directory
- Ship `review.md` (default review template)

**Files:**
- `.aiki/templates/aiki/review.md`

### Phase 3: CLI Integration

**Deliverables:**
- Implement `aiki review` with `--template` flag
- Add template loading in review command
- Support both custom and built-in templates

**Files:**
- `cli/src/commands/review.rs` - Review command with template support
- `cli/src/main.rs` - CLI args

### Phase 4: Generic Task Creation

**Deliverables:**
- New `aiki task create --template` command
- Task creation from arbitrary templates
- Support for non-review workflows

**Files:**
- `cli/src/commands/task.rs` - Add `create` subcommand

### Phase 5: Documentation

**Deliverables:**
- Template authoring guide
- Examples for common workflows
- Integration with flow system

**Files:**
- `ops/now/code-review-task-native.md` - Already updated to use `--template`
- `ops/now/task-templates.md` - This document
- `README.md` - Update examples

---

## Benefits

### 1. Generic Workflows

Not limited to code reviews anymore:

```bash
aiki task create --template refactor-cleanup
aiki task create --template integration-test
aiki task create --template api-docs
aiki task create --template database-migration
```

### 2. Reusable Patterns

Define once, use everywhere (file: `.aiki/templates/custom/team-refactor.md`):

```markdown
# Team Refactoring Workflow

Standard workflow for team refactoring tasks.

# Subtasks

## Identify issues

## Propose changes

## Get approval

## Implement

## Run tests
```

### 3. Better Organization

Templates are readable markdown with clear structure:

```markdown
# Clear heading hierarchy

Parent task context goes here.

# Subtasks

## Step 1

Do X

## Step 2

Do Y
```

### 4. Composable

Templates can reference other templates (future):

```markdown
---
extends: security
assignee: codex  # Override default
---

# Custom Security Review

Inherits subtasks from security template.

Additional instructions here...

# Subtasks

(Subtasks inherited from security template)
```

### 5. Tooling Support

Markdown with YAML frontmatter enables:
- Frontmatter schema validation
- Markdown preview in editors
- IDE autocomplete for frontmatter
- Easy to read and edit

---

## Future Enhancements

### Template Inheritance

File: `.aiki/templates/custom/custom-security.md`

```markdown
---
extends: security  # Inherit from built-in security template
---

# Security Review: {scope}

${parent.instructions}  # Include base instructions

Additional company-specific checks:
- Check for banned libraries
- Verify compliance requirements

# Subtasks

## Digest code changes

${subtasks.0.instructions}  # Inherit first subtask

## Security analysis

${subtasks.1.instructions}  # Inherit second subtask

## Compliance check

Additional subtask for company-specific compliance.
```

### Conditional Subtasks

File: `.aiki/templates/custom/multi-language-test.md`

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
```

### Template Marketplace

```bash
# Install community templates
aiki template install github:org/repo/templates/advanced-review.yml

# List installed templates
aiki template list

# Update templates
aiki template update
```

### Template Variables from Context

File: `.aiki/templates/custom/contextual-review.md`

```markdown
# Review by {author}

Reviewing changes by {author}

**Change Summary:**
- Files changed: {files_count}
- Lines added: {lines_added}
- Lines removed: {lines_removed}

# Subtasks

## Review changes

Analyze the {files_count} modified files.
```

---

## Summary

**Task templates transform Aiki from a review-specific tool to a generic agent workflow system.**

| Aspect | What Templates Provide |
|--------|------------------------|
| **Scope** | Any task workflow, not just reviews |
| **Format** | Markdown with YAML frontmatter |
| **Reusability** | Highly reusable across projects |
| **Customization** | Full task structure (parent + subtasks) |
| **Variables** | Runtime substitution (`{scope}`, `{files}`, etc.) |
| **Composability** | Template inheritance (future) |
| **Readability** | Natural markdown, easy to write and understand |

**Key Commands:**

```bash
# Review with template
aiki review --template security

# Create any task from template
aiki task create --template refactor-cleanup
aiki task create --template api-docs

# List available templates
aiki template list

# Show template details
aiki template show security
```

This design maintains **all existing functionality** from the `--prompt` system while **enabling unlimited custom workflows** for any agent task.
