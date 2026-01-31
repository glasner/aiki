# Task Source Context for Reviews

**Date**: 2026-01-29
**Status**: Done
**Purpose**: Give reviewing agents access to source task context to understand intent

**Related Documents**:
- [Review and Fix Commands](review-and-fix.md) - Uses source context for code review
- [Task Diff Command](task-diff.md) - Shows what changed, this doc shows why

---

## Executive Summary

When reviewing a task, agents need to understand *why* code changed, not just *what* changed. The `sources` field already links tasks to their origin (e.g., `task:abc123`), but the reviewing agent has no easy way to access the source task's instructions/intent.

**Key Features**:
- `aiki task show` includes minimal `<source>` elements by default
- `--with-source` expands to include name + instructions
- Supports task, prompt, file, and comment source types

---

## The Problem

When reviewing a task, the agent needs to understand *why* the task exists. Currently:
- `aiki task show X` shows task metadata and instructions
- But sources are just raw strings like `task:abc123`

The agent would need to:
1. Parse the task ID from the prefix
2. Run `aiki task show abc123` to see the source's instructions
3. Repeat if there's a chain of sources

This is inefficient for reviewers who need the full context.

---

## Design

### Default `aiki task show` Output

Sources shown as minimal references (type + id only). A task can have multiple sources:

```xml
<aiki_task cmd="show" status="ok">
  <task id="lpqrstwo.1" name="Fix: Null pointer check" status="open" priority="p0">
    <assignee>claude-code</assignee>
    <instructions>
      Potential null pointer dereference when accessing user.name.
      Runtime crash if user object is null from auth middleware.
      Suggested fix: check user && user.name before access.
    </instructions>
    <data>
      <field key="file">src/auth.ts</field>
      <field key="line">42</field>
    </data>
    <source type="task" id="xqrmnpst"/>
    <source type="comment" id="c1a2b3c4"/>
  </task>
</aiki_task>
```

### Expanded Output with `--with-source`

```bash
aiki task show lpqrstwo.1 --with-source
```

Each source is expanded with its content:

```xml
<aiki_task cmd="show" status="ok">
  <task id="lpqrstwo.1" name="Fix: Null pointer check" status="open" priority="p0">
    <assignee>claude-code</assignee>
    <instructions>
      Potential null pointer dereference when accessing user.name.
      Runtime crash if user object is null from auth middleware.
      Suggested fix: check user && user.name before access.
    </instructions>
    <data>
      <field key="file">src/auth.ts</field>
      <field key="line">42</field>
    </data>
    <source type="task" id="xqrmnpst">
      <name>Review: changes @</name>
      <instructions>
        Implement JWT-based authentication for the API endpoints.

        Requirements:
        - Validate tokens on each request
        - Handle expired tokens gracefully
        - Log authentication failures
      </instructions>
    </source>
    <source type="comment" id="c1a2b3c4" task_id="xqrmnpst">
      <text>Null check missing on user object access</text>
      <data>
        <field key="severity">error</field>
        <field key="file">src/auth.ts</field>
      </data>
    </source>
  </task>
</aiki_task>
```

### Source Types

| Source Prefix | Default | With `--with-source` |
|---------------|---------|----------------------|
| `task:<id>` | `<source type="task" id="..."/>` | name + instructions |
| `prompt:<id>` | `<source type="prompt" id="..."/>` | prompt text |
| `file:<path>` | `<source type="file" path="..."/>` | file content |
| `comment:<id>` | `<source type="comment" id="..."/>` | text + data |

---

## Use Cases

### Use Case 1: Reviewing Agent Understands Intent

Review template uses `--with-source` to get full context:
```yaml
instructions: |
  Review the completed task.

  1. Run `aiki task show ${task_id} --with-source` to understand:
     - What the task was supposed to do (from expanded sources)
     - What files were changed (from diff summary)
  2. Run `aiki task diff ${task_id}` to read the actual changes
  3. Compare intent (sources) against implementation (diff)
```

The reviewing agent sees:
- The task being reviewed
- The source task's instructions (the original intent)
- The diff (what was actually implemented)

### Use Case 2: Followup Task Shows Review Context

When a reviewer uses `--with-source` on a followup task:
```xml
<task id="lpqrstwo.1" name="Fix: Null pointer check">
  <instructions>
    Potential null pointer dereference...
  </instructions>
  <source type="task" id="xqrmnpst">
    <name>Review: changes @</name>
    <instructions>Implement JWT-based auth...</instructions>
  </source>
</task>
```

The agent knows:
- What needs to be fixed (task instructions)
- What the original intent was (source task)
- Why this matters (context from review)

### Use Case 3: Traversing a Chain

After multiple review cycles:
```
Original task → Review 1 → Followup 1 → Review 2 → Followup 2
```

`aiki task show followup2 --with-source` shows immediate parent with nested refs:
```xml
<source type="task" id="followup1">
  <name>Followup: Review 1</name>
  <instructions>...</instructions>
  <source type="task" id="original"/>  <!-- nested, not expanded -->
</source>
```

To see the full chain, follow the nested source:
```bash
aiki task show followup1 --with-source
```

---

## Implementation

### Phase 1: Source Parsing in `aiki task show`

**Files**:
- `cli/src/commands/task.rs` - Modify `show` subcommand

**Functionality**:
1. Parse each source string into `<source type="..." id="..."/>` elements
2. No lookups in default mode (just parse and format)

**Core logic**:
```rust
/// Parse source string into typed reference (no lookups)
fn parse_source(source: &str) -> Option<SourceRef> {
    let (source_type, id) = source.split_once(':')?;
    match source_type {
        "task" => Some(SourceRef::Task { id: id.to_string() }),
        "prompt" => Some(SourceRef::Prompt { id: id.to_string() }),
        "file" => Some(SourceRef::File { path: id.to_string() }),
        "comment" => Some(SourceRef::Comment { id: id.to_string() }),
        _ => Some(SourceRef::Unknown { raw: source.to_string() }),
    }
}
```

### Phase 2: `--with-source` Flag

**Files**:
- `cli/src/commands/task.rs` - Add `--with-source` flag
- `cli/src/tasks/storage.rs` - Add `expand_source()` helper

**Functionality**:
When `--with-source` is passed, expand each source:
- `task:` → load task, include name + instructions
- `prompt:` → load prompt text from history
- `file:` → read file content
- `comment:` → load comment text + data

```rust
/// Expand source with full content (for --with-source)
fn expand_source(
    storage: &TaskStorage,
    source: &SourceRef,
) -> ExpandedSource {
    match source {
        SourceRef::Task { id } => {
            match storage.get_task(id) {
                Ok(task) => ExpandedSource::Task {
                    id: id.clone(),
                    name: Some(task.name),
                    instructions: task.instructions,
                    nested_sources: task.sources.iter()
                        .filter_map(|s| parse_source(s))
                        .collect(),
                },
                Err(_) => ExpandedSource::TaskNotFound { id: id.clone() },
            }
        }
        SourceRef::File { path } => {
            let content = std::fs::read_to_string(path).ok();
            ExpandedSource::File { path: path.clone(), content }
        }
        // ... similar for prompt, comment
    }
}
```

**Rationale**: Default stays lean for task execution. Reviewers use `--with-source` for full context.

---

## Output Format Details

### Default (Minimal)

All source types use attribute-only format:

```xml
<source type="task" id="abc123"/>
<source type="prompt" id="def456"/>
<source type="file" path="ops/now/design.md"/>
<source type="comment" id="c1a2b3c4"/>
```

### Expanded (`--with-source`)

**Task source**:
```xml
<source type="task" id="abc123">
  <name>Task name here</name>
  <instructions>Full instructions text...</instructions>
  <source type="task" id="parent123"/>  <!-- nested, not expanded -->
</source>
```

**Prompt source**:
```xml
<source type="prompt" id="def456">
  <text>The original prompt text that created this task...</text>
</source>
```

**File source**:
```xml
<source type="file" path="ops/now/design.md">
  <content>Full file content...</content>
</source>
```

**Comment source**:
```xml
<source type="comment" id="c1a2b3c4" task_id="xqrmnpst">
  <text>Comment text here...</text>
  <data>
    <field key="severity">error</field>
    <field key="file">src/auth.ts</field>
  </data>
</source>
```

**Note**: Nested sources within expanded sources are shown in minimal format (not recursively expanded). Follow the source ID to see its full content.

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Source task not found | `<source type="task" id="abc123" error="not_found"/>` |
| File doesn't exist | `<source type="file" path="..." error="not_found"/>` |
| Empty sources array | Omit `<source>` elements entirely |
| Unknown source type | `<source type="unknown" raw="..."/>` |

---

## Benefits

1. **Fast default**: No lookups for task execution (avoids context pollution)
2. **Opt-in context**: `--with-source` gives reviewers full context in one call
3. **Structured output**: Source IDs are parsed and typed, not raw strings
4. **Full content**: No truncation when expanded — complete instructions/content

---

## Future Enhancements

### Template Variable Access

Allow templates to reference source fields directly:
```yaml
instructions: |
  Original intent: ${sources[0].instructions}

  Fix the issue described below...
```

### Source Diff

Show what changed between source task and current task:
```bash
aiki task diff --from-source lpqrstwo.1
```

### Interactive Source Navigation

Browse source chain interactively:
```bash
aiki task trace lpqrstwo.1
```

---

## Summary

**Problem**: Reviewers need to understand *why* a task exists, but expanding sources by default pollutes context for task execution.

**Solution**: `aiki task show --with-source` expands source references on demand.

**Key Design Decisions**:
1. **Minimal by default**: Sources shown as `<source type="..." id="..."/>` (fast, no lookups)
2. **Opt-in expansion**: `--with-source` loads and includes source content
3. **Single-level expansion**: Nested sources shown minimal (not recursive)
4. **Full content**: No truncation when expanded
5. **Error tolerant**: Missing sources shown as errors, don't fail the command

### Review Workflow

```bash
# Understand the task + why it exists (for reviewers)
aiki task show xqrmnpst --with-source
# Shows: task details, instructions, expanded sources

# Read the actual code changes
aiki task diff xqrmnpst
# Shows: full diff
```

### Task Execution Workflow

```bash
# Get task details without extra context
aiki task show xqrmnpst
# Shows: task details, instructions, minimal source refs
```
