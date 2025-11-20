# Plan: Switch JJ to Pure Standalone Mode (No Git Backend)

**Status:** Planning  
**Date:** 2025-11-19  
**Issue:** JJ colocated mode auto-creates Git commits on every `jj describe`

## Problem

JJ's colocated mode automatically creates Git commits on every `jj describe` operation. Users lose control over when Git commits are created. Aiki should use JJ for AI change tracking but let users control Git commits completely.

## Solution

Use **pure JJ storage** (no Git backend, no colocation). JJ tracks AI changes independently, Git commits only when user runs `git commit`.

## Architecture

### Current (Colocated Mode)
```
.git/  → Git repo (auto-synced with JJ)
.jj/   → JJ repo (uses Git as backend, auto-exports)

AI Edit → jj describe → Updates .jj/ → Auto-creates Git commit ❌
```

### New (Pure JJ Mode)
```
.git/  → User's Git repo (unchanged)
.jj/   → Pure JJ storage (completely independent)

AI Edit → jj describe → Updates .jj/ only ✅
User → git commit → prepare-commit-msg extracts co-authors from JJ → Git commit ✅
```

## Changes Required

### 1. Update `cli/src/jj.rs`

**Replace `init_colocated()` method:**
```rust
// OLD: Uses Git backend
pub fn init_colocated(&self) -> Result<()> {
    let settings = Self::create_user_settings()?;
    let (_workspace, _repo) = Workspace::init_colocated_git(&settings, &self.workspace_root)
        .context("Failed to initialize colocated JJ workspace")?;
    Ok(())
}

// NEW: Pure JJ storage
pub fn init(&self) -> Result<()> {
    let settings = Self::create_user_settings()?;
    let store_factories = StoreFactories::default();
    let (_workspace, _repo) = Workspace::init(&settings, &self.workspace_root, &store_factories)
        .context("Failed to initialize JJ workspace")?;
    Ok(())
}
```

**Replace `init_with_git_dir()` method:**
```rust
// OLD: Links to Git directory
pub fn init_with_git_dir(&self, git_dir: &Path) -> Result<()> {
    let settings = Self::create_user_settings()?;
    let (_workspace, _repo) = Workspace::init_external_git(&settings, &self.workspace_root, git_dir)
        .context("Failed to initialize JJ on existing Git repository")?;
    Ok(())
}

// NEW: Pure JJ storage (ignore existing Git)
// Same as init() - no need for separate method
```

**Remove `git_import()` method** - Not needed for pure JJ storage

**Update test (line 98):**
```rust
// OLD test name
fn workspace_init_colocated_creates_both_directories()

// NEW test
fn workspace_init_creates_jj_directory()
```

### 2. Update `cli/src/commands/init.rs`

**Simplify initialization logic (lines 63-105):**

```rust
// OLD: Complex Git detection and branching
if RepoDetector::has_jj(&repo_root) {
    // ...
} else {
    let workspace = jj::JJWorkspace::new(&repo_root);
    if repo_root.join(".git").exists() {
        let git_dir = RepoDetector::resolve_git_dir(&repo_root)?;
        workspace.init_with_git_dir(&git_dir)?;
        workspace.git_import()?;
    } else {
        workspace.init_colocated()?;
    }
}

// NEW: Simple pure JJ init
if RepoDetector::has_jj(&repo_root) {
    if !quiet {
        println!("✓ Found existing JJ repository");
    }
} else {
    if !quiet {
        println!("Initializing JJ repository...");
    }
    let workspace = jj::JJWorkspace::new(&repo_root);
    workspace.init()?;
    if !quiet {
        println!("✓ Initialized JJ repository");
    }
}
```

**Update success message:**
- Remove: "colocated with Git"
- Use: "Initialized JJ repository"

### 3. Update Tests

**Files to update:**
- `cli/tests/blame_tests.rs:278`
- `cli/tests/verify_tests.rs:34`
- `cli/tests/git_hooks_tests.rs:34, 99, 165, 232`

**Change from:**
```rust
Command::new("jj")
    .args(["git", "init", "--colocate"])
    .current_dir(&temp_dir)
    .output()
    .expect("Failed to initialize jj repo");
```

**Change to:**
```rust
Command::new("jj")
    .args(["init"])
    .current_dir(&temp_dir)
    .output()
    .expect("Failed to initialize jj repo");
```

### 4. Update Documentation

**`CLAUDE.md` changes:**

1. **Line 558 - Update JJ vs Git table:**
```markdown
# OLD
| Concept | Git | Jujutsu (jj) |
|---------|-----|--------------|
Aiki is built on **Jujutsu (jj)**, not Git. While jj can colocate with Git repositories, its data model is fundamentally different:

# NEW
| Concept | Git | Jujutsu (jj) |
|---------|-----|--------------|
Aiki uses **both Jujutsu (jj) and Git**. JJ tracks AI changes independently, Git handles version control. Their data models are different:
```

2. **Update Architecture section:**
```markdown
# Add to "Architecture: Change-Centric, Not Commit-Centric" section

## JJ and Git Separation

Aiki uses JJ and Git as separate, independent systems:

- **JJ (.jj/)** - Tracks AI changes with [aiki] metadata blocks
  - Every AI edit creates a JJ change
  - Change descriptions store provenance data
  - Completely independent from Git

- **Git (.git/)** - User's version control system
  - Git commits created only when user runs `git commit`
  - prepare-commit-msg hook extracts co-authors from JJ
  - No automatic synchronization with JJ

This separation ensures:
1. Users control when Git commits are created
2. AI changes are tracked independently in JJ
3. Git history stays clean and user-managed
```

3. **Remove colocated references:**
- Search and remove/update all mentions of "colocated"
- Update examples to show separate `.jj` and `.git` directories

**`ops/README.md` changes:**

Update line 1858:
```markdown
# OLD
JJ uses Git as production-ready storage backend via gitoxide Rust library. Creates standard Git commits compatible with any Git remote. Users can maintain colocated repositories using both `jj` and `git` commands interchangeably.

# NEW
Aiki uses JJ and Git as independent systems. JJ tracks AI changes with stable change IDs and provenance metadata. Git commits are created by users via standard `git commit` workflow. The prepare-commit-msg hook extracts co-author attribution from JJ changes.
```

## Files to Modify

1. `cli/src/jj.rs` - Replace Git-backed init with pure JJ init
2. `cli/src/commands/init.rs` - Simplify init logic, remove Git detection
3. `cli/tests/blame_tests.rs` - Change `jj git init --colocate` → `jj init`
4. `cli/tests/verify_tests.rs` - Change `jj git init --colocate` → `jj init`
5. `cli/tests/git_hooks_tests.rs` - Change `jj git init --colocate` → `jj init` (4 places)
6. `CLAUDE.md` - Update architecture docs, remove colocated references
7. `ops/README.md` - Update JJ/Git architecture description

## Implementation Order

1. Update `cli/src/jj.rs` - Change initialization methods
2. Update `cli/src/commands/init.rs` - Simplify init command
3. Run `cargo build` - Verify compilation
4. Update all test files - Change to `jj init`
5. Run `cargo test` - Verify tests pass
6. Update documentation - CLAUDE.md and ops/README.md
7. Manual testing - Verify no Git commits on `jj describe`

## Success Criteria

- ✅ `jj describe` does NOT create Git commits
- ✅ `jj new` does NOT create Git commits
- ✅ `git commit` creates Git commits with co-authors from JJ
- ✅ `.jj/` uses pure JJ storage (no Git backend)
- ✅ All tests pass with non-colocated mode
- ✅ `aiki init` creates `.jj` directory successfully
- ✅ `prepare-commit-msg` hook can still read JJ changes

## Testing Plan

### Manual Testing Steps

1. **Test pure JJ initialization:**
   ```bash
   cd /tmp/test-repo
   git init
   aiki init
   ls -la  # Should see both .git and .jj
   ```

2. **Test JJ operations don't create Git commits:**
   ```bash
   # Record initial Git commit count
   git log --oneline | wc -l
   
   # Trigger JJ operations via AI edit (or manually)
   jj describe -m "test change"
   jj new
   
   # Verify Git commit count unchanged
   git log --oneline | wc -l  # Should be same
   ```

3. **Test Git commits get co-authors:**
   ```bash
   # Make a change and commit
   echo "test" > file.txt
   git add file.txt
   git commit -m "test commit"
   
   # Verify co-author in commit message
   git log -1 --pretty=full
   # Should contain: Co-authored-by: ...
   ```

4. **Test existing JJ repos:**
   ```bash
   # Verify aiki init detects existing .jj
   aiki init  # Should say "Found existing JJ repository"
   ```

### Automated Test Verification

```bash
# Run full test suite
cargo test

# Specific test categories
cargo test --test blame_tests
cargo test --test git_hooks_tests
cargo test --test verify_tests
```

## Risks and Mitigations

### Risk: Breaking Change for Existing Users
- **Impact:** Users with colocated repos will need to re-initialize
- **Mitigation:** Not handling migration (per requirement)
- **Documentation:** Add breaking change notice to release notes

### Risk: JJ Commands Fail in Non-Colocated Mode
- **Impact:** `jj describe`, `jj new` might behave differently
- **Mitigation:** Test thoroughly before merging
- **Fallback:** Can revert to colocated mode if issues found

### Risk: prepare-commit-msg Hook Can't Read JJ
- **Impact:** Co-authors might not be added to Git commits
- **Mitigation:** Verify hook works with separate .jj directory
- **Testing:** Included in manual testing plan above

## Notes

- **Breaking change** - Existing repos need re-initialization (not handling migration per requirement)
- **Pure JJ** - No Git backend, completely independent storage
- **No aiki sync** - Users use standard `git commit` workflow (per requirement)
- **prepare-commit-msg hook** - Already handles extracting co-authors from JJ, should continue working

## References

- JJ Documentation: https://github.com/martinvonz/jj
- jj-lib API: `Workspace::init()` for pure JJ storage
- Current CLAUDE.md architecture section
- Current ops/README.md line 1858
