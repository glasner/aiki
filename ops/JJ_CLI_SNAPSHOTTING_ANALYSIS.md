# JJ-CLI Snapshotting Analysis: What We're Missing

## Executive Summary

**Critical Finding:** The jj-cli **automatically snapshots the working copy** before every command that needs to read repository state. Our Aiki implementation does NOT do this, which is why we see stale data.

## The Missing Piece: Automatic Snapshotting

### What jj-cli Does

Every jj command follows this pattern:

```rust
// In CommandHelper::workspace_helper()
pub fn workspace_helper(&self, ui: &Ui) -> Result<WorkspaceCommandHelper, CommandError> {
    let (workspace_command, stats) = self.workspace_helper_with_stats(ui)?;
    print_snapshot_stats(ui, &stats, workspace_command.env().path_converter())?;
    Ok(workspace_command)
}
```

This calls `workspace_helper_with_stats()`, which:

```rust
fn workspace_helper_with_stats(&mut self, ui: &Ui) 
    -> Result<(WorkspaceCommandHelper, SnapshotStats), CommandError> {
    // ... loads workspace and repo ...
    
    // THE CRITICAL PART:
    let stats = self.maybe_snapshot_impl(ui)?;
    
    Ok((workspace_command, stats))
}
```

And `maybe_snapshot_impl()` does:

```rust
fn maybe_snapshot_impl(&mut self, ui: &Ui) -> Result<SnapshotStats, SnapshotWorkingCopyError> {
    if !self.may_update_working_copy {
        return Ok(SnapshotStats::default());
    }

    #[cfg(feature = "git")]
    if self.working_copy_shared_with_git {
        self.import_git_head(ui).map_err(snapshot_command_error)?;
    }
    
    // SNAPSHOT THE WORKING COPY FILES
    let stats = self.snapshot_working_copy(ui)?;

    #[cfg(feature = "git")]
    if self.working_copy_shared_with_git {
        self.import_git_refs(ui).map_err(snapshot_command_error)?;
    }
    Ok(stats)
}
```

### The Core Snapshotting API

The actual snapshot happens in `snapshot_working_copy()`:

```rust
fn snapshot_working_copy(ui: &Ui) -> Result<SnapshotStats> {
    // Create a "working copy mutation" lock
    let locked_ws = self.workspace.start_working_copy_mutation()?;
    
    // Check if working copy is stale
    WorkingCopyFreshness::check_stale(&locked_ws)?;
    
    // THIS IS THE KEY API CALL:
    let new_tree_id = locked_ws.locked_wc().snapshot(&options).block_on()?;
    
    // If tree changed, create a transaction to record it
    if new_tree_id != *old_tree_id {
        let mut tx = repo.start_transaction();
        
        // Rewrite working copy commit with new tree
        let commit_builder = tx.repo_mut()
            .rewrite_commit(&wc_commit)
            .set_tree_id(new_tree_id);
        commit_builder.write(tx.repo_mut())?;
        
        // Rebase descendants
        tx.repo_mut().rebase_descendants()?;
        
        // Commit transaction
        tx.commit("snapshot working copy")?;
    }
    
    Ok(stats)
}
```

**The key jj-lib API call is:**
```rust
locked_wc.locked_wc().snapshot(&options).block_on()?
```

This reads the actual filesystem, computes a new tree, and returns the tree ID.

## What Our Implementation Does

In `cli/src/record_change.rs`, we:

1. **set_change_description():**
   - Load workspace/repo
   - Get working copy commit ID
   - Rewrite commit with new description
   - **NO SNAPSHOTTING**

2. **create_new_change():**
   - Load workspace/repo again
   - Get working copy commit (now stale!)
   - Create new commit with parent's tree
   - **NO SNAPSHOTTING**

## Why This Causes the Bug

**Sequence of events:**

1. Claude edits `file.txt` on disk
2. Aiki hook runs `record_change()`
3. We call `set_change_description()`:
   - Loads repo (doesn't see disk changes yet!)
   - Rewrites commit with [aiki] metadata
   - Tree is still OLD
4. We call `create_new_change()`:
   - Loads repo again (still doesn't see disk changes!)
   - Gets parent's tree (which is OLD)
   - Creates new commit with OLD tree
5. Result: Claude's edit is lost!

**What should happen:**

1. Claude edits `file.txt` on disk
2. Aiki hook runs `record_change()`
3. **SNAPSHOT WORKING COPY** (capture disk changes to tree)
4. Call `set_change_description()`:
   - Rewrites commit with [aiki] metadata
   - Tree is NOW UP TO DATE
5. Call `create_new_change()`:
   - Gets parent's tree (which is NOW UP TO DATE)
   - Creates new commit with correct tree
6. Result: Claude's edit is preserved!

## The Fix: Three Approaches

### Approach 1: Snapshot at Start (Recommended)

Add snapshotting to `record_change()` before any operations:

```rust
pub fn record_change(agent_type: AgentType, _sync: bool) -> Result<()> {
    // ... parse input ...
    
    // SNAPSHOT WORKING COPY FIRST
    snapshot_working_copy(Path::new(&hook_data.cwd))?;
    
    // Now proceed with setting description and creating new change
    set_change_description(Path::new(&hook_data.cwd), &description)?;
    create_new_change(Path::new(&hook_data.cwd))?;
    
    Ok(())
}

fn snapshot_working_copy(cwd: &Path) -> Result<()> {
    eprintln!("Snapshotting working copy");
    
    let settings = JJWorkspace::create_user_settings()?;
    let store_factories = StoreFactories::default();
    let working_copy_factories = default_working_copy_factories();
    
    let workspace = Workspace::load(&settings, cwd, &store_factories, &working_copy_factories)?;
    let repo = workspace.repo_loader().load_at_head()?;
    
    // Lock the working copy for mutation
    let mut locked_ws = workspace.start_working_copy_mutation()?;
    
    // Get current working copy commit
    let wc_commit_id = repo.view()
        .get_wc_commit_id(workspace.workspace_name())?;
    let wc_commit = repo.store().get_commit(wc_commit_id)?;
    let old_tree_id = wc_commit.tree_id();
    
    // Snapshot filesystem to tree
    let snapshot_options = SnapshotOptions::default();
    let new_tree_id = locked_ws.locked_wc()
        .snapshot(&snapshot_options)
        .block_on()?;
    
    // If tree changed, record it
    if new_tree_id != *old_tree_id {
        eprintln!("  Tree changed, recording snapshot");
        
        let mut tx = repo.start_transaction();
        
        // Rewrite working copy commit with new tree
        let mut commit_builder = tx.repo_mut().rewrite_commit(&wc_commit).detach();
        commit_builder.set_tree_id(new_tree_id);
        commit_builder.write(tx.repo_mut())?;
        
        // Rebase descendants
        tx.repo_mut().rebase_descendants()?;
        
        // Commit transaction
        tx.commit("snapshot working copy")?;
        
        eprintln!("  Snapshot recorded");
    } else {
        eprintln!("  No changes to snapshot");
    }
    
    // Drop the lock
    locked_ws.finish()?;
    
    Ok(())
}
```

### Approach 2: Use Shell Commands (Current Workaround)

This is what we tried in phase-1.md but abandoned:

```rust
// Run `jj describe` shell command (snapshots automatically)
std::process::Command::new("jj")
    .arg("describe")
    .arg("-m")
    .arg(&description)
    .current_dir(cwd)
    .output()?;

// Run `jj new` shell command (snapshots automatically)
std::process::Command::new("jj")
    .arg("new")
    .current_dir(cwd)
    .output()?;
```

**Why this works:** The jj binary snapshots automatically before every command.

**Why we shouldn't use this:** It requires external jj binary, goes against our architecture of using jj-lib only.

### Approach 3: Snapshot in Each Function (Less Efficient)

Snapshot separately in `set_change_description()` and `create_new_change()`:

```rust
fn set_change_description(cwd: &Path, description: &str) -> Result<()> {
    // Snapshot first
    let (workspace, repo) = snapshot_and_load(cwd)?;
    
    // Then proceed with description change
    // ...
}

fn create_new_change(cwd: &Path) -> Result<()> {
    // Snapshot first (again)
    let (workspace, repo) = snapshot_and_load(cwd)?;
    
    // Then proceed with new change
    // ...
}
```

**Downside:** Snapshots twice, less efficient.

## Recommendation

**Use Approach 1: Snapshot once at the start of `record_change()`.**

This:
- ✅ Mirrors jj-cli behavior
- ✅ Uses jj-lib only (no external binary)
- ✅ Efficient (snapshot once)
- ✅ Ensures all subsequent operations see up-to-date state
- ✅ Makes the working copy lock explicit

## Implementation Checklist

- [ ] Add `snapshot_working_copy()` function
- [ ] Call it at start of `record_change()`
- [ ] Import required types:
  - `jj_lib::working_copy::SnapshotOptions`
  - `jj_lib::workspace::Workspace::start_working_copy_mutation()`
- [ ] Handle the async `snapshot()` call with `.block_on()`
- [ ] Properly manage the `LockedWorkspace` and call `finish()`
- [ ] Test with actual file edits

## Key jj-lib API Calls

```rust
// The essential APIs for snapshotting:
use jj_lib::workspace::Workspace;
use jj_lib::working_copy::SnapshotOptions;

// 1. Lock the workspace for mutation
let mut locked_ws = workspace.start_working_copy_mutation()?;

// 2. Snapshot the filesystem
let snapshot_options = SnapshotOptions::default();
let new_tree_id = locked_ws.locked_wc()
    .snapshot(&snapshot_options)
    .block_on()?;

// 3. If tree changed, record it in a transaction
if new_tree_id != *old_tree_id {
    let mut tx = repo.start_transaction();
    let mut commit_builder = tx.repo_mut().rewrite_commit(&wc_commit).detach();
    commit_builder.set_tree_id(new_tree_id);
    commit_builder.write(tx.repo_mut())?;
    tx.repo_mut().rebase_descendants()?;
    tx.commit("snapshot working copy")?;
}

// 4. Release the lock
locked_ws.finish()?;
```

## References

### jj-cli Source Files

- `cli/src/cli_util.rs`:
  - `workspace_helper()` - Entry point for all commands
  - `maybe_snapshot_impl()` - Conditional snapshotting
  - `snapshot_working_copy()` - Core snapshot logic
  - `WorkspaceCommandTransaction::edit()` - Sets working copy commit
  - `WorkspaceCommandTransaction::finish()` - Completes transaction

- `cli/src/commands/describe.rs`:
  - `cmd_describe()` - How jj-cli implements `jj describe`
  - Pattern: `workspace_helper()` → `start_transaction()` → `rewrite_commit()` → `finish()`

- `cli/src/commands/new.rs`:
  - `cmd_new()` - How jj-cli implements `jj new`
  - Pattern: `workspace_helper()` → `start_transaction()` → `new_commit()` → `edit()` → `finish()`

- `cli/src/commands/edit.rs`:
  - `cmd_edit()` - How jj-cli implements `jj edit`
  - Pattern: `workspace_helper()` → `start_transaction()` → `tx.edit()` → `finish()`

### Key Observations

1. **Every command calls `workspace_helper()` first**
   - This snapshots automatically (unless `--ignore-working-copy`)
   - This is why jj always sees up-to-date state

2. **The `edit()` method is simple:**
   ```rust
   pub fn edit(&mut self, commit: &Commit) -> Result<(), EditCommitError> {
       let name = self.helper.workspace_name().to_owned();
       self.id_prefix_context.take(); // invalidate cache
       self.tx.repo_mut().edit(name, commit) // delegate to MutableRepo
   }
   ```

3. **Transactions are committed with descriptive messages:**
   - "aiki: embed provenance metadata"
   - "aiki: create new change for next edit"
   - "snapshot working copy"

4. **The pattern is consistent:**
   ```
   workspace_helper() [SNAPSHOTS HERE]
     ↓
   start_transaction()
     ↓
   modify repo (rewrite_commit, new_commit, edit, etc.)
     ↓
   finish() [commits transaction, updates working copy]
   ```

## Why This Matters

Without snapshotting, we're operating on **stale repository state**. The jj-lib API works entirely on the **tree** representation of files, not the filesystem directly. If we don't snapshot, the tree is out of sync with the filesystem.

This is the root cause of why our `jj describe` + `jj new` pattern worked when using shell commands (they snapshot automatically) but fails when using jj-lib directly (we must snapshot explicitly).

## Next Steps

1. Implement `snapshot_working_copy()` function
2. Add it to `record_change()` at the start
3. Test with actual file edits from Claude
4. Verify that changes are preserved
5. Update documentation in CLAUDE.md about this pattern
