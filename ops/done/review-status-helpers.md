# Review status: structured data fields for review outcomes

## Context

The spawn scope currently promotes `data.approved` to a top-level `approved` variable as a convenience shorthand. This is inconsistent with how all other data fields are accessed (`data.*`). We want to remove this magic so `approved` must always be accessed as `data.approved`.

Additionally, review close should auto-set structured data fields so spawn conditions can express review outcomes cleanly. The existing `data.issues_found` count field is renamed to `data.issue_count` for clarity.

### Review data fields after this change

| Field | Type | Value | Use case |
|-------|------|-------|----------|
| `data.approved` | bool | `issue_count == 0` | "is it clean?" / "does it need fixing?" (`not data.approved`) |
| `data.issue_count` | int | N | "how many issues?" |

Both are auto-set when a review task closes.

### Spawn condition examples

```yaml
# Clean review — no fix needed
when: "data.approved"

# Needs fix
when: "not data.approved"

# Threshold-based
when: "data.issue_count > 3"

# Subtask access
when: "subtasks.review.data.approved"
when: "not subtasks.review.data.approved"
```

## Changes

### 1. Remove top-level `approved` from spawn scope

**File:** `cli/src/tasks/spawner.rs` (lines 50-56)

Remove these lines from `build_spawn_scope`:
```rust
let approved = task
    .data
    .get("approved")
    .map(|v| v == "true")
    .unwrap_or(false);
scope.push("approved", approved);
```

`data.approved` remains accessible via the existing `data` map (lines 71-92).

### 2. Remove `approved` from subtask child maps

**File:** `cli/src/tasks/spawner.rs` (lines 109-114)

Remove these lines from the subtask map builder:
```rust
let child_approved = child
    .data
    .get("approved")
    .map(|v| v == "true")
    .unwrap_or(false);
child_map.insert("approved".into(), Dynamic::from(child_approved));
```

`subtasks.review.data.approved` already works via the child data map (line 119-125).

### 3. Rename `data.issues_found` → `data.issue_count` and auto-set `data.approved`

**File:** `cli/src/commands/task.rs`

In **both** close codepaths (around lines 2271 and 2781), replace the current `issues_found` insert:

```rust
m.insert("issue_count".to_string(), issue_count.to_string());
m.insert("approved".to_string(), (issue_count == 0).to_string());
```

### 4. Update consumers of old `data.issues_found` (count)

These read the old count field and need to switch to `data.issue_count`:

**File:** `cli/src/commands/review.rs`
- Line 784: `review.data.get("issues_found")` → `review.data.get("issue_count")`
- Line 860: `task.data.contains_key("issues_found")` → `task.data.contains_key("issue_count")`

**File:** `cli/src/commands/fix.rs`
- Line 272: `review_task.data.get("issues_found")` → `review_task.data.get("issue_count")`
- Line 273: Update comment
- Line 281: Update comment
- Line 292: `review_task.data.contains_key("issues_found")` → `review_task.data.contains_key("issue_count")`

### 5. Update template

**File:** `.aiki/templates/aiki/fix/quality.md`

- Line 4: `not subtasks.review.approved` → `not subtasks.review.data.approved`
- Line 37: `parent.subtasks.review.approved` → `parent.subtasks.review.data.approved`

### 6. Update docs

**File:** `cli/docs/tasks/templates.md`
- Line 47: Remove `approved` row from scope table
- Line 55: Remove `approved` from subtask map description (`status`, `outcome`, `data`, `priority`)
- Lines 63, 84, 282, 296-298: `not approved` → `not data.approved`
- Line 87: `data.issues_found > 3` → `data.issue_count > 3`
- Line 91, 118: `data.issues_found` → `data.issue_count`
- Line 301: `data.issues_found > 3` → `data.issue_count > 3`

**File:** `EXPRESSIONS.md`
- Lines 12, 37-38, 49: `approved` → `data.approved` in examples

### 7. Update tests

**`cli/src/tasks/spawner.rs` tests:**
- `test_evaluate_simple_not_approved` (line 467): `"not approved"` → `"not data.approved"`
- `test_evaluate_approved_no_spawn` (line 493): same
- `test_subtask_precedence` (line 547): same
- Line 518: `"issues_found"` → `"issue_count"`
- Line 522: `"data.issues_found > 3"` → `"data.issue_count > 3"`
- Line 583: `"issues_found"` → `"issue_count"`
- Line 589: `"data.issues_found"` → `"data.issue_count"`
- `test_outcome_condition` subtask spawn (line 870): `"not subtasks.review.approved"` → `"not subtasks.review.data.approved"`

**`cli/tests/test_spawn_flow.rs`:**
- Update `when` conditions from `"not approved"` to `"not data.approved"`

**`cli/src/tasks/templates/parser.rs` tests:**
- Update `when` conditions from `"not approved"` to `"not data.approved"`
- Update `until` conditions from `"approved"` to `"data.approved"`

**`cli/src/tasks/templates/spawn_config.rs` tests:**
- Line 100, 131: `data.issues_found` → `data.issue_count`
- Line 113: `"data.issues_found"` → `"data.issue_count"`
- Line 127, 136: `data.issues_found` → `data.issue_count`
- `when` conditions: `"not approved"` → `"not data.approved"`

### Note: `doctor.rs` is unaffected

`doctor.rs` uses `issues_found` as a local variable for diagnostic counting — completely unrelated to the review data field. No changes needed.

## Verification

1. `cargo test` — all tests pass
2. `cargo test spawn` — spawn-specific tests pass
3. `cargo test -p aiki-cli --test test_spawn_flow` — integration tests pass
