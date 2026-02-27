---
status: draft
---

# General Loop Termination

**Date**: 2026-02-24
**Status**: Draft
**Priority**: P2

## Problem

Currently, loop conditions must explicitly check for max iterations:

```yaml
loop:
  until: subtasks.review.approved or data.loop.index1 >= data.max_iterations
```

> **Note on loop counter naming:** The loop system provides `loop.index` (0-based: 0, 1, 2, ...) and `loop.index1` (1-based: 1, 2, 3, ...). The `loop.index1` counter is used for iteration-count comparisons since `max_iterations` is human-friendly (1-based). Some documentation and templates historically use `data.loop.iteration`; this is an alias for `loop.index1` within the template resolver but **does not exist** as a task data key set by the parser. All new code should use `loop.index1`.

This has several issues:

1. **Boilerplate** - Every loop needs to write the same `or data.loop.index1 >= data.max_iterations` check
2. **Error-prone** - Easy to forget the max iteration check, leading to infinite loops
3. **Inconsistent** - Different templates might use different max iteration logic
4. **Harder to override** - If we want to change max iteration behavior globally, we have to update every template

## Proposal

Make max iteration checking the responsibility of the loop runner (spawn evaluation), not the loop condition.

### New Behavior

**Templates write simpler conditions:**
```yaml
loop:
  until: subtasks.review.approved
  max_iterations: 10  # Optional, defaults to system default (e.g., 100)
```

**The spawn evaluator automatically:**
1. Checks if `data.loop.index1 >= max_iterations` BEFORE evaluating the `until` condition
2. If max iterations reached, treats it as if `until` evaluated to true (terminates loop)
3. If max iterations not reached, evaluates the `until` condition normally

### Implementation

**1. Update LoopConfig type** (`cli/src/tasks/templates/types.rs`):
```rust
pub struct LoopConfig {
    /// Rhai expression — loop terminates when this evaluates to true
    pub until: String,
    /// Maximum iterations (default: 100)
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    /// Additional data to pass to each iteration
    #[serde(default)]
    pub data: HashMap<String, serde_yaml::Value>,
}

fn default_max_iterations() -> usize {
    100
}
```

**2. Update loop desugaring** (`cli/src/tasks/templates/parser.rs`):

When converting `loop:` to a spawn entry, pass `max_iterations` as metadata:

```rust
let spawn_entry = SpawnEntry {
    when: format!("not ({})", loop_config.until),
    max_iterations: Some(loop_config.max_iterations),
    task: Some(SpawnTaskConfig {
        template: "self".to_string(),
        autorun: true,
        data: spawn_data,
        // ...
    }),
    // ...
};
```

**3. Update SpawnEntry type** (`cli/src/tasks/templates/spawn_config.rs`):
```rust
pub struct SpawnEntry {
    pub when: String,
    pub max_iterations: Option<usize>,  // New field
    pub task: Option<SpawnTaskConfig>,
    pub subtask: Option<SpawnTaskConfig>,
}
```

**4. Update spawn evaluator** (`cli/src/tasks/spawner.rs`):

```rust
pub fn evaluate_spawns(
    task: &Task,
    graph: &TaskGraph,
    spawns_config: &[SpawnEntry],
) -> Vec<SpawnAction> {
    // ... existing code ...

    for (index, entry) in spawns_config.iter().enumerate() {
        // NEW: Check max iterations FIRST (if specified)
        if let Some(max_iters) = entry.max_iterations {
            let current_index1 = task
                .data
                .get("loop.index1")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1);

            if current_index1 >= max_iters {
                // Max iterations reached - log warning, set flag, skip spawn
                warn!("Loop terminated: max iterations ({}) reached for spawn entry {}", max_iters, index);
                // Set loop.max_reached on the task so downstream templates can react
                // (actual mutation happens via the task close path)
                continue;
            }
        }

        // Evaluate the when condition (existing logic)
        let mut eval_scope = scope.clone();
        let condition_result = evaluator.evaluate(&entry.when, &mut eval_scope);
        // ... rest of existing logic ...
    }
}
```

### Migration

**Backward compatibility:**
- Existing loops with `or data.loop.index1 >= data.max_iterations` continue to work
- New loops can omit the max iteration check from `until` and rely on `max_iterations` field
- If both are present, the spawn evaluator's check happens first (takes precedence)
- Templates referencing `data.loop.iteration` in body text (not in `until` conditions) continue to work via the template resolver's `loop.iteration` alias, but should be migrated to `loop.index1`

**Recommended migration:**
1. Update core templates (aiki/fix, aiki/review) to use new syntax
2. Update loop-flags.md spec to document new approach
3. Keep old approach supported indefinitely (no breaking changes)

## Benefits

1. **Simpler templates** - Loop conditions express only the actual termination logic
2. **Safer** - Can't forget max iteration check (it's always enforced by the system)
3. **More flexible** - Can easily override max iterations per-loop without touching condition logic
4. **Consistent** - All loops get the same max iteration behavior by default

## Example: Fix Loop

**Before:**
```yaml
loop:
  until: subtasks.review_this_fix.approved or data.loop.index1 >= data.max_iterations
  data:
    latest_review: "{{subtasks.review_this_fix.id}}"
```

**After:**
```yaml
loop:
  until: subtasks.review_this_fix.approved
  max_iterations: 10
  data:
    latest_review: "{{subtasks.review_this_fix.id}}"
```

## Test Strategy

### Parser tests (`cli/src/tasks/templates/parser.rs`)

1. **`max_iterations` desugaring** — Verify that a `loop:` block with `max_iterations: 5` produces a `SpawnEntry` with `max_iterations: Some(5)` and the `when` condition only contains the user's `until` expression (no iteration check appended).
2. **Default `max_iterations`** — A `loop:` block without `max_iterations` should desugar to `max_iterations: Some(100)` (the system default).
3. **Loop metadata unchanged** — Existing `loop.index`, `loop.index1`, and `loop.first` metadata should still be emitted in `spawn_data` alongside the new `max_iterations` field.

### Spawn config tests (`cli/src/tasks/templates/spawn_config.rs`)

4. **Serde round-trip** — A YAML `SpawnEntry` with `max_iterations: 3` deserializes to `max_iterations: Some(3)` and one without the field deserializes to `None`.

### Spawn evaluator tests (`cli/src/tasks/spawner.rs`)

5. **Max iterations terminates loop** — Given a task with `data["loop.index1"] = "5"` and a spawn entry with `max_iterations: Some(5)`, the entry should be skipped (no spawn action emitted).
6. **Below max iterations continues** — Given `data["loop.index1"] = "4"` and `max_iterations: Some(5)`, the `when` condition should be evaluated normally.
7. **Off-by-one boundary** — Test the exact boundary: `loop.index1 == max_iterations` should terminate (>=), `loop.index1 == max_iterations - 1` should not.
8. **Missing loop.index1 defaults to 1** — If `data["loop.index1"]` is absent, the evaluator should default to `1` (first iteration), not skip.
9. **`max_iterations: None` skips check** — When `max_iterations` is `None` (no field), the evaluator should evaluate `when` normally with no iteration cap.
10. **Precedence: max_iterations before `when`** — If `max_iterations` is reached, the spawn should be skipped even if the `when` condition would evaluate to true.

### Backward compatibility tests

11. **Old-style `until` with inline iteration check** — A template using `until: subtasks.review.approved or data.loop.index1 >= 10` (without `max_iterations`) should continue to work exactly as before.
12. **Both old and new present** — A template with both an inline check in `until` and a `max_iterations` field should terminate at whichever limit is reached first.

## Decisions (formerly Open Questions)

1. **Default max_iterations value** — `100`. Not globally configurable in v1; templates can override per-loop. Global config can be added later if needed.
2. **Infinite loops** — `max_iterations: 0` means "no limit" (the system-level check is skipped). This is an explicit opt-in to unbounded loops. Templates should document why they need it.
3. **Max-iteration termination UX** — **Emit a warning, do not silently swallow.** When max iterations is reached:
   - The spawner logs a warning: `"Loop terminated: max iterations ({max_iterations}) reached for spawn entry {index}"`
   - The task closes normally (not as an error) — the loop simply stops iterating
   - The task's close summary should include a note that max iterations was reached (e.g., via a `loop.max_reached` data flag set to `true`)
   - Rationale: Silent termination hides potential issues (loop that should have converged but didn't). A warning surfaces this to the operator without failing the task. The `loop.max_reached` flag lets downstream templates react to it (e.g., a fix loop that hit max iterations can surface this in its summary).

## Acceptance Criteria

1. Templates with `loop:` blocks can specify `max_iterations: N` (or omit for default of 100)
2. The spawn evaluator checks `loop.index1 >= max_iterations` before evaluating the `when` condition
3. When max iterations is reached, a warning is logged and `loop.max_reached` is set to `true` in task data
4. When max iterations is NOT reached, `loop.max_reached` is absent or `false`
5. `max_iterations: 0` disables the system-level iteration cap
6. Existing templates with inline iteration checks in `until` continue to work without changes
7. All 12 test cases from the Test Strategy section pass
8. Core templates (`aiki/fix`, `aiki/review`) are updated to use `max_iterations` and drop inline checks

## Files Changed

- `cli/src/tasks/templates/types.rs` - Add `max_iterations` to `LoopConfig`
- `cli/src/tasks/templates/spawn_config.rs` - Add `max_iterations` to `SpawnEntry`
- `cli/src/tasks/templates/parser.rs` - Pass `max_iterations` when desugaring loops
- `cli/src/tasks/spawner.rs` - Check `max_iterations` before evaluating `when` condition
- `ops/now/loop-flags.md` - Update spec to document new approach
