# Per-stage template overrides for build and fix

## Problem

Both `aiki build` and `aiki fix` accept `--template` but rename the parameter to `_template_name` and hardcode internal templates. Since these commands are now composed of discrete stages (decompose, loop, plan/fix, review), a single `--template` flag doesn't map to anything meaningful.

## Design

### Remove `--template`, add per-stage flags

Each stage in the pipeline gets its own override flag. Flags that enable a post-pipeline stage (like `--review` and `--fix` on build) double as template overrides using clap's `default_missing_value`:

- `--review` (no value) → enable review with default template `aiki/review`
- `--review=my/review` → enable review with custom template
- (absent) → no review

### Build CLI

```
aiki build ops/plan.md
aiki build ops/plan.md --decompose=my/decompose --loop=my/loop
aiki build ops/plan.md --review
aiki build ops/plan.md --review=my/review
aiki build ops/plan.md --fix
aiki build ops/plan.md --fix=my/plan/fix
```

```rust
/// Custom decompose template (default: aiki/decompose)
#[arg(long)]
pub decompose: Option<String>,

/// Custom loop template (default: aiki/loop)
#[arg(long = "loop")]
pub loop_template: Option<String>,

/// Run review after build, optionally with custom template
#[arg(long, default_missing_value = "aiki/review", num_args = 0..=1)]
pub review: Option<String>,

/// Run review+fix after build, optionally with custom fix plan template (implies --review)
#[arg(long, default_missing_value = "aiki/plan/fix", num_args = 0..=1)]
pub fix: Option<String>,
```

### Fix CLI

```
aiki fix <review-id>
aiki fix <review-id> --plan=my/plan/fix --decompose=my/decompose --loop=my/loop
aiki fix <review-id> --review=my/review
```

```rust
/// Custom plan template (default: aiki/plan/fix)
#[arg(long)]
pub plan: Option<String>,

/// Custom decompose template (default: aiki/decompose)
#[arg(long)]
pub decompose: Option<String>,

/// Custom loop template (default: aiki/loop)
#[arg(long = "loop")]
pub loop_template: Option<String>,

/// Custom review template for quality loop review step
#[arg(long, default_missing_value = "aiki/review", num_args = 0..=1)]
pub review: Option<String>,
```

### Wiring

Each per-stage flag is passed through to the corresponding `create_from_template()` call:

- `--decompose` → `create_decompose_task()` template name
- `--loop` → `LoopOptions` or `create_from_template()` with loop template
- `--plan` (fix only) → `create_plan_fix_task()` template name
- `--review` → `create_review()` template parameter
- `--fix` (build only) → passed to the post-build `run_fix()` call as the plan template

Defaults are applied at the call site: `args.decompose.unwrap_or("aiki/decompose".to_string())`.

### Interaction with `--async`

The async spawner (`--_continue-async` from the async plan) forwards per-stage flags to the background child process. No special handling needed — they're just CLI args.

## Changes

| File | Change |
|------|--------|
| `cli/src/main.rs` | Remove `--template` from Fix command |
| `cli/src/commands/build.rs` | Remove `--template` from `BuildArgs`, add per-stage flags, wire through |
| `cli/src/commands/fix.rs` | Remove `_template_name` param, add per-stage flags, wire through |
| `cli/src/commands/epic.rs` | Accept template override in `create_epic()` / `create_decompose_task()` |
| `cli/src/commands/loop_cmd.rs` | Accept template override in `LoopOptions` |

## Issue addressed

From review `poswnrlqsxnssp`:

- **Issue 3** — `--template` silently ignored in build.rs and fix.rs
