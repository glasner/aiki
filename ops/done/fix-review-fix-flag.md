# Fix `aiki review --fix` (broken after loop refactor)

## Problem

`aiki review --fix` fails with:

```
Error: Template processing failed: Template 'aiki/fix/loop' not found
  Expected: .aiki/templates/aiki/fix/loop.md
```

**Root cause:** The `implement-loop-refactor` plan (step 11) deleted `aiki/fix/loop.md` because the fix pipeline moved to Rust (`fix.rs`). But the review template (`aiki/review.md` line 114) still references it:

```
{% subtask aiki/fix/loop needs-context:subtasks.record-issues if data.options.fix %}
```

The refactor plan's step 13 ("Remove --start from review.rs") didn't include removing this template reference.

## Design

The old flow was template-driven: `aiki/review.md` conditionally spawned `aiki/fix/loop` as a subtask. The new flow is Rust-driven, consistent with how `build` orchestrates its pipeline. The CLI command owns the workflow sequencing.

### Why Rust-driven, not template-driven

We considered re-adding a template (`aiki/fix/loop`) or using `spawns:` frontmatter with autorun. The fix pipeline is already a multi-stage Rust loop in `fix.rs` (plan → decompose → loop → review, repeating until approved). Expressing that in template `spawns:` config would require spawns to invoke `aiki fix` as a command — spawns creates tasks from templates, not runs commands. Build already made the same choice: `run_build_plan()` chains `run_loop()` → `run_build_review()` in Rust, and the async path uses `--_continue-async` to re-enter the same Rust code in a background process.

### How `--fix` should work

```
aiki review <task-id> --fix
```

1. Creates review task (same as today)
2. Runs the review to completion
3. After review completes, checks for issues (`data.issue_count > 0`)
4. If issues found → calls `fix::run_fix()` with the review task ID
5. If no issues → outputs "approved" (no fix needed)

### Execution mode interactions

| Mode | Behavior with `--fix` |
|------|-----------------------|
| Blocking (default) | Review runs, then fix pipeline runs inline |
| `--async` | Background process runs review then fix (via `--_continue-async`) |
| `--start` + `--fix` | **Error** — these modes conflict |

#### `--async` design (follows build pattern)

The async path follows the same `--_continue-async` pattern as build:

1. **Parent process:** Creates review task, spawns `aiki review <target> --fix --_continue-async <review-id>`, returns immediately
2. **Background process:** Runs the review to completion, checks for issues, runs fix pipeline if needed

This means `run_review()` needs a `continue_async` code path (like `build.rs:run_continue_async()`), and the clap args need a hidden `--_continue-async` flag.

#### `--start` + `--fix` is an error

`--start` means "hand off to the calling agent to perform the review manually." `--fix` means "auto-fix after review." These conflict — if you're reviewing manually, you control what happens next. Error message: `"--fix and --start cannot be used together. Use --fix with blocking or --async mode."`

## Changes

### 1. Remove broken template reference

**File:** `.aiki/templates/aiki/review.md` (line 114)

Delete:
```
{% subtask aiki/fix/loop needs-context:subtasks.record-issues if data.options.fix %}
```

### 2. Make `run_fix` public

**File:** `cli/src/commands/fix.rs`

Change `fn run_fix(...)` from private to `pub fn run_fix(...)` so `review.rs` can call it.

### 3. Add `--start` + `--fix` validation

**File:** `cli/src/commands/review.rs`, in `run_review()` before creating the review

```rust
if fix && start {
    return Err(AikiError::InvalidArgument(
        "--fix and --start cannot be used together. Use --fix with blocking or --async mode.".to_string(),
    ));
}
```

### 4. Call fix pipeline from `run_review()` blocking path

**File:** `cli/src/commands/review.rs`, in `run_review()` around line 747-762

After the review completes in the blocking path:

```rust
} else {
    // Run to completion (default)
    let options = TaskRunOptions::new();
    task_run(cwd, &review_id, options)?;

    // Check for issues
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let has_issues = find_task(&graph.tasks, &review_id)
        .map(|t| t.data.get("issue_count")
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0) > 0)
        .unwrap_or(false);

    if fix && has_issues {
        // Run fix pipeline on the completed review
        super::fix::run_fix(
            cwd,
            &review_id,
            false,           // not async (inline)
            None,            // no continue-async
            None,            // default plan template
            None,            // default decompose template
            None,            // default loop template
            None,            // default review template (for quality loop)
            agent.clone(),   // pass through agent override
            autorun,
            false,           // not --once (full quality loop)
        )?;
    } else if !output_id {
        output_review_completed(&review_id, scope, has_issues)?;
    }
}
```

**Key behavior:** When `--fix` triggers the fix pipeline, the fix pipeline's own output replaces the review completion output (it will output "Approved" or its own status).

### 5. Add async path with `--_continue-async`

**File:** `cli/src/commands/review.rs`

Add hidden `--_continue-async` arg to `ReviewArgs`:

```rust
/// Internal: continue an async review+fix from a previously created review task
#[arg(long = "_continue-async", hide = true)]
pub continue_async: Option<String>,
```

Add `run_continue_async()` function (modeled on `build.rs:run_continue_async()`):

```rust
/// Background process entry point for async review+fix.
///
/// Called when `--_continue-async` is provided. The parent process created
/// the review task and returned immediately. This function runs the review
/// to completion, then runs the fix pipeline if --fix was set.
fn run_continue_async(cwd: &Path, review_id: &str, fix: bool, agent: Option<String>, autorun: bool) -> Result<()> {
    // Run the review
    let options = TaskRunOptions::new();
    task_run(cwd, review_id, options)?;

    if !fix {
        return Ok(());
    }

    // Check for issues
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let has_issues = find_task(&graph.tasks, review_id)
        .map(|t| t.data.get("issue_count")
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0) > 0)
        .unwrap_or(false);

    if has_issues {
        super::fix::run_fix(
            cwd, review_id,
            false, None, None, None, None, None,
            agent, autorun, false,
        )?;
    }

    Ok(())
}
```

Update the async path in `run_review()`:

```rust
} else if run_async {
    // Create review task first
    // ... (existing create_review call happens above) ...

    // Spawn background process: aiki review --_continue-async <review-id> [--fix] [--agent ...]
    let mut spawn_args: Vec<String> = vec![
        "review".to_string(),
        "--_continue-async".to_string(),
        review_id.clone(),
    ];
    if fix {
        spawn_args.push("--fix".to_string());
    }
    if let Some(ref a) = agent {
        spawn_args.push("--agent".to_string());
        spawn_args.push(a.clone());
    }

    let spawn_args_refs: Vec<&str> = spawn_args.iter().map(|s| s.as_str()).collect();
    async_spawn::spawn_aiki_background(cwd, &spawn_args_refs)?;

    if !output_id {
        output_review_async(&review_id, scope)?;
    }
}
```

### 6. Dispatch `--_continue-async` early in `run()`

**File:** `cli/src/commands/review.rs`, at the top of `run()`

```rust
pub fn run(args: ReviewArgs) -> Result<()> {
    let cwd = env::current_dir()...;

    // Continue-async: background process entry point
    if let Some(ref review_id) = args.continue_async {
        return run_continue_async(&cwd, review_id, args.fix, args.agent, args.autorun);
    }

    // ... rest of existing logic ...
}
```

### 7. Verify tests pass

**File:** `cli/src/tasks/templates/resolver.rs` (line 1808)

The test `has_subtask_refs` expects the commented-out `{% subtask aiki/fix/loop %}` to NOT match. This test still passes after removing the template reference (we're removing the line entirely, not commenting it out).

## Task Graph

```
1. Remove template reference from review.md
2. Make run_fix public in fix.rs
3. Add --start + --fix validation in review.rs
4. Add fix pipeline call in review.rs blocking path
   └── depends-on: 2
5. Add --_continue-async flag and run_continue_async() to review.rs
   └── depends-on: 2
6. Update async path in run_review() to use spawn_aiki_background
   └── depends-on: 5
7. Dispatch --_continue-async early in run()
   └── depends-on: 5
8. Verify tests pass (cargo test)
   └── depends-on: 1, 3, 4, 6, 7
```

## Out of Scope

- Supporting `--start` + `--fix` (currently an error; could revisit with autorun infrastructure)
- Adding new template for inline fix (the Rust pipeline is the right approach)
- Removing `--start` from review.rs entirely (separate task from the original refactor plan)
