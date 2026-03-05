# Add `populated-by` link type for epic → decompose

## Context

The builder detects the decompose sub-stage by scanning all `depends-on` targets of the epic and checking `task_type == "decompose"` — a type-based filter on a generic link. The orchestrator sub-stage uses a dedicated `orchestrates` link, which is cleaner: the link kind itself identifies the relationship, no type check needed.

Adding a `populated-by` link (epic → decompose) replaces the generic `depends-on` link. Same direction as `depends-on` so `blocks_ready: true` correctly blocks the epic until decompose completes. The builder finds decompose tasks via `graph.edges.targets(&epic.id, "populated-by")` — no task_type check needed.

## Changes

### 1. Register `populated-by` link kind
**File:** `cli/src/tasks/graph.rs` (LINK_KINDS array, ~line 91)

Add after `decomposes-plan`:
```rust
LinkKind {
    name: "populated-by",
    max_forward: None,
    max_reverse: None,
    blocks_ready: true,   // Blocks epic until decompose completes (replaces depends-on)
    task_only: true,
},
```

### 2. Write `populated-by` link in decompose.rs, remove `depends-on`
**File:** `cli/src/commands/decompose.rs` (~line 110-111)

Replace the `depends-on` link with `populated-by` (same direction: epic → decompose):
```rust
// Old: write_link_event(cwd, &graph, "depends-on", target_id, &decompose_task_id)?;
// New:
write_link_event(cwd, &graph, "populated-by", target_id, &decompose_task_id)?;
```

### 3. Update builder to use `populated-by` link
**File:** `cli/src/tui/builder.rs`

**`build_sub_stages()`** (~line 368-383): Replace `depends-on` + `task_type` scan with:
```rust
for dec_id in graph.edges.targets(&epic.id, "populated-by") {
    if let Some(dec_task) = graph.tasks.get(dec_id) {
        sub_stages.push(SubStageView { ... });
        break;
    }
}
```

**`has_active_build_task()`** (~line 353-360): Replace `depends-on` scan with `populated-by` targets lookup.

**`find_orchestrator_elapsed()`** (~line 424-430): Replace `depends-on` + `task_type` fallback with `populated-by` targets lookup.

### 4. Update builder tests

Update `decompose_substage_found_via_depends_on` test to use `populated-by` link instead of `depends-on` + task_type.

### 5. Add `populated-by` to `is_blocked()` unblock rules
**File:** `cli/src/tasks/graph.rs` (~line 296)

Add `"populated-by"` to `DONE_ONLY_UNBLOCK` so the epic unblocks only when decompose closes with Done outcome:
```rust
const DONE_ONLY_UNBLOCK: &[&str] = &["blocked-by", "depends-on", "needs-context", "populated-by"];
```

## Verification

```bash
cargo test --lib
```
