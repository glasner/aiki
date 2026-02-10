# Design: Task DAG

**Date**: 2026-02-10
**Status**: Design
**Related**: [Beads Dependency Evolution](beads-dependency-evolution.md), [Aiki vs Beads Comparison](aiki-vs-beads-comparison.md), [Task System Design](../done/task-system.md)

---

## Problem

Aiki's task system has no dependency graph. Tasks are nodes with attributes but no edges between them. This means:

- **No blocking**: Can't express "task B can't start until task A is done." The `--blocked` flag creates a separate P0 human task with a text description -- there's no link back to the blocked task, no auto-resolution when the blocker closes.
- **No provenance traversal**: `--source task:abc` stores a string. You can't walk the chain (prompt → task → review → fix) without scanning all tasks and parsing source strings.
- **No impact analysis**: "If I close this task, what unblocks?" is unanswerable.
- **No parallel track computation**: Can't partition work into independent lanes for multiple agents.
- **No critical path**: Can't identify which task, if delayed, delays everything.

Beads solved this with a unified `dependencies` table (Decision 004) and open-ended edge types. After that consolidation, they went from 4 to 19 types with zero schema changes.

## Principle

**If it's a relationship between two things, it's an edge. If it's a property of one thing, it's an attribute.**

The DAG becomes the primary data structure. Tasks are nodes. The graph enables slicing and dicing work by topology, not just by flat filters.

## Design

### Two New Event Variants

```rust
/// Edge added between two nodes
EdgeAdded {
    from: String,                        // source node (task ID or external ref)
    to: String,                          // target node (task ID or external ref)
    edge_type: String,                   // open-ended type string
    metadata: HashMap<String, String>,   // optional key-value pairs on the edge
    timestamp: DateTime<Utc>,
}

/// Edge removed between two nodes
EdgeRemoved {
    from: String,
    to: String,
    edge_type: String,
    timestamp: DateTime<Utc>,
}
```

That's the entire storage extension. Two variants handle every relationship type forever.

### Event Metadata Format

Stored on the `aiki/tasks` branch like all other events:

```
[aiki-task]
event=edge_added
from=mvslrspmoynoxyyywqyutmovxpvztkls
to=nqrtxsypzkwolmnrstvuqxyzplmrwknos
edge_type=blocks
timestamp=2026-02-10T14:30:00Z
[/aiki-task]
```

External references use typed prefixes as `to` targets:

```
[aiki-task]
event=edge_added
from=mvslrspmoynoxyyywqyutmovxpvztkls
to=file:ops/now/design.md
edge_type=source
timestamp=2026-02-10T14:30:00Z
[/aiki-task]
```

### Edge Types (Start With 3)

| Type | From → To | Blocks Ready? | Direction Semantics |
|------|-----------|---------------|---------------------|
| `blocks` | blocked task → blocker task | **Yes** | "I am blocked by this" |
| `source` | task → origin | No | "I came from this" |
| `parent` | child → parent | **Yes** (upward propagation) | "I am a child of this" |

Open-ended string type means adding more later requires zero code changes to storage or events. Just define the semantics and update the ready queue if it's a blocking type.

#### `blocks`

The core workflow dependency. "Task B can't start until task A closes."

```
aiki task dep add <blocked> <blocker>
aiki task dep remove <blocked> <blocker>
```

A task with unresolved `blocks` edges never appears in the ready queue.

#### `source`

Replaces the `sources: Vec<String>` attribute on `Created` events. Each source becomes an edge:

```
Edge { from: "task-abc", to: "prompt:nzwtoqqr",       type: "source" }
Edge { from: "task-def", to: "task:task-abc",          type: "source" }
Edge { from: "task-ghi", to: "comment:c1a2b3",         type: "source" }
Edge { from: "task-ghi", to: "file:ops/now/design.md", type: "source" }
```

The `to` target can be a task ID or a typed external reference. All existing source prefixes (`file:`, `task:`, `comment:`, `issue:`, `prompt:`) work unchanged -- they just become edge targets instead of attribute values.

This enables provenance chain traversal:
```
prompt:xyz → task-abc → task-def (review) → task-ghi (fix)
                ↑
        file:design.md
```

And reverse queries: "What tasks were spawned from this design doc?" becomes a reverse edge lookup on `to=file:ops/now/design.md`, not a full scan.

#### `parent`

Replaces the implicit parent-child encoding in the ID string. Each parent-child relationship becomes an edge:

```
Edge { from: "task-child", to: "task-parent", type: "parent" }
```

The dot-notation ID convention (`parent.1`) can coexist as human-readable sugar. Benefits:

- **Re-parenting**: Move a subtask to a different epic by removing one edge and adding another.
- **Upward blocking propagation**: If any descendant is blocked, walk `parent` edges upward to mark ancestors as blocked too.
- **Uniform queries**: Subtask lookup uses the same edge infrastructure as everything else.

### Materialized Graph

During event replay, build a `TaskGraph` instead of just a `HashMap<String, Task>`:

```rust
/// A single edge in the task DAG
pub struct Edge {
    pub from: String,
    pub to: String,
    pub edge_type: String,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

/// Materialized task graph (computed from events)
pub struct TaskGraph {
    /// Node data (tasks)
    pub tasks: HashMap<String, Task>,

    /// All edges
    pub edges: Vec<Edge>,

    /// Computed indexes (rebuilt on replay)
    /// task_id → task_ids that block it (unresolved)
    pub blocked_by: HashMap<String, Vec<String>>,
    /// task_id → task_ids it blocks
    pub blocks: HashMap<String, Vec<String>>,
    /// task_id → source references (task IDs + external refs)
    pub sources: HashMap<String, Vec<String>>,
    /// parent_id → child task_ids
    pub children: HashMap<String, Vec<String>>,
    /// task_id → parent_id
    pub parent_of: HashMap<String, Option<String>>,
}
```

The indexes are cheap to build during replay -- just iterate edges once and populate HashMaps. At <1000 tasks, this is microseconds.

### Ready Queue Changes

Current:
```rust
pub fn get_ready_queue(tasks: &HashMap<String, Task>) -> Vec<&Task> {
    tasks.values()
        .filter(|t| t.status == TaskStatus::Open)
        .collect()
}
```

DAG-aware:
```rust
pub fn get_ready_queue(graph: &TaskGraph) -> Vec<&Task> {
    graph.tasks.values()
        .filter(|t| t.status == TaskStatus::Open)
        .filter(|t| !graph.is_blocked(&t.id))
        .collect()
}

impl TaskGraph {
    /// A task is blocked if any of its blockers are not Closed
    pub fn is_blocked(&self, task_id: &str) -> bool {
        if let Some(blockers) = self.blocked_by.get(task_id) {
            blockers.iter().any(|b| {
                self.tasks.get(b)
                    .map_or(true, |t| t.status != TaskStatus::Closed)
            })
        } else {
            false
        }
    }

    /// Upward propagation: a parent is blocked if any descendant is blocked
    pub fn is_transitively_blocked(&self, task_id: &str) -> bool {
        if self.is_blocked(task_id) {
            return true;
        }
        if let Some(children) = self.children.get(task_id) {
            children.iter().any(|c| self.is_transitively_blocked(c))
        } else {
            false
        }
    }
}
```

### DAG Operations

Once the graph exists, these operations become natural:

```rust
impl TaskGraph {
    /// What tasks become ready if I close this one?
    pub fn unblocks(&self, task_id: &str) -> Vec<&Task>

    /// Full provenance chain (walk source edges)
    pub fn provenance_chain(&self, task_id: &str) -> Vec<&Edge>

    /// Reverse provenance: what tasks came from this origin?
    pub fn spawned_from(&self, origin: &str) -> Vec<&Task>

    /// Independent parallel tracks (connected components via Union-Find)
    pub fn parallel_tracks(&self) -> Vec<Vec<&Task>>

    /// Critical path (longest chain of blocking edges)
    pub fn critical_path(&self) -> Vec<&Task>

    /// Children of a parent (replaces get_subtasks)
    pub fn children_of(&self, parent_id: &str) -> Vec<&Task>

    /// Cycle detection on blocking edges
    pub fn has_cycles(&self) -> Option<Vec<String>>
}
```

### CLI Commands

```bash
# Add/remove blocking dependency
aiki task dep add <blocked-task> <blocker-task>
aiki task dep remove <blocked-task> <blocker-task>

# Show what a task blocks / is blocked by
aiki task show <id>            # includes "Blocked by:" and "Blocks:" sections

# What unblocks if I close this?
aiki task unblocks <id>

# Show the full DAG for a scope
aiki task graph                 # ASCII DAG of current scope
aiki task graph --tracks        # show parallel execution tracks

# Provenance chain
aiki task trace <id>            # walk source edges back to origin
```

### XML Output Changes

```xml
<aiki_task cmd="show" status="ok">
  <task id="abc" name="Fix auth" status="open" priority="p1">
    <blocked_by>
      <dep task="xyz" name="Refactor token service" status="in_progress"/>
    </blocked_by>
    <blocks>
      <dep task="def" name="Add OAuth flow" status="open"/>
      <dep task="ghi" name="Write auth tests" status="open"/>
    </blocks>
    <sources>
      <source type="file" ref="ops/now/auth-design.md"/>
      <source type="task" ref="lmn" name="Auth epic"/>
    </sources>
    <parent ref="lmn" name="Auth epic"/>
  </task>
</aiki_task>
```

The ready queue list gains a `blocked` count:

```xml
<list ready="3" blocked="2">
  <task id="abc" name="Fix auth" priority="p1"/>
  ...
</list>
```

## Migration

### Phase 1: Add Edge Infrastructure

Add `EdgeAdded` and `EdgeRemoved` event variants. Add `TaskGraph` struct. Change `materialize_tasks` to `materialize_graph`. Existing events and behavior unchanged.

### Phase 2: `blocks` Edge Type + Ready Queue

Implement `aiki task dep add/remove`. Update ready queue to filter by unresolved blockers. Replace the `--blocked` text-field workaround with real edges.

### Phase 3: `source` Edge Type

When creating tasks with `--source`, emit `EdgeAdded` events instead of (or in addition to) storing in the `sources` attribute. Implement `aiki task trace` for provenance chain walking.

Backward compatibility: keep reading `sources` from old `Created` events. New tasks emit edges. Both materialize into the same `sources` index on `TaskGraph`.

### Phase 4: `parent` Edge Type

Emit `parent` edges when creating subtasks. Implement upward blocking propagation. Enable re-parenting.

Backward compatibility: parent-child relationships from dot-notation IDs continue to work. The materializer synthesizes `parent` edges from IDs for old tasks.

### Phase 5: DAG Operations

Implement `parallel_tracks` (Union-Find), `critical_path`, `unblocks`, `has_cycles`. Add `aiki task graph` visualization.

## What We Don't Build

- **19 dependency types**: Start with 3. The infrastructure supports more, but we add them when there's a real use case.
- **Blocking cache**: Beads needs `blocked_issues_cache` for performance at 10K+ issues. At aiki's scale (<1000), computing blocked status during replay is fast enough.
- **Background daemon**: Beads runs a daemon for sync. Aiki's event replay is cheap enough to run on every invocation.
- **Duplicate detection**: No content-hash dedup. Not needed at current scale.
- **Gate coordination**: `waits-for` with all/any semantics. Only relevant for multi-agent orchestration.

## Future Edge Types (When Needed)

| Type | When | Trigger |
|------|------|---------|
| `conditional-blocks` | Fallback workflows needed | "Run B only if A fails" |
| `duplicates` | Scale past ~100 active tasks | Dedup with auto-close |
| `supersedes` | Design doc evolution | Version chain tracking |
| `waits-for` | Multi-agent orchestration | Dynamic fanout gates |

## Success Criteria

- `aiki task dep add B A && aiki task` shows B as blocked, not in ready queue
- `aiki task close A && aiki task` shows B now in ready queue
- `aiki task trace <id>` walks provenance chain back to origin
- `aiki task graph` shows ASCII DAG of current scope
- `aiki task unblocks <id>` shows what closing a task would free up
- Existing `--source` and parent-child behavior unchanged (backward compatible)
- Zero performance regression on typical workloads (<100 tasks)
