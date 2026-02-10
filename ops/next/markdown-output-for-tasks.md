# Markdown Output for Task Commands

**Date**: 2026-02-01
**Status**: Draft
**Purpose**: Add `--markdown` flag to `aiki task show` for human-readable output instead of XML

**Related Documents**:
- [Task System](../done/task-system.md) - Core task management
- [Task Execution: aiki task run](../done/run-task.md) - Agent runtime and task execution

---

## Executive Summary

Add a `--markdown` flag to `aiki task show` that outputs task details in a clean, human-readable markdown format instead of XML.

**Current behavior:**
```bash
aiki task show <task-id>
```
Outputs XML structure designed for machine parsing.

**Proposed:**
```bash
aiki task show <task-id> --markdown
```
Outputs markdown document optimized for human reading, sharing, and documentation.

---

## Motivation

**Use cases:**
1. **Sharing task details** - Copy/paste into chat, docs, or issues
2. **Documentation** - Include task details in project documentation
3. **Human review** - Easier to read than XML for humans
4. **Reporting** - Generate readable task summaries
5. **Archival** - Create human-readable task records

**Why not replace XML?**
- XML output is designed for programmatic parsing and tool integration
- Some users/tools rely on the structured XML format
- Markdown is better for humans, XML is better for machines
- Keep both options available

---

## User Experience

### Command Syntax

```bash
aiki task show <task-id> [--markdown]
```

**Arguments:**
- `<task-id>` - Task ID to display

**Options:**
- `--markdown` - Output in markdown format instead of XML (default: false)

**Alias support:**
```bash
aiki task show <task-id> --md    # Short form
aiki task show <task-id> -m      # Single char flag
```

---

## Output Format

### Current (XML)

```bash
aiki task show abc123
```

```xml
<aiki_task cmd="show" status="ok">
  <task id="abc123" name="Fix login bug" priority="p1" status="in_progress">
    <created_at>2026-02-01T10:00:00Z</created_at>
    <started_at>2026-02-01T10:15:00Z</started_at>
    <assignee>claude-code</assignee>
    <sources>
      <source>file:ops/now/auth-fix.md</source>
      <source>prompt:xyz789</source>
    </sources>
    <comments>
      <comment id="c1" timestamp="2026-02-01T10:20:00Z">
        <text>Found the issue in auth handler</text>
      </comment>
      <comment id="c2" timestamp="2026-02-01T10:25:00Z">
        <text>Implementing fix now</text>
      </comment>
    </comments>
    <subtasks>
      <task id="abc123.1" name="Update auth validation" status="completed"/>
      <task id="abc123.2" name="Add unit tests" status="in_progress"/>
      <task id="abc123.3" name="Update docs" status="pending"/>
    </subtasks>
  </task>
</aiki_task>
```

### Proposed (Markdown)

```bash
aiki task show abc123 --markdown
```

```markdown
# Task: Fix login bug

**ID:** `abc123`  
**Status:** In Progress  
**Priority:** P1  
**Assignee:** claude-code

**Created:** 2026-02-01 10:00:00 UTC  
**Started:** 2026-02-01 10:15:00 UTC  
**Duration:** 25 minutes

---

## Sources

- `file:ops/now/auth-fix.md`
- `prompt:xyz789`

---

## Progress

### Comments

**2026-02-01 10:20:00 UTC**
> Found the issue in auth handler

**2026-02-01 10:25:00 UTC**
> Implementing fix now

---

## Subtasks

- [x] `abc123.1` Update auth validation
- [ ] `abc123.2` Add unit tests *(in progress)*
- [ ] `abc123.3` Update docs

---

## Details

To continue this task:
```bash
aiki task start abc123
```

To add a comment:
```bash
aiki task comment abc123 "Your progress update"
```

To close this task:
```bash
aiki task close abc123 --summary "What you accomplished"
```
```

---

## Format Specification

### Task Header

```markdown
# Task: {task.name}

**ID:** `{task.id}`  
**Status:** {status}  
**Priority:** {priority}  
**Assignee:** {assignee}

**Created:** {created_at}  
[**Started:** {started_at}]  # Only if started
[**Completed:** {completed_at}]  # Only if completed
[**Duration:** {duration}]  # Only if started
```

### Status Formatting

| Internal Status | Display |
|----------------|---------|
| `pending` | Pending |
| `in_progress` | In Progress |
| `completed` | Completed ✓ |
| `wont_do` | Won't Do |
| `blocked` | Blocked |

### Priority Formatting

| Priority | Display |
|----------|---------|
| `p0` | P0 (Urgent) |
| `p1` | P1 (High) |
| `p2` | P2 (Normal) |
| `p3` | P3 (Low) |

### Sources Section

```markdown
## Sources

- `{source1}`
- `{source2}`
```

If no sources: omit section.

### Comments Section

```markdown
## Progress

### Comments

**{timestamp}**
> {comment_text}

**{timestamp}**
> {comment_text}
```

If no comments: omit section.

### Subtasks Section

```markdown
## Subtasks

- [x] `{subtask.id}` {subtask.name}
- [ ] `{subtask.id}` {subtask.name} *(in progress)*
- [ ] `{subtask.id}` {subtask.name}
```

Status indicators:
- `[x]` - Completed
- `[>]` - In progress (with italic note)
- `[ ]` - Pending
- `[-]` - Won't do (with strikethrough on name)
- `[!]` - Blocked (with italic note)

If no subtasks: omit section.

### Details Section (Optional)

```markdown
## Details

To continue this task:
```bash
aiki task start {task.id}
```

[Additional contextual commands based on task state]
```

**Contextual commands:**
- If `pending`: Show `aiki task start`
- If `in_progress`: Show `aiki task comment` and `aiki task close`
- If `completed`: Show `aiki task show` for history
- If parent task: Show how to work on subtasks

---

## Implementation Plan

### Phase 1: Core Markdown Output

**Deliverables:**
- Add `--markdown` flag to `aiki task show`
- Implement markdown formatter for task data
- Support all task fields (name, status, priority, assignee, timestamps)
- Format sources, comments, and subtasks
- Handle edge cases (no sources, no comments, etc.)

**Files:**
- `cli/src/commands/task.rs` - Add `--markdown` flag to show subcommand
- `cli/src/formatters/markdown.rs` - New module for markdown formatting
- `cli/src/formatters/mod.rs` - Export markdown formatter

### Phase 2: Enhanced Formatting

**Deliverables:**
- Status indicators with unicode symbols (✓, !, etc.)
- Duration calculations and formatting
- Contextual help commands in Details section
- Pretty timestamp formatting (relative times: "2 hours ago")

**Files:**
- `cli/src/formatters/markdown.rs` - Enhanced formatting logic

### Phase 3: Extended Commands

**Deliverables:**
- Add `--markdown` to `aiki task list` (table format)
- Add `--markdown` to other task commands where applicable
- Consistent markdown output across all commands

**Files:**
- `cli/src/commands/task.rs` - Add flag to other subcommands
- `cli/src/formatters/markdown.rs` - Add list formatting

---

## Examples

### Simple Task (No Subtasks)

```bash
aiki task show simple123 --markdown
```

```markdown
# Task: Update README

**ID:** `simple123`  
**Status:** Completed ✓  
**Priority:** P2 (Normal)  
**Assignee:** claude-code

**Created:** 2026-02-01 09:00:00 UTC  
**Started:** 2026-02-01 09:05:00 UTC  
**Completed:** 2026-02-01 09:15:00 UTC  
**Duration:** 10 minutes

---

## Sources

- `prompt:readme-update`

---

## Progress

### Comments

**2026-02-01 09:15:00 UTC**
> Added installation instructions and updated examples
```

### Parent Task with Subtasks

```bash
aiki task show parent456 --markdown
```

```markdown
# Task: Implement authentication system

**ID:** `parent456`  
**Status:** In Progress  
**Priority:** P1 (High)  
**Assignee:** claude-code

**Created:** 2026-02-01 08:00:00 UTC  
**Started:** 2026-02-01 08:10:00 UTC  
**Duration:** 2 hours 15 minutes

---

## Sources

- `file:ops/now/auth-system.md`
- `task:planning789`

---

## Subtasks

- [x] `parent456.1` Add JWT library dependency
- [x] `parent456.2` Create auth middleware
- [>] `parent456.3` Add login endpoint *(in progress)*
- [ ] `parent456.4` Add token validation
- [ ] `parent456.5` Write tests

---

## Details

To work on subtasks:
```bash
aiki task start parent456.3  # Continue current subtask
aiki task start parent456.4  # Start next subtask
```

To add a progress update:
```bash
aiki task comment parent456 "Your update"
```
```

### Blocked Task

```bash
aiki task show blocked789 --markdown
```

```markdown
# Task: Deploy to production

**ID:** `blocked789`  
**Status:** Blocked  
**Priority:** P0 (Urgent)  
**Assignee:** claude-code

**Created:** 2026-02-01 07:00:00 UTC

---

## Sources

- `file:ops/now/deploy-plan.md`

---

## Progress

### Comments

**2026-02-01 07:30:00 UTC**
> Blocked waiting for staging environment approval

---

## Details

To unblock this task, resolve the blocker then:
```bash
aiki task start blocked789
```
```

---

## Open Questions

1. **List format** - Should `aiki task list --markdown` output a table, or individual task summaries?
   - Recommendation: Start with table format for list view, full format for show

2. **Color support** - Should markdown output include ANSI colors when outputting to terminal?
   - Recommendation: No colors in markdown (keep it portable), use XML output for colored terminal display

3. **Template customization** - Should users be able to customize the markdown template?
   - Recommendation: v2 feature - start with fixed format

4. **Multi-task output** - Should we support `aiki task show task1 task2 --markdown`?
   - Recommendation: v2 feature - start with single task

---

## Future Enhancements (v2)

**Custom Templates:**
- User-defined markdown templates
- Template variables for task fields
- Stored in `.aiki/templates/task-show.md`

**Extended Formatting:**
- Diff view for task changes over time
- Timeline visualization of task progress
- Graph view for task dependencies

**Export Options:**
- `--format json` for JSON output
- `--format html` for HTML output
- Export multiple tasks to single document

---

## Summary

Adding `--markdown` to `aiki task show` provides human-readable task output for:
- Documentation and sharing
- Human review and reporting
- Archival and record-keeping

The flag complements (not replaces) XML output, giving users the best of both worlds:
- **XML** - Machine-readable, structured, for tools
- **Markdown** - Human-readable, portable, for people

**Implementation:** Single flag, straightforward formatter, no breaking changes to existing behavior.
