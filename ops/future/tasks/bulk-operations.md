# Bulk Operations

**Status**: Future Idea  
**Related**: Task System Phase 1-5

---

## Motivation

Efficiently create multiple tasks or blockers at once.

---

## Design

### Bulk Task Creation via Heredoc

```bash
aiki task add <<EOF
Fix null check in auth.ts
Refactor user validation
Add integration tests
EOF

# XML output:
<aiki_task>
  <added>
    <task id="abc">Fix null check in auth.ts</task>
    <task id="def">Refactor user validation</task>
    <task id="ghi">Add integration tests</task>
  </added>
  
  <context>
    <list ready="3">
      <!-- ... -->
    </list>
  </context>
</aiki_task>
```

### Create Parent with Children

```bash
aiki task add "Add authentication" --children <<EOF
Create User model
Add login endpoint
Add JWT middleware
Update frontend
EOF

# XML output:
<aiki_task>
  <added>
    <task id="abc">
      <name>Add authentication</name>
      <children>
        <task id="abc.1">Create User model</task>
        <task id="abc.2">Add login endpoint</task>
        <task id="abc.3">Add JWT middleware</task>
        <task id="abc.4">Update frontend</task>
      </children>
    </task>
  </added>
</aiki_task>
```

### Bulk Blocker Creation

```bash
aiki task stop abc --blocked <<EOF
Get API credentials from ops
Wait for design decision
Fix dependency bug in lib
EOF

# XML output:
<aiki_task>
  <stopped reason="Blocked by: xyz, def, ghi">
    <task id="abc">
      <name>Fix null check in auth.ts</name>
    </task>
  </stopped>
  
  <added>
    <task id="xyz" assignee="human">Get API credentials from ops</task>
    <task id="def" assignee="human">Wait for design decision</task>
    <task id="ghi" assignee="claude-code">Fix dependency bug in lib</task>
  </added>
</aiki_task>
```

---

## Why Not Phase 1-2

- Adds parsing complexity (heredoc input)
- Current API is clear and explicit
- Most task creation is singular in practice
- Can always add multiple children one at a time with `--parent`

---

## Implementation Notes

- Parse heredoc input line-by-line
- Auto-assign priorities and types per line
- Atomic operation (all succeed or all fail)
