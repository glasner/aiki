# Branch-Aware Absorption

**Date**: 2026-02-27
**Status**: Plan
**Level**: 1 of 3 — [Git Branch Support](../meta/git-branch-support.md)

---

## Problem

When the user switches git branches between turns (e.g., `git checkout feature-x`), isolated workspaces don't adapt. The workspace is still forked from the old branch's `@-`. Absorption rebases the agent's work onto a different lineage than where it started, leading to subtle bugs or misplaced changes.

## Goal

Detect when the main workspace's `@-` has moved to a different lineage and handle it gracefully — either re-forking the workspace or destroying and recreating it.

## Design

On turn start, during workspace reuse in `create_isolated_workspace`, check if the workspace's current parent is still an ancestor of the new `@-`. If not, the user has switched branches and the workspace needs to be reset.

```rust
// When reusing a workspace, check if @- is still on the same lineage
// If the user switched branches, the workspace's parent is no longer
// an ancestor of @-. In that case, destroy and recreate from the new @-.
```

**Detection**: Use `jj log -r '{parent}::@-'` to check ancestry. If the revset is empty, the lineages have diverged.

**Recovery**: Fall through to the destroy/recreate path that already exists for stale workspaces.

## Scope

- ~20 lines in `isolation.rs`
- Only affects `create_isolated_workspace` reuse path
- No UX changes — fully transparent to user and agent

## Open Questions

1. Should we also handle the case where the user creates a *new* branch (`git checkout -b new-feature`)? This is common and the agent should work from the new branch.
2. Should we emit a warning/event when re-forking so the agent knows its context may be stale?
