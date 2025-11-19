# JJ Workspace Automation Plan

## Overview

Add automatic per-session workspace isolation by creating a JJ workspace for each agent session ID. This enables multiple AI agents to work concurrently in the same repository without interfering with each other.

## Background: What is a JJ Workspace?

A JJ workspace is a working copy backed by a single shared repository, created using `jj workspace add`. Key characteristics:

- **Shared Repository, Separate Working Copies**: Multiple working copies backed by a single repo, each with its own physical directory
- **Independent Working States**: Each workspace can be on a different commit/revision
- **Linked to Main Repository**: Each workspace has a `.jj/` directory that links back to the main repository
- **Shared Commit History**: Changes in any workspace are visible to all workspaces (they share the same commit graph)

This is similar to `git worktree` but with JJ's advantages:
- Not tied to branches (JJ uses anonymous branches by default)
- Working copy as commit (automatic snapshots)
- Can continue working with conflicts present

## Why Workspaces for Aiki?

For Aiki's multi-agent orchestration, JJ workspaces provide:

1. **Agent Isolation** - Each AI agent gets its own workspace, preventing interference
2. **Shared History** - All agents see the same repository state and can coordinate
3. **Parallel Development** - Multiple agents can work simultaneously without blocking
4. **Automatic Tracking** - Every change is automatically captured without explicit commits
5. **Conflict Resilience** - Agents can continue working even when conflicts exist

## Implementation Plan

### 1. Create `cli/src/workspace_manager.rs` module

New module to handle session → workspace mapping:

```rust
use crate::error::{AikiError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    session_id: String,
    workspace_name: String,
    workspace_path: PathBuf,
    created_at: chrono::DateTime<chrono::Utc>,
    last_activity: chrono::DateTime<chrono::Utc>,
    status: WorkspaceStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkspaceStatus {
    Active,
    Ended,
    Stale,
}

pub struct WorkspaceManager {
    repo_root: PathBuf,
    config_path: PathBuf, // ~/.aiki/workspaces.toml
}

impl WorkspaceManager {
    /// Ensure a workspace exists for the given session, creating if needed
    pub fn ensure_workspace_for_session(&self, session_id: &str) -> Result<PathBuf>;
    
    /// List all workspaces tracked by Aiki
    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceMetadata>>;
    
    /// Mark a workspace as ended (for cleanup)
    pub fn mark_workspace_ended(&self, session_id: &str) -> Result<()>;
    
    /// Update last activity timestamp for a workspace
    pub fn touch_workspace(&self, session_id: &str) -> Result<()>;
    
    /// Clean up workspaces based on cleanup policy
    pub fn cleanup_workspaces(&self, policy: CleanupPolicy) -> Result<Vec<String>>;
    
    // Private helpers
    fn load_metadata(&self) -> Result<HashMap<String, WorkspaceMetadata>>;
    fn save_metadata(&self, metadata: &HashMap<String, WorkspaceMetadata>) -> Result<()>;
    fn create_workspace(&self, session_id: &str) -> Result<PathBuf>;
}

pub enum CleanupPolicy {
    /// Remove workspaces marked as Ended
    Ended,
    /// Remove workspaces with no activity for specified duration
    Stale(chrono::Duration),
    /// Remove all non-active workspaces
    All,
}
```

**Key responsibilities:**
- Check `~/.aiki/workspaces.toml` for existing session → workspace mappings
- Create new workspace with `jj workspace add` if not found
- Track workspace metadata (creation time, last activity, status)
- Return workspace path for use in operations
- Handle cleanup based on policy

### 2. Enhance `cli/src/jj.rs` with workspace operations

Add methods to `JJWorkspace` struct:

```rust
impl JJWorkspace {
    /// Create a new workspace
    /// Wraps: jj workspace add <path> --name <name>
    pub fn workspace_add(&self, name: &str, path: &Path) -> Result<()>;
    
    /// List all workspaces in the repository
    /// Wraps: jj workspace list
    pub fn workspace_list(&self) -> Result<Vec<WorkspaceInfo>>;
    
    /// Forget a workspace (remove tracking, optionally delete files)
    /// Wraps: jj workspace forget <name>
    pub fn workspace_forget(&self, name: &str, delete_files: bool) -> Result<()>;
    
    /// Get the current workspace name
    pub fn current_workspace(&self) -> Result<String>;
}

#[derive(Debug)]
pub struct WorkspaceInfo {
    pub name: String,
    pub path: PathBuf,
    pub current: bool,
}
```

### 3. Integrate into ACP proxy (`cli/src/commands/acp.rs`)

Around lines 90-105 where `session/new` extracts `cwd`:

```rust
// Current code extracts cwd from session/new
if let Some(cwd) = params.get("cwd").and_then(|v| v.as_str()) {
    session_cwd = Some(cwd.to_string());
}

// NEW: Create/get workspace for this session
if let Some(session_id) = &session_id {
    let workspace_manager = WorkspaceManager::new(&repo_root)?;
    let workspace_path = workspace_manager.ensure_workspace_for_session(session_id)?;
    
    // Update cwd to use workspace path
    session_cwd = Some(workspace_path.to_string_lossy().to_string());
    
    // Touch workspace to update last activity
    workspace_manager.touch_workspace(session_id)?;
}
```

Also intercept `session/end` or similar messages to mark workspace as ended:

```rust
"session/end" => {
    if let Some(session_id) = &session_id {
        let workspace_manager = WorkspaceManager::new(&repo_root)?;
        workspace_manager.mark_workspace_ended(session_id)?;
    }
}
```

### 4. Update event handling (`cli/src/handlers.rs`)

Modify event handlers to:
- Accept workspace path context
- Ensure all JJ operations use workspace directory
- Update last activity timestamp on each event

```rust
pub fn handle_start(event: &AikiStartEvent) -> Result<()> {
    // Touch workspace to update activity
    if let Some(session_id) = &event.session_id {
        let workspace_manager = WorkspaceManager::new(&event.repo_root)?;
        workspace_manager.touch_workspace(session_id)?;
    }
    
    // Rest of handler logic...
}
```

Consider updating `AikiEvent` struct to carry workspace context if needed.

### 5. Add error types (`cli/src/error.rs`)

```rust
#[derive(Error, Debug)]
pub enum AikiError {
    // ... existing variants ...
    
    #[error("Failed to create workspace for session '{0}': {1}")]
    WorkspaceCreationFailed(String, String),
    
    #[error("Workspace not found for session: '{0}'")]
    WorkspaceNotFound(String),
    
    #[error("Failed to cleanup workspace '{0}': {1}")]
    WorkspaceCleanupFailed(String, String),
    
    #[error("Workspace metadata corrupted: {0}")]
    WorkspaceMetadataCorrupted(String),
}
```

### 6. Add cleanup command (`cli/src/commands/workspace.rs`)

New command module for workspace management:

```rust
pub fn run_list(repo_path: Option<PathBuf>) -> Result<()> {
    let workspace_manager = WorkspaceManager::new(&repo_path.unwrap_or_else(|| env::current_dir().unwrap()))?;
    let workspaces = workspace_manager.list_workspaces()?;
    
    for ws in workspaces {
        println!("{} {} ({})", 
            ws.session_id,
            ws.workspace_path.display(),
            ws.status);
    }
    Ok(())
}

pub fn run_cleanup(repo_path: Option<PathBuf>, policy: CleanupPolicy) -> Result<()> {
    let workspace_manager = WorkspaceManager::new(&repo_path.unwrap_or_else(|| env::current_dir().unwrap()))?;
    let cleaned = workspace_manager.cleanup_workspaces(policy)?;
    
    println!("Cleaned up {} workspace(s)", cleaned.len());
    for session_id in cleaned {
        println!("  - {}", session_id);
    }
    Ok(())
}
```

Add to CLI in `main.rs`:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...
    
    /// Manage JJ workspaces for agent sessions
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    /// List all agent session workspaces
    List {
        /// Repository path (defaults to current directory)
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    
    /// Clean up ended or stale workspaces
    Cleanup {
        /// Repository path (defaults to current directory)
        #[arg(long)]
        repo: Option<PathBuf>,
        
        /// Cleanup policy: ended, stale, all
        #[arg(long, default_value = "ended")]
        policy: String,
        
        /// For stale policy, hours of inactivity before cleanup
        #[arg(long, default_value = "24")]
        stale_hours: u64,
    },
}
```

### 7. Automatic cleanup integration

Add cleanup hooks in several places:

1. **On `aiki init`** - Clean up stale workspaces from previous sessions
2. **On ACP proxy shutdown** - Clean up ended sessions
3. **Periodic background cleanup** (optional) - Run every N hours

```rust
// In commands/init.rs
pub fn run(quiet: bool) -> Result<()> {
    // ... existing init logic ...
    
    // Clean up stale workspaces
    if !quiet {
        println!("Cleaning up stale workspaces...");
    }
    let workspace_manager = WorkspaceManager::new(&current_dir)?;
    workspace_manager.cleanup_workspaces(CleanupPolicy::Stale(chrono::Duration::hours(24)))?;
    
    Ok(())
}
```

## Key Design Decisions

### Workspace Naming and Location

- **Workspace naming**: Use session ID as workspace name for easy mapping
- **Storage location**: Adjacent to main repo (e.g., `../myproject-session-abc123`)
  - Keeps workspaces close to main repo
  - Easy to find and debug
  - Doesn't clutter main working directory

### Persistence Strategy

- **Track mappings in `~/.aiki/workspaces.toml`**
  - Global across all repositories
  - Survives repository deletion
  - Can track metadata (timestamps, status)

Example `workspaces.toml`:
```toml
[workspaces."session-abc123"]
session_id = "session-abc123"
workspace_name = "myproject-session-abc123"
workspace_path = "/Users/me/code/myproject-session-abc123"
created_at = "2025-01-19T10:30:00Z"
last_activity = "2025-01-19T11:45:00Z"
status = "Active"

[workspaces."session-def456"]
session_id = "session-def456"
workspace_name = "myproject-session-def456"
workspace_path = "/Users/me/code/myproject-session-def456"
created_at = "2025-01-18T09:00:00Z"
last_activity = "2025-01-18T09:30:00Z"
status = "Ended"
```

### Cleanup Strategy

**When to clean up:**
1. **Automatic on `aiki init`** - Clean stale workspaces (default: 24h inactivity)
2. **On session end** - Mark workspace as "Ended" (cleanup later)
3. **Manual command** - `aiki workspace cleanup` for explicit cleanup
4. **ACP proxy shutdown** - Clean ended sessions

**What "cleanup" means:**
1. Run `jj workspace forget <name>` to remove JJ tracking
2. Delete the physical workspace directory
3. Remove entry from `~/.aiki/workspaces.toml`

**Safety considerations:**
- Check for uncommitted changes before deletion (warn user)
- Provide `--force` flag to override safety checks
- Log cleanup operations for debugging
- Consider archiving workspace metadata for forensics

**Cleanup policies:**
- `Ended`: Only clean workspaces marked as ended by session/end message
- `Stale`: Clean workspaces with no activity for N hours (default: 24)
- `All`: Clean all non-active workspaces (dangerous, require confirmation)

### Shared Provenance

- **All workspaces share the same JJ repo**
  - Change descriptions are visible across all workspaces
  - Provenance metadata is global, not per-workspace
  - No need to track "which workspace made this change"

- **Workspace as implementation detail**
  - Not exposed in `[aiki]` metadata blocks
  - Users and other tools don't need to know about workspaces
  - Transparent isolation for concurrent agents

### Session End Detection

**ACP session lifecycle:**
- `session/new` - Create/get workspace
- `session/update` - Touch workspace (update last_activity)
- `session/end` - Mark workspace as Ended

**Fallback for crashed sessions:**
- If no `session/end` received, workspace becomes "Stale" after timeout
- Cleanup policy handles stale workspaces automatically
- No manual intervention required

## Testing Considerations

### Unit Tests

- `workspace_manager.rs`:
  - Test workspace creation for new sessions
  - Test workspace reuse for existing sessions
  - Test metadata loading/saving
  - Test cleanup policies

### Integration Tests

- Test concurrent sessions (multiple workspaces)
- Test workspace isolation (changes in one don't affect another)
- Test provenance visibility across workspaces
- Test cleanup (forget + delete)
- Test session end handling
- Test stale workspace detection

### Manual Testing

- Create multiple agent sessions
- Verify separate workspaces created
- Make changes in each workspace
- Verify provenance is shared
- Test cleanup commands
- Test crash recovery (kill agent, verify cleanup)

## Rollout Strategy

### Phase 1: Core Infrastructure (MVP)
- Implement `workspace_manager.rs`
- Add JJ workspace operations to `jj.rs`
- Basic error handling
- Manual cleanup command only

### Phase 2: ACP Integration
- Integrate into ACP proxy
- Automatic workspace creation on `session/new`
- Activity tracking on `session/update`
- Mark ended on `session/end`

### Phase 3: Automatic Cleanup
- Add cleanup on `aiki init`
- Add cleanup on ACP shutdown
- Stale workspace detection
- Safety checks (uncommitted changes)

### Phase 4: Polish
- Better error messages
- Logging and debugging
- Performance optimization
- Documentation and examples

## Open Questions

1. **Should we support named workspaces?**
   - Currently: workspace name = session ID
   - Alternative: Let users provide friendly names
   - Tradeoff: Simplicity vs flexibility

2. **Should cleanup be opt-in or opt-out?**
   - Current plan: Automatic cleanup with safety checks
   - Alternative: Require explicit `aiki workspace cleanup`
   - Tradeoff: Convenience vs control

3. **How to handle workspace conflicts?**
   - If workspace path already exists but not tracked
   - If session ID collision (very unlikely)
   - If JJ workspace creation fails

4. **Should we limit number of concurrent workspaces?**
   - Prevent runaway workspace creation
   - Warn user if too many active sessions
   - Suggest cleanup if threshold exceeded

5. **Archiving vs deletion?**
   - Should we archive workspace metadata for debugging?
   - Keep `.tar.gz` of workspace for forensics?
   - Retention policy for archives?

## Success Criteria

- ✅ Multiple agents can work concurrently without interference
- ✅ Workspaces are automatically created and cleaned up
- ✅ Provenance is shared across all workspaces
- ✅ No manual workspace management required
- ✅ Graceful handling of crashes and stale sessions
- ✅ Clear error messages for workspace issues
- ✅ Performance: workspace creation < 1 second
- ✅ Storage: old workspaces don't accumulate indefinitely

## Related Documentation

- `ops/phase-1.md` - Architecture overview
- `ops/ROADMAP.md` - Strategic vision
- `CLAUDE.md` - JJ terminology and best practices
- JJ documentation: https://github.com/martinvonz/jj/blob/main/docs/working-copy.md
