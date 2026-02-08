# Experiment: Replace XML Output with Markdown

Test whether `<xml>` output in task commands is actually helping agents, by replacing all task command output with markdown equivalents.

## Scope

All task subcommands that output XML via `XmlBuilder` in `cli/src/commands/task.rs` and formatting functions in `cli/src/tasks/xml.rs`.

Commands affected: `list`, `add`, `start`, `stop`, `close`, `show`, `comment`, `update`, `undo`, `wait`, `template`, plus error responses.

---

## Before/After: Each Command

### `aiki task` (list)

**Before (XML):**
```xml
<aiki_task cmd="list" status="ok">
  <list total="3">
    <task id="abc123..." name="Fix auth bug" priority="p0"/>
    <task id="def456..." name="Add tests" priority="p2" assignee="claude-code"/>
    <task id="ghi789..." name="Update docs" priority="p3"/>
  </list>
  <context>
    <in_progress>
      <task id="jkl012..." name="Current work"/>
    </in_progress>
    <list ready="3">
      <task id="abc123..." name="Fix auth bug" priority="p0"/>
      <task id="def456..." name="Add tests" priority="p2" assignee="claude-code"/>
      <task id="ghi789..." name="Update docs" priority="p3"/>
    </list>
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Tasks (3 ready)

### In Progress
- **jkl012...** — Current work

### Ready
| ID | Priority | Name | Assignee |
|----|----------|------|----------|
| abc123... | p0 | Fix auth bug | |
| def456... | p2 | Add tests | claude-code |
| ghi789... | p3 | Update docs | |
```

---

### `aiki task add "New task"`

**Before (XML):**
```xml
<aiki_task cmd="add" status="ok">
  <added>
    <task id="xyz999..." name="New task" priority="p2"/>
  </added>
  <context>
    <in_progress/>
    <list ready="4">
      <task id="abc123..." name="Fix auth bug" priority="p0"/>
      ...
    </list>
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Added
- **xyz999...** — New task (p2)

### In Progress
(none)

### Ready (4)
- **abc123...** [p0] Fix auth bug
- ...
```

---

### `aiki task start <id>`

**Before (XML):**
```xml
<aiki_task cmd="start" status="ok">
  <stopped reason="Starting new task">
    <task id="old123..." name="Previous task"/>
  </stopped>
  <added>
    <task id="new456..." name="Quick-start task" priority="p2"/>
  </added>
  <started>
    <task id="new456..." priority="p2" name="Quick-start task">
      <instructions><![CDATA[Do the thing...]]></instructions>
    </task>
  </started>
  <context>
    <in_progress>
      <task id="new456..." name="Quick-start task"/>
    </in_progress>
    <list ready="2">
      <task id="abc123..." name="Fix auth bug" priority="p0"/>
      ...
    </list>
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Stopped
- **old123...** — Previous task (reason: Starting new task)

## Added
- **new456...** — Quick-start task (p2)

## Started
- **new456...** [p2] Quick-start task

### Instructions
Do the thing...

### In Progress
- **new456...** — Quick-start task

### Ready (2)
- **abc123...** [p0] Fix auth bug
- ...
```

---

### `aiki task stop`

**Before (XML):**
```xml
<aiki_task cmd="stop" status="ok">
  <stopped reason="Blocked on API">
    <task id="abc123..." name="Fix auth bug"/>
  </stopped>
  <context>
    <in_progress/>
    <list ready="3">
      <task id="abc123..." name="Fix auth bug" priority="p0"/>
      ...
    </list>
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Stopped
- **abc123...** — Fix auth bug (reason: Blocked on API)

### In Progress
(none)

### Ready (3)
- **abc123...** [p0] Fix auth bug
- ...
```

---

### `aiki task close <id> --comment "Done"`

**Before (XML):**
```xml
<aiki_task cmd="close" status="ok">
  <closed outcome="done">
    <task id="abc123..." name="Fix auth bug"/>
  </closed>
  <notice>All subtasks complete. Parent task abc... is now closeable.</notice>
  <context>
    <in_progress/>
    <list ready="2">
      ...
    </list>
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Closed (done)
- **abc123...** — Fix auth bug

> **Notice:** All subtasks complete. Parent task abc... is now closeable.

### In Progress
(none)

### Ready (2)
- ...
```

---

### `aiki task show <id>`

**Before (XML):**
```xml
<aiki_task cmd="show" status="ok">
  <task id="abc123..." name="Fix auth bug" status="in_progress" priority="p0">
    <source type="file" ref="ops/now/design.md"/>
    <instructions><![CDATA[Fix the auth bug in login handler]]></instructions>
    <subtasks>
      <task id="abc123...1" status="closed" name="Investigate root cause"/>
      <task id="abc123...2" status="in_progress" name="Apply fix"/>
    </subtasks>
    <progress completed="1" total="2" percentage="50"/>
    <comments>
      <comment timestamp="2026-02-08T..." id="c1a2b3">Found the issue in token validation</comment>
    </comments>
    <changes count="3">
      <change id="xyz..." timestamp="2026-02-08T..." />
      <change id="uvw..." timestamp="2026-02-08T..." />
    </changes>
  </task>
  <context>
    <in_progress>
      <task id="abc123..." name="Fix auth bug"/>
    </in_progress>
    <list ready="2">
      ...
    </list>
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Task: Fix auth bug
- **ID:** abc123...
- **Status:** in_progress
- **Priority:** p0
- **Source:** file:ops/now/design.md

### Instructions
Fix the auth bug in login handler

### Subtasks (1/2 — 50%)
| ID | Status | Name |
|----|--------|------|
| abc123...1 | closed | Investigate root cause |
| abc123...2 | in_progress | Apply fix |

### Comments
- **2026-02-08T...** (c1a2b3): Found the issue in token validation

### Changes (3)
- xyz... (2026-02-08T...)
- uvw... (2026-02-08T...)

### In Progress
- **abc123...** — Fix auth bug

### Ready (2)
- ...
```

---

### `aiki task comment <id> "Progress update"`

**Before (XML):**
```xml
<aiki_task cmd="comment" status="ok">
  <comment_added task_id="abc123..." timestamp="2026-02-08T...">
    <text>Progress update</text>
  </comment_added>
  <context>
    <in_progress>
      <task id="abc123..." name="Fix auth bug"/>
    </in_progress>
    <list ready="2">
      ...
    </list>
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Comment Added
- **Task:** abc123...
- **Time:** 2026-02-08T...
- **Text:** Progress update

### In Progress
- **abc123...** — Fix auth bug

### Ready (2)
- ...
```

---

### `aiki task update <id> --name "New name" --p0`

**Before (XML):**
```xml
<aiki_task cmd="update" status="ok">
  <updated>
    <task id="abc123..." name="New name" priority="p0"/>
    <data>
      <field key="category" value="auth"/>
    </data>
  </updated>
  <context>
    ...
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Updated
- **abc123...** — New name (p0)
- **Data:** category=auth

### In Progress
- ...

### Ready (N)
- ...
```

---

### `aiki task undo <id>`

**Before (XML):**
```xml
<aiki_task cmd="undo" status="ok">
  <undone task="abc123..." files="5"/>
  <backup bookmark="aiki-undo-backup-20260208"/>
  <context>
    ...
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Undone
- **abc123...** — 5 files reverted
- **Backup:** aiki-undo-backup-20260208

### In Progress
- ...

### Ready (N)
- ...
```

---

### `aiki task wait <id1> <id2>`

**Before (XML):**
```xml
<aiki_task cmd="wait" status="ok">
  <task id="abc123..." name="First task" status="closed" outcome="done">
    <comment>Completed successfully</comment>
  </task>
  <task id="def456..." name="Second task" status="closed" outcome="done">
    <comment>All tests pass</comment>
  </task>
  <context>
    ...
  </context>
</aiki_task>
```

**After (Markdown):**
```markdown
## Wait Complete
| ID | Name | Status | Outcome | Last Comment |
|----|------|--------|---------|--------------|
| abc123... | First task | closed | done | Completed successfully |
| def456... | Second task | closed | done | All tests pass |

### In Progress
- ...

### Ready (N)
- ...
```

---

### Error responses (all commands)

**Before (XML):**
```xml
<aiki_task cmd="start" status="error">
  <error>Task 'abc123...' is closed. Use --reopen --reason to reopen it.</error>
</aiki_task>
```

**After (Markdown):**
```markdown
**Error** (start): Task 'abc123...' is closed. Use --reopen --reason to reopen it.
```

---

## Implementation Notes

### What changes

- `cli/src/tasks/xml.rs` — Replace `XmlBuilder` with `MdBuilder` (or rename); replace all `format_*` functions with markdown equivalents
- `cli/src/commands/task.rs` — Update all call sites (~25 `XmlBuilder::new(...)` usages)
- `cli/src/commands/agents_template.rs` — Update the "Task Output Format" section in `AIKI_BLOCK_TEMPLATE` to show markdown examples instead of XML, and update the "Reading the output" bullets to reference headings instead of XML tags
- `AGENTS.md` — Regenerate via `aiki doctor` after updating the template (AGENTS.md is derived from agents_template.rs)
- No escaping needed (no XML special chars to worry about)
- The `<context>` footer becomes a consistent `### In Progress` / `### Ready` section

### Patterns

| XML Pattern | Markdown Equivalent |
|-------------|-------------------|
| `<aiki_task cmd="X" status="ok">` | `## X` heading |
| `<context>` footer | `### In Progress` + `### Ready (N)` |
| `<task id="..." name="..." .../>` | `- **id** — name (priority)` or table row |
| `<error>msg</error>` | `**Error** (cmd): msg` |
| `<![CDATA[...]]>` | Plain text block |
| `<notice>...</notice>` | `> **Notice:** ...` |
| `scope="..."` attribute | Could be a line: `**Scope:** ...` |

### Testing approach

Run real commands before/after and compare how well agents parse + respond to each format.
