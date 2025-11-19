# Change ID Implementation - Correct Approach

## Key Insight: Change ID vs Commit ID

After the Codex code review, we realized a fundamental misunderstanding about jj's data model:

### The Problem
The original implementation tracked **commit IDs**, which are content-based hashes that change every time a commit is rewritten. This meant:
1. We captured the working copy commit ID
2. We rewrote the commit to add `aiki:{id}` description  
3. The commit ID changed, making the stored ID stale
4. The database pointed to a commit that no longer existed

### The Solution
jj provides **change IDs** which are stable identifiers that persist across rewrites:

- **Commit ID**: `b5f09d877b72b67606d2f18aa0a91cdc5fefbe1d` (changes on every rewrite)
- **Change ID**: `28be28a352aca126968ef80fab15117e` (stable, persists across rewrites)

When we rewrite a commit (e.g., to change its description), the change ID stays the same but the commit ID changes.

## Implementation

### Database Schema
```sql
CREATE TABLE provenance_records (
    ...
    jj_change_id TEXT,           -- Stable identifier (NEW)
    -- jj_commit_id removed entirely
    jj_operation_id TEXT
);
```

### Hook Flow
1. **Capture change ID** from working copy:
   ```rust
   let change_id = get_working_copy_change_id(&repo_path)?;
   // Returns the stable change_id from the working copy commit
   ```

2. **Store in database**:
   ```rust
   let provenance = ProvenanceRecord {
       jj_change_id: Some(change_id.clone()),
       // ...
   };
   db.insert_provenance(&provenance)?;
   ```

3. **Set description (background thread)**:
   ```rust
   set_change_description(&repo_path, &change_id, provenance_id)?;
   // Rewrites commit to add "aiki:{id}" description
   // The change_id remains stable!
   ```

### Key Functions

#### `get_working_copy_change_id()`
Extracts the change ID from the working copy commit:
```rust
fn get_working_copy_change_id(repo_path: &str) -> Result<String> {
    // Load workspace and get working copy commit
    let commit = repo.store().get_commit(wc_commit_id)?;
    
    // Return the stable change_id
    Ok(commit.change_id().hex())
}
```

#### `set_change_description()`
Sets the description on the change (runs in background thread):
```rust
fn set_change_description(
    repo_path: &str, 
    change_id_str: &str, 
    provenance_id: i64
) -> Result<()> {
    // Find commit with this change_id
    let commit = /* ... */;
    
    // Verify it's the right change
    assert_eq!(commit.change_id(), &change_id);
    
    // Rewrite commit with new description
    let new_commit = tx.repo_mut()
        .rewrite_commit(&commit)
        .set_description(format!("aiki:{}", provenance_id))
        .write()?;
    
    // The commit ID changes, but the change_id stays the same!
    Ok(())
}
```

## Benefits

1. **No stale IDs**: The change_id we store in the database remains valid forever
2. **Proper jj semantics**: We're using jj's data model correctly
3. **Simpler code**: No need to update the database after rewriting commits
4. **Fast queries**: Can look up changes by stable ID without worrying about rewrites

## Test Results

```
✅ End-to-end test passed!
  ✓ record-change captured working copy change ID
  ✓ JJ change ID captured: 28be28a352aca126968ef80fab15117e
  ✓ JJ change ID is valid (stable across rewrites)
  ✓ Hook execution time: 8.38ms (target: <25ms)
```

## Files Changed

- `cli/src/provenance.rs`: `ProvenanceRecord` now has `jj_change_id` (removed `jj_commit_id`)
- `cli/src/db.rs`: Schema uses `jj_change_id`, removed `update_commit_id()` method
- `cli/src/record_change.rs`:
  - `get_working_copy_change_id()`: Extracts change ID from working copy
  - `set_change_description()`: Sets description on the change (renamed from `link_jj_operation`)
- `cli/tests/end_to_end_tests.rs`: Verify change_id tracking

## Performance

Hook execution remains fast (~8ms) because:
- Reading change ID is instant (just accessing a field on the commit object)
- Setting description happens in background thread
- No database updates needed after rewrite

All 40 tests pass ✅
