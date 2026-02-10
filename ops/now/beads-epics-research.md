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

Epics were part of beads' design from the start. Yegge describes the schema design process: **"I left the schema up to Claude, asking only for parent/child pointers (for epics) and blocking-issue pointers."** Claude then designed the dependency types, making the schema more powerful than GitHub Issues while remaining simple.

Epics exist because real projects have natural hierarchical decomposition:
- A large initiative (epic) breaks into features/tasks
- Tasks break into subtasks
- Dependencies exist both within and across these hierarchies

Without epics, agents couldn't model "Auth System" -> "Login UI" + "JWT middleware" + "Session management". They'd just have a flat list of disconnected tasks.

## How Epics Work

### Not a Separate Struct

A critical design decision: **epics use the exact same `Issue` struct as all other issue types.** There is no separate `Epic` struct. An epic is simply an `Issue` with `IssueType == "epic"`. This keeps the model simple -- an epic is just an issue that happens to have children.

### Hierarchical IDs

Beads uses a dot-notation hierarchy:

```
bd-a3f8       (Epic)
bd-a3f8.1     (Task under epic)
bd-a3f8.1.1   (Sub-task)
```

IDs are hash-based to prevent merge collisions in multi-agent/multi-branch workflows.

### Issue Types (5 built-in)

```go
TypeBug     IssueType = "bug"
TypeFeature IssueType = "feature"
TypeTask    IssueType = "task"
TypeEpic    IssueType = "epic"
TypeChore   IssueType = "chore"
```

Each type has different lint requirements:
- **Epic** requires `## Success Criteria`
- **Task/Feature** requires `## Acceptance Criteria`
- **Bug** requires `## Steps to Reproduce`

### Parent-Child via Dependencies

Parent-child is implemented entirely through the dependency system, not through a dedicated field on the Issue struct:

```
dependencies table:
  issue_id      = child issue ID
  depends_on_id = parent (epic) ID
  type          = "parent-child"
```

The `IssueDetails` struct does add a computed `Parent *string` field for convenience.

### Dependency Types (19 total, evolved from 4 original)

The original 4 types designed by Claude:

| Type | Category | Blocks Ready Work? |
|------|----------|-------------------|
| `blocks` | Workflow | Yes |
| `parent-child` | Workflow | Yes |
| `related` | Association | No |
| `discovered-from` | Association | No |

Later additions in chronological order:

| Type | Category | Blocks? | When Added |
|------|----------|---------|------------|
| `conditional-blocks` | Workflow | Yes | Early (B runs only if A fails) |
| `relates-to` | Graph link | No | v0.30 |
| `replies-to` | Graph link | No | v0.30 (conversation threading) |
| `duplicates` | Graph link | No | v0.30 |
| `supersedes` | Graph link | No | v0.30 (version chains) |
| `waits-for` | Workflow | Yes | v0.36 (fanout gate coordination) |
| `tracks` | Convoy | No | v0.42 (cross-project references) |
| `authored-by` | Entity (HOP) | No | v0.47 |
| `assigned-to` | Entity (HOP) | No | v0.47 |
| `approved-by` | Entity (HOP) | No | v0.47 |
| `attests` | Entity (HOP) | No | v0.47 (skill attestation) |
| `until` | Reference | No | Later |
| `caused-by` | Reference | No | Later (audit trail) |
| `validates` | Reference | No | Later |
| `delegated-from` | Delegation | No | Later (completion cascades up) |

Only 4 of 19 types block ready work: `blocks`, `parent-child`, `conditional-blocks`, `waits-for`.

Custom types are also allowed (any string up to 50 chars).

### Blocking Propagation Through Hierarchy

Beads uses a **cached blocking model** with a `blocked_issues_cache` table (rebuilt transactionally). Four blocking mechanisms:

1. **Direct blocks**: A `blocks` dependency from B to open A makes B blocked
2. **Conditional blocks**: B blocked by `conditional-blocks` until A closes with a failure keyword
3. **Waits-for gates**: B blocked until all dynamic children of a spawner are closed
4. **Parent-child propagation (recursive)**: If any descendant is blocked, the blockage cascades UP to the parent via recursive CTE (depth limit: 50 levels)

Note: blockage propagates **upward** (child blocked → parent blocked), not downward as I initially reported. This prevents claiming an epic is "ready" when some of its children are stuck.

### EpicStatus Type

```go
type EpicStatus struct {
    Epic             *Issue `json:"epic"`
    TotalChildren    int    `json:"total_children"`
    ClosedChildren   int    `json:"closed_children"`
    EligibleForClose bool   `json:"eligible_for_close"`
}
```

`GetEpicsEligibleForClosure()` identifies open epics where all children are closed. This enables auto-closure of epics when all subtasks complete.

### The `discovered-from` Type

During work on an issue, agents naturally discover edge cases, bugs, or refactoring needs. `discovered-from` links the newly created issue back to the issue being worked on when the discovery was made.

Key properties:
- **Non-blocking** -- does NOT affect the `bd ready` queue
- **Association type** -- informational, not workflow
- **Provenance tracking** -- answers "why does this issue exist?"
- **Excluded from cycle detection** -- can't create problematic cycles

## Evolution of Beads

### Phase 1: TypeScript + PostgreSQL

Beads started as a TypeScript project with PostgreSQL. The problem: Claude kept running `DROP TABLE` and destroying the database. The architecture was fragile.

### Phase 2: Go + Git/JSONL + SQLite

Yegge rewrote beads in Go with a git-backed JSONL storage model:
- Issues stored as JSONL lines in `.beads/beads.jsonl`
- SQLite used as a fast local cache (hydrates from JSONL on demand)
- Git provides versioning, branching, and merge conflict resolution
- Background daemon for auto-sync

### Phase 3: Dependency Type Explosion

The dependency system evolved from 4 original types to 19:
- **v0.30**: Graph link types (`relates-to`, `replies-to`, `duplicates`, `supersedes`) for knowledge graph and conversation threading
- **v0.36**: `waits-for` for async gate coordination (fanout patterns)
- **v0.42**: `tracks` for cross-project convoy membership
- **v0.47**: HOP entity types (`authored-by`, `assigned-to`, `approved-by`, `attests`) for identity/governance
- **Later**: Reference types (`until`, `caused-by`, `validates`) and delegation (`delegated-from`)

### Phase 4: Gas Town (Multi-Agent Orchestration)

Yegge built Gas Town on top of beads -- a multi-agent orchestration framework. This represents the evolution from "agents working on individual tasks" to "swarms of agents working on epics."

## Comparison: Beads vs. Aiki

### Provenance: `discovered-from` vs. `--source`

Beads' `discovered-from` dependency and aiki's `--source` flag solve the same problem -- **tracing why work exists** -- but with different mechanisms:

| Aspect | Beads `discovered-from` | Aiki `--source` |
|--------|------------------------|-----------------|
| Mechanism | Dependency edge in graph | Field on task event |
| Storage | Row in dependencies table | `source=` lines in event metadata |
| Multiplicity | One edge per relationship | Multiple sources per task (`Vec<String>`) |
| Queryable | Via dependency graph traversal | Via `aiki task list --source` |
| Direction | Child → parent (discovered during) | Task → origin (came from) |
| Types | Single type | Typed prefixes: `file:`, `task:`, `comment:`, `issue:`, `prompt:` |
| Auto-resolve | No | `--source prompt` auto-resolves to JJ change_id |

Aiki's source system is actually **richer in provenance types** -- it distinguishes between file origins, task origins, comment origins, issue origins, and prompt origins as first-class distinctions, whereas beads uses a single `discovered-from` edge type.

### Hierarchy

| Aspect | Beads | Aiki |
|--------|-------|------|
| ID format | `bd-` + hash + `.N` | 32-char k-z + `.N` |
| Parent-child | Dependency edge (`parent-child` type) | Encoded in ID (via `get_parent_id()`) |
| Auto-close | `GetEpicsEligibleForClosure()` | Parent auto-closes when all subtasks close |
| Scoping | N/A (flat list, filtered) | `ScopeSet` - starting parent scopes view to subtasks only |

Aiki's scoping model (starting a parent hides other root tasks and shows only subtasks) has no direct equivalent in beads.

### Task Types

| Aspect | Beads | Aiki |
|--------|-------|------|
| Type field | `IssueType` enum (5 built-in) | `task_type: Option<String>` (free-form) |
| Built-in types | bug, feature, task, epic, chore | None enforced; inferred from name/sources |
| Type inference | No | Yes (`infer_task_type()`: "review" if name contains review, "bug" if contains fix/bug, etc.) |
| Lint per type | Yes (different required sections) | No |
| Flow dispatch | No | Yes (`review.started`, `bug.closed` hooks) |

### Full Comparison Table

| Aspect | Beads | Aiki |
|--------|-------|------|
| Storage | JSONL + SQLite + git | Event-sourced JJ branch (no files) |
| Hierarchy | Dot-notation, dependency-based | Dot-notation, ID-encoded |
| Dependencies | 19 types (4 blocking) | `blocked_reason` field (no dep graph) |
| Types | 5 built-in enum | Free-form string, inferred |
| Provenance | `discovered-from` edge | `--source` with typed prefixes |
| Output | JSON-first | XML-first |
| Ready queue | `bd ready` (graph-aware) | `get_ready_queue()` (scope-aware) |
| Epic status | `EpicStatus` struct | Parent auto-close on all subtasks done |
| Blocking propagation | Upward recursive CTE, cached | Not implemented (no dep graph) |
| Templates | No | Yes (with variable substitution) |
| Flow integration | Standalone binary | Deep (task events trigger hooks) |

### Key Takeaways for Aiki

1. **Aiki's `--source` is already richer than beads' `discovered-from`** -- typed prefixes, auto-resolution, and multi-source support are features beads doesn't have. The source system is a genuine innovation.

2. **The missing piece is a dependency graph.** Beads' power comes from blocking propagation through the hierarchy and `bd ready` computing truly ready work. Aiki has the hierarchy but not the graph-aware blocking.

3. **Epic as a type vs. epic as a pattern.** Beads makes `epic` a first-class type with lint requirements (`## Success Criteria`). Aiki treats any parent task as implicitly epic-like. Both approaches work -- beads is more explicit, aiki is more fluid.

4. **19 dep types may be over-engineered for most use cases.** Only 4 of 19 affect ready work. The HOP entity types and graph links serve Gas Town's multi-agent orchestration needs more than typical single-agent workflows.

5. **Blocking propagates upward, not downward.** This is the right design -- it answers "can I claim this epic is ready?" rather than "should I stop all children because the parent is stuck?"

---

## Sources

- [steveyegge/beads (GitHub)](https://github.com/steveyegge/beads)
- [Beads README](https://github.com/steveyegge/beads/blob/main/README.md)
- [Beads Quickstart](https://github.com/steveyegge/beads/blob/main/docs/QUICKSTART.md)
- [Beads Go Package](https://pkg.go.dev/github.com/steveyegge/beads)
- [Beads CHANGELOG](https://github.com/steveyegge/beads/releases)
- [Beads Architecture](https://github.com/steveyegge/beads/blob/main/docs/ARCHITECTURE.md)
- [Beads Graph Links](https://github.com/steveyegge/beads/blob/main/docs/graph-links.md)
- [Beads FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)
- [Introducing Beads (Medium)](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a)
- [The Beads Revolution (Medium)](https://steve-yegge.medium.com/the-beads-revolution-how-i-built-the-todo-system-that-ai-agents-actually-want-to-use-228a5f9be2a9)
- [Beads Best Practices (Medium)](https://steve-yegge.medium.com/beads-best-practices-2db636b9760c)
- [Dicklesworthstone/beads_viewer (GitHub)](https://github.com/Dicklesworthstone/beads_viewer)
- [Aiki Task System Design](../done/task-system.md)
- Aiki source: `cli/src/tasks/types.rs`, `cli/src/tasks/id.rs`, `cli/src/tasks/storage.rs`, `cli/src/tasks/manager.rs`, `cli/src/commands/task.rs`
