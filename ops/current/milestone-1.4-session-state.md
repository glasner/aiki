# Milestone 1.4: Session State Persistence

This document outlines the implementation plan for the Session State Persistence system (Milestone 1.4).

See [milestone-1.md](./milestone-1.md) for the full Milestone 1 overview.

---

## Overview

Session State Persistence allows flows to store and retrieve data across multiple events within a session, enabling stateful workflows.

**Key Capabilities:**
- Track edited files and affected repos
- Store intermediate computation results
- Built-in helper functions
- Automatic cleanup on session end

---

## Core Features

### 1. Session State Storage

Data persists in `.aiki/.session-state/`:

```
.aiki/.session-state/
├── edited-files.log           # List of edited files
├── affected-repos.log         # List of affected repos
├── build-frontend.log         # Build output
├── build-backend.log          # Build output
├── last-check-timestamp       # When last check ran
├── loop-count.txt             # PostResponse loop count
└── active-checks.json         # PostResponse active checks
```

**Properties:**
- Plain text files (easy to inspect/debug)
- One value per file or line-delimited lists
- Ephemeral (not committed to git/jj)
- Namespaced by session ID
- Auto-created/cleaned up

### 2. Writing to Session State

Use shell commands to write data:

```yaml
PostToolUse:
  - shell: echo "$event.file_path" >> .aiki/.session-state/edited-files.log
  - let: repo = self.determine_repo_for_file($event.file_path)
  - shell: echo "$repo" >> .aiki/.session-state/affected-repos.log
```

**Manual writes:**
```yaml
PostResponse:
  - shell: date +%s > .aiki/.session-state/last-check-timestamp
  - shell: echo "$error_count" > .aiki/.session-state/error-count.txt
```

### 3. Built-in Helper Functions

Flow engine provides helper functions to read session state:

```yaml
PostResponse:
  # Get list of edited files
  - let: files = self.get_edited_files()
  
  # Get list of affected repos  
  - let: repos = self.get_affected_repos()
  
  # Determine repo for a file path
  - let: repo = self.determine_repo_for_file("/path/to/file.ts")
  
  # Count errors in build logs
  - let: error_count = self.count_build_errors()
```

**Helper function implementation:**
```rust
// Built-in functions that read from session state
impl BuiltinFunctions {
    fn get_edited_files(&self) -> Result<Vec<String>> {
        let path = session_state_dir()?.join("edited-files.log");
        if !path.exists() {
            return Ok(Vec::new());
        }
        
        let content = fs::read_to_string(path)?;
        Ok(content.lines().map(|s| s.to_string()).collect())
    }
    
    fn get_affected_repos(&self) -> Result<Vec<String>> {
        let path = session_state_dir()?.join("affected-repos.log");
        if !path.exists() {
            return Ok(Vec::new());
        }
        
        let content = fs::read_to_string(path)?;
        let repos: HashSet<String> = content.lines().map(|s| s.to_string()).collect();
        Ok(repos.into_iter().collect())
    }
}
```

### 4. Session Lifecycle

```
Session Start
  ↓
Create .aiki/.session-state/ directory
  ↓
Events fire, flows execute
  ↓
Write to session state files
  ↓
Read from session state in subsequent events
  ↓
Session End
  ↓
Cleanup .aiki/.session-state/ directory
```

**Cleanup triggers:**
- `aiki session end` command
- New session starts
- Manual: `rm -rf .aiki/.session-state/`

---

## Use Cases

### Use Case 1: Multi-Repo Build Detection

```yaml
PostToolUse:
  - let: repo = self.determine_repo_for_file($event.file_path)
  - shell: echo "$repo" >> .aiki/.session-state/affected-repos.log

PostResponse:
  - let: repos = self.get_affected_repos()
  - shell: |
      for repo in $repos; do
        cd $repo && npm run build 2>&1 | tee .aiki/.session-state/build-$repo.log
      done
  
  - let: error_count = self.count_build_errors()
  - if: $error_count > 0
    then:
      autoreply: "Build failed with $error_count errors"
```

### Use Case 2: Incremental Checks

```yaml
PostResponse:
  # Only run expensive checks if files changed since last check
  - let: last_check = self.read_file(".aiki/.session-state/last-check-timestamp")
  - let: files_changed_since = self.files_modified_since($last_check)
  
  - if: $files_changed_since.length > 0
    then:
      - shell: npm run typecheck
      - shell: date +%s > .aiki/.session-state/last-check-timestamp
```

### Use Case 3: Error Accumulation

```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors()
  - let: lint_errors = self.count_lint_errors()
  - let: total_errors = $ts_errors + $lint_errors
  
  - shell: echo "$total_errors" > .aiki/.session-state/total-errors.txt
  
  # Later, another flow can read this
  - let: previous_errors = self.read_file(".aiki/.session-state/total-errors.txt")
  - if: $total_errors > $previous_errors
    then:
      autoreply: "Error count increased from $previous_errors to $total_errors"
```

### Use Case 4: Task Progress Tracking

```yaml
PostResponse:
  - let: completed_tasks = self.read_file(".aiki/.session-state/completed-tasks.txt")
  - let: new_completed = $completed_tasks + 1
  - shell: echo "$new_completed" > .aiki/.session-state/completed-tasks.txt
  
  - if: $new_completed >= 5
    then:
      autoreply: "Great progress! You've completed $new_completed tasks this session."
```

---

## Implementation Tasks

### Core Session State Manager

- [ ] Create `cli/src/flows/session_state.rs`
  - [ ] `session_state_dir()` - Get session state directory path
  - [ ] `init_session_state()` - Create directory on session start
  - [ ] `cleanup_session_state()` - Remove directory on session end
  - [ ] `write_session_file(name, content)` - Write to session state file
  - [ ] `read_session_file(name)` - Read from session state file

### Built-in Helper Functions

- [ ] Implement `cli/src/flows/functions/get_edited_files.rs`
- [ ] Implement `cli/src/flows/functions/get_affected_repos.rs`
- [ ] Implement `cli/src/flows/functions/determine_repo_for_file.rs`
- [ ] Implement `cli/src/flows/functions/count_build_errors.rs`
- [ ] Register functions in flow engine

### Engine Integration

- [ ] Call `init_session_state()` on session start
- [ ] Call `cleanup_session_state()` on session end
- [ ] Add `.aiki/.session-state/` to `.gitignore` automatically
- [ ] Session ID namespacing (for concurrent sessions)

### Testing

- [ ] Unit tests: Session state directory creation
- [ ] Unit tests: Write/read session files
- [ ] Unit tests: Built-in helper functions
- [ ] Unit tests: Cleanup on session end
- [ ] Integration tests: Multi-event state persistence
- [ ] Integration tests: Concurrent sessions don't interfere
- [ ] E2E tests: Real multi-repo build scenario

### Documentation

- [ ] Tutorial: "Using Session State"
- [ ] Cookbook: Common patterns (build detection, progress tracking)
- [ ] Reference: Built-in helper functions
- [ ] Examples: Real-world session state usage

---

## Success Criteria

✅ Session state persists across events  
✅ `.aiki/.session-state/` directory created/managed automatically  
✅ Built-in helper functions work correctly  
✅ State is cleaned up on session end  
✅ Concurrent sessions don't interfere (session ID namespacing)  
✅ Can write arbitrary data to session state  
✅ Can read arbitrary data from session state  
✅ Session state not committed to git/jj  

---

## Technical Design

### Session State Directory

```rust
pub fn session_state_dir() -> Result<PathBuf> {
    let workspace_root = find_workspace_root()?;
    Ok(workspace_root.join(".aiki/.session-state"))
}
```

### Session ID Namespacing

For concurrent sessions:

```
.aiki/.session-state/
├── session-abc123/
│   ├── edited-files.log
│   └── affected-repos.log
└── session-xyz789/
    ├── edited-files.log
    └── affected-repos.log
```

**Implementation:**
```rust
pub fn session_state_file(session_id: &str, filename: &str) -> PathBuf {
    session_state_dir()
        .join(session_id)
        .join(filename)
}
```

### Auto-Cleanup

```rust
pub fn cleanup_old_sessions(max_age_hours: u64) -> Result<()> {
    let state_dir = session_state_dir()?;
    
    for entry in fs::read_dir(state_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        
        if let Ok(modified) = metadata.modified() {
            let age = SystemTime::now().duration_since(modified)?;
            if age.as_secs() > max_age_hours * 3600 {
                fs::remove_dir_all(entry.path())?;
            }
        }
    }
    
    Ok(())
}
```

---

## Built-in Helper Functions Reference

### `get_edited_files()`

Returns list of files edited in this session.

**Source:** `.aiki/.session-state/edited-files.log`

**Example:**
```yaml
- let: files = self.get_edited_files()
- if: $files contains ".ts"
  then:
    - flow: aiki/typescript-check
```

### `get_affected_repos()`

Returns unique list of repos affected in this session.

**Source:** `.aiki/.session-state/affected-repos.log` (deduplicated)

**Example:**
```yaml
- let: repos = self.get_affected_repos()
- shell: |
    for repo in $repos; do
      cd $repo && npm run build
    done
```

### `determine_repo_for_file(file_path)`

Determines which repo a file belongs to (for monorepos).

**Logic:**
- Walk up from file path looking for package.json or Cargo.toml
- Return directory containing package definition
- Return empty string if not in a repo

**Example:**
```yaml
- let: repo = self.determine_repo_for_file("packages/backend/src/api.ts")
# repo = "packages/backend"
```

### `count_build_errors()`

Counts error lines in all build-*.log files.

**Pattern:** Lines matching "error", "Error", "ERROR"

**Example:**
```yaml
- let: error_count = self.count_build_errors()
- if: $error_count > 0
  then:
    autoreply: "Build failed with $error_count errors"
```

---

## Session State File Formats

### edited-files.log

```
packages/backend/src/api.ts
packages/backend/src/db.ts
packages/frontend/src/App.tsx
```

**Format:** One file path per line

### affected-repos.log

```
packages/backend
packages/backend
packages/frontend
```

**Format:** One repo path per line (may contain duplicates, deduplicated by `get_affected_repos()`)

### loop-count.txt

```
2
```

**Format:** Single integer

### active-checks.json

```json
["a3f9b2c1", "7e4d2a89"]
```

**Format:** JSON array of check IDs

---

## Expected Timeline

**Week 2**

- Days 1-2: Session state manager, helper functions
- Days 3-4: Engine integration, cleanup
- Day 5: Testing and documentation

---

## Future Enhancements

### 1. Structured Data Support

Store JSON/YAML in session state:

```yaml
- let: data = { repo: "backend", errors: 5 }
- shell: echo '$data' > .aiki/.session-state/build-status.json

- let: status = self.read_json(".aiki/.session-state/build-status.json")
- if: $status.errors > 0
  then:
    autoreply: "Repo $status.repo has $status.errors errors"
```

### 2. Session State Query DSL

Query session state more easily:

```yaml
- let: recent_errors = session.query("build-*.log", pattern: "error")
- let: edited_count = session.count("edited-files.log")
```

### 3. Cross-Session Persistence

Optionally persist some data across sessions:

```yaml
PostResponse:
  - shell: echo "$metric" >> .aiki/.persistent/metrics.log  # Not cleaned up
```

---

## References

- [milestone-1.md](./milestone-1.md) - Milestone 1 overview
- [milestone-1.2-post-response.md](./milestone-1.2-post-response.md) - PostResponse uses session state
- [ROADMAP.md](../ROADMAP.md) - Strategic context
