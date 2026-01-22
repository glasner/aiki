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
aiki review --template aiki/security

# Same power as old --prompt system
# But now templates can define any workflow:
aiki task create --template myorg/refactor-cleanup
aiki task create --template myorg/integration-test
aiki task create --template myorg/documentation-audit
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
aiki review --template aiki/security

# Refactoring workflows
aiki task create --template myorg/refactor-cleanup

# Testing workflows
aiki task create --template myorg/integration-test

# Documentation workflows
aiki task create --template myorg/api-docs
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

A **task template** is a markdown file with YAML frontmatter that defines:

1. **Task structure** - Parent task + subtasks
2. **Instructions** - What the agent should do in each subtask
3. **Metadata** - Type, assignee, scope patterns
4. **Variables** - Placeholders filled at runtime

### Template Structure

Templates are markdown files with YAML frontmatter:

```markdown
<!-- cli/src/tasks/templates/builtin/review.md (bundled in binary) -->
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
```

**Template Structure:**
- **Filename** - Template name inferred from filename (e.g., `review.md` → name is `review`)
- **Frontmatter** (YAML between `---`) - Optional metadata like type, assignee, description (no need for `name` field!)
- **Text before first `#`** - Ignored/dropped (use frontmatter for metadata instead)
- **First `# Heading`** - Task name, supports variables like `{data.scope}`, `{assignee}`, etc.
- **Content before `# Subtasks`** - Parent task instructions
- **`# Subtasks`** - Marks beginning of subtasks section (optional, see below)
- **`## Subheadings`** under `# Subtasks` - Each h2 defines a subtask (heading text = subtask name)
- **Content under each `##`** - Subtask instructions

**Parsing Rules:**
- Text between frontmatter and first `# heading` is dropped (not used)
- Only `## headings` that appear **after** the `# Subtasks` marker become subtasks
- Any `## headings` **before** `# Subtasks` are treated as part of the parent task instructions
- The `# Subtasks` heading is optional. If absent, the template defines a parent task with zero subtasks
- If `# Subtasks` is present, all `## headings` under it become subtasks

### Template Variables

Templates support runtime variable substitution. Variables can be:
1. **Built-in** - Provided automatically by commands
2. **Custom** - Passed via CLI flags

**Substitution Rules:**

Variable substitution is **plain text only** with these guarantees:

1. **Single-pass evaluation** - Variables are substituted once, values are never re-evaluated
2. **No recursion** - Values containing `{braces}` are inserted as literal text
3. **Safe by design** - Substitution happens after frontmatter parsing, values cannot inject YAML
4. **Deterministic** - Same inputs always produce same output

**Escaping literal braces:**

Use double braces to include literal `{variable}` syntax in templates:

```markdown
# Template:
To reference variables, use {{{{data.foo}}}} syntax.

# After substitution:
To reference variables, use {{data.foo}} syntax.
```

**Variable Validation:**

Missing variables cause errors. If a template references `{data.foo}` but `--data foo=...` was not provided, the command fails with:

```
Error: Variable '{data.foo}' referenced but not provided
  In template: <template-name>
  Use: --data foo=<value>
```

**Security Examples:**

```bash
# Safe: Braces in values are literal text
aiki task create --template myorg/deploy \
  --data region="us-{east}-1"
# Result: region is literally "us-{east}-1", no nested substitution

# Safe: Special characters don't break parsing
aiki task create --template myorg/note \
  --data message="---\nThis is not frontmatter"
# Result: Inserted as plain text in body, not parsed as YAML
```

#### Built-in Variables (from Task)

Built-in variables are populated from the task struct:

| Variable | Description | Example |
|----------|-------------|---------|
| `{id}` | Task ID | `xqrmnpstklzv...` |
| `{assignee}` | Assigned agent | `codex`, `claude-code` |
| `{priority}` | Task priority | `p0`, `p1`, `p2`, `p3` |
| `{type}` | Task type | `review`, `refactor`, `test` |
| `{created}` | Creation timestamp | `2026-01-20T10:00:00Z` |

**Variable Scoping in Subtasks:**

When rendering subtask instructions, variables resolve to the **subtask's context** (with fallback to parent):

| Variable | Resolution |
|----------|------------|
| `{id}` | Subtask's own ID (e.g., `parent_id.1`) |
| `{assignee}` | Subtask's assignee (from subtask frontmatter) → parent's assignee |
| `{priority}` | Subtask's priority (from subtask frontmatter) → parent's priority |
| `{data.*}` | Subtask's data merged over parent's data |
| `{parent.id}` | Parent task's ID |
| `{parent.assignee}` | Parent task's assignee |
| `{parent.data.*}` | Parent task's data fields |

**Example:**
```markdown
## Security audit
---
assignee: security-specialist
priority: p0
---

Task {id} assigned to {assignee} (priority: {priority})
Parent task: {parent.id}
```

Renders as:
```markdown
Task xqrmnpst.2 assigned to security-specialist (priority: p0)
Parent task: xqrmnpst
```

#### Review-Specific Variables (in data)

When using `aiki review`, review-specific information is passed via data:

| Variable | Description | Example |
|----------|-------------|---------|
| `{data.scope}` | Review scope (change ID, revset) | `@`, `xqrmnpst`, `main..@` |
| `{data.files}` | Comma-separated file list | `src/auth.ts, src/crypto.rs` |

**Example:**
```markdown
# Review: {data.scope}

Review changes in {data.scope} (files: {data.files})
Assigned to: {assignee}
```

When used with `aiki review @`, this becomes:
```markdown
# Review: @

Review changes in @ (files: src/auth.ts, src/middleware.ts)
Assigned to: codex
```

#### Custom Variables (aiki task create)

Templates can reference:
- **Data fields** using `{data.key}` syntax
- **Source** using `{source}` variable

##### Source Variable

The `{source}` variable accesses the task's source lineage (set via `--source`):

```bash
# Link task to a plan document
aiki task create --template myorg/build \
  --source file:ops/now/feature.md
```

**Template:**
```markdown
# Build feature from {source}

Build the feature described in {source}.

# Subtasks

## Review plan

Read and understand {source}.

## Build

Follow the plan in {source}.

## Test

Verify implementation matches {source}.
```

**Result:**
```markdown
# Build feature from file:ops/now/feature.md

Build the feature described in file:ops/now/feature.md.

# Subtasks

## Review plan

Read and understand file:ops/now/feature.md.

## Build

Follow the plan in file:ops/now/feature.md.

## Test

Verify implementation matches file:ops/now/feature.md.
```

**Source Prefixes:**
- `file:` - Document/plan file
- `task:` - Parent task ID
- `comment:` - Git forge comment

##### Data Variables

Templates can also use custom data fields:

```bash
aiki task create --template myorg/deploy \
  --data environment="production" \
  --data region="us-east-1"
```

**Template:**
```markdown
# Deploy to {data.environment}

Deploy application to {data.environment} in {data.region}.
```

**Type Coercion:**

`--data` values are automatically coerced:
- `"true"` / `"false"` → boolean
- Numeric strings (`"5"`, `"3.14"`) → number
- Everything else → string

**Example:**
```bash
aiki task create --template myorg/test \
  --data parallel="true" \
  --data threads="4"
```

Results in: `parallel=true` (boolean), `threads=4` (number)

---

## Template Format

### Full Template Specification

```markdown
---
# Frontmatter - Metadata (all optional, inferred from filename/defaults)
version: 1.2.0                # Optional: Semantic version (stored in task.template as name@version)
description: Human-readable description  # Optional

# Task defaults
type: review                  # Optional: Task type (default: generic)
assignee: codex               # Optional: Default assignee (can be overridden)
priority: p2                  # Optional: Default priority

# Optional: Custom data
data:
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

The simplest possible template:

```markdown
# Minimal task

Do something.

# Subtasks

## Do the work

Instructions here.
```

**Note**: No frontmatter needed! Template name is inferred from filename (`minimal.md` → `minimal`).

### Template with Variables

File: `.aiki/templates/myorg/custom-review.md`

```markdown
# Review {data.scope} by {assignee}

Reviewing: {data.scope}
Assigned to: {assignee}
Files: {data.files}

Analyze the changes and provide feedback.

# Subtasks

## Analyze {data.scope}

Run: jj diff --revision {data.scope}
```

### Subtask Metadata

Subtasks can have their own frontmatter using `---` delimiters immediately after the `## heading`:

```markdown
## Review code
---
priority: p0
assignee: security-specialist
---

Review instructions here...
```

**Parsing Rules:**
- If the first non-blank line after a `## heading` is `---`, it opens a YAML frontmatter block closed by another `---`
- Subtask frontmatter must appear before any other content (including HTML comments)
- Otherwise, all content is treated as instructions
- `---` appearing elsewhere in subtask content is treated as a markdown horizontal rule

**Supported Fields:**
- `priority` - Override task priority for this subtask
- `assignee` - Override assignee for this subtask
- `data` - Additional data specific to this subtask

---

## Built-in Templates

Aiki ships with these built-in templates:

### 1. `review` - General Code Review (Default)

**Location**: Bundled in binary at `cli/src/tasks/templates/builtin/review.md`

**Purpose**: General code quality, functionality, basic security, and performance

**Subtasks:**
1. **Digest code changes** - Examine what was modified
2. **Review code** - Check functionality, quality, security, performance

**Usage:**
```bash
aiki review                       # Uses default aiki/review template
aiki review --template aiki/review  # Explicit
```

### Template Location Strategy

```
.aiki/
├── templates/
│   ├── aiki/                  # Built-in templates (namespaced with aiki/)
│   │   └── review.md          # Referenced as "aiki/review"
│   └── myorg/                 # User-defined templates (custom namespace)
│       ├── refactor-cleanup.md  # Referenced as "myorg/refactor-cleanup"
│       ├── api-docs.md          # Referenced as "myorg/api-docs"
│       └── integration-test.md  # Referenced as "myorg/integration-test"
```

**Template Naming:**
- **Built-in templates**: Always use `aiki/` prefix (e.g., `aiki/review`)
- **Custom templates**: Use custom namespace prefix (e.g., `myorg/refactor-cleanup`)
- **Benefits**: 
  - No loading order conflicts
  - Clear distinction between built-in and custom
  - Custom templates can't accidentally shadow built-ins
  - Organizations can namespace their templates

**Resolution:**
- Template name includes the namespace: `aiki/review` (built-in templates bundled in binary)
- User templates stored in `.aiki/templates/{namespace}/` (e.g., `.aiki/templates/myorg/review.md`)
- Custom template: `myorg/refactor-cleanup` → `.aiki/templates/myorg/refactor-cleanup.md`
- No fallback search needed - template name is exact path within `.aiki/templates/`

---

## Custom Templates

Users can create custom templates for any workflow.

### Example: Refactoring Template

**File**: `.aiki/templates/myorg/refactor-cleanup.md`

```markdown
---
description: Systematic code cleanup and refactoring
type: refactor
assignee: claude-code
---

# Refactor: {data.scope}

Systematic refactoring and cleanup of {data.scope}.

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

**File**: `.aiki/templates/myorg/api-docs.md`

```markdown
---
description: Generate comprehensive API documentation
type: documentation
assignee: claude-code
---

# Document API: {data.module}

Create comprehensive API documentation for {data.module}.

# Subtasks

## Inventory public APIs

List all public functions, types, and modules in {data.module}.

For Rust:
```bash
rg "^pub " {data.module}
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
aiki review --template aiki/review       # Built-in templates use aiki/ prefix
aiki review --template aiki/security
aiki review --template aiki/performance
aiki review --template aiki/style

# Custom templates (with namespace):
aiki task create --template myorg/refactor-cleanup
aiki task create --template myorg/api-docs
aiki task create --template myorg/integration-test
```

### New `aiki task create` Command

```bash
# Create task from template
aiki task create --template <name> [options]

# Options:
#   --data <key>=<value>  - Set metadata (accessible as {data.key} in template)
#   --assignee <agent>        - Override template assignee
#   --priority <level>        - Override template priority

# Examples:

# Refactor with scope metadata (links to what's being refactored)
aiki task create --template myorg/refactor-cleanup \
  --data scope="src/auth.rs"

# Link to a plan document via source
aiki task create --template myorg/build \
  --source file:ops/now/feature.md

# Override assignee
aiki task create --template myorg/api-docs --assignee claude-code

# Multiple data fields
aiki task create --template myorg/build \
  --data component="dashboard" \
  --data language="rust"

# Combine source + data
aiki task create --template myorg/build \
  --source file:ops/now/feature.md \
  --data target="v2.0"
```

**Behavior:**
1. Load template from `.aiki/templates/`
2. Substitute variables:
   - Built-in: `{id}`, `{assignee}`, `{priority}`, `{type}`, `{created}`
   - Source: `{source}` from `--source` flag
   - Data: `{data.key}` for any `--data` flag
3. Create parent task + all subtasks atomically
4. Store source and data on task for lineage tracking and querying

**Variable Precedence:**
1. Command-line flags (`--source`, `--data`) (highest priority)
2. Built-in variables from context
3. Template frontmatter defaults

**Data Merge Semantics:**

CLI `--data` merges with frontmatter `data:`. CLI values override matching keys; frontmatter keys without CLI overrides are preserved.

**Example:**
```yaml
---
data:
  environment: staging
  region: us-west-1
---
```

```bash
aiki task create --template myorg/deploy --data region="us-east-1"
```

**Result:** `environment=staging`, `region=us-east-1`

**Implementation Note:** This requires renaming `task.metadata` to `task.data` throughout the codebase for consistency.

---

## Error Messages

Templates are validated when used, producing clear error messages:

### Template Not Found

```
Error: Template 'security' not found
  Expected: .aiki/templates/security.md
  
  Did you mean one of these?
    - aiki/security (built-in)
    - myorg/refactor-cleanup (custom)
```

### Variable Not Provided

```
Error: Variable '{data.scope}' referenced but not provided
  In template: review
  Use: --data scope=<value>
```

### YAML Frontmatter Parse Error

```
Error: Invalid template frontmatter
  File: .aiki/templates/my-review.md
  Line 3: expected ':' but found '}'
```

### Markdown Structure Error

```
Error: Invalid template structure
  File: .aiki/templates/my-review.md
  Missing required '# ' heading for task name
```

---

## Implementation

### Phase 1: Template Loading Infrastructure

**Deliverables:**
- Template parser (YAML → TaskTemplate struct)
- Template resolution (custom → aiki → error)
- Variable substitution engine (support `{data.key}` syntax with security guarantees)
- Template validation
- **Prerequisite: Rename `metadata` to `data`** - Refactor `task.metadata` → `task.data` throughout codebase

**Variable Substitution Implementation:**

The substitution engine must guarantee security and determinism:

```rust
// Pseudocode for safe substitution
fn load_template(path: &Path, data: &HashMap<String, Value>) -> Result<Task> {
    let content = read_file(path)?;

    // 1. Parse frontmatter FIRST (no substitution in YAML)
    let (frontmatter, body) = parse_frontmatter(&content)?;

    // 2. Substitute variables in body only (single-pass, no recursion)
    //    Values are converted to strings for text substitution
    let substituted_body = substitute_once(&body, &data)?;

    // 3. Parse markdown structure from substituted body
    let task = parse_task_structure(&substituted_body)?;

    // 4. Store typed data on task (not the string-rendered version)
    task.data = data;

    Ok(task)
}

fn substitute_once(text: &str, data: &HashMap<String, Value>) -> Result<String> {
    let mut result = text.to_string();

    // Handle escaping: {{...}} -> {...} (literal braces)
    result = result.replace("{{", "\x00").replace("}}", "\x01");

    // Substitute variables: {key} -> value.to_string() (typed value rendered as text)
    for (key, value) in data {
        let pattern = format!("{{{}}}", key);
        let rendered = match value {
            Value::Bool(b) => b.to_string(),      // "true" or "false"
            Value::Number(n) => n.to_string(),    // "42" or "3.14"
            Value::String(s) => s.clone(),        // as-is
            Value::Null => "".to_string(),        // empty
            _ => serde_json::to_string(value)?,   // arrays/objects as JSON
        };
        result = result.replace(&pattern, &rendered);
    }

    // Restore escaped braces
    result = result.replace("\x00", "{").replace("\x01", "}");

    Ok(result)
}
```

**Typed Data Storage:**

Data values are stored with their types preserved (using `serde_json::Value`):

| CLI Input | Stored Type | Stored Value |
|-----------|-------------|--------------|
| `--data enabled="true"` | `Value::Bool` | `true` |
| `--data count="42"` | `Value::Number` | `42` |
| `--data name="test"` | `Value::String` | `"test"` |
| `--data ratio="3.14"` | `Value::Number` | `3.14` |

This enables:
- **Querying by type** - Find tasks where `data.enabled == true` (boolean comparison)
- **JSON serialization** - Data serializes correctly without quote escaping issues
- **Template conditionals** (future) - `{{#if data.enabled}}...{{/if}}`

**Key guarantees:**
- Frontmatter parsing happens before substitution (values can't inject YAML)
- Single-pass substitution (no recursion, deterministic)
- Values are plain text (braces in values are literal, not evaluated)

**Validation Timing:**
- Templates are validated when used (`aiki task create --template X`), not at `aiki init` time
- Invalid templates produce clear error messages at usage time (see Error Messages section)

**Files:**
- `cli/src/tasks/templates/mod.rs` - Template module
- `cli/src/tasks/templates/parser.rs` - Markdown + frontmatter parsing
- `cli/src/tasks/templates/resolver.rs` - Template resolution
- `cli/src/tasks/templates/types.rs` - TaskTemplate struct
- `cli/src/tasks/templates/variables.rs` - Variable substitution with `{data.key}` support
- `cli/src/tasks/types.rs` - Rename `metadata` field to `data`
- `cli/src/commands/task.rs` - Rename `--metadata` flag to `--data`

**Types:**
```rust
pub struct TaskTemplate {
    pub name: String,
    pub version: String,  // Semantic version (e.g., "1.2.0")
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
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub data: HashMap<String, Value>,  // From subtask frontmatter
}

/// Fields captured when creating a task from a template
/// (stored in TaskEvent::Created and materialized Task)
pub struct CreatedTaskFields {
    pub template: String,         // "name@version" (e.g., "myorg/review@1.2.0")
    pub working_copy: String,     // JJ change_id at creation time
    pub instructions: String,     // Template instructions with variables substituted
    pub data: HashMap<String, Value>,  // Merged from template defaults + CLI --data
}
```

**Working Copy Capture:**

When a task is created from a template, the current JJ working copy `change_id` is captured in `task.working_copy`. This enables:

1. **Historical template lookup** - Retrieve the exact template version used:
   ```bash
   jj show <working_copy>:.aiki/templates/<template>.md
   ```

2. **Reproducibility** - Understand what instructions the agent received at the time

3. **Audit trail** - Link tasks back to the workspace state when they were created

### Phase 2: Built-in Templates

**Deliverables:**
- Bundle `review.md` template in binary (using `include_str!` like `flow.yaml`)
- Load built-in templates from binary at runtime

**Files:**
- `cli/src/tasks/templates/builtin/review.md` - Bundled template file
- `cli/src/tasks/templates/builtin/mod.rs` - Template loader using `include_str!`

### Phase 3: Generic Task Creation

**Deliverables:**
- New `aiki task create --template` command
- Add `--template` support to `aiki task start` (create + start)
- Task creation from arbitrary templates
- Support for non-review workflows
- Template filtering: `aiki task list --template <name>`

**Files:**
- `cli/src/commands/task.rs` - Add `create` subcommand, `--template` flag to `start`, and `--template` filter

**Implementation Notes:**
- Store template name and version in `task.template` field as `name@version` (e.g., `myorg/review@1.2.0`)
- Store working copy in `task.working_copy` field (JJ change_id at task creation time, for historical template lookup)
- `task.data` remains for user-defined custom metadata
- `aiki task start --template <name>` creates task from template and immediately starts it (quick-start pattern)
- `aiki task list --template build` filters to tasks with `template` matching "build" (any version)
- `aiki task list --template build@1.2.0` filters to tasks with exact template version
- Enables querying all tasks created from a specific template or version
- Historical template lookup: `jj show <working_copy>:.aiki/templates/<template>.md`

**Quick-Start Pattern:**

`aiki task start --template <name>` creates and immediately starts a task from a template:

```bash
# Create + start in one command
aiki task start --template myorg/refactor-cleanup \
  --data scope="src/auth.rs"

# Equivalent to:
aiki task create --template myorg/refactor-cleanup --data scope="src/auth.rs"
TASK_ID=$(aiki task list --format=json | jq -r '.[0].id')
aiki task start $TASK_ID
```

This mirrors the existing `aiki task start "description"` quick-start pattern, but for template-based tasks.

### Phase 4: Template Discovery Commands

**Deliverables:**
- `aiki task template list` - List all available templates
- `aiki task template show <name>` - Show template details

**Purpose:**
Users need to discover and inspect built-in `aiki/` templates and any custom templates they've created/downloaded.

**Commands:**

```bash
# List all available templates
aiki task template list

# Output:
Available templates:
  aiki/review              - General code quality, functionality, security, performance
  myorg/refactor-cleanup   - Systematic code cleanup and refactoring
  myorg/api-docs           - Generate comprehensive API documentation
  myorg/integration-test   - Create comprehensive integration tests

# Show template details
aiki task template show aiki/review

# Output:
Template: aiki/review
Source: Built-in (bundled in binary)
Description: General code quality, functionality, basic security

# Review: {data.scope}

Code review orchestration task.
...
```

**Files:**
- `cli/src/commands/task/template.rs` - Template subcommands (list, show)
- `cli/src/tasks/templates/discovery.rs` - Template discovery logic

**Implementation Notes:**
- `list` shows both built-in templates (from binary) and user templates (from `.aiki/templates/`)
- Built-in templates loaded via `include_str!` (like `flow.yaml`)
- User templates scanned from `.aiki/templates/` directory recursively
- Groups by namespace (aiki/, myorg/, etc.)
- `show` displays template name, source (built-in or file path), description, and full content
- Both commands work without requiring a task to be created

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

## Future Enhancements

See [`ops/future/templates/`](../future/templates/) for detailed future enhancement proposals:

- [Additional Review Templates](../future/templates/additional-review-templates.md) - Built-in security, performance, and style templates
- [Template Inheritance](../future/templates/template-inheritance.md) - Extend templates with custom additions
- [Conditional Subtasks](../future/templates/conditional-subtasks.md) - Dynamic subtasks based on repository context
- [Template Validation](../future/templates/template-validation.md) - Validate templates without creating tasks
- [Template Marketplace](../future/templates/template-marketplace.md) - Share and install community templates
- [Template Context Variables](../future/templates/template-context-variables.md) - Auto-populate variables from git/jj status

---

## Summary

**Task templates transform Aiki from a review-specific tool to a generic agent workflow system.**

| Aspect | What Templates Provide |
|--------|------------------------|
| **Scope** | Any task workflow, not just reviews |
| **Format** | Markdown with YAML frontmatter |
| **Reusability** | Highly reusable across projects |
| **Customization** | Full task structure (parent + subtasks) |
| **Variables** | Runtime substitution (`{assignee}`, `{data.scope}`, `{source}`, etc.) |
| **Readability** | Natural markdown, easy to write and understand |

**Key Commands:**

```bash
# Create task from template
aiki task create --template myorg/refactor-cleanup \
  --data scope="src/auth.rs"

# Start task from template (create + start)
aiki task start --template myorg/api-docs \
  --data module="src/core"

# With source linkage
aiki task create --template myorg/build \
  --source file:ops/now/feature.md

# Query tasks by template
aiki task list --template myorg/build              # Any version
aiki task list --template myorg/build@1.2.0        # Specific version
aiki task list --template myorg/refactor-cleanup --status open
```

This design maintains **all existing functionality** from the `--prompt` system while **enabling unlimited custom workflows** for any agent task.
