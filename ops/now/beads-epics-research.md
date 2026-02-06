# Beads Epics Research

**Date**: 2026-02-06
**Purpose**: Understand why epics were introduced in beads and how they evolved

---

## Background: What is Beads?

[Beads](https://github.com/steveyegge/beads) is a git-backed issue tracker created by Steve Yegge, designed specifically for AI coding agents. It stores issues as JSONL in `.beads/`, uses SQLite as a local cache, and treats git as the source of truth. The project was vibe-coded in ~6 days with Claude, going from idea to 1000+ GitHub stars.

The core problem beads solves is **agent amnesia** -- what Yegge calls the "50 First Dates" problem. Agents wake up each session with no memory of prior work. Markdown plans rot fast and aren't queryable. Beads gives agents persistent, structured, dependency-aware memory that travels with the code in git.

## Why Epics Were Introduced

### The Problem: Plan Jungle

Before beads, Yegge had accumulated **605 markdown plan files** in varying stages of decay. He describes having "fever dreams of an agent-friendly work plan: hierarchical, well-organized, flexible, versioned, adaptive, and easy to turn into a prioritized work queue." His agents -- "paratroopers dropped onto Plan Mountain" -- always got lost.

The fundamental issues with markdown plans:
- **Not queryable** -- agents must parse dozens of files to reconstruct the work graph
- **Bit-rot fast** -- agents rarely update plans as they work
- **No dependency tracking** -- no way to know what blocks what
- **No hierarchy** -- flat lists can't model complex projects

### The Solution: Hierarchical Issues with Epics

Epics were part of beads' design from the start. Yegge describes the schema design process: **"I left the schema up to Claude, asking only for parent/child pointers (for epics) and blocking-issue pointers."** Claude then designed four dependency link types (later five), making the schema more powerful than GitHub Issues while remaining simple.

Epics exist because real projects have natural hierarchical decomposition:
- A large initiative (epic) breaks into features/tasks
- Tasks break into subtasks
- Dependencies exist both within and across these hierarchies

Without epics, agents couldn't model "Auth System" -> "Login UI" + "JWT middleware" + "Session management". They'd just have a flat list of disconnected tasks.

## How Epics Work

### Hierarchical IDs

Beads uses a dot-notation hierarchy:

```
bd-a3f8       (Epic)
bd-a3f8.1     (Task under epic)
bd-a3f8.1.1   (Sub-task)
```

IDs are hash-based to prevent merge collisions in multi-agent/multi-branch workflows.

### Issue Types

```go
TypeBug     // Bug reports
TypeFeature // Feature requests
TypeTask    // General tasks
TypeEpic    // Epics (parent containers)
TypeChore   // Maintenance work
```

An epic is created with: `bd create "Auth System" -t epic -p 1`

Child tasks are created under it and automatically get `.1`, `.2`, `.3` suffixes.

### Five Dependency Types

Claude designed these dependency types for the schema:

| Type | Purpose |
|------|---------|
| `DepBlocks` | Standard blocking dependency |
| `DepRelated` | Related issues (informational) |
| `DepParentChild` | Epic/subtask hierarchy |
| `DepDiscoveredFrom` | Provenance -- how work was discovered during agent sessions |
| `DepConditionalBlocks` | B runs only if A fails |

The `DepParentChild` type is what gives epics their structure. The `DepDiscoveredFrom` type is unique to beads -- it tracks provenance, giving agents "unprecedented sleuthing and forensics powers when trying to figure out how a train-wreck happened with multiple workers."

### Blocking Propagation

A key epic behavior: **when a parent (epic) is blocked, all children are automatically blocked**, even if they have no direct blockers. This prevents agents from working on subtasks of a stalled initiative.

### Epic Status

Beads defines a dedicated `EpicStatus` type (separate from regular `Status`) for tracking epic-level progress across all children.

## Evolution of Beads

### Phase 1: TypeScript + PostgreSQL

Beads started as a TypeScript project with PostgreSQL. The problem: Claude kept running `DROP TABLE` and destroying the database. The architecture was fragile.

### Phase 2: Go + Git/JSONL + SQLite

Yegge rewrote beads in Go with a git-backed JSONL storage model:
- Issues stored as JSONL lines in `.beads/beads.jsonl`
- SQLite used as a fast local cache (hydrates from JSONL on demand)
- Git provides versioning, branching, and merge conflict resolution
- Background daemon for auto-sync

This solved both the data loss problem (git makes it nearly impossible to lose data permanently) and the performance problem (SQLite for queries, git for durability).

Yegge's assessment: "AIs cannot seem to stop themselves from writing dodgy TypeScript, whereas it doesn't seem possible to write bad Go code. The worst it ever gets is mediocre."

### Phase 3: Community Ecosystem

Multiple community tools emerged around epics:
- **beads_viewer** (Go TUI by Jeff Emanuel) -- kanban + graph views with HITS analysis identifying epics
- **beads_rust** (Rust port by Jeff Emanuel) -- frozen at classic SQLite + JSONL architecture
- **beads-kanban-ui** -- web kanban with epic/subtask management
- **bsv** (Rust TUI) -- tree navigation organized by epic/task/subtask
- **beads-orchestration** -- Claude Code multi-agent system with epic/subtask support

### Phase 4: Gas Town (Multi-Agent Orchestration)

Yegge built Gas Town on top of beads -- a multi-agent orchestration framework. This represents the evolution from "agents working on individual tasks" to "swarms of agents working on epics." Yegge describes "hurling swarms of Claude Code Opus instances at a big epic, or bug backlog."

## Key Design Insights

### Agent-First, Not Human-Adapted

> "Beads feels like it was designed for how agents actually work, not adapted from human workflows. The --json flags everywhere, the discovered-from dependency type, the ready-work detection -- these aren't features bolted onto a human tool. They're primitives for agent cognition."

### Epics as Organizational Containers

Epics serve as more than just grouping -- they're computational units in the dependency graph. The `bd dep tree bd-a3f8e9` command shows the complete hierarchy, and `bd ready` can identify which subtasks within an epic are ready for work based on resolved dependencies.

### Real-World Scale

In one session, Yegge generated **128 issues, six main epics, with five sub-epics**, featuring complex interdependencies and parent/child relationships. This demonstrates that epics aren't a theoretical feature -- they're essential for managing real project complexity.

### Pattern: Directory-to-Epic Mapping

One notable usage pattern: treating every directory as an epic and every file as a bead. This "forces" agents to methodically work through each file in a refactoring effort rather than getting lost.

## Relevance to Aiki

Aiki's task system is [documented as heavily inspired by beads](../done/task-system.md#relationship-to-beads). Key differences:

| Aspect | Beads | Aiki |
|--------|-------|------|
| Storage | JSONL + SQLite + git | Event-sourced JJ branch |
| Hierarchy | Dot-notation IDs (`bd-a3f8.1`) | Dot-notation IDs (`parent.1`) |
| Dependencies | 5 types (blocks, related, parent-child, discovered-from, conditional) | Blocks only (currently) |
| Types | Bug, Feature, Task, Epic, Chore | No type system (yet) |
| Output | JSON-first | XML-first |
| Integration | Standalone binary | Deeply integrated with flows |
| Provenance | `discovered_from` dependency | JJ change ID linking |

The most relevant lessons from beads' epic design for aiki:
1. **Hierarchical IDs work** -- dot-notation is intuitive and queryable
2. **Blocking propagation through hierarchy** is essential
3. **Epics need dedicated status tracking** beyond individual task status
4. **Agent-first output format matters** -- beads uses JSON, aiki uses XML
5. **The dependency graph is the key differentiator** over flat task lists

---

## Sources

- [steveyegge/beads (GitHub)](https://github.com/steveyegge/beads)
- [Beads README](https://github.com/steveyegge/beads/blob/main/README.md)
- [Beads Quickstart](https://github.com/steveyegge/beads/blob/main/docs/QUICKSTART.md)
- [Beads Go Package](https://pkg.go.dev/github.com/steveyegge/beads)
- [Introducing Beads (Medium)](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a)
- [The Beads Revolution (Medium)](https://steve-yegge.medium.com/the-beads-revolution-how-i-built-the-todo-system-that-ai-agents-actually-want-to-use-228a5f9be2a9)
- [Beads Best Practices (Medium)](https://steve-yegge.medium.com/beads-best-practices-2db636b9760c)
- [Beads Blows Up (Medium)](https://steve-yegge.medium.com/beads-blows-up-a0a61bb889b4)
- [The Future of Coding Agents (Medium)](https://steve-yegge.medium.com/the-future-of-coding-agents-e9451a84207c)
- [Dicklesworthstone/beads_viewer (GitHub)](https://github.com/Dicklesworthstone/beads_viewer)
- [Dicklesworthstone/beads_rust (GitHub)](https://github.com/Dicklesworthstone/beads_rust)
- [Beads Community Tools](https://github.com/steveyegge/beads/blob/main/docs/COMMUNITY_TOOLS.md)
- [Aiki Task System Design](../done/task-system.md)
