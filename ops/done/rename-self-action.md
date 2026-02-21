# Rename `self:` action to `call:`

## Problem

The YAML action key `self:` is confusing because the value almost always starts with `self.` too:

```yaml
- self: self.workspace_create_if_concurrent
```

The left `self` is the action discriminator ("invoke a built-in function"). The right `self.` is just a prefix on the function name that gets stripped before resolution. This redundancy creates noise and the key name doesn't describe what the action actually does.

## Proposed change

Rename the YAML key from `self:` to `call:`. The value must now explicitly declare its scope — bare names are rejected:

```yaml
# Before
- self: self.workspace_create_if_concurrent

# After — self. prefix (current module)
- call: self.workspace_create_if_concurrent

# After — fully qualified (explicit cross-module)
- call: aiki/core.workspace_create_if_concurrent

# REJECTED — bare name is ambiguous
- call: workspace_create_if_concurrent  # error: must use self. or full namespace
```

## Decision: require explicit scope on the value side

Bare names (e.g., `call: workspace_create_if_concurrent`) are no longer accepted. The value must use either:
- `self.<function>` — resolves relative to the current hook's module
- `aiki/<module>.<function>` — fully qualified, explicit cross-module call

This makes the call target unambiguous at a glance. The engine currently strips `self.` silently — that logic stays, but bare names now return an error instead of silently resolving.

## Files to change

### Rust

**`cli/src/flows/types.rs`**
- `SelfAction`: change `#[serde(rename = "self")]` → `#[serde(rename = "call")]`
- `SelfAction.self_` field: rename to `call` (and update all accesses)
- `Action::Self_` variant: rename to `Action::Call` (cosmetic, matches the new key)
- Doc comments referencing `self:` syntax

**`cli/src/flows/engine.rs`**
- `use` import: `SelfAction` stays (or rename to `CallAction` for consistency)
- `Action::Self_(self_action)` match arms (lines 540, 567, 669) → `Action::Call(...)`
- `SelfAction { self_: ... }` struct literals (lines 737, 803, 1697, 1727, 3595) → `CallAction { call: ... }`
- `execute_self` function: rename to `execute_call`, update `action.self_` → `action.call`
- Add validation: if value doesn't start with `self.` or `aiki/`, return `AikiError::InvalidFunctionPath` (or similar)
- Comments referencing `self:` syntax (lines 2000-2001)

### YAML

**`cli/src/flows/core/hooks.yaml`** — 6 occurrences:
```
line 48:  - self: self.workspace_create_if_concurrent
line 93:          - self: self.write_ai_files_change
line 98:          - self: self.restore_original_files_change
line 157: - self: self.workspace_absorb_all
line 185: - self: self.workspace_create_if_concurrent
line 193: - self: self.workspace_absorb_all
```
All become `call:`.

### Docs

**`ops/now/jj-workspaces.md`** — 5 occurrences (lines 249, 254, 258, 406, 410, 413, 417): update to `call:`.

`ops/done/` files: leave as-is (historical record).

## Out of scope

- Value-side resolution logic in `execute_self`/`execute_call` is unchanged
- No new functionality — pure rename

## Verification

```bash
cargo build
grep -r "self:" cli/src/flows cli/tests --include="*.yaml" --include="*.rs"  # should return 0 results
```
