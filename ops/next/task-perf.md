# Task Performance Optimization

**Status**: 🔴 Design Phase  
**Priority**: High  
**Depends On**: Milestone 1.4 Task System  
**Related**: `task-system.md`, `close-and-comment.md`

---

## Overview

As the task system matures, performance becomes critical for a good user experience. This document outlines:
1. A benchmarking system to measure task command performance
2. Performance optimization plan for `aiki task add` and `aiki task close`
3. Guidelines for maintaining performance as features grow

**Performance Goals:**

| Command | Initial Target (P95) | Stretch Goal (P95) |
|---------|---------------------|-------------------|
| `aiki task add` | < 100ms | < 50ms |
| `aiki task close` | < 100ms | < 50ms |
| `aiki task list` | < 200ms (1000+ tasks) | < 100ms (1000+ tasks) |
| `aiki task show` | < 100ms | < 50ms |

---

## Table of Contents

1. [Benchmarking System](#benchmarking-system)
2. [Performance Analysis](#performance-analysis)
3. [Optimization Plan: task add](#optimization-plan-task-add)
4. [Optimization Plan: task close](#optimization-plan-task-close)
5. [Implementation](#implementation)
6. [Monitoring](#monitoring)

---

## Benchmarking System

### Benchmark Framework

Task benchmarks already exist at `cli/benches/task_benchmarks.rs`. The benchmark suite includes:

- **create_task** - Task creation with generated ID vs JJ change_id
- **write_event** - Writing different event types (Created, Started, Stopped, Closed)
- **read_events** - Reading events with varying task counts (1, 5, 10, 25, 50)
- **sequential_tasks** - Creating multiple tasks in sequence
- **task_lifecycle** - Full lifecycle (create → start → stop → close)

**Missing benchmarks to add:**
- `bench_task_show` - Displaying task details
- `bench_task_list` - Listing tasks at scale (100, 1000, 5000 tasks)

### Benchmark Structure

Benchmarks follow the pattern in `cli/benches/task_benchmarks.rs`:

```rust
// Example: Adding bench_task_show to existing benchmarks
fn bench_task_show(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_show");

    for num_tasks in [1, 10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_tasks),
            num_tasks,
            |b, &num_tasks| {
                let temp_dir = setup_temp_repo();
                let cwd = temp_dir.path();

                // Pre-create tasks, save last ID for show
                let mut task_id = String::new();
                for i in 0..num_tasks {
                    task_id = create_task(
                        cwd,
                        format!("Task {}", i),
                        TaskPriority::P2,
                        None,
                        Utc::now(),
                    )
                    .expect("Failed to create task");
                }

                b.iter(|| {
                    // Simulate task show: read events and find task
                    let events = read_events(black_box(cwd))
                        .expect("Failed to read events");
                    let _task = events
                        .iter()
                        .filter_map(|e| match e {
                            TaskEvent::Created { task_id: id, .. }
                                if id == &task_id => Some(id),
                            _ => None,
                        })
                        .next();
                });
            },
        );
    }

    group.finish();
}

// Example: Adding bench_task_list at scale
fn bench_task_list_scaled(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_list_scaled");

    for num_tasks in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_tasks),
            num_tasks,
            |b, &num_tasks| {
                let temp_dir = setup_temp_repo();
                let cwd = temp_dir.path();

                // Pre-create many tasks
                for i in 0..num_tasks {
                    create_task(
                        cwd,
                        format!("Task {}", i),
                        TaskPriority::P2,
                        None,
                        Utc::now(),
                    )
                    .expect("Failed to create task");
                }

                b.iter(|| {
                    read_events(black_box(cwd)).expect("Failed to read events");
                });
            },
        );
    }

    group.finish();
}
```

### Running Benchmarks

```bash
# Run all task benchmarks
cargo bench --bench task_benchmarks

# Run specific benchmark group
cargo bench --bench task_benchmarks -- create_task
cargo bench --bench task_benchmarks -- read_events

# Save baseline for comparison
cargo bench --bench task_benchmarks -- --save-baseline main

# Compare against baseline
cargo bench --bench task_benchmarks -- --baseline main

# Generate detailed HTML report (opens target/criterion/report/index.html)
cargo bench --bench task_benchmarks
```

### Benchmark Reports

Generate reports with:
- Median, P95, P99 latencies
- Throughput (ops/sec)
- Memory usage
- Comparison with baseline
- Flame graphs for hotspots

---

## Performance Analysis

### Current Bottlenecks (Hypothetical)

Based on typical command structure, likely bottlenecks:

1. **JJ Repository Operations**
   - Loading repo (`Workspace::load()`)
   - Creating transactions
   - Committing transactions
   - Reading from aiki/tasks branch

2. **Event Serialization**
   - YAML serialization/deserialization
   - String formatting
   - File I/O

3. **Task ID Generation**
   - Change ID calculation
   - Random ID generation if applicable

4. **Task Queries**
   - Reading all events from branch
   - Filtering and aggregation
   - Building task state

### Profiling Strategy

```bash
# Profile with flamegraph (requires: cargo install flamegraph)
cargo flamegraph --bench tasks

# Profile with perf (Linux)
perf record -F 99 -g cargo bench --bench tasks
perf report

# Profile with Instruments (macOS)
cargo build --bench tasks --release
# Then run Instruments.app on target/release/deps/tasks-*
```

---

## Optimization Plan: task add

### Phase 1: Measurement (1 day)

**Goal:** Establish baseline performance

**Tasks:**
1. Implement `bench_task_add` benchmark
2. Run with 0, 10, 100, 1000 task repositories
3. Generate flame graph
4. Identify top 3 bottlenecks

**Acceptance:**
- Baseline numbers documented
- Flame graph showing hotspots
- Clear understanding of bottlenecks

### Phase 2: Low-Hanging Fruit (2 days)

**Common optimizations:**

1. **Reduce JJ Operations**
   ```rust
   // Before: Load repo for every operation
   pub fn add(opts: AddOptions) -> Result<()> {
       let repo = load_repo()?;  // Expensive
       let tx = repo.start_transaction();
       // ...
   }
   
   // After: Reuse repo if in same process
   pub fn add_batch(opts: Vec<AddOptions>) -> Result<()> {
       let repo = load_repo()?;  // Once
       for opt in opts {
           let tx = repo.start_transaction();
           // ...
       }
   }
   ```

2. **Optimize Task ID Generation**
   ```rust
   // Before: Use full change ID calculation
   let task_id = generate_full_change_id()?;  // Slow
   
   // After: Use optimized ID generation
   let task_id = generate_task_id_fast()?;  // Fast
   ```

3. **Lazy Loading**
   ```rust
   // Before: Load all tasks to find next ID
   let all_tasks = load_all_tasks()?;
   let next_id = calculate_next_id(&all_tasks)?;
   
   // After: Use counter or timestamp-based ID
   let next_id = generate_id_from_timestamp()?;
   ```

4. **Batch Event Writing**
   ```rust
   // Before: One transaction per event
   write_event(event1)?;
   write_event(event2)?;
   
   // After: Single transaction for all events
   write_events(&[event1, event2])?;
   ```

**Target:** 30-50% reduction in P95 latency

### Phase 3: Deep Optimizations (3 days)

If Phase 2 doesn't hit targets:

1. **Event Caching**
   - Cache recently added tasks in memory
   - Invalidate on writes from other processes
   - Use file system timestamps for invalidation

2. **Parallel Processing**
   - Parse events in parallel
   - Use rayon for event aggregation

3. **Custom Serialization**
   - Replace YAML with faster format (MessagePack, bincode)
   - Keep YAML for git diffs, use binary for performance

4. **Incremental State**
   - Maintain task index file
   - Update incrementally on add/close
   - Rebuild on corruption detection

**Target:** Hit <50ms P95 goal

---

## Optimization Plan: task close

### Phase 1: Measurement (1 day)

**Goal:** Establish baseline performance

**Tasks:**
1. Implement `bench_task_close` benchmark
2. Profile with different comment sizes
3. Test with `--summary` flag
4. Identify bottlenecks

**Acceptance:**
- Baseline numbers for close with/without summary
- Understanding of event writing overhead

### Phase 2: Optimizations (2 days)

**Optimization strategies:**

1. **Atomic Multi-Event Writes**
   ```rust
   // Before: Separate transactions
   emit_event(Closed { summary, ... })?;  // Transaction 1

   // After: Single optimized transaction
   let events = vec![Closed { summary, ... }];
   emit_events_atomic(&events)?;  // Single transaction
   ```

2. **Reduce Transaction Overhead**
   ```rust
   // Minimize work inside transaction
   pub fn close(opts: CloseOptions) -> Result<()> {
       // Pre-compute everything outside transaction
       let task = load_task(&opts.task_id)?;
       let timestamp = Utc::now();
       let events = build_close_events(&opts, timestamp)?;
       
       // Single fast transaction
       let repo = load_repo()?;
       let mut tx = repo.start_transaction();
       write_events_to_branch(&mut tx, &events)?;
       tx.commit("aiki: close task")?;
       
       Ok(())
   }
   ```

3. **Skip Unnecessary Validation**
   ```rust
   // For well-formed close operations, skip heavy validation
   pub fn close_fast(opts: CloseOptions) -> Result<()> {
       // Assume task exists and is open
       // Validate only critical invariants
       // Write events directly
   }
   ```

**Target:** <50ms P95, even with `--summary`

### Phase 3: Batch Operations (2 days)

Support closing multiple tasks efficiently:

```rust
// CLI support
aiki task close task1 task2 task3 --summary "Batch close"

// Implementation
pub fn close_batch(task_ids: Vec<String>, opts: CloseOptions) -> Result<()> {
    let repo = load_repo()?;  // Once
    let mut tx = repo.start_transaction();
    
    for task_id in task_ids {
        let events = build_close_events(task_id, &opts)?;
        write_events_to_tx(&mut tx, &events)?;
    }
    
    tx.commit("aiki: close multiple tasks")?;
    Ok(())
}
```

**Target:** Batch close of 10 tasks < 200ms total

---

## Implementation

### Phase 1: Benchmarking Infrastructure (1 week)

**Week 1:**
- Extend existing `cli/benches/task_benchmarks.rs` with missing benchmarks
- Add bench_task_show for single task lookup
- Add bench_task_list_scaled for 100-1000 task scenarios
- Document baseline performance
- Generate initial flame graphs

**Deliverables:**
- Extended `cli/benches/task_benchmarks.rs` with show/list benchmarks
- Performance baseline document
- CI integration for regression detection

### Phase 2: task add Optimization (1 week)

**Week 2:**
- Implement low-hanging fruit optimizations
- Re-benchmark and compare
- If needed, implement deep optimizations
- Update documentation

**Deliverables:**
- Optimized `task add` implementation
- Performance improvements documented
- Regression tests for performance

### Phase 3: task close Optimization (1 week)

**Week 3:**
- Optimize close operations
- Implement atomic multi-event writes
- Add batch close support (optional)
- Re-benchmark

**Deliverables:**
- Optimized `task close` implementation
- Support for `--summary` without performance penalty
- Batch close API (if time permits)

### Phase 4: Monitoring and Documentation (2 days)

**Week 4:**
- Set up performance monitoring in CI
- Document optimization techniques
- Create performance troubleshooting guide
- Update CLAUDE.md with performance guidelines

**Deliverables:**
- CI performance regression checks
- Performance guide in docs
- Updated development guidelines

**Total Timeline:** 3.5 weeks

---

## Monitoring

### CI Integration

Add performance checks to CI:

```yaml
# .github/workflows/performance.yml
name: Performance

on:
  pull_request:
    paths:
      - 'cli/src/tasks/**'
      - 'cli/benches/**'

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Run benchmarks
        run: cargo bench --bench task_benchmarks -- --save-baseline pr

      - name: Compare with main
        run: |
          git fetch origin main
          git checkout origin/main
          cargo bench --bench task_benchmarks -- --save-baseline main
          git checkout -

      - name: Check for regressions
        run: |
          cargo bench --bench task_benchmarks -- --baseline main | tee results.txt
          # Fail if any benchmark is >10% slower
          if grep -q "change:.*>.*+10%" results.txt; then
            echo "Performance regression detected!"
            exit 1
          fi
```

### Performance Dashboard

Track metrics over time:
- P50, P95, P99 latencies for each command
- Operations per second
- Memory usage
- Regression alerts

### Continuous Profiling

Integrate with profiling tools:
- Flamegraph generation on main branch
- Periodic perf analysis
- Memory profiling with valgrind/heaptrack

---

## Success Criteria

### Must Have
- ✅ Benchmark suite with task add, close, list, show
- ✅ Baseline performance documented
- ✅ `aiki task add` < 100ms P95 (initial target)
- ✅ `aiki task close` < 100ms P95 (initial target)
- ✅ `aiki task list` < 200ms P95 with 1000+ tasks (initial target)
- ✅ `aiki task show` < 100ms P95 (initial target)
- ✅ CI regression detection

### Should Have
- ✅ `aiki task add` < 50ms P95 (stretch goal)
- ✅ `aiki task close` < 50ms P95 (stretch goal)
- ✅ `aiki task list` < 100ms P95 with 1000+ tasks (stretch goal)
- ✅ `aiki task show` < 50ms P95 (stretch goal)
- ✅ Batch operations support
- ✅ Performance guide documentation
- ✅ Flame graph generation

### Nice to Have
- ✅ Continuous profiling dashboard
- ✅ Memory usage optimization
- ✅ Parallel event processing
- ✅ Performance monitoring in production

---

## Performance Guidelines

### For Developers

When adding new task features:

1. **Measure first** - Run benchmarks before/after changes
2. **Avoid N+1 queries** - Load data in bulk when possible
3. **Reuse transactions** - Batch operations in single transaction
4. **Profile regularly** - Generate flame graphs for complex changes
5. **Test at scale** - Benchmark with 1000+ task repository

### Code Review Checklist

- [ ] Benchmarks run and compared with baseline
- [ ] No new N+1 query patterns
- [ ] Transaction scope minimized
- [ ] Event serialization efficient
- [ ] No regressions > 10% in P95 latency

---

## References

- Milestone 1.4: Task System
- Rust Performance Book: https://nnethercote.github.io/perf-book/
- Criterion.rs: https://bheisler.github.io/criterion.rs/book/
- Flamegraph: https://github.com/flamegraph-rs/flamegraph
