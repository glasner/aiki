# Task Write Locking

**Status**: Future idea
**Related**: [task-branch-fan-out.md](../now/isolation/task-branch-fan-out.md)

---

## Context

The task-branch-fan-out fix chains task events linearly and advances the `aiki/tasks` bookmark after each write. With concurrent agents, two writes can race — both fork from the same bookmark position, creating a temporary divergent bookmark. JJ handles this gracefully and the fork resolves naturally on the next write, but it's not ideal.

## Idea

Reuse the existing `acquire_absorb_lock()` pattern from `isolation.rs` to serialize task writes. This would guarantee strictly linear chains with no temporary forks.

```rust
// Pseudocode
let _lock = acquire_task_write_lock(cwd)?;
write_event(cwd, &event)?;
// lock drops here, next writer proceeds
```

## Why not now

- Task writes are infrequent (human-driven, not high-throughput)
- JJ's divergent bookmark handling makes the race harmless in practice
- The fan-out fix already drops head count from ~31K to ~1; temporary forks add 1-2 heads briefly
- Adding locking increases complexity and latency for marginal benefit

## When to revisit

- If automated pipelines start writing many task events concurrently (e.g., batch agent spawning)
- If divergent bookmark resolution causes read-path issues (e.g., `read_events()` seeing duplicates)
- If `aiki doctor` head count checks start flagging persistent forks above threshold
