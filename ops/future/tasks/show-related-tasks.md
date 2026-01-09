# Show Related Tasks

**Status**: Future Idea  
**Related**: Task System

---

## Motivation

Help users discover related work by showing tasks that share context.

---

## Design

Add `--with-related` flag to `aiki task show`:

```bash
aiki task show err-abc --with-related
```

---

## Example Output

```xml
<aiki_task>
  <task id="err-abc">
    <name>Fix null check in auth.ts</name>
    <related>
      <same_file>
        <task id="err-def"/>
      </same_file>
      <blocks/>
      <blocked_by/>
      <siblings>
        <task id="feat-123.1"/>
        <task id="feat-123.3"/>
      </siblings>
    </related>
  </task>
</aiki_task>
```

---

## Implementation Notes

- Query tasks by file scope overlap
- Show blocking/blocked relationships
- Show sibling tasks (same parent)
- Consider adding "frequently worked on together" heuristic
