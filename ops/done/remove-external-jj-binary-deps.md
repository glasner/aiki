# Remove External JJ Binary Dependencies

## Status: DONE (2025-01-09)

## Problem

Currently, `cli/src/record_change.rs` shells out to the external `jj` binary via `Command::new("jj")`, which requires users to have jj-cli installed separately. This contradicts our architectural goal of using only the bundled `jj-lib` crate.

**Files with external binary calls:**
- `cli/src/record_change.rs:60-72` - `jj log -r @ --no-graph -T commit_id`
- `cli/src/record_change.rs:74-89` - `jj describe -r <commit_id> -m <description>`

## Goal

Use only the bundled `jj-lib = "0.35.0"` dependency so users don't need to install two binaries (aiki + jj-cli).

## Tasks

### 1. Replace `get_working_copy_commit_id()` with jj-lib API

**Current implementation** (cli/src/record_change.rs:60-72):
```rust
fn get_working_copy_commit_id(repo_path: &str) -> Result<String> {
    let output = Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("@")
        .arg("--no-graph")
        .arg("-T")
        .arg("commit_id")
        .current_dir(repo_path)
        .output()
        .context("Failed to execute 'jj log' command...")?;
    
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
```

**Required replacement:**
```rust
fn get_working_copy_commit_id(repo_path: &Path) -> Result<String> {
    // 1. Load workspace using jj_lib::workspace::Workspace::load()
    // 2. Get working copy from workspace
    // 3. Get current operation from workspace
    // 4. Load repository at operation
    // 5. Get working copy commit from repo.view()
    // 6. Return commit.id().hex() as string
}
```

**jj-lib APIs to use:**
- `jj_lib::workspace::Workspace::load(settings, workspace_root, store_factories())`
- `workspace.working_copy()`
- `workspace.repo_loader()`
- `repo_loader.load_at_head(settings)`
- `repo.view().get_wc_commit_id(workspace_id)`
- `commit_id.hex()` for string representation

### 2. Replace `link_jj_operation()` with jj-lib API

**Current implementation** (cli/src/record_change.rs:74-89):
```rust
fn link_jj_operation(repo_path: &str, commit_id: &str, provenance_id: i64) -> Result<()> {
    let description = format!("aiki:{}", provenance_id);
    
    Command::new("jj")
        .arg("describe")
        .arg("-r")
        .arg(commit_id)
        .arg("-m")
        .arg(&description)
        .current_dir(repo_path)
        .spawn()?; // Asynchronous operation linking
    
    Ok(())
}
```

**Required replacement:**
```rust
fn link_jj_operation(repo_path: &Path, commit_id: &str, provenance_id: i64) -> Result<()> {
    // 1. Load workspace and repository
    // 2. Parse commit_id string to CommitId
    // 3. Start a transaction on the repo
    // 4. Get commit from repo.store().get_commit(commit_id)
    // 5. Rewrite commit with new description via CommitBuilder
    // 6. Commit transaction with metadata
}
```

**jj-lib APIs to use:**
- `jj_lib::backend::CommitId::from_hex(commit_id)`
- `repo.start_transaction(settings, &description)`
- `repo.store().get_commit(&commit_id)`
- `commit.rewrite()` or `CommitBuilder::for_rewrite_from(settings, commit)`
- `commit_builder.set_description(description)`
- `commit_builder.write()`
- `transaction.commit(description)`

### 3. Update imports in record_change.rs

**Add necessary jj-lib imports:**
```rust
use jj_lib::backend::CommitId;
use jj_lib::config::StackedConfig;
use jj_lib::repo::Repo;
use jj_lib::settings::UserSettings;
use jj_lib::workspace::Workspace;
use jj_lib::commit::CommitBuilder;
use jj_lib::store::Store;
```

**Remove:**
```rust
use std::process::Command; // If only used for jj commands
```

### 4. Share UserSettings creation logic

Currently `cli/src/jj.rs` has `create_user_settings()` helper. Consider:
- Making it public and reusing in record_change.rs
- OR extracting to a shared module
- OR inlining in both places (it's just 2 lines)

### 5. Handle store factories

jj-lib requires store factories for loading workspaces. Use:
```rust
use jj_lib::default_store_factories;

let store_factories = default_store_factories();
```

### 6. Error handling improvements

jj-lib returns `jj_lib::backend::BackendError` and other error types. Ensure proper error context with:
```rust
.map_err(|e| anyhow::anyhow!("Failed to load workspace: {}", e))?
```

### 7. Update tests

**In `cli/tests/cli_tests.rs`:**
- Verify record_change functions work with jj-lib only
- Test get_working_copy_commit_id returns valid commit IDs
- Test link_jj_operation updates commit descriptions
- No external jj binary should be required for tests

### 8. Update documentation

**Files to update:**
- `ops/mvp-idiomatic-jj-integration.md` - Confirm no external binary needed
- `ops/phase-1.md` - Update any references to jj CLI commands
- `ops/done/phase-0.md` - Note completion of binary removal
- `README.md` (if exists) - Remove any jj installation instructions

## Implementation Notes

### Working Copy Commit ID Retrieval

The working copy commit is the current snapshot of uncommitted changes. In jj-lib:
```rust
let workspace_id = workspace.workspace_id();
let wc_commit_id = repo.view().get_wc_commit_id(&workspace_id)?;
```

### Commit Description Updates

Updating a commit description is a mutation operation requiring a transaction:
```rust
let mut tx = repo.start_transaction(&settings, "update commit description");
let commit = repo.store().get_commit(&commit_id)?;
let mut commit_builder = CommitBuilder::for_rewrite_from(&settings, &commit);
commit_builder.set_description(new_description);
let new_commit = commit_builder.write()?;
tx.mut_repo().rewrite_commit(&commit, &new_commit)?;
tx.commit("aiki: link provenance")?;
```

### Settings Management

Consider caching UserSettings instead of recreating on every call:
```rust
// In a struct or lazy_static
lazy_static! {
    static ref JJ_SETTINGS: UserSettings = {
        let config = StackedConfig::with_defaults();
        UserSettings::from_config(config).expect("Failed to create JJ settings")
    };
}
```

## Success Criteria

- [x] No `Command::new("jj")` calls anywhere in codebase
- [x] `cargo build` succeeds without external jj binary
- [x] All tests pass without jj-cli installed
- [x] `get_working_copy_commit_id()` returns same result as before
- [x] `link_jj_operation()` updates commit descriptions correctly
- [x] User only needs to install `aiki` binary
- [x] Documentation updated to reflect jj-lib-only approach

## Testing Plan

1. **Unit tests** - Test new jj-lib functions in isolation
2. **Integration tests** - Verify record_change workflow end-to-end
3. **Manual testing** - Uninstall jj-cli and verify aiki works
4. **CI/CD** - Ensure CI doesn't install jj-cli

## Dependencies

- Requires `jj-lib = "0.35.0"` (already in Cargo.toml)
- May need additional jj-lib exports if APIs are private

## References

- jj-lib documentation: https://github.com/martinvonz/jj/tree/main/lib
- jj-lib API examples in src/commands/describe.rs (jj repo)
- Current bundled integration: `cli/src/jj.rs`

## Estimated Effort

- Replace get_working_copy_commit_id: 1-2 hours
- Replace link_jj_operation: 2-3 hours
- Testing and debugging: 2-3 hours
- Documentation updates: 1 hour

**Total: 6-9 hours**

## Priority

**HIGH** - This is a core architectural requirement. Users should not need to install jj-cli separately.

---

## Implementation Summary

### Changes Made

#### 1. Made `create_user_settings()` public (cli/src/jj.rs)
Changed visibility from private to `pub(crate)` so it can be reused across modules.

#### 2. Replaced `get_working_copy_commit_id()` (cli/src/record_change.rs)
**Before**: Executed `jj log -r @ --no-graph -T commit_id` via external binary
**After**: Uses jj-lib APIs:
- `Workspace::load()` to load the workspace
- `workspace.repo_loader().load_at_head()` to get the repository
- `repo.view().get_wc_commit_id()` to get the working copy commit ID
- `ObjectId::hex()` to convert to string

#### 3. Replaced `link_jj_operation()` (cli/src/record_change.rs)
**Before**: Executed `jj describe -r <commit_id> -m "aiki:<id>"` via external binary
**After**: Uses jj-lib APIs:
- `hex::decode()` + `CommitId::new()` to parse commit ID from hex string
- `repo.start_transaction()` to begin a mutation
- `tx.repo_mut().rewrite_commit()` to rewrite the commit
- `CommitBuilder::set_description()` to update the description
- `tx.commit()` to persist changes

#### 4. Updated dependencies (cli/Cargo.toml)
Added `hex = "0.4"` for converting hex strings to bytes.

#### 5. Removed external binary dependency (cli/src/record_change.rs)
Removed `use std::process::Command;` as it's no longer needed.

### Key API Discoveries

1. **`load_at_head()`** takes no arguments (not `&settings`)
2. **`start_transaction()`** takes no arguments (not `&settings`)
3. **`rewrite_commit()`** takes only `&commit` (not `&settings, &commit`)
4. **`workspace_id()`** is actually **`workspace_name()`**
5. **`CommitId::from_hex()`** requires `'static` lifetime, so use `hex::decode()` + `CommitId::new()` instead
6. **`ObjectId` trait** must be imported to use `.hex()` method on CommitId

### Test Results

All 37 tests pass:
- 25 unit tests in src/main.rs
- 9 integration tests in tests/cli_tests.rs
- 3 integration tests in tests/record_change_tests.rs

Build succeeds with no warnings (except deprecated test API warnings).

### Actual Effort

Approximately 2 hours total, much faster than estimated 6-9 hours due to:
- Well-designed jj-lib APIs
- Comprehensive error messages from Rust compiler
- Good test coverage catching issues early

### Notes

The implementation is **synchronous** whereas the original `link_jj_operation()` used `.spawn()` for async execution. The new version commits the transaction immediately. This is acceptable because:
1. JJ transactions are fast (operation log based)
2. The function is already called in a context where some delay is acceptable
3. Error handling is now more robust (we know if the commit description was set)
