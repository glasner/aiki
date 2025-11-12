# Phase 0: Initial CLI & JJ Setup - Implementation Plan

## Overview

Phase 0 establishes the minimal CLI infrastructure and JJ integration required for repository initialization. This phase focuses solely on delivering a working `aiki init` command that sets up a repository with JJ integration.

**Scope**: Minimum viable `aiki init` command only. Additional commands (status, config, doctor) will be added in future phases as needed.

---

## Milestone 0.1: Basic CLI & JJ Integration ✅ COMPLETED

**Goal**: Create minimal CLI with jj-lib integration.

### Tasks
- [x] Set up Rust project structure
- [x] Add CLI framework dependency (clap with derive features)
- [x] Add jj-lib dependency for direct JJ integration
- [x] Implement single `init` command with help text
- [x] Add version flag
- [x] Create JJ workspace wrapper using jj-lib
- [x] Add basic error handling
- [x] Write unit tests
- [x] Write integration tests

### Deliverables
```bash
aiki --version          # Shows version
aiki --help             # Shows help text
aiki init --help        # Shows init command help
aiki init               # Initializes JJ colocated repository
```

### Success Criteria
- ✅ CLI compiles and runs
- ✅ Uses jj-lib crate directly (no external JJ binary required)
- ✅ Help text displays correctly
- ✅ Can initialize colocated JJ/Git repositories
- ✅ All tests pass (6/6 tests passing)

### Technical Notes
- Use `clap` v4 with derive macros
- Use `jj-lib` v0.35.0 for JJ integration (no shelling out to commands)
- Minimal structure: `src/main.rs` and `src/jj.rs`
- Follow Unix exit codes (0 = success, 1 = error)
- No async needed - uses jj-lib's synchronous API
- JJWorkspace wrapper provides clean API around jj-lib's Workspace type

---

## Milestone 0.2: Repository Detection

**Goal**: Detect if we're in a Git repository and if JJ is already initialized.

### Tasks
- [ ] Implement Git repository detection (look for `.git` directory)
- [ ] Implement JJ repository detection (look for `.jj` directory)
- [ ] Add repository path resolution (find repo root from subdirectory)
- [ ] Add basic validation and error messages

### Detection Logic
```rust
pub struct RepoDetector {
    current_dir: PathBuf,
}

impl RepoDetector {
    pub fn find_repo_root(&self) -> Result<PathBuf>;
    pub fn has_git(&self) -> bool;
    pub fn has_jj(&self) -> bool;
}
```

### Success Criteria
- ✓ Can detect Git repositories
- ✓ Can detect existing JJ repositories
- ✓ Can find repository root from any subdirectory
- ✓ Provides clear error when not in a repository

### Technical Notes
- Walk up directory tree looking for `.git`
- Check for `.jj` directory to detect existing JJ repo
- Stop at filesystem root or home directory

---

## Milestone 0.3: Minimal `aiki init` Implementation

**Goal**: Implement working `aiki init` command that initializes JJ in a Git repository.

### Tasks
- [ ] Implement `aiki init` command logic
  - [ ] Check if already initialized (`.aiki` exists)
  - [ ] Verify Git repository exists
  - [ ] Initialize JJ repository (`jj git init --colocate`)
  - [ ] Create minimal `.aiki/` directory
  - [ ] Create minimal config file
- [ ] Add idempotency (safe to run multiple times)
- [ ] Add progress output
- [ ] Write integration tests

### Directory Structure Created by `aiki init`

Based on the MVP specification, `aiki init` creates:

```
.aiki/
├── cache/               # Review cache (for Phase 1)
│   └── index.json      # Cache index
├── logs/                # Watcher logs (for Phase 1)
├── tmp/                 # Temporary files
└── config.toml          # Configuration
```

Also updates `.gitignore` to include `.aiki/`

### Minimal Configuration
```toml
[aiki]
version = "0.1.0"
initialized_at = "2025-11-07T12:34:56Z"

[review]
# Debounce duration for rapid file changes (milliseconds)
debounce_ms = 300

# Cache size limit (megabytes)
cache_size_mb = 100

# Enable AI review (requires API key)
ai_review_enabled = false

[workers]
# Run static analysis (clippy, eslint, etc.)
static_analysis = true

# Run type checking (tsc, rust-analyzer, etc.)
type_checking = true

# Number of parallel review workers
parallelism = 4

[git]
# Block commits on critical issues
block_on_critical = true

# Block commits on warnings
block_on_warnings = false

# Auto-escalate to human after N failed attempts
auto_escalate_after = 3
```

### Deliverables

```bash
cd /path/to/git/repo
aiki init

# Output:
# Initializing Aiki in /path/to/git/repo
# ✓ Found Git repository
# ✓ Initializing JJ repository
# ✓ Created .aiki directory
# 
# Aiki initialized successfully!
```

**If already initialized:**
```bash
aiki init

# Output:
# Aiki is already initialized in this repository.
# Initialized on: 2025-11-07 12:34:56
```

**If not in a Git repo:**
```bash
aiki init

# Output:
# Error: Not in a Git repository
# 
# Run 'git init' first, or navigate to an existing Git repository.
```

### Success Criteria
- [ ] `aiki init` creates `.aiki` directory and config
- ✅ `aiki init` initializes colocated JJ repository using jj-lib
- [ ] `aiki init` is idempotent (safe to run multiple times)
- [ ] Works in Git repositories (both fresh and existing)
- ✅ Clear error messages for common problems
- ✅ Completes in <5 seconds

### Technical Notes
- Use `jj_lib::workspace::Workspace::init_colocated_git()` to create Git-backed JJ repo
- No external JJ binary required - uses jj-lib crate directly
- `.aiki/config.toml` is minimal - just version and timestamp
- No logging framework needed yet - use simple stdout/stderr
- Make it idempotent by checking for `.aiki` directory first

### Advantages of jj-lib Crate Approach
Using jj-lib directly instead of shelling out to the `jj` CLI provides several benefits:

1. **No External Dependencies**: Users don't need to install the JJ CLI separately
2. **Better Error Handling**: Direct API access provides structured error types instead of parsing stderr
3. **Type Safety**: Compile-time checking of JJ operations via Rust's type system
4. **Performance**: No process spawning overhead for each JJ operation
5. **Easier Testing**: No need to mock external processes or check for binary availability
6. **Consistent Behavior**: Same JJ version guaranteed (locked via Cargo.toml)

---

## Testing Strategy

### Unit Tests
- Repository detection logic
- Path resolution
- Idempotency checks

### Integration Tests
```rust
#[test]
fn test_init_in_git_repo() {
    // Create temp Git repo
    // Run aiki init
    // Verify .aiki and .jj exist
}

#[test]
fn test_init_twice_is_idempotent() {
    // Run aiki init
    // Run aiki init again
    // Verify no errors
}

#[test]
fn test_init_outside_git_repo() {
    // Run in non-Git directory
    // Verify error message
}
```

### Manual Testing Scenarios
1. Fresh Git repository
2. Existing Git repository with history
3. Running `aiki init` twice
4. Running outside a Git repository
5. Running in a subdirectory of a Git repository

---

## Dependencies

### Required Crates
```toml
[dependencies]
clap = { version = "4.5", features = ["derive", "cargo"] }
anyhow = "1.0"
jj-lib = "0.35.0"              # Direct JJ integration (no external binary needed)
toml = "0.8"
serde = { version = "1.0", features = ["derive"] }
chrono = "0.4"

[dev-dependencies]
tempfile = "3.0"
assert_cmd = "2.0"
predicates = "3.0"
```

### System Requirements
- Rust 1.70+
- **No external JJ binary required** (using jj-lib crate)
- Git 2.30+ (jj-lib uses Git as backend for colocated repos)

---

## What We're NOT Building (Yet)

The following are explicitly out of scope for Phase 0:
- ❌ `aiki status` command
- ❌ `aiki config` command (get/set operations)
- ❌ `aiki doctor` command
- ❌ Configuration file loading/merging
- ❌ Logging framework
- ❌ JJ version checking
- ❌ Timeouts or retry logic
- ❌ Git hooks setup
- ❌ Complex error recovery

These will be added in future phases as needed.

---

## Success Metrics

### Completion Criteria
- [ ] All three milestones delivered
- [ ] Integration tests passing
- [ ] Manual testing completed on macOS and Linux
- [ ] README with installation and usage instructions
- [ ] Can successfully run `aiki init` in a Git repository

### User Experience Goals
1. `aiki init` completes in <5 seconds
2. Error messages are clear and actionable
3. Works on first try in a Git repository
4. Safe to run multiple times

---

## Next Phase

Upon completion of Phase 0, we will have:
- ✓ Working `aiki init` command
- ✓ Basic JJ integration (colocated with Git)
- ✓ Repository detection
- ✓ Minimal configuration file

This minimal foundation enables **Phase 1: Provenance Tracking & Agent Attribution**, which will add:
- Agent detection (process monitoring to identify which AI tool is active)
- JJ operation metadata capture (attributing changes to specific agents)
- Edit-level attribution (knowing which agent edited which lines)
- Basic provenance UI (viewing agent history)
- Foundation for testing autonomous review in Phase 2

**Key Insight:** Separating provenance tracking into its own phase enables comprehensive testing of autonomous review. We need to know "which agent did what" before we can validate that agents correctly respond to review feedback.
