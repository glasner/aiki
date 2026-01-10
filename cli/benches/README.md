# Task System Benchmarks

This directory contains benchmarks for the Aiki task system, specifically measuring the performance of task creation and event writing operations.

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench --bench task_benchmarks

# Run benchmarks in quick mode (faster, less accurate)
cargo bench --bench task_benchmarks -- --quick

# Run specific benchmark
cargo bench --bench task_benchmarks create_task
```

## Benchmark Suite

### 1. `create_task/*`
Compares two approaches for task ID generation:

**`generated_id` (current approach):**
- Generates a JJ-compatible change_id locally using hash function
- Writes event with generated ID
- **Performance:** ~86ms per task

**`jj_change_id` (alternative approach):**
- Creates a JJ change first
- Queries JJ for the change_id
- Updates change description with the ID
- **Performance:** ~144ms per task

**Conclusion:** Generating IDs locally is 40% faster (58ms saved per task) while maintaining JJ change_id format compatibility.

### 2. `write_event/*`
Measures the time to write different types of events:
- **created**: Write a Created event (~87ms)
- **started**: Write a Started event (~89ms)
- **stopped**: Write a Stopped event (~89ms)
- **closed**: Write a Closed event (~91ms)

All event types have similar performance since they all involve creating a JJ change.

### 3. `read_events/{n}`
Measures the time to read events with varying numbers of tasks (1, 5, 10, 25, 50).

**Key Findings:**
- Reading events scales well with task count
- 1 task: ~18ms
- 50 tasks: ~21ms
- Performance is dominated by `jj log` execution, not parsing

### 4. `sequential_tasks/{n}`
Measures realistic workflow of creating multiple tasks sequentially (5, 10, 25 tasks).

**Performance:**
- 5 tasks: ~688ms (137.6ms per task)
- 10 tasks: ~1.4s (140ms per task)
- 25 tasks: ~3.6s (142ms per task)

Performance is very consistent per-task, indicating no degradation with more tasks.

### 5. `task_lifecycle`
Measures a complete task lifecycle:
1. Create task
2. Start task
3. Stop task
4. Close task

**Performance:** ~344ms for complete lifecycle

This is approximately 2.5x the time of a single task creation, which makes sense since it involves 4 JJ operations total.

## Performance Characteristics

### Bottlenecks
The primary performance bottleneck is **JJ process spawning**. Each operation spawns multiple `jj` commands:
- Creating a task: 3 JJ commands (new, log, describe, bookmark set)
- Writing an event: 2 JJ commands (new, bookmark set)
- Reading events: 1 JJ command (log)

### Scaling
The task system scales linearly with the number of operations:
- Per-task overhead is constant (~140ms)
- Reading scales sub-linearly (only +15% for 50x more tasks)

### Optimization Opportunities
1. **Batch operations**: Group multiple events into single JJ changes
2. **Use jj-lib directly**: Avoid process spawning overhead
3. **Cache reads**: Keep event log in memory between operations

## Interpreting Results

The benchmarks use Criterion.rs which provides:
- **Time estimates** with confidence intervals
- **Regression detection** across runs
- **Statistical analysis** to filter noise

Look for:
- **Consistent times** across runs (good)
- **Wide confidence intervals** (indicates noise)
- **Performance regressions** (Criterion will warn)

## Benchmark Implementation

See `task_benchmarks.rs` for implementation details. The benchmarks:
- Use `tempfile::TempDir` for isolated test repos
- Initialize fresh JJ repos for each iteration
- Use Criterion's `iter_batched` for proper setup/teardown
- Measure realistic workflows, not just individual functions
