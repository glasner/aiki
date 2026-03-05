# Unify workflow template flags to `--X-template`

## Problem

Template override flags are inconsistent across commands:
- `build --decompose foo` is a template override, but `build --review` is a trigger
- `fix --template` vs `fix --decompose` vs `fix --loop` vs `fix --review` — mix of naming patterns
- `loop --loop-template` was redundant (already fixed to `--template`)
- `review --fix` is a boolean trigger with no template override

The presence of a step-specific template flag should both **trigger the step** (if optional) and **set the template**.

## New flag convention

Every command gets `--template` for its own primary template. When a command orchestrates sub-steps, those use `--X-template` where X is the step name:

| Flag pattern | Meaning |
|---|---|
| `--template <T>` | Primary template for this command |
| `--X-template [T]` | Trigger step X and/or override its template |

Presence of `--X-template` (even without a value) triggers the step. Providing a value overrides the default template.

## Changes per command

### `decompose` — no changes needed
Already uses `--template` for its primary template. No sub-steps.

### `loop` — no changes needed
Already uses `--template` (just fixed from `--loop-template`). No sub-steps.

### `build`

| Before | After | Notes |
|---|---|---|
| `--decompose <T>` | `--decompose-template <T>` | Template override (decompose always runs) |
| `--loop <T>` | `--loop-template <T>` | Template override (loop always runs) |
| `--review [T]` | `--review-template [T]` | Triggers review + optional template |
| `--fix [T]` | `--fix-template [T]` | Triggers fix (implies review) + optional template |

### `fix`

| Before | After | Notes |
|---|---|---|
| `--template <T>` | `--template <T>` | **No change** — primary template stays |
| `--decompose <T>` | `--decompose-template <T>` | Template override |
| `--loop <T>` | `--loop-template <T>` | Template override |
| `--review [T]` | `--review-template [T]` | Template override for quality loop review |

### `review`

| Before | After | Notes |
|---|---|---|
| `--template <T>` | `--template <T>` | **No change** — primary template stays |
| `--fix` (boolean) | `--fix-template [T]` | Triggers fix + optional template override |

## Files to change

### Rust source
1. **`cli/src/commands/build.rs`** — `BuildArgs` struct: rename `decompose` → `decompose_template`, `loop_template` stays (already named correctly internally), `review` → `review_template`, `fix` → `fix_template`. Update `#[arg(long = ...)]` attributes. Update all spawn_args strings. Update all usage sites (`args.decompose` → `args.decompose_template`, etc.).

2. **`cli/src/main.rs`** — `Fix` variant: rename `decompose` → `decompose_template` with `#[arg(long = "decompose-template")]`, `loop_template` gets `#[arg(long = "loop-template")]` (was `"loop"`), `review` → `review_template` with `#[arg(long = "review-template")]`. Update the match arm destructuring and call to `commands::fix::run`.

3. **`cli/src/commands/fix.rs`** — Update spawn_args strings from `"--decompose"` → `"--decompose-template"`, `"--loop"` → `"--loop-template"`, `"--review"` → `"--review-template"`, `"--template"` → `"--template"` (no change for primary).

4. **`cli/src/commands/review.rs`** — `ReviewArgs` struct: rename `fix: bool` → `fix_template: Option<String>` with `#[arg(long = "fix-template", default_missing_value = "aiki/plan/fix", num_args = 0..=1)]`. Update all usage sites that check `fix` boolean to check `fix_template.is_some()`. Update `build_async_review_args` to pass `"--fix-template"` + value. Update `CreateReviewParams` to carry the fix template from the CLI flag.

### Docs
5. **`cli/docs/sdlc/build.md`** — Update flag table and examples
6. **`cli/docs/sdlc/fix.md`** — Update flag table
7. **`cli/docs/sdlc/review.md`** — Update flag table and examples
8. **`cli/docs/sdlc/loop.md`** — Fix stale `--loop-template` reference to `--template`

### Tests
9. **`cli/src/commands/build.rs`** — Update test structs that construct `BuildArgs` (field names change)
10. **`cli/src/commands/review.rs`** — Update `build_async_review_args` tests (`"--fix"` → `"--fix-template"`)

## Result

After this change, `--help` output looks like:

```
aiki build [OPTIONS] [TARGET]
  --decompose-template <T>   Custom decompose template (default: aiki/decompose)
  --loop-template <T>        Custom loop template (default: aiki/loop)
  --review-template [T]      Run review after build (default: aiki/review)
  --fix-template [T]         Run review+fix after build (default: aiki/plan/fix)

aiki fix [OPTIONS] [TASK_ID]
  --template <T>             Custom plan template (default: aiki/plan/fix)
  --decompose-template <T>   Custom decompose template (default: aiki/decompose)
  --loop-template <T>        Custom loop template (default: aiki/loop)
  --review-template [T]      Custom review template (default: aiki/review)

aiki review [OPTIONS] [TARGET]
  --template <T>             Review template (default: aiki/review)
  --fix-template [T]         Auto-fix issues (default: aiki/plan/fix)
```
