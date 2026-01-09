# Code Provenance

**Status**: Future Idea  
**Related**: Task System Phase 1-5

---

## Goal

Track which JJ changes attempted/fixed tasks (bidirectional linking).

---

## Features

- Tasks reference JJ change IDs when closed
- JJ change descriptions reference tasks worked on/closed
- Query which changes touched a task
- Query which tasks a change affected

---

## Examples

### Close Task with Code Change Reference

```bash
# Automatically captures current JJ change_id
aiki task close err-abc
```

### View Task's Code History

```bash
aiki task show err-abc --code-history

# XML output:
<aiki_task>
  <task id="err-abc">
    <name>Fix null check in auth.ts</name>
    <attempts>
      <attempt change_id="qpvuntsm" outcome="stopped" reason="Wrong approach"/>
      <attempt change_id="rlvkpnrz" outcome="done"/>
    </attempts>
  </task>
</aiki_task>
```

### View Change's Tasks

```bash
aiki provenance qpvuntsm --tasks

# XML output:
<provenance change_id="qpvuntsm">
  <works_on>
    <task id="err-abc"/>
  </works_on>
  <closes>
    <task id="err-def"/>
  </closes>
</provenance>
```

---

## Implementation

- `code_change` field on `Closed` event
- Store task references in JJ change description `[aiki]` block
- Commands to query bidirectional links
