# Evolution of Beads Dependency Types

**Date**: 2026-02-07
**Purpose**: Trace why each of the 19 dependency types was introduced, from original PRs and release notes

---

## Overview

Beads started with 4 dependency types. Over ~10 weeks (Nov 2025 → Feb 2026), it grew to 19. This wasn't planned upfront -- each type was introduced to solve a specific problem that emerged as beads evolved from a simple issue tracker to a multi-agent orchestration platform.

The story follows a clear arc:

1. **Core workflow** (v0.1): `blocks`, `parent-child`, `related`, `discovered-from`
2. **Edge consolidation** (v0.30.6): Decision 004 unifies all edges into one `dependencies` table
3. **Knowledge graph** (v0.30.3): `relates-to`, `replies-to`, `duplicates`, `supersedes`
4. **Error handling** (pre-v0.34): `conditional-blocks`
5. **Cross-project** (v0.34): External dependency references
6. **Molecules & gates** (v0.33-v0.37): `waits-for` for fanout coordination
7. **Convoys** (v0.42): `tracks` for multi-agent group work
8. **HOP entities** (v0.35-v0.47): `authored-by`, `assigned-to`, `approved-by`, `attests`
9. **Audit & delegation** (late): `caused-by`, `validates`, `until`, `delegated-from`

---

## The Original Four (v0.1, ~Nov 2025)

These were present from day one. Yegge said: **"I left the schema up to Claude, asking only for parent/child pointers (for epics) and blocking-issue pointers."** Claude designed the full dependency model.

### 1. `blocks` — "X must close before Y starts"

**Category:** Workflow (affects ready queue)

The most fundamental dependency type. Without it, there's no way to express sequential ordering. Beads' design philosophy is **parallel by default** -- children of an epic all run concurrently unless explicit `blocks` edges create sequence.

Common pitfall documented: "Temporal language inverts dependencies. 'A before B' means `bd dep add B A` (B needs A), not the reverse." This was confusing enough to warrant its own section in ADVANCED.md.

**Ready queue impact:** An issue with an unresolved `blocks` dependency never appears in `bd ready`. The `blocked_issues_cache` (added v0.24.0) materializes this computation for performance.

### 2. `parent-child` — Epic/subtask hierarchy

**Category:** Workflow (affects ready queue)

Enables the hierarchical ID system (`bd-a3f8.1`, `bd-a3f8.1.1`). Parent-child is stored as a dependency edge in the `dependencies` table, not as a field on the Issue struct.

**Key behavior:** Blocking propagates **upward**. If a child is blocked, the parent is also blocked (recursive CTE, depth limit 50). This prevents claiming an epic is "ready" when its children are stuck.

**Added in v0.24.1:** The `--parent` flag on `bd create` was added to make creating parent-child dependencies ergonomic: `bd create "Fix auth" --parent bd-a3f8`.

### 3. `related` — Soft reference link

**Category:** Association (no ready queue impact)

Informational link. "These two issues are related, but neither blocks the other." The simplest of the association types. Excluded from cycle detection since it can't create problematic workflow cycles.

### 4. `discovered-from` — "Found this during work on that"

**Category:** Association (no ready queue impact)

The most agent-specific of the original four. When an agent is working on issue A and discovers a bug, edge case, or refactoring need, it creates issue B with a `discovered-from` edge back to A.

This is pure provenance -- it answers "why does this issue exist?" It gives agents "unprecedented sleuthing and forensics powers when trying to figure out how a train-wreck happened with multiple workers."

**Non-blocking by design:** Discovering work shouldn't prevent the original work from completing. The agent files the finding and keeps going.

---

## Decision 004: Edge Schema Consolidation (v0.30.6, Dec 19)

Before v0.30.6, different relationship types lived in different places -- some as fields on the Issue struct, some in separate tables. **Decision 004** unified everything into a single `dependencies` table:

```sql
CREATE TABLE dependencies (
    issue_id TEXT NOT NULL,
    depends_on_id TEXT NOT NULL,
    type TEXT NOT NULL,           -- the dependency type string
    created_at TEXT NOT NULL,
    created_by TEXT DEFAULT '',
    metadata TEXT DEFAULT '',     -- added in Decision 004
    thread_id TEXT DEFAULT '',    -- added in Decision 004
    PRIMARY KEY (issue_id, depends_on_id)
);
```

The `metadata` and `thread_id` fields were added to support richer edge semantics without proliferating tables.

This consolidation was critical -- it meant adding a new dependency type required zero schema changes. Just define a new string constant and update `IsWellKnown()`. This enabled the rapid expansion that followed.

Release notes: "Phase 1-4: Edge schema consolidation infrastructure" and "traverse all dependency types, not just parent-child" in the graph visualization.

---

## Knowledge Graph Types (v0.30.3, Dec 18)

Released as part of the "messaging & knowledge graph epic" (`bd-kwro`). These four types were introduced together under the "graph links and hooks system."

### 5. `relates-to` — Bidirectional knowledge graph edge

**Category:** Graph link (no ready queue impact)

Different from `related`: `relates-to` is explicitly bidirectional. `bd relate A B` creates links in both directions. `bd unrelate` removes both.

**Why not just use `related`?** The `related` type is a one-directional association. `relates-to` is for building a navigable knowledge graph where you can traverse from either end. Think "see also" links in documentation.

### 6. `replies-to` — Conversation threading

**Category:** Graph link (no ready queue impact)

Creates message threads for agent-to-agent communication. This was introduced alongside `bd mail` commands and identity configuration.

**Motivation:** Gas Town needed agents to communicate. Rather than a separate messaging system, beads modeled messages as issues with `replies-to` edges forming threads. This meant the full issue tooling (search, filter, graph) worked on conversations too.

**Schema:** The `thread_id` field added in Decision 004 directly supports this -- it groups messages into conversation threads.

### 7. `duplicates` — Deduplication link

**Category:** Graph link (no ready queue impact)

When multiple agents discover the same problem, they create duplicate issues. `bd duplicate A --of B` marks A as a duplicate of canonical B. The duplicate (A) is **automatically closed**.

**Motivation:** At scale (128+ issues per session), duplicates are inevitable. Without explicit dedup, the backlog grows with phantom work. The `duplicates` edge preserves the history (you can see what was merged) while keeping the work queue clean.

### 8. `supersedes` — Version chain link

**Category:** Graph link (no ready queue impact)

When a design evolves, the old version shouldn't be worked on. `bd supersede old --with new` closes the old issue and creates a version chain.

**Motivation:** RFC-style evolution. Design doc v1 gets superseded by v2, which gets superseded by v3. The chain is navigable -- you can trace how a design evolved. Without this, stale design issues linger in the backlog.

---

## Error Handling (pre-v0.34)

### 9. `conditional-blocks` — "B runs only if A fails"

**Category:** Workflow (affects ready queue)

The recovery workflow type. If task A succeeds, B is never needed. If A fails (closes with specific failure keywords: "failed", "rejected", "wontfix", "canceled", "abandoned", "blocked", "error", "timeout", "aborted"), then B becomes ready.

**Motivation:** Real projects need fallback plans. "Try the fast approach; if it fails, fall back to the slow but reliable one." Without conditional blocking, you'd need a human or orchestrator to manually create the fallback task after seeing a failure.

**Ready queue impact:** B stays blocked as long as A is open. If A closes successfully, B stays blocked forever (never needed). If A closes with a failure keyword, B becomes ready.

---

## Cross-Project Dependencies (v0.34.0, Dec 22)

Not a new dependency type per se, but a major extension to the dependency system:

- `external:` prefix for cross-project references
- `bd ready` filters by external dep satisfaction
- Cross-store wisp-to-digest squash

This laid groundwork for `tracks` and federation.

---

## Molecules and Gates (v0.33-v0.37, Dec 21-23)

Molecules represent "epics with execution intent." The molecule system (`bd mol pour`, `bd mol bond`, `bd mol squash`, `bd mol wisp`) needed a new dependency type for coordination.

### 10. `waits-for` — Fanout gate aggregation

**Category:** Workflow (affects ready queue)

**Introduced:** v0.35.0 (Dec 23), enhanced in v0.36.0 and v0.37.0

The fan-in/fan-out pattern:

```
Epic: Process files
├── File A (ready, runs parallel)
├── File B (ready, runs parallel)
├── File C (ready, runs parallel)
└── Aggregate (waits-for: blocked until all files close)
```

**Motivation:** Molecules often need a "gather" step after parallel work. Without `waits-for`, you'd need N explicit `blocks` edges (one per file), and you'd have to know all the files upfront. `waits-for` says "wait for ALL children of this spawner to close" -- it's dynamic.

**Gate types** (added in v0.37.0):
- `WaitsForAllChildren` — proceed when ALL are done (default)
- `WaitsForAnyChildren` — proceed when ANY one is done (race pattern)

**Evolution:**
- v0.35.0: Basic `waits-for` dependency type
- v0.36.0: Formula system support for `needs` and `waits_for` fields
- v0.37.0: Full fanout gate implementation with condition evaluator
- v0.43.0: Step-level gate evaluation (Phase 1: Human Gates), GitHub gate integration (`gh:run`, `gh:pr`)

The "Christmas Ornament" pattern uses dynamic bonding with `waits-for`:
```bash
for resource in $(discover); do
  bd mol bond mol-template $PARENT_ID --ref arm-$resource
done
```
Parent waits for all dynamically created arms via `waits-for` gates.

---

## Convoy Tracking (v0.42.0)

### 11. `tracks` — Convoy-to-issue tracking

**Category:** Convoy (no ready queue impact)

**Introduced:** v0.42.0 (commit b8a5ee1, bead bd-3roq)

Convoys are bundles of beads assigned to agents for parallel execution. `tracks` links a convoy to its constituent issues.

**Motivation:** When multiple agents work simultaneously, each needs their own "track" -- a set of non-conflicting issues. `tracks` creates parallel execution lanes from the dependency graph's connected components, sorted by priority within each track. Agents grab different tracks without conflicts.

**Also in v0.42.0:**
- `convoy` issue type with reactive completion
- `refs` field for cross-references with relationship types
- Structured labels for agent beads

---

## HOP Entity Types (v0.35.0-v0.47.0)

HOP = "Human-Oriented Programming." These types represent the relationship between entities (people, agents) and work items. They were introduced to support attribution, governance, and skill tracking.

### 12. `authored-by` — Creator relationship

**Category:** Entity / HOP (no ready queue impact)

**Introduced:** v0.35.0 (alongside "owner field for human attribution in HOP CV chains")

Links an issue to the entity (human or agent) that created it. Distinct from `created_by` (which is a timestamp-level field) -- `authored-by` is a first-class dependency edge that can be traversed in the graph.

**Motivation:** When multiple agents create work, you need to trace accountability. "Who filed this bug?" becomes a graph query rather than parsing metadata.

### 13. `assigned-to` — Assignment relationship

**Category:** Entity / HOP (no ready queue impact)

Links an issue to its assigned worker. Again, distinct from the `assignee` field -- the edge enables graph traversal ("what has agent X been assigned?") and historical tracking (edges persist after reassignment).

### 14. `approved-by` — Approval relationship

**Category:** Entity / HOP (no ready queue impact)

Links an issue to the entity that approved it. Works with the `Validation` struct which records outcomes: `ValidationAccepted`, `ValidationRejected`, `ValidationRevisionRequested`.

**Motivation:** Governance workflows in multi-agent systems. "Has a human approved this change before it ships?"

### 15. `attests` — Skill attestation

**Category:** Entity / HOP (no ready queue impact)

**Introduced:** v0.47.0 (Jan 11, 2026, commit a803da3)

"X attests Y has skill Z." The most specialized of the HOP types.

**Motivation:** In multi-agent orchestration, you want to route work to agents with the right skills. `attests` creates a verifiable skill graph: agent A attests that agent B can handle Rust refactoring, so the orchestrator routes Rust work to B.

**Related:** The `crystallizes` column (also v0.47.0) and `QualityScore` field on the Issue struct support this skill/quality attestation system.

---

## Audit and Delegation Types (late additions)

These appear in `types.go` but have limited release note documentation. They were likely added incrementally as Gas Town's orchestration needs grew.

### 16. `until` — "Active until target closes"

**Category:** Reference (likely non-blocking)

A temporal dependency: issue A remains active/relevant until issue B closes. Different from `blocks` -- `until` doesn't prevent work, it defines a lifespan.

**Probable motivation:** Temporary workarounds, feature flags, or monitoring tasks that should auto-resolve when the underlying issue is fixed.

### 17. `caused-by` — "Triggered by target" (audit trail)

**Category:** Reference (non-blocking)

Distinct from `discovered-from`: `caused-by` implies a causal chain ("this bug was CAUSED BY that change"), while `discovered-from` is observational ("I FOUND this while working on that").

**Probable motivation:** Root cause analysis. When a deployment causes issues, `caused-by` chains enable tracing back to the originating change.

### 18. `validates` — Approval/validation relationship

**Category:** Reference (likely non-blocking)

Works alongside `approved-by` but with different semantics. `approved-by` is entity→issue (who approved), while `validates` is issue→issue (this test validates that requirement).

**Probable motivation:** Traceability matrices. Linking test issues to requirement issues proves coverage.

### 19. `delegated-from` — Work delegation chain

**Category:** Delegation (non-blocking, but completion cascades up)

"Work delegated from parent; completion cascades up." When an orchestrator delegates work to a sub-agent, `delegated-from` tracks the chain. Completion of the delegated work can auto-close the delegating task.

**Probable motivation:** Gas Town's multi-agent delegation. An orchestrator creates a high-level task, delegates subtasks to specialized agents, and needs automatic rollup when delegation completes.

---

## Taxonomy Summary

### By Category

| Category | Types | Count | Blocks Ready? |
|----------|-------|-------|---------------|
| **Workflow** | `blocks`, `parent-child`, `conditional-blocks`, `waits-for` | 4 | Yes |
| **Association** | `related`, `discovered-from` | 2 | No |
| **Graph Link** | `relates-to`, `replies-to`, `duplicates`, `supersedes` | 4 | No |
| **Entity (HOP)** | `authored-by`, `assigned-to`, `approved-by`, `attests` | 4 | No |
| **Convoy** | `tracks` | 1 | No |
| **Reference** | `until`, `caused-by`, `validates` | 3 | No |
| **Delegation** | `delegated-from` | 1 | No |

### By Introduction Timeline

| When | Types Added | Trigger |
|------|-------------|---------|
| v0.1 (Nov '25) | `blocks`, `parent-child`, `related`, `discovered-from` | Core design |
| v0.24 (Nov '25) | — | `blocked_issues_cache` for performance |
| v0.30.3 (Dec 18) | `relates-to`, `replies-to`, `duplicates`, `supersedes` | Knowledge graph + messaging epic |
| v0.30.6 (Dec 19) | — | Decision 004: unified dependencies table |
| pre-v0.34 | `conditional-blocks` | Error handling / fallback workflows |
| v0.35.0 (Dec 23) | `waits-for`, `authored-by`, `assigned-to`, `approved-by` | Molecules + HOP foundation |
| v0.37.0 (Dec) | — | Fanout gate evaluation + GitHub gates |
| v0.42.0 (Jan '26) | `tracks` | Convoy multi-agent coordination |
| v0.47.0 (Jan 11) | `attests` | Skill attestation for work routing |
| Late additions | `until`, `caused-by`, `validates`, `delegated-from` | Gas Town orchestration maturity |

### By Design Validation

Custom types are also allowed -- any non-empty string up to 50 characters. The `IsWellKnown()` method checks for the 19 built-in types, but the system is open. This means users can experiment with domain-specific edge types without forking beads.

---

## What This Means for Aiki

The 19 types emerged from real problems, but they cluster into tiers of necessity:

### Tier 1: Needed for any dependency graph (aiki should have these)
- **`blocks`** — core sequential ordering
- **`parent-child`** — already implicit in aiki's ID system, but upward blocking propagation is missing

### Tier 2: Valuable for agent workflows (aiki should consider)
- **`discovered-from`** — already covered by `--source` (aiki is ahead here)
- **`conditional-blocks`** — fallback plans; small incremental cost on top of `blocks`

### Tier 3: Valuable at scale (consider later)
- **`duplicates`** — dedup at 100+ issues
- **`supersedes`** — design evolution tracking
- **`waits-for`** — dynamic fanout patterns (only if aiki gets multi-agent)

### Tier 4: Skip (Gas Town specific)
- **`relates-to`, `replies-to`** — knowledge graph / messaging (aiki isn't a knowledge graph)
- **`tracks`** — convoy patterns (needs Gas Town-level orchestration)
- **HOP entity types** — attribution/governance (overkill without multi-org)
- **`until`, `caused-by`, `validates`, `delegated-from`** — audit/delegation (niche)

---

## Sources

- [steveyegge/beads releases](https://github.com/steveyegge/beads/releases) (pages 1-5)
- [Beads CHANGELOG](https://github.com/steveyegge/beads/blob/main/CHANGELOG.md)
- [graph-links.md](https://github.com/steveyegge/beads/blob/main/docs/graph-links.md)
- [MOLECULES.md](https://github.com/steveyegge/beads/blob/main/docs/MOLECULES.md)
- [ADVANCED.md](https://github.com/steveyegge/beads/blob/main/docs/ADVANCED.md)
- [ARCHITECTURE.md](https://github.com/steveyegge/beads/blob/main/docs/ARCHITECTURE.md)
- [internal/types/types.go](https://github.com/steveyegge/beads/blob/main/internal/types/types.go)
- [Introducing Beads (Medium)](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a)
- [The Beads Revolution (Medium)](https://steve-yegge.medium.com/the-beads-revolution-how-i-built-the-todo-system-that-ai-agents-actually-want-to-use-228a5f9be2a9)
