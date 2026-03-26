# Fix Once For Build

**Date**: 2026-03-26
**Status**: Draft
**Purpose**: Allow `aiki build` to pass `--once` semantics to its fix step, so fix runs a single pass without the post-fix review loop.

**Related Documents**:
- [build.rs](../../cli/src/commands/build.rs) - Build command and `drive_build` fix iteration
- [fix.rs](../../cli/src/commands/fix.rs) - Fix command with existing `--once` flag

---

## Executive Summary

The `aiki fix` command supports `--once` to run a single fix pass without the post-fix review loop. When `aiki build --fix` invokes fix, it hardcodes `once: false`. Users need a way to pass `--once` through the build command to get single-pass fix behavior.

---

## User Experience

```bash
# Single-pass fix after build (no post-fix review loop inside each fix step)
aiki build plan.md --fix --once

# Equivalent long form
aiki build plan.md --fix-once

# With custom template
aiki build plan.md --fix-template custom/fix --once
```

The `--once` flag on build requires `--fix` or `--fix-template` (it modifies fix behavior). If `--once` is passed without `--fix`/`--fix-template`, emit a warning or error.

**Note:** `--fix-once` is sugar for `--fix --once`.

---

## How It Works

1. **BuildArgs**: Add `--once` bool flag and `--fix-once` convenience flag
2. **Flag resolution**: `--fix-once` implies `--fix` and `--once`. Standalone `--once` requires `--fix`/`--fix-template`
3. **BuildOpts**: Add `fix_once: bool` field
4. **`run_fix_step`**: Accept `once: bool` parameter, forward to `run_fix()`
5. **`drive_build`**: When `fix_once` is true, after a Fix step completes, skip injecting further Fix→Decompose→Loop→Review→RegressionReview cycles (the fix already ran single-pass internally, so no review feedback to iterate on)

### Data flow

```
BuildArgs { fix_once, once }
  → flag resolution (fix_once || (fix && once))
  → BuildOpts { fix_once: bool }
  → drive_build() reads opts.fix_once
    → run_fix_step() passes opts.fix_once as `once` to run_fix()
    → if fix_once, drive_build skips post-fix review injection
```

---

## Use Cases

1. **Quick quality pass**: User wants a single fix pass on build output to catch obvious issues, without the full iterative review loop. Common during development when iterating quickly.

2. **CI/time-bounded builds**: In CI or when time is limited, `--fix-once` gives one shot at fixing review issues without open-ended iteration.

---

## Implementation Plan

### Phase 1: Add flags and threading (single PR)

1. **`BuildArgs`** (build.rs ~line 84): Add two clap args:
   - `--once` (`bool`) — requires `--fix` or `--fix-template`
   - `--fix-once` (`bool`) — convenience, implies `--fix`

2. **`BuildOpts`** (build.rs ~line 104): Add `fix_once: bool`

3. **Flag resolution** (build.rs ~line 349): Resolve `fix_once = args.fix_once || (args.once && fix_template.is_some())`. If `args.once && !fix_after`, emit error.

4. **`run_fix_step`** (build.rs ~line 892): Add `once: bool` parameter, forward to `run_fix()` call at line 948.

5. **`drive_build`** (build.rs ~line 219): When injecting fix cycles at line 258, pass `opts.fix_once` to the Fix step. When `fix_once` is true, skip injecting the Decompose→Loop→Review→RegressionReview steps after Fix (only inject the Fix step itself).

6. **All call sites**: Thread `fix_once` through `run_build_plan`, `run_build_epic`, `run_continue_async`, and spawn args.

7. **Tests**: Add unit tests for:
   - `--fix-once` implies `--fix` and sets `fix_once`
   - `--fix --once` sets `fix_once`
   - `--once` without `--fix` is an error
   - `drive_build` with `fix_once` skips post-fix review injection

---

## Error Handling

- `--once` without `--fix` or `--fix-template`: Error with message "The --once flag requires --fix or --fix-template"
- `--fix-once` with `--once`: Redundant but not an error (both set the same flag)

---

## Open Questions

(none)

---
