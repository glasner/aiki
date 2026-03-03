# Remove `--severity` flag from `aiki review issue add`

## Goal

Simplify the CLI by removing the `--severity` flag. Agents should use `--high` or `--low` directly (default is medium when neither is specified).

**Before:** `aiki review issue add <id> "desc" --severity high`
**After:** `aiki review issue add <id> "desc" --high`

## Changes

### 1. CLI arg definition (`cli/src/commands/review.rs` ~L311-332)

In `ReviewIssueSubcommands::Add`:
- Remove the `severity: Option<String>` field and its `#[arg(long, value_parser = parse_severity)]`
- Remove `conflicts_with = "severity"` from `--high` and `--low`
- Update doc comments on `--high` / `--low` (no longer "shorthand for --severity")

### 2. Dispatch site (`cli/src/commands/review.rs` ~L390)

Update the destructure + call to `run_issue_add`:
- Remove `severity` from the pattern match
- Remove `severity` param from the `run_issue_add` call

### 3. `run_issue_add` function (`cli/src/commands/review.rs` ~L826-834)

- Remove `severity: Option<String>` parameter
- Simplify resolution logic: just check `high` / `low` / else `"medium"`

### 4. Dead code cleanup (`cli/src/commands/review.rs` ~L280-285)

- Remove `parse_severity()` function (only used by the deleted `--severity` arg)

### 5. Template instructions (`.aiki/templates/aiki/review.md` ~L35-39)

Update the severity docs to remove `--severity` references:

```markdown
**Severity** (pick one per issue):
- `--high` — Must fix: incorrect behavior, bug, or contract violation
- (default) — Should fix: suboptimal, missing, or inconsistent (no flag needed)
- `--low` — Could fix: style, naming, cosmetic
```

## Files touched

| File | What changes |
|------|-------------|
| `cli/src/commands/review.rs` | Remove `--severity` arg, update `Add` variant, simplify `run_issue_add`, delete `parse_severity` |
| `.aiki/templates/aiki/review.md` | Update severity docs |

## Testing

- `cargo build` — confirms no compile errors
- `cargo test` — existing tests pass (none test `--severity` directly)
- Manual: `aiki review issue add <id> "test" --high` still works
- Manual: `aiki review issue add <id> "test" --severity high` should error (unknown flag)
