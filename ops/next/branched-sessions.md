# Branched Sessions

**Date**: 2026-02-27
**Status**: Draft
**Level**: 2 of 3 — [Git Branch Support](../meta/git-branch-support.md)
**Depends on**: [Branch-Aware Absorption](../now/branch-aware-absorbtion.md) (Level 1)

---

## Problem

All isolated workspaces absorb into the same target: the main workspace's `@`. There's no way for an agent to work toward a specific branch. If a user wants one agent on `feature-auth` and another on `fix-typo`, both agents' changes merge into the same linear chain.

## Goal

Let the user (or agent) specify which JJ bookmark/branch a workspace should target for absorption, so independent features stay on independent branches.

## Design Sketch

### Workspace Target

Add a `target` field to `IsolatedWorkspace` — either a JJ bookmark name or a change ID. When absorbing, rebase onto the target instead of `@-`/`@`.

```
aiki workspace --target feature-auth
```

### Absorption Changes

- `absorb_workspace` rebases onto the target bookmark's head instead of the default `@`
- The lock model becomes per-target — absorptions to different branches don't need serialization, but absorptions to the same branch still do

### UX Questions (Unresolved)

These need real use cases from Level 1 to answer:

- **How does the user specify the target?** CLI flag? Task metadata? Agent instruction? Session config?
- **What happens when the target bookmark doesn't exist?** Create it automatically? Error?
- **Can an agent switch targets mid-session?** What happens to already-absorbed changes?
- **How does the agent know its target?** Injected context on turn start? Query command?

### JJ Plumbing

JJ already supports this at the mechanical level — workspaces can target any change, and bookmarks can point anywhere. The hard part is the UX and the absorption model, not the underlying operations.

## What's Hard

1. **The single-`@` assumption runs deep** — every `jj` command in absorption resolves relative to the default workspace. Per-target absorption needs explicit workspace context.
2. **Lock contention model changes** — must become per-target instead of per-repo.
3. **User's git state is unpredictable** — if the user also commits to the target branch via git, JJ and git diverge.
4. **Context window burden** — agents need branch context injected and preserved across compaction.

## Open Questions

1. What's the most natural way for users to express "put this agent's work on branch X"?
2. If aiki creates bookmarks for branch targets, who cleans them up?
3. How does this interact with the conflict resolution workflow?
