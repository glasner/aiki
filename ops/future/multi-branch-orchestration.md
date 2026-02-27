# Multi-Branch Orchestration

**Date**: 2026-02-27
**Status**: Exploratory
**Level**: 3 of 3 — [Git Branch Support](../meta/git-branch-support.md)
**Depends on**: [Branched Sessions](../next/branched-sessions.md) (Level 2)

---

## Problem

Even with per-branch workspaces, someone has to decide which agent works on which branch, and someone has to merge the results. Today that's entirely manual.

## Vision

A planner agent decomposes a large task into independent branches, spawns agents per branch, and orchestrates merging when they're done. Essentially: CI/CD built into aiki.

## Why This Is a Different Product

Level 1 and 2 extend the existing workspace isolation model. Level 3 is a fundamentally different architecture:

- **Merge orchestration** — conflict resolution across branches is much harder than within a linear chain. You need a strategy (rebase? merge commit? squash?) and a way to handle failures.
- **Agent coordination** — a planner needs to understand task dependencies, parallelize safely, and handle partial failures.
- **Branch lifecycle management** — creating, naming, tracking, merging, and cleaning up branches at scale.
- **Conflict resolution at merge time** — agent work might be "wasted" if branches can't merge cleanly. Need strategies for re-doing work on top of merged state.

## Rough Shape

```
User: "Implement auth system"
  └─ Planner decomposes into:
       ├─ Branch: auth/database-schema   → Agent A
       ├─ Branch: auth/api-endpoints     → Agent B
       └─ Branch: auth/frontend-forms    → Agent C
  └─ Planner merges results:
       ├─ A + B merge cleanly ✓
       └─ (A+B) + C has conflicts → resolve or re-run C
```

## Prerequisites

- Level 2 (branched sessions) must be proven in real use
- Understanding of how often branch merges conflict in practice
- Design for how agents handle merge failures
- Task graph system mature enough for dependency tracking

## Open Questions

1. Is this even the right model? Maybe sequential with checkpoints is better than parallel with merges.
2. How does the planner know which tasks are independent enough to parallelize?
3. What's the failure mode when a merge can't be resolved automatically?
4. Could JJ's native conflict handling (conflicts as first-class objects) make this easier than git-style merging?
