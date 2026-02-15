# Rename `--for` to `--assignee`

## Problem

The CLI uses `--for` as the primary flag name with `assignee` as an alias, but everywhere else in the codebase the canonical name is `assignee`:

- Data model: `TaskEvent::Created { assignee }`, `MaterializedTask.assignee`
- Storage: `assignee=claude-code`
- Templates: `assignee: claude-code` in frontmatter
- Internal code: `assignee_arg`, `new_assignee`, `effective_assignee`

The CLI flags should match.

## Changes

### 1. Flip primary/alias in all 4 clap `#[arg]` definitions

**List** (line ~203):
```rust
// before
#[arg(long = "for", visible_alias = "assignee", value_name = "AGENT")]
// after
#[arg(long = "assignee", alias = "for", value_name = "AGENT")]
```

**Add** (line ~250):
```rust
// before
#[arg(long = "for", visible_alias = "assignee", value_name = "AGENT")]
// after
#[arg(long = "assignee", alias = "for", value_name = "AGENT")]
```

**Start** (line ~328):
```rust
// before
#[arg(long = "for", visible_alias = "assignee", value_name = "AGENT")]
// after
#[arg(long = "assignee", alias = "for", value_name = "AGENT")]
```

**Update** (line ~414):
```rust
// before
#[arg(long = "for", visible_alias = "assignee", ...)]
// after
#[arg(long = "assignee", alias = "for", ...)]
```

Use `alias` (hidden) instead of `visible_alias` for `--for` — it still works but doesn't clutter `--help`.

### 2. Update error message hint (line ~3765)

```rust
// before
"No updates specified. Use --name, --data, --instructions, --for, --unassign, or --p0/--p1/--p2/--p3"
// after
"No updates specified. Use --name, --data, --instructions, --assignee, --unassign, or --p0/--p1/--p2/--p3"
```

### 3. Update CLAUDE.md

The `aiki task update` reference in CLAUDE.md mentions `--for` — update to `--assignee`.

## Not changing

- Rust field names (already `assignee`)
- Storage format (already `assignee=`)
- Template frontmatter (already `assignee:`)
- Any internal logic
