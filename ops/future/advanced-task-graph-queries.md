# Advanced Task Graph Queries

**Date**: 2026-02-10
**Status**: Future / Not Yet Needed
**Related**: [Task DAG Design](../now/task-dag.md)

---

## When to Implement

Add these features when basic DAG operations (blocked-by, source, parent edges) are stable and we have evidence that users need advanced graph analysis.

**Triggers:**
- Users ask "what's the critical path?" or "show me parallel tracks"
- Task graphs exceed 200+ nodes with complex dependencies
- Performance of naive algorithms becomes a problem (>50ms for common operations)

## Petgraph Integration

### Why Petgraph

Once we need any of these algorithms, petgraph becomes worth the dependency cost:

1. **Shortest path algorithms** (Dijkstra, A*) — "What's the fastest way to unblock this task?"
2. **Strongly connected components** (Tarjan) — Detect circular dependencies in complex graphs
3. **Minimum spanning tree** — Optimize parallel execution plans
4. **Graph algorithms with O(E log V) or better** — When our O(V²) naive implementations become too slow

### Data Structure Compatibility

Our current design (from task-dag.md) is already petgraph-ready:

```rust
/// Current TaskGraph (HashMap adjacency lists)
pub struct TaskGraph {
    pub tasks: FastHashMap<String, Task>,
    blocked_by: OnceLock<FastHashMap<String, Vec<String>>>,
    blocks: OnceLock<FastHashMap<String, Vec<String>>>,
    sources: OnceLock<FastHashMap<String, Vec<String>>>,
    children: OnceLock<FastHashMap<String, Vec<String>>>,
    parent_of: OnceLock<FastHashMap<String, Option<String>>>,
}
```

### Conversion to Petgraph

When needed, convert on-demand:

```rust
impl TaskGraph {
    /// Convert to petgraph for complex algorithms
    pub fn to_petgraph(&self) -> petgraph::Graph<&Task, LinkKind> {
        use petgraph::Graph;
        
        let mut graph = Graph::new();
        let mut node_map = FastHashMap::default();
        
        // Add all tasks as nodes
        for (id, task) in &self.tasks {
            let node_idx = graph.add_node(task);
            node_map.insert(id.clone(), node_idx);
        }
        
        // Add blocked-by edges
        for (from_id, to_ids) in self.blocked_by() {
            let from_idx = node_map[from_id];
            for to_id in to_ids {
                if let Some(&to_idx) = node_map.get(to_id) {
                    graph.add_edge(from_idx, to_idx, EdgeKind::BlockedBy);
                }
            }
        }
        
        // Add source edges (only task→task, not external refs)
        for (from_id, to_ids) in self.sources() {
            let from_idx = node_map[from_id];
            for to_id in to_ids {
                if let Some(&to_idx) = node_map.get(to_id) {
                    graph.add_edge(from_idx, to_idx, EdgeKind::Source);
                }
            }
        }
        
        // Add parent edges
        for (child_id, parent_id_opt) in self.parent_of() {
            if let Some(parent_id) = parent_id_opt {
                let child_idx = node_map[child_id];
                if let Some(&parent_idx) = node_map.get(parent_id) {
                    graph.add_edge(child_idx, parent_idx, EdgeKind::Parent);
                }
            }
        }
        
        graph
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    BlockedBy,
    Source,
    Parent,
}
```

### Don't Use Petgraph For

These are simple enough to implement directly:

- **Simple DFS/BFS** — 20 lines of code with a Vec stack/queue
- **Cycle detection** — DFS-based, already needed for blocked-by validation
- **Reverse index lookups** — HashMap is perfect for this (what we have now)
- **Operations with <1000 nodes** — Naive algorithms are fast enough

## Advanced Query Operations

### Critical Path Analysis

Find the longest chain of blocking dependencies:

```rust
impl TaskGraph {
    /// Find the critical path (longest chain of blocked-by links)
    /// Returns tasks in order from start to end of critical path
    pub fn critical_path(&self) -> Vec<&Task> {
        let pg = self.to_petgraph();
        
        // Use petgraph's bellman_ford with negative weights
        // (longest path = shortest path with negated weights)
        use petgraph::algo::bellman_ford;
        
        // Find source nodes (no incoming blocked-by links)
        let sources: Vec<_> = pg.node_indices()
            .filter(|&n| pg.neighbors_directed(n, petgraph::Incoming).count() == 0)
            .collect();
        
        // For each source, find longest path to any sink
        // Return the longest one
        todo!("Implement using bellman_ford with negative weights")
    }
}
```

### Parallel Track Detection

Find independent subgraphs that can be worked on in parallel:

```rust
impl TaskGraph {
    /// Find independent parallel tracks (connected components)
    /// Returns groups of tasks that can be executed in parallel
    pub fn parallel_tracks(&self) -> Vec<Vec<&Task>> {
        let pg = self.to_petgraph();
        
        use petgraph::algo::kosaraju_scc;
        
        // Find strongly connected components
        let sccs = kosaraju_scc(&pg);
        
        sccs.into_iter()
            .map(|component| {
                component.into_iter()
                    .map(|node_idx| pg[node_idx])
                    .collect()
            })
            .collect()
    }
}
```

### Impact Analysis

What becomes ready if I close this task?

```rust
impl TaskGraph {
    /// What tasks become ready if I close this one?
    /// This is simple enough we don't need petgraph
    pub fn unblocks(&self, task_id: &str) -> Vec<&Task> {
        self.blocks()
            .get(task_id)
            .map(|blocked_ids| {
                blocked_ids.iter()
                    .filter_map(|id| self.tasks.get(id))
                    .filter(|t| {
                        // Only include if this is the LAST blocker
                        let other_blockers: Vec<_> = self.blocked_by()
                            .get(&t.id)
                            .unwrap()
                            .iter()
                            .filter(|&b| b != task_id)
                            .filter(|&b| {
                                self.tasks.get(b)
                                    .map_or(true, |bt| bt.status != TaskStatus::Closed)
                            })
                            .collect();
                        
                        other_blockers.is_empty()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
```

### Provenance Chain Walking

Walk source links to find the origin of a task:

```rust
impl TaskGraph {
    /// Full provenance chain (walk source links back to origin)
    /// Returns chain from origin to this task
    pub fn provenance_chain(&self, task_id: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut current = task_id.to_string();
        let mut visited = std::collections::HashSet::new();
        
        loop {
            if !visited.insert(current.clone()) {
                // Cycle detected, stop
                break;
            }
            
            chain.push(current.clone());
            
            // Get source links for current task
            if let Some(sources) = self.sources().get(&current) {
                // Follow first task→task link (skip external refs)
                if let Some(next) = sources.iter()
                    .find(|s| self.tasks.contains_key(*s))
                {
                    current = next.clone();
                    continue;
                }
            }
            
            // No more task→task links, we've reached the origin
            break;
        }
        
        chain.reverse(); // Origin first
        chain
    }
    
    /// Reverse provenance: what tasks came from this origin?
    /// Origin can be external ref (e.g., "file:design.md")
    pub fn spawned_from(&self, origin: &str) -> Vec<&Task> {
        self.sources()
            .iter()
            .filter_map(|(task_id, sources)| {
                if sources.contains(&origin.to_string()) {
                    self.tasks.get(task_id)
                } else {
                    None
                }
            })
            .collect()
    }
}
```

## CLI Commands

These would be added once the algorithms are implemented:

```bash
# Critical path analysis
aiki task critical-path              # Show longest blocking chain
aiki task critical-path --from <id>  # Critical path from specific task

# Parallel tracks
aiki task tracks                     # Show independent work streams
aiki task tracks --assign            # Suggest task assignment to agents

# Impact analysis (this one is simple, could add earlier)
aiki task unblocks <id>              # What becomes ready if I close this?

# Advanced provenance
aiki task trace <id>                 # Full chain back to origin
aiki task spawned file:design.md     # All tasks from this source
```

## Performance Considerations

### When to Optimize

Profile first! Only add petgraph when:

1. **Measurement shows need**: Use `cargo flamegraph` to identify bottlenecks
2. **Scale justifies it**: >200 tasks with complex dependency graphs
3. **User pain**: Operations take >50ms and users notice

### Benchmarks to Add

```rust
#[bench]
fn bench_critical_path_naive(b: &mut Bencher) {
    let graph = setup_complex_graph(100);
    b.iter(|| graph.critical_path_naive());
}

#[bench]
fn bench_critical_path_petgraph(b: &mut Bencher) {
    let graph = setup_complex_graph(100);
    b.iter(|| graph.critical_path());
}
```

### Memory Tradeoffs

**Current (HashMap adjacency lists):**
- Memory: O(V + E) where V = tasks, E = edges
- Lookup: O(1) average case
- Iteration: O(V + E)

**Petgraph conversion:**
- Additional O(V + E) temporary memory during conversion
- Petgraph's Graph: similar O(V + E) but with index overhead
- Only convert when needed (don't store both representations)

## Migration Path

1. **Phase 0-4**: Build basic DAG with HashMap adjacency lists (current plan)
2. **Phase 5**: Add simple algorithms that don't need petgraph (unblocks, provenance_chain)
3. **Future Phase 6**: When needed, add petgraph and advanced algorithms
4. **Benchmark**: Measure before/after to justify the dependency cost

## Success Criteria

Only implement this when users request it AND profiling shows need:

- Critical path analysis completes in <100ms for 200-node graphs
- Parallel track detection identifies independent work streams correctly
- Zero regression in basic operations (task list, ready queue, etc.)
- Petgraph dependency is justified by measurable user value

## References

- [Petgraph Documentation](https://docs.rs/petgraph/)
- [Task DAG Design](../now/task-dag.md) - Current implementation plan
- [Performance Analysis](../done/performance-analysis.md) - Baseline measurements
