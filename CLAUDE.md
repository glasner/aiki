# Claude AI Guidelines for Aiki Development

This document provides important context and guidelines for AI assistants (especially Claude) working on the Aiki codebase.

---

## Critical: JJ vs Git Terminology

### The Fundamental Distinction

Aiki is built on **Jujutsu (jj)**, not Git. While jj can colocate with Git repositories, its data model is fundamentally different:

| Concept | Git | Jujutsu (jj) |
|---------|-----|--------------|
| **Atomic unit** | Commit (immutable) | **Change** (mutable) |
| **Primary identifier** | Commit SHA (changes on rewrite) | **Change ID** (stable across rewrites) |
| **Metadata location** | Commit message | **Change description** |
| **Content hash** | Commit SHA | Commit ID (transient) |

### Key Terminology Rules

**USE "CHANGE" when referring to:**
- ✅ JJ's atomic unit of version control (working copy state)
- ✅ User-facing error messages about JJ state
- ✅ Documentation describing JJ operations
- ✅ API documentation for provenance tracking
- ✅ Comments explaining JJ behavior

**USE "COMMIT" only when referring to:**
- ✅ Git commits (when interfacing with Git)
- ✅ JJ operation log commits (`tx.commit()` calls)
- ✅ The jj-lib API (which uses `get_commit()` method names)
- ✅ Git import operations

### Examples

**CORRECT:**
```rust
// Get the working copy change ID (stable identifier)
let change_id = commit.change_id();  // API method name is unavoidable

// Error: File not found in any head changes
anyhow::bail!("File not found in any head changes");

/// Parse provenance metadata from change description
pub fn from_description(description: &str) -> Result<Option<Self>>

/// The change_id is a stable identifier that persists across rewrites,
/// unlike commit_id which changes every time the change is rewritten.
```

**INCORRECT:**
```rust
// ❌ WRONG: File not found in any head commits
anyhow::bail!("File not found in any head commits");

// ❌ WRONG: Parse provenance from commit description
pub fn from_description(description: &str) -> Result<Option<Self>>

// ❌ WRONG: commit_id changes when commit content changes
/// unlike commit_id which changes every time the commit content changes.
```

---

## Architecture: Change-Centric, Not Commit-Centric

### Core Principle

Aiki tracks **JJ changes** with stable **change IDs**, not Git commits or JJ commit IDs.

```
┌─────────────────────────────────────────────────┐
│  Working Copy = A "Change" in JJ               │
│  • Has stable change_id (persists on rewrite)  │
│  • Has transient commit_id (changes on rewrite)│
│  • Has mutable description (stores [aiki] data)│
└─────────────────────────────────────────────────┘
```

### Why This Matters

1. **Change IDs are stable** - They don't change when you amend, rebase, or rewrite
2. **Commit IDs are transient** - They change on every rewrite operation
3. **Provenance uses change IDs** - All our metadata references stable change_ids
4. **Git commits are different** - When we export to Git, that's a separate concern

### Implementation Pattern

```rust
// CORRECT PATTERN: Always use change_id as the primary identifier
let change_id = get_working_copy_change_id(&repo_path)?;
set_change_description(&repo_path, &change_id, &provenance)?;

// The jj-lib API uses "commit" in method names, but we're working with changes:
let commit = repo.store().get_commit(&commit_id)?;  // API method name
let change_id = commit.change_id();  // Extract stable identifier
let description = commit.description();  // Get change description

// Document this clearly in comments:
// Load the commit object (represents a change in jj) to get its change_id
```

---

## JJ --help

These are the built in commands for jj. We should also try to take advantage of native functionality before building our own.

Commands:
  abandon           Abandon a revision
  absorb            Move changes from a revision into the stack of mutable revisions
  bisect            Find a bad revision by bisection
  bookmark          Manage bookmarks [default alias: b]
  commit            Update the description and create a new change on top [default alias: ci]
  config            Manage config options
  describe          Update the change description or other metadata [default alias: desc]
  diff              Compare file contents between two revisions
  diffedit          Touch up the content changes in a revision with a diff editor
  duplicate         Create new changes with the same content as existing ones
  edit              Sets the specified revision as the working-copy revision
  evolog            Show how a change has evolved over time [aliases: evolution-log]
  file              File operations
  fix               Update files with formatting fixes or other changes
  gerrit            Interact with Gerrit Code Review
  git               Commands for working with Git remotes and the underlying Git repo
  help              Print this message or the help of the given subcommand(s)
  interdiff         Compare the changes of two commits
  log               Show revision history
  metaedit          Modify the metadata of a revision without changing its content
  new               Create a new, empty change and (by default) edit it in the working copy
  next              Move the working-copy commit to the child revision
  operation         Commands for working with the operation log [aliases: op]
  parallelize       Parallelize revisions by making them siblings
  prev              Change the working copy revision relative to the parent revision
  rebase            Move revisions to different parent(s)
  redo              Redo the most recently undone operation
  resolve           Resolve conflicted files with an external merge tool
  restore           Restore paths from another revision
  revert            Apply the reverse of the given revision(s)
  root              Show the current workspace root directory (shortcut for `jj workspace root`)
  show              Show commit description and changes in a revision
  sign              Cryptographically sign a revision
  simplify-parents  Simplify parent edges for the specified revision(s)
  sparse            Manage which paths from the working-copy commit are present in the working copy
  split             Split a revision in two
  squash            Move changes from a revision into another revision
  status            Show high-level repo status [default alias: st]
  tag               Manage tags
  undo              Undo the last operation
  unsign            Drop a cryptographic signature
  util              Infrequently used commands such as for generating shell completions
  version           Display version information
  workspace         Commands for working with workspaces
  
---


## Code Review Checklist

When reviewing or writing code, check for these common mistakes:

### ❌ Common Mistakes

1. **Using "commit" when referring to JJ changes**
   - Error messages mentioning "commits" instead of "changes"
   - Comments saying "commit description" instead of "change description"
   - Documentation referring to "commit IDs" instead of "change IDs"

2. **Confusing commit_id with change_id**
   - Using commit_id as a stable identifier (it's not!)
   - Storing commit_id in metadata instead of change_id
   - Comparing commit_ids across rewrites (they'll differ)

3. **Mixing Git and JJ concepts**
   - Calling JJ operations "git commits"
   - Using Git terminology for JJ-specific features

### ✅ Best Practices

1. **Be explicit about what you're referring to**
   ```rust
   // GOOD: Clear that we're working with JJ changes
   /// Get the stable change_id from the working copy
   fn get_working_copy_change_id(repo_path: &str) -> Result<String>
   
   // BAD: Ambiguous
   /// Get the working copy ID
   fn get_working_copy_id(repo_path: &str) -> Result<String>
   ```

2. **Add clarifying comments when using jj-lib API**
   ```rust
   // GOOD: Explains the API's terminology
   // Load the commit object (represents a change in jj)
   let commit = repo.store().get_commit(&commit_id)?;
   let change_id = commit.change_id();  // The stable change identifier
   
   // BAD: No context
   let commit = repo.store().get_commit(&commit_id)?;
   let change_id = commit.change_id();
   ```

3. **Use consistent terminology in user-facing output**
   ```rust
   // GOOD: Uses "change" terminology
   eprintln!("  Change ID: {}", change_id);
   
   // BAD: Confusing mix
   eprintln!("  Commit: {}", change_id);
   ```

---

## Metadata Storage Pattern

### The `[aiki]...[/aiki]` Block

All provenance metadata is stored in JJ **change descriptions** using this format:

```
[aiki]
agent=claude-code
session=claude-session-abc123
tool=Edit
confidence=High
method=Hook
[/aiki]
```

**Key points:**
- Stored in the **change description** (not commit message)
- Change description is mutable (can be updated via `jj describe`)
- Associated with a **change_id** (stable) not commit_id (transient)
- Retrieved from JJ when needed (not stored separately)

### Why No Separate Database?

Originally, Aiki considered using SQLite to store provenance. We pivoted to storing everything in JJ change descriptions because:

1. **Single source of truth** - JJ already has all the data (diffs, timestamps, files)
2. **No sync issues** - Metadata can't drift from the actual changes
3. **Simpler architecture** - No database to manage or migrate
4. **JJ-native** - Uses JJ's own data model correctly
5. **Queryable** - Can use JJ revsets: `jj log -r 'description("agent=claude-code")'`

---

## Common Scenarios

### Scenario 1: Adding New Fields to Provenance

**DO:**
```rust
// Add to the [aiki] block format
format!(
    "[aiki]\nagent={}\nsession={}\ntool={}\nnew_field={}\n[/aiki]",
    agent_type, session_id, tool_name, new_value
)
```

**DON'T:**
```rust
// Don't create a separate storage mechanism
// Don't use commit_id as identifier
// Don't store redundant data that JJ already has
```

### Scenario 2: Querying Provenance

**DO:**
```rust
// Use change_id to look up changes
let change_id = get_working_copy_change_id(repo)?;
let commit = repo.store().get_commit_by_change_id(&change_id)?;
let description = commit.description();
let provenance = ProvenanceRecord::from_description(description)?;
```

**DON'T:**
```rust
// Don't query by commit_id (it changes on rewrite)
// Don't cache change descriptions separately
// Don't duplicate JJ's native queries
```

### Scenario 3: Error Messages

**DO:**
```rust
anyhow::bail!("File not found in any head changes. Has the file been tracked in jj?");
```

**DON'T:**
```rust
anyhow::bail!("File not found in any head commits. Has it been committed?");
```

---

## When "Commit" is Correct

There are legitimate uses of "commit" in this codebase:

### 1. JJ Operation Log

```rust
// CORRECT: This commits a transaction to the operation log
tx.commit("aiki: embed provenance metadata")?;
```

This is a JJ operation, not a version control commit.

### 2. Git Interop

```rust
// CORRECT: Importing Git commits
pub fn git_import(&self) -> Result<()> {
    git::import_refs(tx.repo_mut(), &git_settings)?;
    // ...
}
```

When actually dealing with Git, "commit" is correct.

### 3. JJ-lib API Methods

```rust
// CORRECT: The API method is named get_commit()
let commit = repo.store().get_commit(&commit_id)?;
```

The jj-lib API uses "commit" in method names. This is unavoidable but should be documented:

```rust
// Load the commit object (represents a change in jj)
let commit = repo.store().get_commit(&commit_id)?;
```

---

## Testing

When writing tests, maintain the change/commit distinction:

```rust
#[test]
fn test_change_description_metadata() {  // ✅ GOOD: Uses "change"
    let change_id = get_working_copy_change_id(temp_dir.path())?;
    set_change_description(&change_id, &provenance)?;
    // ...
}

#[test]
fn test_commit_metadata() {  // ❌ BAD: Ambiguous
    // ...
}
```

---

## Documentation

When documenting Aiki's architecture:

**DO:**
- Explain the change/commit distinction upfront
- Use "change" for JJ's atomic unit
- Use "change ID" for stable identifiers
- Use "change description" for metadata storage
- Add a "JJ Terminology" section to major docs

**DON'T:**
- Assume readers know JJ terminology
- Use "commit" and "change" interchangeably
- Skip explaining the stability of change_id

---

## Summary: Quick Reference

| Term | Meaning | When to Use |
|------|---------|-------------|
| **Change** | JJ's atomic unit (mutable) | Most of the time in Aiki |
| **Change ID** | Stable identifier (hex string) | Primary identifier for provenance |
| **Change description** | Metadata field on a change | Where [aiki] blocks live |
| **Commit** | Git's atomic unit OR jj operation | Git interop, operation log |
| **Commit ID** | Content hash (transient in jj) | Internal jj-lib usage only |

**Default assumption:** When in doubt, you're probably talking about a **change** with a **change ID** in a **change description**.

---

## Questions?

If you're unsure whether to use "commit" or "change":

1. Are you referring to something in JJ's data model? → Use **"change"**
2. Are you calling a jj-lib API method? → Use the API's terminology but add a comment
3. Are you interfacing with Git? → Use **"commit"**
4. Are you committing a JJ transaction? → Use **"commit"** (operation log)

**When in doubt, use "change"** - it's correct 90% of the time in Aiki's codebase.

---

## Related Documentation

- `ops/CHANGE_ID_IMPLEMENTATION.md` - Technical deep-dive on change_id vs commit_id
- `ops/phase-1.md` - Architecture docs (now includes terminology guide)
- `ops/ROADMAP.md` - Strategic vision showing the change-centric approach
