---
status: draft
---

# Needs Review Status

**Date**: 2026-02-06
**Status**: Idea
**Priority**: P3

---

## Problem

When an agent closes a task, it looks "done" — but it hasn't been reviewed yet. There's a window between close and review where a task appears ready to ship but really isn't.

```
         agent closes          review starts       review approves
              │                     │                    │
──────────────┼─────────────────────┼────────────────────┼──────
              │◄── looks done ─────►│◄── in review ─────►│ actually done
              │    but isn't        │    but still        │
              │                     │    looks done       │
```

"Closed" currently conflates two meanings:
- **"I finished my work"** — agent is done coding
- **"This is ready to ship"** — someone verified it's good

---

## Proposed Approach: Close Outcome `needs_review`

Add a new close outcome alongside `done` and `wont_do`. When an agent closes a task, it uses `--needs-review` (or this is the default for agent-closed tasks). The task shows up differently in listings until a review approves it.

```bash
# Agent finishes work
aiki task close <id> --needs-review --summary "Implemented feature"

# Task shows as needing review in listings
aiki task
#  ⬡ abc  "Implement feature"  [needs_review]

# After review approves, status updates to done
aiki review <id> --start
# ... review approves ...
# Task outcome becomes done
```

### Why this approach

- Smallest change that solves the problem
- Reuses existing close machinery (outcome field)
- `aiki task` can visually distinguish reviewed vs unreviewed work
- No new statuses needed — task is still "closed", just with a different outcome
- Compatible with the fix-original-task plan (fix reopens, review approves)

### Alternatives considered

- **New `pending_review` status**: Adds a third open/closed/pending_review state to all task logic. Heavier.
- **Don't close until reviewed**: Requires agents to change behavior (stop instead of close). Awkward.
- **Visual badge only**: Zero workflow change, but no queryable state — can't filter for unreviewed tasks.

---

## Open Questions

- Should `needs_review` be the default outcome when agents close tasks, or opt-in?
- Should `aiki review --approve` automatically flip the outcome to `done`?
- How does this interact with tasks that don't need review (small fixes, docs)?
