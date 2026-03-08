---
draft: true
---

# Conflict Command

**Date**: 2026-03-04
**Status**: Draft
**Purpose**: Expand `aiki resolve` into an `aiki conflict` command group with `resolve` and `list` subcommands.

**Related Documents**:
- [SDLC Docs](cli/docs/sdlc.md) - Workflow documentation
- [Resolve Command](cli/src/commands/resolve.rs) - Current resolve implementation
- [Resolve Template](.aiki/tasks/resolve.md) - Current resolve template
- [Isolation Module](cli/src/session/isolation.rs) - Workspace absorption & conflict detection

---

## Executive Summary

Currently `aiki resolve <change-id>` is a top-level command that creates a task to resolve JJ merge conflicts. We want to promote this into a command group — `aiki conflict` — so we can add related functionality. The existing resolve behavior moves to `aiki conflict resolve`, and a new `aiki conflict list` command provides agents with a structured view of current conflicts across the repo.

---

## User Experience

### `aiki conflict resolve <change-id>` (existing behavior, new path)

```bash
# Resolve conflicts on a specific change (blocking)
aiki conflict resolve abc123

# Async mode
aiki conflict resolve abc123 --async

# Start mode (hand off to caller)
aiki conflict resolve abc123 --start
```

Identical to current `aiki resolve` — just nested under `conflict`.

### `aiki conflict list` (new)

```bash
# List all conflicted changes in the repo
aiki conflict list

# Machine-readable output for agents
aiki conflict list --output json
```

**Example output (default):**
```
Conflicts (2):

  zmqrstvw — Fix auth handler
    src/auth.rs (modify/modify)
    src/middleware.rs (modify/delete)

  kpnmxyzw — Update config
    config.toml (modify/modify)
```

**Example output (JSON):**
```json
[
  {
    "change_id": "zmqrstvw",
    "description": "Fix auth handler",
    "files": [
      {"path": "src/auth.rs", "type": "modify/modify"},
      {"path": "src/middleware.rs", "type": "modify/delete"}
    ]
  }
]
```

---

## How It Works

### Command Group Structure

`aiki conflict` becomes a clap subcommand group (like `aiki task` has `add`, `start`, `close`, etc.):

```rust
// main.rs
#[command(subcommand)]
Conflict(commands::conflict::ConflictCommands),

// commands/conflict.rs (new, replaces resolve.rs)
#[derive(Subcommand)]
pub enum ConflictCommands {
    Resolve(ResolveArgs),
    List(ListArgs),
}
```

### `conflict list` Implementation

Uses existing JJ queries already present in the codebase:

1. `jj log -r "conflicts()" --no-graph -T 'change_id ++ "\n"'` — get all conflicted change IDs
2. For each change: `jj resolve --list -r <id>` — get conflicted files
3. `jj log -r <id> -T 'description'` — get change description
4. Format and output

The `conflicts()` revset is already used in `functions.rs:1111` for post-absorption checks, so this is well-trodden ground.

### Template Move

- Current: `.aiki/tasks/resolve.md`
- New: `.aiki/tasks/aiki/conflict/resolve.md`

The `create_from_template()` function already supports nested paths — template name becomes `"aiki/conflict/resolve"`.

### Backward Compatibility

`aiki resolve` should be removed (not aliased). This is an internal tool and the old command has only existed briefly. Clean break is fine.

---

## Use Cases

1. **Agent auto-resolution after absorption**: Hook detects conflicts → agent runs `aiki conflict list` to see scope → runs `aiki conflict resolve <id>` for each
2. **Human triage**: Developer runs `aiki conflict list` to see all conflicts at a glance before deciding which to tackle
3. **Multi-conflict sessions**: When multiple workspaces merge simultaneously, `list` gives agents a complete picture instead of resolving blindly

---

## Implementation Plan

### Phase 1: Restructure Command

1. Create `cli/src/commands/conflict.rs` with `ConflictCommands` enum containing `Resolve` and `List` subcommands
2. Move resolve logic from `resolve.rs` into `conflict.rs` (or keep as `conflict/resolve.rs` submodule — prefer single file since resolve is only ~150 lines of logic)
3. Update `main.rs`: replace `Resolve` variant with `Conflict` variant
4. Update `commands/mod.rs`: replace `pub mod resolve` with `pub mod conflict`
5. Move template: `.aiki/tasks/resolve.md` → `.aiki/tasks/aiki/conflict/resolve.md`
6. Update template name reference in the command code

### Phase 2: Add `conflict list`

1. Add `ListArgs` struct (flags: `--output json`)
2. Implement `list()` function:
   - Query `jj log -r "conflicts()" ...` for conflicted changes
   - For each: get file list via `jj resolve --list`
   - For each: get description via `jj log`
   - Format output (human-readable default, JSON with `--output json`)
3. Wire into `ConflictCommands::List` match arm

### Phase 3: Update Docs & Hooks

1. Update `cli/docs/sdlc.md` — replace `aiki resolve` references with `aiki conflict resolve`, add `aiki conflict list`
2. Update `AGENTS.md` / `CLAUDE.md` if they reference `aiki resolve`
3. Update `cli/src/flows/core/hooks.yaml` — the `CONFLICT RESOLUTION REQUIRED` autoreply message should reference `aiki conflict resolve` instead of `aiki resolve`
4. Update any references in `functions.rs` that mention resolve
5. Remove `cli/src/commands/resolve.rs`

---

## Open Questions

1. Should `aiki conflict list` also show the conflict *type* per file (modify/modify, modify/delete, add/add)? JJ's `resolve --list` output may or may not include this detail — needs investigation.
