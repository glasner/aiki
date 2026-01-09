# Task Search

**Status**: Future Idea  
**Related**: Task System

---

## Motivation

Allow searching tasks by name, body content, or other attributes.

---

## Design

```bash
aiki task search "auth"
```

---

## Example Output

```xml
<aiki_task>
  <list total="2">
    <task id="err-abc" match="name">Fix null check in auth.ts</task>
    <task id="feat-123" match="name">Add authentication</task>
  </list>
</aiki_task>
```

---

## Implementation Notes

- Search across task name, body, scope files
- Support regex patterns
- Highlight matching text in results
- Consider adding filters: `aiki task search "auth" --open --p0`
