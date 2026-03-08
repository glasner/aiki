# Plan: Add short flags for workflow commands (`-r` and `-f`)

## Problem
The common workflow commands require longer flags that slow repeated usage:
- `--review`
- `--fix`

For high-frequency runs (`aiki build`, `aiki review`), this adds friction and typing overhead.

## Goal
Add concise short flags with predictable semantics:
- `-r` → `--review`
- `-f` → `--fix`

Scope is intentionally narrow: no behavior changes, only CLI ergonomics.

## Scope
- Add short flags to **build** and **review** command surfaces where `--review` / `--fix` exist today.
- Preserve existing long flags and defaults.
- Keep compatibility with existing scripts.

## Proposed behavior

### Canonical mappings
- `-r` is an alias for `--review`.
- `-f` is an alias for `--fix`.

### Equivalence examples
- `aiki build plan.md -r` == `aiki build plan.md --review`
- `aiki build plan.md -f` == `aiki build plan.md --fix`
- `aiki review <id> -f` == `aiki review <id> --fix` (where supported)

### Combination behavior
- `-r -f` may be combined and must match long-flag behavior.
- If command currently supports grouped short options via parser defaults, `-rf` should work; if not, document expected usage as `-r -f`.

## Implementation plan

### Phase 1 — Inventory and parser map
Identify exact command structs/args where:
- `--review` is defined,
- `--fix` is defined,
- help text/examples mention only long form.

Deliverable: file/function map for build + review command argument definitions.

### Phase 2 — Add short aliases
Update CLI arg definitions (clap/args layer) to add shorts:
- `short = r` for `review` flag
- `short = f` for `fix` flag

Constraints:
- avoid collisions with existing short flags in same command scope,
- preserve current defaults and conflicts/requires rules.

### Phase 3 — Update help and docs
- Ensure `--help` shows both short + long forms.
- Update docs/examples to prefer short forms for common workflows while keeping long forms in reference sections.

### Phase 4 — Tests
Add/adjust tests for:
1. `-r` maps exactly to `--review`.
2. `-f` maps exactly to `--fix`.
3. combined usage (`-r -f`) matches long-flag behavior.
4. no regressions in existing long-form tests.

## Acceptance criteria
1. `build` command accepts `-r` and `-f` where long forms are accepted.
2. `review` command accepts `-f` (and `-r` only if that command already has a review-mode flag).
3. Long flags remain fully supported.
4. Help output and docs include new short flags.
5. Test suite covers alias behavior and compatibility.

## Risks / caveats
- Potential short-flag collisions with existing command-local flags.
- Inconsistent parser definitions across subcommands can create uneven UX; inventory first to avoid partial rollout.
- Shell scripts that parse help output may need minor updates if they rely on exact formatting.

## Immediate next steps
1. Locate build/review arg definitions.
2. Add short aliases (`r`, `f`) and run tests.
3. Update command docs and examples.
4. Ship as a focused CLI ergonomics PR.
