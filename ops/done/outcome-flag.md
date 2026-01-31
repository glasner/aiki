# Replace --wontdo flag with --outcome option

**Status**: Done
**Related**: Task lifecycle events

---

## Summary

Replace the boolean `--wontdo` flag on `aiki task close` with a more flexible `--outcome <value>` option that accepts outcome values like `done` or `wont_do`.

## Current Behavior

```bash
aiki task close <ID> --comment "summary"           # outcome = done (implicit)
aiki task close <ID> --comment "summary" --wontdo  # outcome = wont_do
```

## Proposed Behavior

```bash
aiki task close <ID> --comment "summary"                    # outcome = done (default)
aiki task close <ID> --comment "summary" --outcome done     # outcome = done (explicit)
aiki task close <ID> --comment "summary" --outcome wont_do  # outcome = wont_do
```

## Motivation

1. **Consistency with event payload**: The `task.closed` event payload already uses `outcome: "done"` or `outcome: "wont_do"` - the CLI should mirror this
2. **Discoverability**: `--outcome` is more intuitive than a boolean `--wontdo` flag
3. **Extensibility**: Future outcomes (e.g., `blocked`, `duplicate`, `deferred`) can be added without new flags
4. **LLM-friendly**: Agents naturally expect `--outcome <value>` pattern (as evidenced by my incorrect assumption)

## Implementation

### 1. Update CLI argument parsing

In `cli/src/commands/task.rs`, change the `close` subcommand:

```rust
// Before
#[arg(long)]
wontdo: bool,

// After
#[arg(long, default_value = "done")]
outcome: String,
```

### 2. Validate outcome values

Add validation to accept only known outcomes:

```rust
fn validate_outcome(outcome: &str) -> Result<()> {
    match outcome {
        "done" | "wont_do" => Ok(()),
        _ => Err(anyhow!("Invalid outcome '{}'. Valid values: done, wont_do", outcome))
    }
}
```

### 3. Update close logic

```rust
// Before
let outcome = if args.wontdo { "wont_do" } else { "done" };

// After
validate_outcome(&args.outcome)?;
let outcome = &args.outcome;
```

### 4. Backwards compatibility (optional)

Keep `--wontdo` as a deprecated alias:

```rust
#[arg(long, default_value = "done")]
outcome: String,

#[arg(long, hide = true)]  // Hidden but still works
wontdo: bool,

// In handler:
let outcome = if args.wontdo { "wont_do".to_string() } else { args.outcome };
```

### Files to Modify

1. `cli/src/commands/task.rs` - Update `CloseArgs` struct and `run_close()` function

### Testing

1. Test `--outcome done` works
2. Test `--outcome wont_do` works
3. Test invalid outcome is rejected
4. Test default is `done` when `--outcome` is omitted
5. (If keeping backwards compat) Test `--wontdo` still works

## Migration

No user-facing migration needed if we keep `--wontdo` as a hidden alias. Otherwise, update any scripts/documentation that use `--wontdo`.
