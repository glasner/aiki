# Session Progress Summary

**Status**: Future Idea  
**Related**: Task System, Sessions

---

## Motivation

Show what was accomplished in the current session.

---

## Design

```bash
aiki task summary
```

---

## Example Output

```xml
<session_summary>
  <completed>
    <task id="err-abc"/>
    <task id="err-def"/>
  </completed>
  <in_progress>
    <task id="feat-xyz"/>
  </in_progress>
  <created count="5"/>
  <time_active>45 minutes</time_active>
</session_summary>
```

---

## Implementation Notes

- Query tasks by session_id from current session
- Show completed vs created ratio
- Calculate active time from session events
- Consider adding to `aiki sessions show` output
