# Plan: Short/Prefix Task ID Resolution

## Context

Task IDs are 32-character strings of k-z letters (e.g., `mvslrspmoynoxyyywqyutmovxpvztkls`). Currently all commands require the full ID. This makes the CLI painful to use interactively. JJ supports shortest-unique-prefix resolution for change IDs; we should too.

## Approach

Add prefix matching so any unambiguous prefix resolves to the full task ID. `mvslrsp` resolves to `mvslrspmoynoxyyywqyutmovxpvztkls`. Subtask prefixes work too: `mvslrsp.1` resolves to `mvslrspmoynoxyyywqyutmovxpvztkls.1`.

## Changes

### 1. New error variant — `cli/src/error.rs`

Add `AmbiguousTaskId` and `SubtaskNotFound` next to `TaskNotFound`:

```rust
#[error("Ambiguous task ID prefix '{prefix}' — matches {count} tasks:\n{matches}")]
AmbiguousTaskId { prefix: String, count: usize, matches: String }

#[error("Task '{root}' has no subtask '.{subtask}'")]
SubtaskNotFound { root: String, subtask: String }

#[error("Task ID prefix '{prefix}' is too short (minimum 3 characters)")]
PrefixTooShort { prefix: String }
```

### 2. New function `is_task_id_prefix` — `cli/src/tasks/id.rs`

Checks if a string *looks like* a task ID prefix (all k-z, optional `.N` suffixes). Separate from `is_task_id` which requires exactly 32 chars.

```rust
pub fn is_task_id_prefix(input: &str) -> bool
```

Minimum prefix length of 3 characters (before the dot). This avoids noisy ambiguous errors from typos — with a k-z alphabet, 1-2 char prefixes are almost always ambiguous.

Returns true for: `"mvslrsp"`, `"mvslrsp.1"`, `"kvx"`, full 32-char IDs.
Returns false for: `"Fix bug"`, `"implement-login"`, `"abc"` (outside k-z range), `""`, `"k"` (too short), `"kv"` (too short).

### 3. Update `find_task` to handle resolution — `cli/src/tasks/manager.rs`

**Design principle:** Encapsulate resolution logic inside `find_task` so all call sites automatically benefit. Return `Result` so ambiguity errors propagate naturally with `?` — no silent swallowing.

**Critical:** `find_task` returns a `&Task` whose `.id` field is the canonical full ID. Call sites that need the ID string for downstream operations (writing events, building revsets, generating subtask IDs) **must use `task.id`**, not the raw user input. See §5 for the full audit.

```rust
/// Find a task by ID or prefix
///
/// Accepts full IDs or unique prefixes. Returns the task or an error
/// (TaskNotFound, AmbiguousTaskId, SubtaskNotFound).
pub fn find_task<'a>(tasks: &'a HashMap<String, Task>, id_or_prefix: &str) -> Result<&'a Task> {
    // Fast path: exact match
    if let Some(task) = tasks.get(id_or_prefix) {
        return Ok(task);
    }

    // Try prefix resolution
    let full_id = resolve_task_id_internal(tasks, id_or_prefix)?;
    tasks.get(&full_id).ok_or_else(|| AikiError::TaskNotFound(full_id))
}
```

**Migration:** Existing call sites that do `find_task(&tasks, id).ok_or_else(|| TaskNotFound(...))` simplify to `find_task(&tasks, id)?`. Call sites that pattern-match on `Option` switch to `Result` matching. This is a bigger signature change but gives strictly better UX — every call site now surfaces the real error (ambiguous, not found, subtask missing) instead of a generic "task not found".

**Add internal resolution helper:**

```rust
/// Internal helper for prefix resolution
/// Returns full ID or error. Use this when you need error details (ambiguous, not found).
fn resolve_task_id_internal(tasks: &HashMap<String, Task>, prefix: &str) -> Result<String> {
    // Fast path: exact match
    if tasks.contains_key(prefix) {
        return Ok(prefix.to_string());
    }
    
    // Subtask prefix: "mvslrsp.1"
    if let Some((root_prefix, suffix)) = prefix.split_once('.') {
        let full_root = resolve_root_prefix(tasks, root_prefix)?;
        let full_id = format!("{}.{}", full_root, suffix);
        
        // Verify subtask exists
        if tasks.contains_key(&full_id) {
            Ok(full_id)
        } else {
            Err(AikiError::SubtaskNotFound {
                root: full_root,
                subtask: suffix.to_string(),
            })
        }
    } else {
        // Root prefix: "mvslrsp"
        resolve_root_prefix(tasks, prefix)
    }
}

/// Resolve a root task prefix (no dots)
fn resolve_root_prefix(tasks: &HashMap<String, Task>, prefix: &str) -> Result<String> {
    // Enforce minimum prefix length (3 chars) to avoid noisy ambiguous errors
    if prefix.len() < 3 {
        return Err(AikiError::PrefixTooShort { prefix: prefix.to_string() });
    }

    // Collect unique root IDs matching the prefix
    let mut matches: Vec<String> = tasks
        .keys()
        .filter_map(|id| {
            // Extract root part (before first '.')
            let root = id.split('.').next().unwrap();
            if root.starts_with(prefix) {
                Some(root.to_string())
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>() // deduplicate
        .into_iter()
        .collect();
    
    matches.sort();
    
    match matches.len() {
        0 => Err(AikiError::TaskNotFound(prefix.to_string())),
        1 => Ok(matches[0].clone()),
        _ => {
            // Build helpful error message with task names
            let match_list = matches
                .iter()
                .filter_map(|id| tasks.get(id).map(|t| format!("  {} — {}", &id[..id.len().min(8)], t.name)))
                .collect::<Vec<_>>()
                .join("\n");
            
            Err(AikiError::AmbiguousTaskId {
                prefix: prefix.to_string(),
                count: matches.len(),
                matches: match_list,
            })
        }
    }
}
```

**Public API for ID-only resolution (when you need the full ID string, not the task):**

```rust
/// Resolve a task ID prefix to a full ID
///
/// Use this when you need the resolved ID string (e.g., for batch validation
/// or before the task map is available). Most call sites should use `find_task`.
pub fn resolve_task_id(tasks: &HashMap<String, Task>, prefix: &str) -> Result<String> {
    resolve_task_id_internal(tasks, prefix)
}
```

**Benefits:**
- All call sites automatically get correct error messages with `?`
- No silent swallowing of ambiguity — `AmbiguousTaskId` propagates naturally
- Single source of truth for resolution logic
- Natural API: `find_task(&tasks, "mvslrsp")?` just works

### 4. Exports — `cli/src/tasks/mod.rs`

Add `is_task_id_prefix` and `resolve_task_id` to pub exports. `find_task` signature changes from `Option<&Task>` to `Result<&Task>`.

### 5. Update call sites — full audit

Since `find_task` now returns `Result`, most call sites simplify — the `.ok_or_else(|| TaskNotFound(...))` wrappers are replaced by `?`.

**Key principle for write paths:** Any call site that uses the user-provided ID string for downstream operations (writing events, building revsets, generating subtask IDs) **must switch to the resolved canonical ID** from `task.id` or `resolve_task_id`. Using the raw prefix would produce wrong IDs, broken events, or panics.

#### 5a. Read-only call sites (simplify to `?`)

These only read the `&Task` returned by `find_task`. The `Option→Result` change is sufficient:

```rust
// Before:
let task = find_task(&tasks, id).ok_or_else(|| AikiError::TaskNotFound(id.to_string()))?;

// After:
let task = find_task(&tasks, id)?;
```

#### 5b. Write paths that continue using the raw ID — **must resolve first**

These call sites validate with `find_task` but then use the raw user-input ID for subsequent operations. With prefix resolution, the raw ID is a prefix — these would produce wrong IDs/events or panics.

**`run_add` (task.rs ~1063)** — uses `parent_id` for `reopen_if_closed`, `get_next_subtask_number`, `generate_child_id`:
```rust
// Before:
let parent_task = find_task(&tasks, parent_id).ok_or_else(|| ...)?;
reopen_if_closed(cwd, parent_id, ...);  // ← raw prefix!

// After:
let parent_task = find_task(&tasks, parent_id)?;
let parent_id = &parent_task.id;  // ← rebind to canonical ID
reopen_if_closed(cwd, parent_id, ...);
```

**`run_stop` (task.rs ~1550)** — uses raw `id` after validation:
```rust
// Before:
if let Some(task) = find_task(&tasks, &id) { ... }
id  // ← raw prefix used as task_id

// After:
let task = find_task(&tasks, &id)?;
task.id.clone()  // ← canonical ID
```

**`run_show` (task.rs ~2361)** — uses raw `id` for subtask lookups:
```rust
// Before:
if find_task(&tasks, &id).is_none() { return Err(...); }
id  // ← raw prefix

// After:
let task = find_task(&tasks, &id)?;
task.id.clone()  // ← canonical ID
```

**`run_diff` (task.rs ~3051)** — uses raw `id` for `build_task_revset_pattern`:
```rust
// Before:
if find_task(&tasks, &id).is_none() { return Err(...); }
let pattern = build_task_revset_pattern(&id);  // ← raw prefix!

// After:
let task = find_task(&tasks, &id)?;
let pattern = build_task_revset_pattern(&task.id);  // ← canonical ID
```

**`task_run` (runner.rs ~75)** — uses raw `task_id` throughout:
```rust
// Before:
let task = find_task(&tasks, task_id).ok_or_else(|| ...)?;
// continues using task_id for status updates, event writes

// After:
let task = find_task(&tasks, task_id)?;
let task_id = &task.id;  // ← rebind to canonical ID
```

#### 5c. Batch resolution paths (need `resolve_task_id`)

- `run_start` — batch ID validation before starting
- `run_close` — batch ID validation before closing

These resolve a list of IDs upfront. Use `resolve_task_id` to get canonical IDs before proceeding.

#### 5d. Special case — `run_start` description-vs-ID detection (line ~1261)

Current: `if ids.len() == 1 && !is_task_id(&ids[0])` → quick-start as description

New logic:
```rust
if ids.len() == 1 && !is_task_id(&ids[0]) {
    if is_task_id_prefix(&ids[0]) {
        // Looks like a prefix — try to resolve
        match resolve_task_id(&tasks, &ids[0]) {
            Ok(full_id) => vec![full_id],                    // found it
            Err(AikiError::TaskNotFound(_)) => /* quick-start as description */,
            Err(e) => return Err(e),                         // ambiguous → error
        }
    } else {
        // Not even a valid prefix → definitely a description, quick-start
    }
}
```

### 6. Update `is_task_id` gates in other commands

Several commands branch on `is_task_id(input)` to decide whether the argument is a task ID or a file path. Since `is_task_id` requires exactly 32 chars, prefixes fall through to the file-path branch. These need to also check `is_task_id_prefix`.

**`build.rs` (~line 83):**
```rust
// Before:
if is_task_id(&target) {
    run_build_plan(...)
} else {
    run_build_spec(...)
}

// After:
if is_task_id(&target) || is_task_id_prefix(&target) {
    run_build_plan(...)  // resolve_task_id happens inside
} else {
    run_build_spec(...)
}
```

**`plan.rs` (~line 216):**
```rust
// Before:
let plan = if is_task_id(arg) {
    find_task(&tasks, arg).ok_or_else(|| ...)?
} else {
    find_plan_for_spec(&tasks, arg).ok_or_else(|| ...)?
};

// After:
let plan = if is_task_id(arg) || is_task_id_prefix(arg) {
    find_task(&tasks, arg)?  // find_task handles prefix resolution
} else {
    find_plan_for_spec(&tasks, arg).ok_or_else(|| ...)?
};
```

### 7. Tests

**In `id.rs`**: Tests for `is_task_id_prefix` — valid prefixes, invalid inputs, subtask prefixes.

**In `manager.rs`**: Tests for `find_task` with prefixes:
- Exact match (full ID) — existing behavior
- Unique prefix match — new behavior
- Ambiguous prefix → `AmbiguousTaskId` error with match list
- Not found → `TaskNotFound` error
- Subtask prefix resolution (`mvslrsp.1`)
- Subtask not found → `SubtaskNotFound` error (root resolved, subtask `.99` doesn't exist)
- Deduplication (parent + subtasks count as 1 root match)
- Prefix too short (1-2 chars) → treated as non-prefix, not resolved

**In `manager.rs`**: Tests for `resolve_task_id`:
- Same cases as `find_task` but returns `Result<String>` (the full ID)

**In `error.rs`**: Tests for `AmbiguousTaskId` and `SubtaskNotFound` display formatting.

## Verification

1. `cargo test --manifest-path cli/Cargo.toml --lib` — all unit tests pass
2. `cargo build --manifest-path cli/Cargo.toml` — clean build
3. Manual test: `aiki task show <short-prefix>` resolves correctly
4. Manual test: ambiguous prefix shows helpful error with matching tasks

## Summary

**Key insight:** `find_task` returns `Result` with resolution built in. This means:

- **Most call sites simplify** — replace `.ok_or_else(|| TaskNotFound(...))` with just `?`
- **Ambiguity errors propagate naturally** — no silent swallowing, no fallthrough dance
- **Distinct error types** — `TaskNotFound`, `AmbiguousTaskId`, `SubtaskNotFound` give the user actionable messages
- **Minimum 3-char prefix** — enforced in `resolve_root_prefix`, avoids noisy errors from typos
- **Single source of truth** — all resolution logic lives in one place
- **Write paths use canonical IDs** — all call sites that continue using the ID after `find_task` rebind to `task.id` instead of using raw user input
- **`is_task_id` gates updated** — commands that branch on `is_task_id` (build.rs, plan.rs) also check `is_task_id_prefix` so prefixes route correctly
