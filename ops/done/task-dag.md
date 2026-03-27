# Design: Task DAG

**Date**: 2026-02-10
**Status**: Design
**Related**: [Beads Dependency Evolution](beads-dependency-evolution.md), [Aiki vs Beads Comparison](aiki-vs-beads-comparison.md), [Task System Design](../done/task-system.md)
**Prerequisite**: [Orchestrator Task Type](orchestrator-task-type.md) — renames `build` type to `orchestrator`, adds cascade-stop behavior

---

## Problem

Aiki's task system has no dependency graph. Tasks are nodes with attributes but no edges between them. This means:  

- **No blocking**: Can't express "task B can't start until task A is done." The `--blocked` flag creates a separate P0 human task with a text description -- there's no link back to the blocked task, no auto-resolution when the blocker closes.
- **No provenance traversal**: `--source task:abc` stores a string. You can't walk the chain (prompt → task → review → fix) without scanning all tasks and parsing source strings.
- **No impact analysis**: "If I close this task, what unblocks?" is unanswerable.
- **No parallel track computation**: Can't partition work into independent lanes for multiple agents.
- **No critical path**: Can't identify which task, if delayed, delays everything.

Beads solved this with a unified `dependencies` table (Decision 004) and open-ended edge kinds. After that consolidation, they went from 4 to 19 kinds with zero schema changes.

## Principle

**If it's a relationship between two things, it's an edge. If it's a property of one thing, it's an attribute.**

The DAG becomes the primary data structure. Tasks are nodes. The graph enables slicing and dicing work by topology, not just by flat filters.

## Design

### Two New Event Variants

```rust
/// Link added between two nodes
LinkAdded {
    from: String,                        // source node (task ID or external ref)
    to: String,                          // target node (task ID or external ref)
    kind: String,                        // open-ended type string
    timestamp: DateTime<Utc>,
}

/// Link removed between two nodes
LinkRemoved {
    from: String,
    to: String,
    kind: String,
    reason: Option<String>,              // audit trail for why the link was removed
    timestamp: DateTime<Utc>,
}
```

That's the entire storage extension. Two variants handle every relationship kind forever.

**Link identity**: A link is uniquely identified by its `(from, to, kind)` triple. Adding a link that already exists is a no-op (no duplicate event emitted). This makes link operations idempotent — running `aiki task link B --blocked-by A` twice produces one link.

### Event Metadata Format

Stored on the `aiki/tasks` branch like all other events:

```
[aiki-task]
event=link_added
from=mvslrspmoynoxyyywqyutmovxpvztkls
to=nqrtxsypzkwolmnrstvuqxyzplmrwknos
kind=blocked-by
timestamp=2026-02-10T14:30:00Z
[/aiki-task]
```

External references use typed prefixes as `to` targets:

```
[aiki-task]
event=link_added
from=mvslrspmoynoxyyywqyutmovxpvztkls
to=file:ops/now/design.md
kind=sourced-from
timestamp=2026-02-10T14:30:00Z
[/aiki-task]
```

### Edge Kinds (Start With 7)

| Kind | From → To | Semantics | Cardinality | Affects Ready Queue? |
|------|-----------|-----------|-------------|----------------------|
| `blocked-by` | task → blocker | "I can't start until blocker closes" | Many | **Yes** — task excluded from ready queue |
| `sourced-from` | task → origin | "I came from this" | Many | No |
| `subtask-of` | child → parent | "I am a subtask of this" | **Single** | No (see [Parent Semantics](#parent-semantics)) |
| `implements` | task → spec | "I implement this specification" | **Single** | No |
| `orchestrates` | orchestrator → plan | "I drive this plan's subtasks to completion" | **Single** | No |
| `scoped-to` | task → target | "I operate on this" | Many | No |
| `supersedes` | replacement → predecessor | "I replace this failed/abandoned attempt" | **Single** | No |

**Cardinality**: "Single" means at most one active link of that kind per `from` task. Single-link kinds enforce this unconditionally — closed task status does not relax the constraint. To replace an existing link, either explicitly unlink first, or use auto-replace behavior (see [Single-Link Auto-Replace](#single-link-auto-replace)).

Open-ended string kind means adding more later requires zero code changes to storage or events. Just define the semantics and update the ready queue if it's a blocking kind.

**Edge kind naming rules:**

1. **The subject (first positional arg) is always `from`** — `link A --blocked-by B` means `from=A, to=B`
2. **Passive/prepositional = subject is the dependent** — `blocked-by`, `subtask-of`, `sourced-from`, `scoped-to` all point from the dependent to the thing it depends on
3. **Active voice = subject is the actor** — `implements`, `orchestrates` point from the actor/deliverer to the thing it acts on
4. **The flag reads as a sentence** — passive links use "A **is** [predicate] B" (`link A --blocked-by B` → "A is blocked by B"); active links drop the "is" (`link A --orchestrates B` → "A orchestrates B")

#### `blocked-by`

The core workflow dependency. "Task B can't start until task A closes."

```
aiki task link B --blocked-by A
aiki task unlink B --blocked-by A
```

Direction: `from=B, to=A, kind=blocked-by` — reads as "B is blocked by A." The CLI reads the same way: `link B --blocked-by A`.

A task with unresolved `blocked-by` edges never appears in the ready queue.

**Cycle detection at write time**: When adding a `blocked-by` link, run a DFS via `edges.targets(id, "blocked-by")` to verify acyclicity before emitting the event. Reject with an error if the link would create a cycle. This prevents silent deadlocks in the ready queue.

#### `sourced-from`

Replaces the `sources: Vec<String>` attribute on `Created` events. Each source becomes a link:

```
Link { from: "mvslrspmoynoxyyywqyutmovxpvztkls", to: "prompt:nzwtoqqr",       kind: "sourced-from" }
Link { from: "nqrtxsypzkwolmnrstvuqxyzplmrwknos", to: "mvslrspmoynoxyyywqyutmovxpvztkls", kind: "sourced-from" }
Link { from: "xkttqnylzmwoprstuvxyznmlkqwprstvx", to: "comment:c1a2b3",         kind: "sourced-from" }
Link { from: "xkttqnylzmwoprstuvxyznmlkqwprstvx", to: "file:ops/now/design.md", kind: "sourced-from" }
```

**Canonical storage rule**: Both `from` and `to` MUST store canonical identifiers. For tasks, this means the full 32-character task ID — never a short prefix, never a `task:` prefix. For external references, it means the typed form (`file:`, `prompt:`, `comment:`, `issue:`).

**Target normalization at write time**: The CLI accepts user-friendly input and normalizes to canonical form before emitting the event. **Normalization is kind-aware** — task-only link kinds (`blocked-by`, `subtask-of`, `orchestrates`, `supersedes`) reject targets that don't resolve to a task, preventing silent creation of unresolvable links.

**New error variant** (`cli/src/error.rs`):

```rust
#[error("Invalid link target for '{kind}': '{target}' is not a task. \
         {kind} links require a task ID as target")]
InvalidLinkTarget {
    kind: String,
    target: String,
},
```

```rust
/// Normalize a link target to its canonical storage form.
/// Called at write time — the event always stores canonical IDs.
/// Kind-aware: task-only kinds reject non-task targets instead of
/// silently coercing them to file: paths.
fn normalize_link_target(input: &str, kind: &str, tasks: &HashMap<String, Task>) -> Result<String> {
    // 1. Strip task: prefix if present
    let stripped = input.strip_prefix("task:").unwrap_or(input);

    // 2. If it's already a full 32-char task ID, use it directly
    if is_task_id(stripped) {
        return Ok(stripped.to_string());
    }

    // 3. If it has an external reference prefix
    if has_external_ref_prefix(stripped) {
        if is_task_only_kind(kind) {
            return Err(AikiError::InvalidLinkTarget {
                kind: kind.to_string(),
                target: stripped.to_string(),
            });
        }
        return Ok(stripped.to_string());
    }

    // 4. Try resolving as a short task ID prefix
    let matches: Vec<&str> = tasks.keys()
        .filter(|id| id.starts_with(stripped))
        .map(|id| id.as_str())
        .collect();
    match matches.len() {
        1 => Ok(matches[0].to_string()),
        0 if is_task_only_kind(kind) => {
            Err(AikiError::InvalidLinkTarget {
                kind: kind.to_string(),
                target: stripped.to_string(),
            })
        }
        0 => {
            // Flexible-target kinds: treat as file path, auto-prefix with file:
            Ok(format!("file:{}", stripped))
        }
        _ => Err(AikiError::AmbiguousTaskId {
            prefix: stripped.to_string(),
            count: matches.len(),
            matches: format_matches(&matches),
        }),
    }
}

/// Task-only kinds: target MUST resolve to a task ID.
/// External refs and file: fallback are rejected.
/// Driven by the LINK_KINDS registry — no hardcoded list.
fn is_task_only_kind(kind: &str) -> bool {
    LINK_KINDS.iter()
        .find(|k| k.name == kind)
        .map_or(false, |k| k.task_only)
}

fn has_external_ref_prefix(s: &str) -> bool {
    s.starts_with("file:") || s.starts_with("prompt:")
        || s.starts_with("comment:") || s.starts_with("issue:")
}
```

This means:
- `aiki task link B --sourced-from task:mvslr...` → stores full ID `mvslr...` (prefix stripped)
- `aiki task link B --sourced-from mvslr` → resolves short ID to full `mvslrspmoynoxyyywqyutmovxpvztkls`
- `aiki task link B --sourced-from file:design.md` → stores `file:design.md` (external ref, kept as-is)
- `aiki task link B --sourced-from design.md` → stores `file:design.md` (auto-prefixed)
- `aiki task link B --sourced-from abc` → error if `abc` matches multiple task IDs
- `aiki task link B --blocked-by typo123` → **error** if `typo123` matches zero tasks (not silently coerced to `file:typo123`)
- `aiki task link B --blocked-by file:design.md` → **error** — `blocked-by` requires a task target
- `aiki task link B --subtask-of nonexistent` → **error** — `subtask-of` requires a task target

**The `from` field is always a task ID** and follows the same rules: the CLI resolves short IDs to full 32-char canonical IDs before emitting the event.

The `to` target can be a full task ID or a typed external reference. All existing source prefixes (`file:`, `comment:`, `issue:`, `prompt:`) work unchanged — they just become link targets instead of attribute values. The `task:` prefix is accepted for ergonomics but stripped during normalization.

This enables provenance chain traversal:
```
prompt:xyz → task-abc → task-def (review) → task-ghi (fix)
                ↑
        file:design.md
```

And reverse queries: "What tasks were spawned from this design doc?" becomes a reverse link lookup on `to=file:ops/now/design.md`, not a full scan.

#### `subtask-of`

Replaces the implicit parent-child encoding in the ID string. Each parent-child relationship becomes a link:

```
Link { from: "task-child", to: "task-parent", kind: "subtask-of" }
```

**The legacy encoded child-ID convention is removed.** All tasks, including subtasks, get regular 32-character IDs. Parent-child relationships exist only as links. Benefits:

- **Short IDs for subtasks**: Without the `.1` suffix, short ID resolution works for subtasks like any other task
- **Re-parenting**: Move a subtask to a different epic by removing one link and adding another
- **Uniform queries**: Subtask lookup uses the same link infrastructure as everything else
- **Simpler ID system**: All task IDs are just 32-char strings, no special parsing for dots
- **Consistent design**: Parent-child is purely a link relationship, not encoded in two places (ID + link)

<a id="parent-semantics"></a>
**Parent semantics — readiness vs. closure are separate concerns:**

- **Readiness**: A `subtask-of` link does NOT block the parent from the ready queue. An epic can be "in progress" even if some subtasks are blocked. Only `blocked-by` links affect readiness.
- **Closure**: A parent task cannot be closed while it has open children. This is a validation check on `task close`, not a ready-queue filter. Attempting to close a parent with open children returns an error listing the open subtasks.

**Constraint enforcement for `subtask-of` links:**

To prevent ambiguity in parent-child relationships and enable safe graph traversal, the following constraints are enforced at write time:

1. **Single parent (unconditional)**: A task can have at most one `subtask-of` link, regardless of whether either task is open or closed. If a `subtask-of` link already exists, **auto-replace** kicks in: the old link is removed and the new one is added (see [Single-Link Auto-Replace](#single-link-auto-replace)). No `supersedes` link is emitted — re-parenting is reorganization, not abandonment.

2. **No cycles**: Before adding a `subtask-of` link from A to B, verify that B is not a descendant of A (direct or transitive). This prevents cycles like A→B→C→A. Use the same DFS-based cycle detection as `blocked-by` links.

3. **Implementation location**: These checks happen in the link creation logic (Phase 4 implementation), before emitting the `LinkAdded` event:

```rust
// Pseudocode for subtask-of link validation
fn validate_subtask_of_link(graph: &TaskGraph, child_id: &str, parent_id: &str) -> Result<()> {
    // Check 1: No cycles (same logic as blocked-by)
    if would_create_cycle_in_children(graph, child_id, parent_id) {
        return Err(AikiError::LinkCycle { kind: "subtask-of".to_string() });
    }

    Ok(())
    // Note: single-parent enforcement is handled by auto-replace logic,
    // not by rejecting the link. See Single-Link Auto-Replace.
}
```

#### `implements`

Links a plan task to the spec it implements. The plan task is the canonical owner of the spec relationship — other tasks (planning, orchestrator, review) relate to the spec indirectly through the graph. Distinct from `sourced-from`: `sourced-from` answers "why does this task exist?" while `implements` answers "what does this task deliver?"

```bash
aiki task link <plan> --implements ops/now/auth-design.md
aiki task unlink <plan> --implements ops/now/auth-design.md
```

Direction: `from=plan, to=file:ops/now/auth-design.md, kind=implements` — reads as "plan implements auth-design.md."

**Target normalization**: Same canonical storage rule as all links. Bare paths are auto-prefixed with `file:`. The `from` field resolves to a full 32-char task ID:

```bash
# User types (no prefix required):
aiki task link <plan> --implements ops/now/auth-design.md

# Stored as (canonical form):
LinkAdded { from: "<full-32-char-id-of-plan>", to: "file:ops/now/auth-design.md", kind: "implements" }
```

**Single-link constraint**: A task can have at most one `implements` link. If a task already implements a spec, linking it to a new spec triggers auto-replace: the old link is removed, the new one is added, and a `supersedes` link is emitted from the new plan to the old plan (see [Single-Link Auto-Replace](#single-link-auto-replace)). This constraint is enforced unconditionally — closed task status does not relax it.

**Replaces `data.spec`** on plan tasks. Previously, `data.spec` was duplicated across planning tasks, plan tasks, and orchestrator tasks as a join key. Now the plan task owns the relationship, and other tasks reach the spec through the graph.

**Enables spec-coverage queries:**

```bash
# What tasks implement this spec?
aiki task list --implements ops/now/auth-design.md

# Is this spec fully implemented? (all implementation tasks closed)
aiki task list --implements ops/now/auth-design.md --open
```

#### `orchestrates`

Links an orchestrator task to the plan it drives to completion. The orchestrator works through the plan's subtasks. Replaces the `data.plan` back-pointer that orchestrator tasks previously stored.

```bash
aiki task link <orchestrator> --orchestrates <plan>
aiki task unlink <orchestrator> --orchestrates <plan>
```

Direction: `from=orchestrator, to=plan, kind=orchestrates` — reads as "orchestrator orchestrates plan."

**Single-link constraint (both directions)**: An orchestrator can orchestrate at most one plan, and a plan can have at most one orchestrator. These constraints are enforced unconditionally — closed task status does not relax them. If a conflict exists, auto-replace kicks in: the old link is removed, the new one is added, and a `supersedes` link is emitted (see [Single-Link Auto-Replace](#single-link-auto-replace)).

**Enables:**
- "What plan is this orchestrator running?" → `edges.target(orchestrator, "orchestrates")` → plan
- "Get spec from orchestrator" → `edges.target(orchestrator, "orchestrates")` → plan → `edges.target(plan, "implements")` → spec
- "What orchestrator is running this plan?" → `edges.referrer(plan, "orchestrates")` → orchestrator
- Status monitor: show plan subtask tree under orchestrator task (replaces `root_task.data.get("plan")`)

#### `scoped-to`

Links a task to what it operates on. Used by planning tasks (scoped to a spec file) and review tasks (scoped to a task, spec, or set of tasks).

```bash
aiki task link <planning-task> --scoped-to ops/now/feature.md
aiki task link <review-task> --scoped-to abc123
```

Direction: `from=task, to=target, kind=scoped-to`

**Target normalization**: Same canonical storage rule as all links. The CLI resolves short task IDs to full 32-char IDs and auto-prefixes file paths with `file:`:

```bash
# User types:
aiki task link A --scoped-to ops/now/feature.md
aiki task link A --scoped-to mvslr

# Stored as (canonical form):
LinkAdded { from: "<full-32-char-id-of-A>", to: "file:ops/now/feature.md",          kind: "scoped-to" }
LinkAdded { from: "<full-32-char-id-of-A>", to: "mvslrspmoynoxyyywqyutmovxpvztkls", kind: "scoped-to" }
```

Multiple links allowed (e.g., session review scoped to several tasks). No single-link constraint.

**Replaces:**
- `data.spec` on planning tasks — planning task is now `scoped-to` its spec file
- `data.scope.kind` + `data.scope.id` + `data.scope.task_ids` on review tasks — the target prefix (`file:`, bare task ID) replaces `scope.kind`, multiple `scoped-to` links replace `scope.task_ids`
- `data.scope.name` is dropped entirely — computed from the link target at display time, not stored

#### `supersedes`

Links a replacement task to the predecessor it replaces. Used when a plan or orchestrator is abandoned and a new one takes over. Enables traversal of abandoned attempts.

```bash
aiki task link B --supersedes A
aiki task unlink B --supersedes A
```

Direction: `from=B (replacement), to=A (predecessor), kind=supersedes` — reads as "B supersedes A."

**Single-link constraint**: A task can supersede at most one predecessor. Chains are built by following `supersedes` links: C → B → A represents three attempts.

**Typically emitted automatically** by single-link auto-replace on `implements` and `orchestrates` kinds (see below). Can also be used manually.

**Enables:**
- "Show all attempts at this spec" → `edges.referrers(spec, "implements")` → current plan → walk `edges.target(id, "supersedes")` backward
- "What replaced this task?" → `edges.referrer(task, "supersedes")` → replacement
- "What did this task replace?" → `edges.target(task, "supersedes")` → predecessor

<a id="single-link-auto-replace"></a>
#### Single-Link Auto-Replace

For single-link kinds, linking when a link already exists triggers automatic replacement instead of an error. The behavior varies by kind:

**`implements` and `orchestrates` — replace with `supersedes`:**

When the existing link holder is a different task (e.g., spec already has an implementor):

```bash
# User types:
aiki task link B --implements spec.md
# spec.md already implemented by Plan A.
# System emits:
#   LinkRemoved { from: A, to: file:spec.md, kind: "implements" }
#   LinkAdded   { from: B, to: file:spec.md, kind: "implements" }
#   LinkAdded   { from: B, to: A,            kind: "supersedes" }
# Output: "Superseded: A previously implemented spec.md"
```

**`subtask-of` — re-parent (no `supersedes`):**

```bash
# User types:
aiki task link child --subtask-of new-parent
# child already has old-parent.
# System emits:
#   LinkRemoved { from: child, to: old-parent, kind: "subtask-of" }
#   LinkAdded   { from: child, to: new-parent, kind: "subtask-of" }
# Output: "Re-parented: child moved from old-parent to new-parent"
```

**`supersedes` — replace (simple swap):**

```bash
# User types:
aiki task link C --supersedes B
# C already supersedes A.
# System emits:
#   LinkRemoved { from: C, to: A, kind: "supersedes" }
#   LinkAdded   { from: C, to: B, kind: "supersedes" }
```

### Materialized Graph

During event replay, build a `TaskGraph` instead of just a `HashMap<String, Task>`:

```rust
/// Fast HashMap using ahash for non-cryptographic hashing
type FastHashMap<K, V> = hashbrown::HashMap<K, V, ahash::RandomState>;

/// Generic edge store — indexes all links by kind.
///
/// Two parallel maps: forward (from → [to]) and reverse (to → [from]),
/// both keyed by link kind. Adding a new link kind requires zero changes
/// to this struct — just define the kind string and register its metadata
/// in LINK_KINDS.
pub struct EdgeStore {
    /// kind → (from_id → [to_id])
    forward: FastHashMap<String, FastHashMap<String, Vec<String>>>,
    /// kind → (to_id → [from_id])
    reverse: FastHashMap<String, FastHashMap<String, Vec<String>>>,
}

impl EdgeStore {
    /// Forward lookup: given a `from` node and kind, return all targets.
    /// For many-link kinds, returns all targets. For single-link kinds,
    /// returns a slice of 0 or 1 elements.
    pub fn targets(&self, from: &str, kind: &str) -> &[String] {
        self.forward.get(kind)
            .and_then(|m| m.get(from))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Reverse lookup: given a `to` node and kind, return all referrers.
    pub fn referrers(&self, to: &str, kind: &str) -> &[String] {
        self.reverse.get(kind)
            .and_then(|m| m.get(to))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Forward lookup for single-link kinds: return the one target (if any).
    /// Convenience wrapper — panics in debug if kind is not single-link.
    pub fn target(&self, from: &str, kind: &str) -> Option<&str> {
        debug_assert!(
            LINK_KINDS.iter().any(|k| k.name == kind && k.max_forward == Some(1)),
            "target() called on many-link kind '{kind}'"
        );
        self.targets(from, kind).first().map(|s| s.as_str())
    }

    /// Reverse lookup for single-link kinds: return the one referrer (if any).
    pub fn referrer(&self, to: &str, kind: &str) -> Option<&str> {
        self.referrers(to, kind).first().map(|s| s.as_str())
    }

    /// Check if a specific forward link exists.
    pub fn has_link(&self, from: &str, to: &str, kind: &str) -> bool {
        self.targets(from, kind).contains(&to.to_string())
    }
}

/// Link kind metadata — defines cardinality rules and blocking behavior.
/// Checked at write time when adding links.
pub struct LinkKind {
    /// The kind string (e.g., "blocked-by")
    pub name: &'static str,
    /// Max active forward links per `from` node.
    /// None = unlimited, Some(1) = single-link kind (auto-replace on conflict).
    pub max_forward: Option<usize>,
    /// Max active reverse links per `to` node.
    /// None = unlimited, Some(1) = single reverse (e.g., orchestrates: one
    /// orchestrator per plan).
    pub max_reverse: Option<usize>,
    /// Whether unresolved links of this kind exclude the `from` task
    /// from the ready queue.
    pub blocks_ready: bool,
    /// Whether targets must resolve to task IDs (vs. external refs).
    pub task_only: bool,
}

/// Registry of all link kinds. Adding a new kind = one entry here + zero
/// changes to EdgeStore, TaskGraph, or materialization.
pub const LINK_KINDS: &[LinkKind] = &[
    LinkKind { name: "blocked-by",   max_forward: None,    max_reverse: None,    blocks_ready: true,  task_only: true  },
    LinkKind { name: "sourced-from", max_forward: None,    max_reverse: None,    blocks_ready: false, task_only: false },
    LinkKind { name: "subtask-of",   max_forward: Some(1), max_reverse: None,    blocks_ready: false, task_only: true  },
    LinkKind { name: "implements",   max_forward: Some(1), max_reverse: None,    blocks_ready: false, task_only: false },
    LinkKind { name: "orchestrates", max_forward: Some(1), max_reverse: Some(1), blocks_ready: false, task_only: true  },
    LinkKind { name: "scoped-to",    max_forward: None,    max_reverse: None,    blocks_ready: false, task_only: false },
    LinkKind { name: "supersedes",   max_forward: Some(1), max_reverse: None,    blocks_ready: false, task_only: true  },
];

/// Materialized task graph (computed from events)
///
/// Events (on the aiki/tasks branch) are the source of truth; the
/// EdgeStore indexes are derived during replay. Adding a new link kind
/// requires zero changes here — just add an entry to LINK_KINDS.
///
/// ## Performance Design
///
/// **Fast HashMap implementation**: Uses `ahash::RandomState` for 2-5x faster
/// hashing on task IDs (short strings). Falls back to `std` HashMap on platforms
/// where ahash is unavailable.
///
/// **Petgraph compatibility**: The adjacency lists can be converted to
/// `petgraph::Graph` when needed for advanced algorithms (critical path,
/// connected components). Conversion to petgraph node indices happens on-demand.
///
/// **All indexes eagerly built**: We build all indexes during materialization.
/// At <1000 tasks with <500 links, this is microseconds. If profiling shows
/// performance issues, we can add lazy construction later.
pub struct TaskGraph {
    /// Node data (tasks)
    pub tasks: FastHashMap<String, Task>,

    /// Generic edge indexes (forward + reverse for every link kind)
    pub edges: EdgeStore,
}
```

**Why generic over named fields?** The original design had 13 named `HashMap` fields (one forward + one reverse per kind). Every new kind required adding 2+ fields, updating materialization, and writing accessors. The generic `EdgeStore` fulfills the spec's promise that "adding more [kinds] later requires zero code changes to storage." New kinds get an entry in `LINK_KINDS` and nothing else changes.

**Cardinality enforcement at write time:** Before emitting a `LinkAdded` event, the writer looks up the kind in `LINK_KINDS` and checks `max_forward` / `max_reverse`. If the limit is `Some(1)` and a link already exists, single-link auto-replace fires (see [Single-Link Auto-Replace](#single-link-auto-replace)). The `EdgeStore` itself is cardinality-agnostic — it always stores `Vec<String>` — and relies on the write path to enforce limits.

**`task_only` enforcement at write time:** Kinds with `task_only: true` reject targets that don't resolve to a task ID (see `normalize_link_target`). This replaces the hardcoded `is_task_only_kind()` function — the check becomes `LINK_KINDS.iter().find(|k| k.name == kind).map_or(false, |k| k.task_only)`.

The indexes are cheap to build during replay — just process LinkAdded/LinkRemoved events and populate the forward/reverse maps. At <1000 tasks, this is microseconds.

**External references**: Link targets like `file:ops/now/design.md` or `prompt:nzwtoqqr` are opaque strings — they don't resolve to entries in `tasks`. Operations like `spawned_from` filter links by `to` target and return the `from` task. The graph does not validate that external references exist; they're provenance metadata, not live pointers.

**Future: Advanced algorithms**: For complex graph operations (critical path, parallel tracks), see [Advanced Task Graph Queries](../future/advanced-task-graph-queries.md). The adjacency list design is already compatible with petgraph if we need it later.

### Ready Queue Changes

**IMPORTANT**: The DAG-aware ready queue adds blocking checks but MUST preserve existing scope and visibility filtering. The current implementation has three variants (`get_scoped_ready_queue`, `get_ready_queue_for_scope_set`, `get_ready_queue_for_agent`) that apply different filters. The DAG changes augment these, not replace them.

Current (simplified for illustration):
```rust
pub fn get_ready_queue(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    tasks.values()
        .filter(|t| t.status == TaskStatus::Open)
        .collect()
}
```

DAG-aware (adds blocking filter):
```rust
pub fn get_ready_queue(graph: &TaskGraph) -> Vec<&Task> {
    graph.tasks.values()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| !graph.is_blocked(&t.id))
        .collect()
}
```

**Migration strategy for existing ready queue functions**:

The actual implementation has scope-based filtering (see `cli/src/tasks/manager.rs:545`, `:662`, `:710`). During Phase 2 (blocked-by implementation), update each variant to add the blocking check:

1. **`get_scoped_ready_queue`** — Add `.filter(|t| !graph.is_blocked(&t.id))` after scope filter
2. **`get_ready_queue_for_scope_set`** — Add blocking check in the loop that builds `ready`
3. **`get_ready_queue_for_agent`** — Add `.filter(|t| !graph.is_blocked(&t.id))` after assignee filter

The blocking check is the same in all cases — it's just added to the existing filter chain. Do NOT remove scope or assignee filtering.

```rust
impl TaskGraph {
    /// A task is blocked if any of its blockers are not Closed.
    /// Only `blocked-by` links affect this — parent links do not.
    pub fn is_blocked(&self, task_id: &str) -> bool {
        self.edges.targets(task_id, "blocked-by")
            .iter()
            .any(|b| self.tasks.get(b)
                .map_or(true, |t| t.status != TaskStatus::Closed))
    }

    /// A parent task cannot be closed while it has open children.
    /// Returns the list of open children, or empty if closeable.
    pub fn open_children(&self, task_id: &str) -> Vec<&Task> {
        self.edges.referrers(task_id, "subtask-of")
            .iter()
            .filter_map(|c| self.tasks.get(c))
            .filter(|t| t.status != TaskStatus::Closed)
            .collect()
    }
}
```

### DAG Operations

Once the graph exists, these basic operations become natural:

```rust
impl TaskGraph {
    /// Full provenance chain: walk `sourced-from` links via
    /// `edges.targets(id, "sourced-from")`. Uses a visited set to handle
    /// cycles — sourced-from has no write-time cycle check (cycles aren't
    /// inherently invalid in provenance), so traversal must be cycle-safe.
    pub fn provenance_chain(&self, task_id: &str) -> Vec<String>

    /// Reverse provenance: what tasks came from this origin?
    /// Uses `edges.referrers(origin, "sourced-from")`. Origin can be an
    /// external ref (e.g., "file:design.md"). Single-hop reverse lookup,
    /// no cycle risk.
    pub fn spawned_from(&self, origin: &str) -> Vec<&Task>

    /// Children of a parent: `edges.referrers(parent_id, "subtask-of")`.
    /// Replaces get_subtasks.
    pub fn children_of(&self, parent_id: &str) -> Vec<&Task>

    /// Cycle detection for a proposed new link (used at write time).
    /// Walks `edges.targets(id, kind)` via DFS to verify acyclicity.
    /// Works for any cycle-checked kind (blocked-by, subtask-of).
    pub fn would_create_cycle(&self, from: &str, to: &str, kind: &str) -> bool

    /// Walk `subtask-of` links upward via `edges.target(id, "subtask-of")`
    /// to get the full ancestor chain. Returns parent IDs from immediate
    /// parent to root. Includes a visited set as defense-in-depth (write-time
    /// cycle checks are the primary protection for subtask-of).
    pub fn ancestor_chain(&self, task_id: &str) -> Vec<String>
}
```

**Cycle protection strategy:**

| Traversal | Cycle risk | Protection |
|-----------|-----------|------------|
| `provenance_chain` (walks `sourced-from`) | Yes — no write-time check | **Visited set at traversal time** (primary) |
| `ancestor_chain` (walks `subtask-of`) | Low — write-time DFS check | Write-time check (primary); visited set (defense-in-depth) |
| `spawned_from` (reverse `sourced-from`) | None — single hop | N/A |
| `is_blocked` (checks `blocked-by`) | Low — write-time DFS check | Write-time check (primary) |

**Note:** Advanced operations (critical path, parallel tracks) are documented in [Advanced Task Graph Queries](../future/advanced-task-graph-queries.md) and only needed when basic DAG features are stable.

### Ancestor Chain Provenance

**Problem:** With legacy encoded child IDs removed, the revset pattern `task=parent.` no longer matches subtask changes. We need a way to query "all changes made under this parent task" without expanding all descendant IDs into OR clauses at query time.

**Solution:** Write ancestor task IDs into the provenance block at change time. When an agent works on subtask `child` whose parent is `parent`, the `[aiki]` block includes `task=` lines for the full ancestor chain:

```
[aiki]
task=child
task=parent
agent=claude-code
[/aiki]
```

For deeper nesting (grandchild → child → parent):

```
[aiki]
task=grandchild
task=child
task=parent
agent=claude-code
[/aiki]
```

This means `description(substring:"task=parent")` matches changes made by any descendant — the same O(1) query behavior as the old prefix-matching approach, without coupling to the ID format.

**Why this works:**
- The `tasks` field on `ProvenanceRecord` is already a `Vec<String>` that supports multiple `task=` lines
- `to_description()` writes one `task=` line per entry; `from_description()` parses them all back
- The existing `build_task_revset_pattern` stays at 2 clauses regardless of tree depth
- No graph materialization needed at query time — the ancestor information is baked into each change's description

**Implementation — provenance write path** (`cli/src/flows/core/functions.rs`):

`get_in_progress_tasks_for_session` currently returns the flat list of in-progress task IDs. After the `TaskGraph` is available, extend this to include ancestors:

```rust
fn get_in_progress_tasks_for_session(cwd: &Path, session_id: &str) -> Vec<String> {
    let events = read_events(cwd).unwrap_or_default();
    let graph = materialize_graph(&events);

    let mut task_ids = get_in_progress_task_ids_for_session(&graph.tasks, session_id);

    // Append ancestor chain for each in-progress task
    let mut ancestors = Vec::new();
    for id in &task_ids {
        ancestors.extend(graph.ancestor_chain(id));
    }

    // Deduplicate while preserving order (in-progress tasks first, then ancestors)
    let mut seen = HashSet::new();
    task_ids.extend(ancestors);
    task_ids.retain(|id| seen.insert(id.clone()));

    task_ids
}
```

**`ancestor_chain` implementation** (on `TaskGraph`):

```rust
pub fn ancestor_chain(&self, task_id: &str) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut visited = HashSet::new();
    visited.insert(task_id.to_string());
    let mut current = task_id;
    while let Some(parent) = self.edges.target(current, "subtask-of") {
        if !visited.insert(parent.to_string()) {
            break; // cycle detected — defense-in-depth (write-time checks should prevent this)
        }
        ancestors.push(parent.to_string());
        current = parent;
    }
    ancestors
}
```

This is 1-2 hash lookups for typical nesting depth. The `edges.target()` call is a single-link convenience wrapper over the generic forward index. The visited set is defense-in-depth — write-time cycle checks on `subtask-of` are the primary protection.

**Revset pattern — no change needed:**

```rust
fn build_task_revset_pattern(task_id: &str) -> String {
    format!(
        "description(substring:\"task={}\") ~ ::aiki/tasks",
        task_id
    )
}
```

After the transition, the legacy child-match `task=X.` clause can be dropped entirely. A single `description(substring:"task=X")` matches the task's own changes AND all descendant changes (because descendants write `task=X` in their provenance). During the transition period, keep both clauses for backward compatibility with old provenance blocks that only wrote `task=child`.

**Ordering guarantee:** The `tasks` Vec is ordered with the most specific task first (the task being directly worked on) followed by ancestors from immediate parent to root. This ensures that `tasks[0]` is always the leaf task, matching the current behavior where the first `task=` line identifies the specific work item.

### CLI Commands

The first positional argument is always the subject task. The flag names the relationship and takes the target as its value.

```bash
# Link / unlink (link type as flag, target as flag value)
aiki task link A --blocked-by B                              # A is blocked by B
aiki task link A --sourced-from file:design.md               # A's source is design.md
aiki task link A --subtask-of B                                # A is a subtask of B
aiki task link A --implements ops/now/auth-design.md           # A implements this spec
aiki task link A --orchestrates B                              # A orchestrates B
aiki task link A --scoped-to ops/now/feature.md              # A operates on this file
aiki task link A --scoped-to abc123                          # A operates on this task
aiki task link B --supersedes A                              # B replaces A
aiki task unlink A --blocked-by B                            # remove relationship
aiki task unlink A --implements ops/now/auth-design.md

# List links
aiki task link list                      # all links in current scope
aiki task link list A                    # all links involving task A
aiki task link list --blocked-by         # only blocked-by kind
aiki task link list --implements  # only implements kind

# Query by link target
aiki task list --implements ops/now/feature.md  # tasks implementing this spec
aiki task list --scoped-to ops/now/feature.md          # tasks scoped to this file

# Show what a task blocks / is blocked by
aiki task show <id>            # includes "Blocked by:" and "Blocks:" sections

# What unblocks if I close this?
aiki task list --blocked-by <id>

# Show the full DAG for a scope
aiki task graph                 # ASCII DAG of current scope

# Provenance chain
aiki task trace <id>            # walk source edges back to origin
```

### Output Format Changes

**`task show` with DAG information:**

```
Task: Fix auth
ID: abc
Status: open
Priority: p1

Blocked by:
- xyz — Refactor token service (in_progress)

Blocks:
- def [open] Add OAuth flow
- ghi [open] Write auth tests

Sources: file:ops/now/auth-design.md, task:lmn
Implements: ops/now/auth-design.md
Orchestrator: def — Build auth feature

Parent: lmn — Auth epic

In Progress:
- abc — Fix auth

Ready (2):
- ...
```

**`task list` with blocked count:**

```
In Progress:
- abc — Fix auth

Ready (3):
- def [p1] Add OAuth flow
- ghi [p2] Write auth tests
- jkl [p2] Update docs

Blocked (2):
- mno [p0] Deploy (blocked by: abc)
- pqr [p1] Integration tests (blocked by: def)
```

## Migration

### Phase 0: Performance Foundation

**Before implementing DAG features, lay performance groundwork:**

**Add `ahash` dependency** (`Cargo.toml`):
```toml
ahash = "0.8"
hashbrown = "0.14"  # Fast HashMap implementation
```

**Create type alias** (`cli/src/tasks/types.rs` or new `cli/src/tasks/collections.rs`):
```rust
/// Fast HashMap using ahash for non-cryptographic hashing
pub type FastHashMap<K, V> = hashbrown::HashMap<K, V, ahash::RandomState>;
```

**Migrate existing HashMaps incrementally:**
- Start with `tasks/manager.rs` (hot path)
- Then `tasks/storage.rs` 
- Use `FastHashMap::with_capacity()` when size is known (e.g., `HashMap::with_capacity(events.len())`)

**Estimated effort:** 2-4 hours  
**Risk:** Low — transparent drop-in replacement  
**Benefit:** 2-5x faster task operations, foundation for DAG indexes

### Phase 1: Add Link Infrastructure

Add `LinkAdded` and `LinkRemoved` event variants. Add `TaskGraph` struct with generic `EdgeStore` and `LinkKind` registry. Change `materialize_tasks` to `materialize_graph`. Existing events and behavior unchanged.

**Implementation:** Build all edge indexes during event replay. The `materialize_graph` function processes all events, populates the `EdgeStore` forward/reverse maps for each link kind, and returns the complete `TaskGraph`. Simple, predictable, and fast enough at current scale. Adding a new link kind requires only a new entry in `LINK_KINDS`.

### Phase 2: `blocked-by` Link Kind + Ready Queue

Implement `aiki task link/unlink` with `--blocked-by` flag and `aiki task link list`. Update ready queue to filter by unresolved blockers. Replace the `--blocked` text-field workaround with real links. Include cycle detection at write time (`would_create_cycle` check before emitting LinkAdded).

### Phase 3: `sourced-from`, `implements`, `orchestrates`, `scoped-to`, `supersedes`

Implement remaining non-blocking link kinds:

- **`sourced-from`**: Migrate `sources` attribute to links. When creating tasks with `--source`, emit `LinkAdded` events with `kind=sourced-from` instead of (or in addition to) storing in the `sources` attribute. Implement `aiki task trace` for provenance chain walking.

- **`implements`**: Plan tasks link to their spec. Bare paths auto-prefixed with `file:`. Already registered in `LINK_KINDS` — the generic `EdgeStore` handles forward/reverse indexing automatically. Add `aiki task list --implements <spec>` for spec-coverage queries. Replaces `data.spec` on plan tasks.

- **`orchestrates`**: Build command emits this when linking an orchestrator task to a plan. Already registered in `LINK_KINDS` with `max_forward: Some(1), max_reverse: Some(1)`. Replaces `data.plan` on orchestrator tasks.

- **`scoped-to`**: Planning tasks and review tasks link to their operational target. Already registered in `LINK_KINDS` with unlimited cardinality. Replaces `data.spec` on planning tasks and `data.scope.kind` + `data.scope.id` + `data.scope.task_ids` on review tasks. `data.scope.name` is dropped (computed from link target).

- **`supersedes`**: Emitted automatically by single-link auto-replace on `implements` and `orchestrates`. Can also be used manually. Already registered in `LINK_KINDS`. Enables abandoned-attempt traversal.

- **Single-link auto-replace**: Implement the auto-replace behavior driven by `LINK_KINDS` metadata. When linking would violate `max_forward` or `max_reverse` cardinality, automatically remove the old link and add the new one, with `supersedes` emitted for `implements`/`orchestrates` and simple re-parenting for `subtask-of`.

- **Remove `plan` task type**: Every query that uses `task_type == "plan"` is a relationship query — exactly what link traversal replaces. After links are in place, the `plan` type is redundant. The `build` type is already renamed to `orchestrator` by the [Orchestrator Task Type](orchestrator-task-type.md) spec; `orchestrator` is kept for cascade-stop behavior but its *relationship queries* (finding related tasks by type) are replaced by links.

Backward compatibility: keep reading `data.spec`, `data.plan`, `data.scope.*`, and `sources` from old events. New tasks emit links. Both materialize into the same graph indexes.

#### Phase 3: Attribute → Link Migration Call Sites

Every place that writes or reads the old attributes, grouped by what replaces it.

**`data.spec` → `implements` (plan task) + `scoped-to` (planning task)**

*Writes (stop writing `data.spec`, emit links instead):*

| Location | What it does today | Migration |
|---|---|---|
| `plan.rs:422` | Planning task: `data.insert("spec", spec_path)` | Emit `scoped-to → file:{spec_path}` link on planning task instead |
| `plan.rs:394` | Planning task: `variables.set_data("spec", spec_path)` | Keep — this is a template variable for the agent, not storage |
| `build.rs:551` | Orchestrator task: `data.insert("spec", spec_path)` | Remove — orchestrator task no longer stores spec; reaches it via `orchestrates → plan → implements` |
| `.aiki/templates/aiki/plan.md:28` | Template: `--data spec={{data.spec}}` | Agent emits `implements` link on the plan task it creates instead of `--data spec=` |

*Reads (replace `data.get("spec")` with graph lookups):*

| Location | What it does today | Migration |
|---|---|---|
| `plan.rs:280,294` | `find_plan_for_spec()`: filter `data.get("spec") == spec_path` | `graph.edges.referrers("file:{spec_path}", "implements")` — generic reverse lookup |
| `plan.rs:314` | `find_created_plan()`: filter `data.get("spec") == spec_path` | Same reverse lookup |
| `build.rs:274` | `run_build_plan()`: get spec path from plan's `data.spec` | `graph.edges.target(plan_id, "implements")` — single-link forward lookup |
| `build.rs:361,419,434` | `find_plan_for_spec()`: filter `data.get("spec") == spec_path` | Same as plan.rs — `graph.edges.referrers(spec, "implements")` |
| `build.rs:453` | `cleanup_stale_builds()`: find orchestrators for spec | `graph.edges.referrers("file:{spec_path}", "implements")` → plan → `graph.edges.referrer(plan, "orchestrates")` → orchestrator |
| `build.rs:578,685` | Display: show spec path in output | `graph.edges.target(plan_id, "implements")` |
| `plan.rs:565` | Display: show spec path in output | `graph.edges.target(plan_id, "implements")` |

*Templates (update `{{data.spec}}` references):*

| Location | What it does today | Migration |
|---|---|---|
| `.aiki/templates/aiki/plan.md:6,12,27,28` | Uses `{{data.spec}}` for task name, instructions, and `--data` flag | Keep `{{data.spec}}` as template variable (set via `variables.set_data`). Remove the `--data spec=` line from template — agent emits `implements` link instead |
| `.aiki/templates/aiki/build.md:6` | Uses `{{data.spec}}` for task name | Resolve from `orchestrates → plan → implements` or keep as template variable during orchestrator task creation |

*Tests (update to use graph lookups):*

| Location | Count | Description |
|---|---|---|
| `build.rs:836-1283` | ~18 sites | Test tasks with `data.insert("spec", ...)` and assertions on `data.get("spec")` |
| `plan.rs:693-832` | ~8 sites | Test tasks with `data.insert("spec", ...)` |

---

**`data.plan` → `orchestrates`**

*Write (stop writing, emit link instead):*

| Location | What it does today | Migration |
|---|---|---|
| `build.rs:553` | `data.insert("plan", plan_id)` on orchestrator task | Emit `LinkAdded { from: orchestrator_task_id, to: plan_id, kind: "orchestrates" }` after creating orchestrator task |

*Read (replace with graph lookup):*

| Location | What it does today | Migration |
|---|---|---|
| `status_monitor.rs:302` | `root_task.data.get("plan")` → render plan subtask tree | `graph.edges.target(orchestrator_task_id, "orchestrates")` → get plan → render subtasks |

*Templates:*

| Location | What it does today | Migration |
|---|---|---|
| `.aiki/templates/aiki/build.md:18` | `{% subtask aiki/plan if not data.plan %}` — skip planning if plan exists | Check `edges.target(task_id, "orchestrates")`: skip planning subtask if orchestrator task already orchestrates a plan |

---

**`data.scope.*` → `scoped-to`**

*Writes (stop writing scope attributes, emit links instead):*

| Location | What it does today | Migration |
|---|---|---|
| `review.rs:95` | `data.insert("scope.kind", kind.as_str())` | Drop — inferred from link target prefix |
| `review.rs:96` | `data.insert("scope.id", id)` | Emit `scoped-to` link: `LinkAdded { from: review_task, to: target, kind: "scoped-to" }` where target is bare task ID or `file:` path |
| `review.rs:97` | `data.insert("scope.name", name())` | Drop — computed from link target at display time |
| `review.rs:99` | `data.insert("scope.task_ids", ids.join(","))` | Emit one `scoped-to` link per task ID |
| `review.rs:306` | `scope.to_data()` passed to `create_review_task_from_template` | Emit links after task creation instead of passing as data |
| `review.rs:309-311` | `sources = [task:{id}]` for task-scoped reviews | Keep as `sourced-from` link — `sourced-from` answers "why does this review exist?" (provenance), while `scoped-to` answers "what does it examine?" (scope). Both are needed for full provenance chain traversal (prompt → task → review → fix). Emit `sourced-from` to the trigger task, `scoped-to` for the full scope. |

*Reads (replace with graph lookups):*

| Location | What it does today | Migration |
|---|---|---|
| `review.rs:106-133` | `ReviewScope::from_data()`: reconstruct scope from `data.scope.*` | `ReviewScope::from_links(graph, task_id)`: reconstruct from `edges.targets(task_id, "scoped-to")`. Kind inferred from target prefix. Task IDs from multiple links. |
| `fix.rs:136-141` | `ReviewScope::from_data(&review_task.data)` then `scope.kind` / `scope.id` to find original task | Same — use `from_links` |

*Templates (update `{{data.scope.*}}` references):*

| Location | What it does today | Migration |
|---|---|---|
| `.aiki/templates/aiki/review.md:6` | `{{data.scope.name}}` in task name | Compute from link targets at template expansion time |
| `.aiki/templates/aiki/review.md:16` | `{% subtask aiki/review/{{data.scope.kind}} %}` — dynamic subtask dispatch | Infer kind from link target prefix, set as template variable |
| `.aiki/templates/aiki/review/task.md:6-7` | `{{data.scope.id}}` for `aiki task show` / `aiki task diff` commands | Resolve from `edges.targets(task_id, "scoped-to")` |
| `.aiki/templates/aiki/review/session.md:3` | `{{data.scope.task_ids}}` | Join task IDs from `edges.targets(task_id, "scoped-to")` |
| `.aiki/templates/aiki/review/spec.md:3,8` | `{{data.scope.id}}` for spec file path | Resolve from `edges.targets(task_id, "scoped-to")` |
| `.aiki/templates/aiki/review/implementation.md:3` | `{{data.scope.id}}` for plan ID | Resolve from `edges.targets(task_id, "scoped-to")` |
| `resolver.rs:1087-1112` | `create_review_task_from_template` takes `scope_data` HashMap | Take link targets instead, set template variables from them |
| `resolver.rs:1202,1204` | Template test fixtures use `{{data.scope.name}}` / `{{data.scope.id}}` | Update test fixtures |

*Tests (update all scope serialization tests):*

| Location | Count | Description |
|---|---|---|
| `review.rs:667-832` | ~15 tests | `ReviewScope` to_data/from_data roundtrip, scope name formatting, missing field handling |
| `fix.rs:471-488` | 2 tests | `ReviewScope::from_data` in fix context |
| `resolver.rs:1359-1391,1847-1870` | ~4 tests | Template variable context with `scope.*` data |
| `variables.rs:369-370,457-463` | ~2 tests | Variable substitution with `data.scope.*` |
| `conditionals.rs:2251-2282` | ~2 tests | Dynamic subtask dispatch with `{{data.scope.kind}}` |

---

**`task_type "plan"` → removed; `task_type "build"` → renamed to `"orchestrator"` (relationship queries replaced by links)**

The `plan` type is removed entirely — its queries are replaced by link traversal. The `build` type is renamed to `orchestrator` by the [prerequisite spec](orchestrator-task-type.md); the `orchestrator` type is kept for cascade-stop behavior, but its *relationship queries* are replaced by links. The `review` type is NOT changed — it has special formatting, guard checks, and `aiki fix` behavior that links don't replace.

*Writes:*

| Location | What it does today | Migration |
|---|---|---|
| `plan.rs:722` | `planning_task.task_type = Some("plan")` | Remove — planning task no longer needs a type; `scoped-to` link identifies it |
| `build.rs` (creation sites) | `task_type = Some("build")` on orchestrator tasks | Already handled by [orchestrator spec](orchestrator-task-type.md): changed to `"orchestrator"` |

*Reads (relationship queries → graph lookups):*

| Location | What it does today | Migration |
|---|---|---|
| `plan.rs:281,295` | `t.task_type != Some("plan")` — exclude planning task in `find_plan_for_spec` | Remove — `graph.edges.referrers(spec, "implements")` returns only the plan task directly |
| `build.rs:360` | `t.task_type == Some("build")` — find orchestrators for a spec | `graph.edges.referrer(plan, "orchestrates")` via `graph.edges.referrers(spec, "implements")` → plan |
| `build.rs:420-421,435-436` | Exclude plan/build when searching for plan output | Remove — `graph.edges.referrers(spec, "implements")` is a direct lookup |
| `build.rs:452` | `t.task_type == Some("build")` — stale orchestrator cleanup | `graph.edges.referrers(spec, "implements")` → plan → `graph.edges.referrer(plan, "orchestrates")` → orchestrator |

*Tests:*

| Location | Count | Description |
|---|---|---|
| `build.rs:996-1117` | ~10 sites | `task.task_type = Some("build")` → change to `"orchestrator"`, then replace relationship queries with graph lookups |
| `build.rs:858-895` | ~2 sites | Test tasks with `task_type = Some("plan")` for exclusion tests → remove exclusion logic |
| `plan.rs:715-722` | ~1 site | Test task with `task_type = Some("plan")` → remove |

---

**Template variable strategy:**

Templates still need values like `scope.name`, `scope.id`, `scope.kind` for variable substitution. During migration:

1. **At task creation time**: resolve link targets into template variables before expanding
2. **Set `data.scope.kind`** from the target prefix (`file:` → `spec`, bare task ID → `task`, multiple targets → `session`)
3. **Set `data.scope.id`** from the first link target (stripped of prefix)
4. **Set `data.scope.name`** computed from kind + id (e.g., `"Task (abc123)"`)
5. **These are ephemeral template variables**, not stored in events — the links are the source of truth

### Phase 4: `subtask-of` Link Kind

Emit `subtask-of` links when creating subtasks. Implement closure validation (parent can't close with open children). Enable re-parenting.

**Remove encoded child-ID support**: All subtasks get regular 32-character IDs. The `task add --subtask-of <id>` command generates a new full ID and emits a `subtask-of` link. Short ID resolution works for subtasks.

**Ancestor chain provenance**: Update the provenance write path to include ancestor task IDs in the `[aiki]` block (see [Ancestor Chain Provenance](#ancestor-chain-provenance)). This ensures `task diff <parent>` and other revset-based queries match descendant changes without OR-expanding all child IDs at query time.

Backward compatibility: For existing tasks with legacy encoded child IDs, the materializer:
1. Parses the dot to extract parent ID
2. Synthesizes a `subtask-of` link during replay
3. Issues a deprecation warning suggesting migration to link-based IDs

Backward compatibility for provenance: Old changes written before ancestor chain support will only have `task=<subtask-id>`. During the transition, `build_task_revset_pattern` keeps the legacy child-match clause (`task=X.`) to catch these. After migration, when all active tasks use full IDs with ancestor chain provenance, that clause is removed.

#### Encoded Child-ID Removal: Code Impact Analysis

Removing encoded child IDs is a **major refactor** affecting core task management code:

**Files requiring changes (9):**
- `cli/src/tasks/id.rs` — functions implementing legacy child-ID parsing
- `cli/src/tasks/manager.rs` — functions using encoded child IDs for parent-child logic
- `cli/src/commands/task.rs` — 4+ call sites generate child IDs
- `cli/src/commands/plan.rs`, `build.rs` — Template subtask generation
- `cli/src/commands/agents_template.rs` — Documents `.0` planning task
- `cli/src/tasks/templates/` — Static subtask expansion

**Core functions affected:**

*ID Generation & Parsing (id.rs):*
- `generate_child_id(parent_id, child_number)` → **Replace with `generate_task_id(name)`**
- `get_parent_id(task_id)` → **Check edges first, fall back to dot parsing for old tasks**
- `is_subtask_of`, `is_direct_subtask_of` → **Use edge lookups**
- `get_child_number`, `get_next_subtask_number` → **Remove (no longer needed)**
- `is_task_id`, `is_task_id_prefix` → **Simplify to only validate 32-char root IDs**

*Task Manager (manager.rs):*
- `has_subtasks`, `get_subtasks` → **Use `TaskGraph.children` index**
- `get_scoped_ready_queue` → **Use edge-based parent lookups**
- `get_current_scope_set` → **Use `TaskGraph.parent_of` index**
- `all_subtasks_closed`, `get_unclosed_subtasks` → **Use edge-based queries**
- `get_all_unclosed_descendants` → **Recursive edge traversal**

*Task Commands (task.rs):*

**Line 1071-1072: Subtask creation (`task add --subtask-of`)**
```rust
// OLD:
let subtask_num = get_next_subtask_number(parent_id, task_ids.into_iter());
let child_id = generate_child_id(parent_id, subtask_num);

// NEW:
let child_id = generate_task_id(&name);
// Emit subtask-of link after creating task
```

**Line 1384-1432: The `.0` planning task**
```rust
// OLD: Auto-create a deterministic planning subtask when starting a parent with subtasks
let planning_id = generate_child_id(&task_id, 0);

// NEW: Generate regular ID, emit subtask-of link
let planning_id = generate_task_id("Review all subtasks and start first batch");
// Emit: LinkAdded { from: planning_id, to: task_id, kind: "subtask-of" }
```

**Line 3905-3906, 4078, 4148, 4298: Template subtask generation**
```rust
// OLD: Sequential numbering
let subtask_id = generate_child_id(parent_id, i + 1);

// NEW: Each subtask gets its own ID
let subtask_id = generate_task_id(&subtask.name);
// Emit subtask-of link for each
```

**Tests requiring updates:**
- `cli/src/tasks/manager.rs` — 60+ test assertions using encoded parent/child IDs
- Tests for scoping, ready queue, subtask ordering, `.0` planning task behavior

**Documentation requiring updates:**
- `AGENTS.md`, `cli/AGENTS.md` — Parent-child relationship examples
- `ops/done/task-system.md` — Original task system design
- `ops/now/aiki-vs-beads-comparison.md` — May reference legacy encoded child IDs

**Special features to handle:**

*The `.0` Planning Task:*
- Current: When starting a parent with subtasks, auto-creates a deterministic planning child
- Migration: Generate regular task ID, emit subtask-of link
- Behavior preserved: Still auto-creates planning task, just with full ID

*Short ID Resolution:*
- Current: Can't use short IDs for subtasks (ambiguous with `parent` prefix)
- After: All tasks use same 32-char ID format, short IDs work uniformly

*Template System:*
- Current: Static subtasks defined with implicit numbering (`.1`, `.2`, etc.)
- Migration: Each subtask needs a name for ID generation, or use sequential naming

**Estimated effort:** 2-3 days of focused work + thorough testing

**Migration strategy:**
1. Add link infrastructure first (Phases 1-3)
2. Implement dual support (emit both legacy encoded IDs and `subtask-of` links)
3. Migrate ID generation incrementally (new tasks get full IDs + subtask-of links)
4. Update all `generate_child_id` call sites to use `generate_task_id` + subtask-of link emission
5. Add deprecation warnings for legacy child-ID parsing
6. Remove legacy child-ID support after transition period

### Phase 5: Basic DAG Operations

Implement simple query operations that don't require advanced algorithms:
- `provenance_chain` — Walk source links back to origin (cycle-safe via visited set)
- `spawned_from` — Reverse provenance lookup (single-hop, no cycle risk)

Advanced operations (critical path, parallel tracks) are deferred to [Advanced Task Graph Queries](../future/advanced-task-graph-queries.md).

### Phase 6: Documentation Updates

After basic link infrastructure (Phases 1-3) is working and agents can start using the new commands, update agent documentation:

**Update `cli/src/commands/agents_template.rs` (AIKI_BLOCK_TEMPLATE):**

1. **Update existing command syntax** to use new link-based commands:
   - Change `--parent` to `--subtask-of` in subtask examples
   - Update `--source` examples to mention link-based implementation
   - Add `aiki task link` command examples to Quick Reference section

2. **Add "Task Relationships" section** after "Task Priorities":
   - Explain the design principle: "If it's a relationship between two things, it's a link. If it's a property of one thing, it's an attribute."
   - Document the seven link types and their semantics (including `supersedes`)
   - Show `aiki task link` command usage with examples for each type
   - Explain when to use each link type (`blocked-by`, `sourced-from`, `subtask-of`, `implements`, `orchestrates`, `scoped-to`, `supersedes`)
   - Document single-link auto-replace behavior for `implements`, `orchestrates`, `subtask-of`, and `supersedes`
   - Include provenance chain examples using `aiki task trace`

3. **Update subtask documentation**:
   - Replace encoded child-ID examples with full 32-char task IDs
   - Explain that parent-child relationships are now `subtask-of` links
   - Update examples to show `aiki task link <child-id> --subtask-of <parent-id>`
   - Clarify that short IDs now work for all tasks including subtasks

**Update AGENTS.md files** (both root and `cli/AGENTS.md`):
- Run `aiki doctor` to regenerate from updated template
- Verify all command examples use new syntax
- Ensure link system documentation is present and accurate

**Bump AIKI_BLOCK_VERSION** to `"1.14"` to track this change.

This ensures agents have clear guidance on when to use links vs. attributes and how to leverage the DAG for task management.

## What We Don't Build (In This Phase)

- **19 dependency kinds**: Start with 7. The infrastructure supports more, but we add them when there's a real use case.
- **Blocking cache**: Beads needs `blocked_issues_cache` for performance at 10K+ issues. At aiki's scale (<1000), computing blocked status during replay is fast enough.
- **Background daemon**: Beads runs a daemon for sync. Aiki's event replay is cheap enough to run on every invocation.
- **Duplicate detection**: No content-hash dedup. Not needed at current scale.
- **Gate coordination**: `waits-for` with all/any semantics. Only relevant for multi-agent orchestration.
- **Advanced graph algorithms**: Critical path, parallel tracks, etc. See [Advanced Task Graph Queries](../future/advanced-task-graph-queries.md) for future enhancements.

## Future Edge Kinds (When Needed)

| Kind | When | Trigger |
|------|------|---------|
| `conditional-blocks` | Fallback workflows needed | "Run B only if A fails" |
| `duplicates` | Scale past ~100 active tasks | Dedup with auto-close |
| `waits-for` | Multi-agent orchestration | Dynamic fanout gates |

## Success Criteria

- `aiki task link B --blocked-by A && aiki task` shows B as blocked, not in ready queue
- `aiki task close A && aiki task` shows B now in ready queue
- `aiki task link list --blocked-by` shows all blocking links
- `aiki task list --blocked-by <id>` shows what closing a task would free up
- `aiki task trace <id>` shows full trace
- `aiki task link A --implements ops/now/spec.md` stores link with `file:` prefix
- `aiki task list --implements ops/now/spec.md` returns the plan task
- `aiki task link <orchestrator> --orchestrates <plan>` stores link; forward and reverse lookups work
- Status monitor renders plan subtree via `edges.target(orchestrator, "orchestrates")` (no more `data.plan`)
- `aiki task link <task> --scoped-to ops/now/feature.md` stores link with `file:` prefix
- `aiki task link <task> --scoped-to <short-id>` resolves to full 32-char task ID before storing
- Review tasks use `scoped-to` links instead of `data.scope.*` attributes
- `data.spec`, `data.plan`, `data.scope.*` no longer written by new code
- `task_type "plan"` no longer set on new tasks (removed)
- `task_type "build"` replaced by `"orchestrator"` (per [orchestrator spec](orchestrator-task-type.md))
- No code queries `task_type` for relationship lookups — all use graph traversal (orchestrator type retained only for cascade-stop behavior)
- All stored link events use canonical IDs: full 32-char task IDs (never short prefixes, never `task:` prefix), typed external refs (`file:`, `prompt:`, etc.)
- Short ID input on CLI is resolved to full canonical ID at write time; ambiguous short IDs are rejected with an error
- Backward compatibility during transition: existing encoded child IDs parse correctly and synthesize subtask-of links
- `task diff <parent>` includes all descendant changes via ancestor chain provenance (no OR expansion in revset)
- Provenance blocks for subtask changes include `task=` lines for all ancestors up to root
- End state: new subtasks use full 32-char IDs with subtask-of links; legacy encoded child-ID support removed after migration
- `aiki task link B --supersedes A` stores link; forward and reverse lookups work
- `aiki task link B --implements spec.md` when A already implements spec.md → auto-replaces A, emits `supersedes` link from B to A
- `aiki task link child --subtask-of new-parent` when child has old-parent → auto-replaces, no `supersedes` link (re-parenting)
- `aiki task link B --blocked-by typo123` → `InvalidLinkTarget` error when `typo123` matches no tasks (not silently coerced to `file:`)
- `aiki task link B --blocked-by file:design.md` → `InvalidLinkTarget` error — task-only kinds reject external refs
- `provenance_chain` terminates on `sourced-from` cycles (visited set)
- `ancestor_chain` terminates on `subtask-of` cycles (defense-in-depth visited set)
- Review tasks emit both `sourced-from` (trigger) and `scoped-to` (scope) links — provenance chain traversal intact
- Zero performance regression on typical workloads (<100 tasks)
