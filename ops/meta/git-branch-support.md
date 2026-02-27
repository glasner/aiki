# Git Branch Support

**Date**: 2026-02-27
**Status**: Active

---

## Overview

Today, all isolated workspaces fork from the same point (`@-` of the main workspace) and absorb back into the same linear chain. Every concurrent agent session works on the same effective "branch" — the user's current working state.

This limits what's possible: agents can't target different features, branch switches during a session cause subtle bugs, and there's no path to parallel work across independent branches.

Git branch support is a three-level progression, each building on the last:

| Level | Plan | Status | Summary |
|-------|------|--------|---------|
| 1 | [Branch-Aware Absorption](../now/branch-aware-absorbtion.md) | Plan | Handle branch switches gracefully during a session |
| 2 | [Branched Sessions](../next/branched-sessions.md) | Draft | Let workspaces target specific branches |
| 3 | [Multi-Branch Orchestration](../future/multi-branch-orchestration.md) | Exploratory | Automatic fan-out across branches with merge orchestration |

## The Core Tension

The current system is elegant because it has **one absorption target**: the main workspace's `@`. Everything chains linearly. Branch-per-workspace breaks this in fundamental ways:

1. **JJ doesn't have git branches** — JJ has bookmarks, and the relationship between JJ bookmarks and git branches is mediated by `jj git export`.
2. **Absorption assumes a single target** — the two-phase rebase is designed around one `@` that all workspaces funnel into.
3. **The user's git branch is outside aiki's control** — users run `git checkout`, `git branch`, etc. independently.

## What JJ Gives Us for Free

JJ workspaces already support multiple named workspaces, each with their own working copy change. JJ bookmarks can point at any change and be moved freely. `jj git export` pushes bookmarks to git refs. The plumbing exists — the challenge is building the right abstractions on top.

## Strategy

**Level 1** solves a real, current pain point with minimal complexity (~20 lines). It's a clear win.

**Level 2** is where the interesting design questions live. The UX is unclear — how does a user communicate "put this agent's work on branch X"? Real usage of Level 1 should inform these decisions.

**Level 3** is a fundamentally different product. It should only be considered after Level 2 proves the model and we understand real-world branch merge conflict rates.

Each level validates assumptions needed by the next. Don't skip ahead.
