# Phase 1: Claude Code Provenance (Hook-Based) - Implementation Plan

## Overview

Phase 1 establishes hook-based provenance tracking exclusively for Claude Code. This phase focuses on answering the question: **"Which code changes did Claude Code make?"** with 100% accuracy.

**Scope**: Claude Code attribution via PostToolUse hooks, transparent tracking with automatic JJ integration, edit-level attribution, and provenance CLI.

**Key Architecture Decision:** Use Claude Code's native PostToolUse hooks for 100% accurate attribution. This provides perfect accuracy with dramatically simpler implementation compared to file watching approaches.

**Multi-Agent Support:** Deferred to Phase 3. Phase 1 focuses exclusively on proving the hook-based architecture with Claude Code.

**Platform Focus:** Cross-platform (hooks work on macOS, Linux, Windows).

**Key Dependencies:** Phase 0 complete (CLI infrastructure, JJ integration, repository initialization)

---

## Architecture: Hook-Based Detection Only (No SQLite)

```
┌─────────────────────────────────────────────────────────────┐
│  Claude Code edits file via Edit or Write tool              │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ↓
┌─────────────────────────────────────────────────────────────┐
│  Claude Code PostToolUse Hook (AUTOMATIC)                   │
│    ├─ Triggered immediately after Edit/Write                │
│    ├─ Receives JSON via stdin:                              │
│    │   • file_path (for context, not stored)                │
│    │   • old_string → new_string (not stored, jj has diff)  │
│    │   • session_id (grouping edits)                        │
│    │   • tool_name (Edit or Write)                          │
│    └─ Calls: aiki record-change --claude-code               │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ↓
┌─────────────────────────────────────────────────────────────┐
│  aiki record-change (CLI Subcommand)                        │
│    1. Parse JSON from stdin                                 │
│    2. Extract: session_id, tool_name, agent_type            │
│    3. Load JJ workspace via jj-lib                          │
│       → Get working copy change_id (stable identifier)      │
│    4. Build lightweight ProvenanceRecord:                   │
│       → Only metadata jj doesn't know:                      │
│         • agent=claude-code                                 │
│         • session=<session_id>                              │
│         • tool=Edit|Write                                   │
│         • confidence=High                                   │
│         • method=Hook                                       │
│         • timestamp=<ISO8601>                               │
│    5. Spawn background thread:                              │
│       → Serialize to [aiki]...[/aiki] format (~150 bytes)   │
│       → Rewrite commit description with metadata            │
│       → Change_id stays stable across rewrite               │
│    6. Return immediately (total: ~7-8ms)                    │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ↓ (commit description updated by background thread)
                        │
                        ↓ (jj operation created with metadata in description)
                        │
┌─────────────────────────────────────────────────────────────┐
│  JJ Commit Graph (Single Source of Truth)                   │
│    ├─ Commit description contains [aiki]...[/aiki] block    │
│    ├─ File paths: query via "jj log -r <id> --summary"      │
│    ├─ Diffs: query via "jj diff -r <id>"                    │
│    ├─ Change ID: stable identifier (persists across rewrites)│
│    └─ Ready for revset queries (future):                    │
│       → jj log -r 'description("agent=claude-code")'        │
│       → jj log -r 'description("session=xyz")'              │
└─────────────────────────────────────────────────────────────┘

Future: jj's native `annotate` API provides line-level attribution
```

**Why hook-based approach is optimal:**
- **100% accuracy** - Claude Code tells us exactly what happened
- **Dramatically simple** - No process detection, no file watching
- **Race-condition free** - `jj snapshot` captures exact state immediately
- **Fast enough** - ~50ms for snapshot (acceptable for edit workflow)
- **DB is source of truth** - Full provenance data in queryable database
- **JJ operations are lightweight** - Just contain provenance_id reference
- **Flexible** - Easy to add fields to DB without changing JJ format
- **Perfect timeline** - Complete history in JJ operation log
- **Fast** - 2-3 weeks implementation vs 4-6 weeks for multi-layer detection
- **Cross-platform** - Hooks work on macOS, Linux, Windows
- **Low maintenance** - No OS-specific code, no heuristics
- **Proves architecture** - Validates provenance system before expanding

---

## Milestone 1.1: Claude Code Hook Integration (Primary Detection)

**Goal**: Implement Claude Code PostToolUse hook handler for 100% accurate attribution.

### Tasks
- [ ] Create Claude Code hook configuration (`.claude/settings.json`)
- [ ] Implement `aiki record-change --claude-code` subcommand
- [ ] Parse PostToolUse JSON payload
- [ ] Extract file path, changes, session ID
- [ ] Record provenance with High confidence
- [ ] Add hook installation to `aiki init`
- [ ] Add provenance database initialization to `aiki init`
- [ ] Write unit tests for record-change handler
- [ ] Write integration tests with mock hook data

### Claude Code Hook Configuration

```json
// .claude/settings.json (created by `aiki init`)
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "aiki record-change",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

**What the hook receives (stdin JSON):**
```json
{
  "session_id": "abc123...",
  "transcript_path": "/path/to/transcript.json",
  "cwd": "/Users/dev/project",
  "hook_event_name": "PostToolUse",
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/Users/dev/project/src/auth.py",
    "old_string": "def verify_token(token):",
    "new_string": "def verify_token(token: str) -> bool:"
  },
  "tool_output": "Successfully edited /Users/dev/project/src/auth.py"
}
```

### Record Change Implementation

```rust
// cli/src/record_change.rs
// Called via: aiki record-change (subcommand, not separate binary)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::process::Command;
use chrono::Utc;

#[derive(Deserialize)]
struct HookInput {
    session_id: String,
    cwd: String,
    tool_name: String,
    tool_input: ToolInput,
}

#[derive(Deserialize)]
struct ToolInput {
    file_path: String,
    #[serde(default)]
    old_string: Option<String>,
    #[serde(default)]
    new_string: Option<String>,
}

pub fn record_change() -> Result<()> {
    // Read JSON from stdin
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin.read_to_string(&mut buffer)?;

    // Parse hook data
    let hook_data: HookInput = serde_json::from_str(&buffer)?;

    // Get commit_id (JJ auto-snapshots working copy during this command)
    let commit_id = get_working_copy_commit_id(&hook_data.cwd)?;

    // Build provenance record with commit_id
    let provenance = ProvenanceRecord {
        id: None,
        agent: AgentInfo {
            agent_type: AgentType::ClaudeCode,
            version: None,
            detected_at: Utc::now(),
            confidence: AttributionConfidence::High,
            detection_method: DetectionMethod::Hook,
        },
        file_path: PathBuf::from(&hook_data.tool_input.file_path),
        session_id: hook_data.session_id.clone(),
        tool_name: hook_data.tool_name.clone(),
        timestamp: Utc::now(),
        change_summary: Some(ChangeSummary {
            old_string: hook_data.tool_input.old_string.clone(),
            new_string: hook_data.tool_input.new_string.clone(),
        }),
        jj_commit_id: Some(commit_id),  // From snapshot
        jj_operation_id: None,          // Will be filled by op_heads watcher
    };

    // Single DB write with all data
    let db_path = PathBuf::from(&hook_data.cwd).join(".aiki/provenance/attribution.db");
    let db = ProvenanceDatabase::open(&db_path)?;
    let provenance_id = db.insert_provenance(&provenance)?;

    // Link JJ operation to DB record (async, describe the specific commit)
    link_jj_operation(&hook_data.cwd, &commit_id, provenance_id)?;

    // Return - total time ~15-25ms
    Ok(())
}

fn get_working_copy_commit_id(repo_path: &str) -> Result<String> {
    // JJ automatically snapshots the working copy when running commands
    // This command gets the commit ID and triggers the snapshot
    let output = Command::new("jj")
        .arg("log")
        .arg("-r")
        .arg("@")
        .arg("--no-graph")
        .arg("-T")
        .arg("commit_id")
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("jj log failed: {}", stderr));
    }

    let commit_id = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    Ok(commit_id)
}

fn link_jj_operation(repo_path: &str, commit_id: &str, provenance_id: i64) -> Result<()> {
    // Create lightweight description with just the provenance_id
    let description = format!("aiki:{}", provenance_id);

    // Describe the specific commit (not working copy which might have changed)
    // This can be async since the commit is already created
    Command::new("jj")
        .arg("describe")
        .arg("-r")
        .arg(commit_id)  // Specify exact commit to describe
        .arg("-m")
        .arg(&description)
        .current_dir(repo_path)
        .spawn()?; // spawn() - doesn't wait for describe to finish

    Ok(())
}
```

### Core Data Structures

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    agent_type: AgentType,
    version: Option<String>,
    detected_at: DateTime<Utc>,
    confidence: AttributionConfidence,
    detection_method: DetectionMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentType {
    ClaudeCode,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttributionConfidence {
    High,    // 100% - Hook-based detection
    Medium,  // 70-80% - lsof or directory check (Phase 3)
    Low,     // 40-60% - Heuristic (Phase 3)
    Unknown, // No detection succeeded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    Hook,                   // Claude Code PostToolUse hook
    Unknown,               // Fallback (Phase 3)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    id: Option<i64>,       // Database ID (auto-generated)
    agent: AgentInfo,
    file_path: PathBuf,
    session_id: String,
    tool_name: String,     // "Edit" or "Write"
    timestamp: DateTime<Utc>,
    change_summary: Option<ChangeSummary>,
    jj_commit_id: Option<String>,     // JJ commit ID from snapshot
    jj_operation_id: Option<String>,  // JJ operation ID from op_heads watcher
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    old_string: Option<String>,
    new_string: Option<String>,
}
```

### Hook Installation (in `aiki init`)

```rust
impl InitCommand {
    fn install_claude_code_hooks(&self) -> Result<()> {
        let settings_dir = self.repo_path.join(".claude");
        let settings_file = settings_dir.join("settings.json");

        // Create .claude directory if it doesn't exist
        fs::create_dir_all(&settings_dir)?;

        // Read existing settings or create new
        let mut settings: serde_json::Value = if settings_file.exists() {
            let content = fs::read_to_string(&settings_file)?;
            serde_json::from_str(&content)?
        } else {
            serde_json::json!({})
        };

        // Add PostToolUse hooks
        if settings.get("hooks").is_none() {
            settings["hooks"] = serde_json::json!({});
        }

        settings["hooks"]["PostToolUse"] = serde_json::json!([
            {
                "matcher": "Edit|Write",
                "hooks": [
                    {
                        "type": "command",
                        "command": "aiki record-change",
                        "timeout": 5
                    }
                ]
            }
        ]);

        // Write back
        fs::write(&settings_file, serde_json::to_string_pretty(&settings)?)?;

        println!("✓ Installed Claude Code hooks (.claude/settings.json)");

        Ok(())
    }
}
```

### Success Criteria
- ✅ Hook handler receives and parses Claude Code JSON
- ✅ JJ commit_id retrieved via `jj log -r @` (auto-snapshots during command)
- ✅ **Single DB write** with all provenance data including commit_id
- ✅ Hook completes in <25ms (get commit + DB write + spawn)
- ✅ No race conditions - JJ snapshots atomically at command start
- ✅ Provenance ID embedded in JJ operation description ("aiki:12345")
- ✅ DB contains both jj_commit_id and jj_operation_id
- ✅ Session IDs tracked for grouping edits
- ✅ Change details (old/new strings) captured in DB
- ✅ Works with both Edit and Write tools
- ✅ Graceful failure (doesn't break Claude Code if handler errors)


### Technical Notes
- Hook handler is a subcommand (`aiki record-change`) not a separate binary
- **Three-step process**:
  1. `jj log -r @ -T commit_id` (~10ms) - JJ auto-snapshots & returns commit_id
  2. Write provenance to DB with commit_id (~5-10ms) - get provenance_id
  3. `jj describe -m "aiki:{id}"` (async via `spawn()`) - links to DB record
- Returns after DB write, before describe completes (~15-25ms total)
- Exit code 0 = success (Claude continues normally)
- Exit code 2 = blocking error (Claude sees the error)
- **Single DB write** - all provenance data written once
- **Automatic snapshot** - JJ snapshots working copy during `jj log` command
- **DB contains both JJ IDs**: commit_id (from hook) and operation_id (from watcher)
- JJ operations contain lightweight reference: "aiki:12345"
- op_heads watcher updates DB records with jj_operation_id
- **Race-condition free** - JJ snapshots atomically at command start
- **Flexible schema** - easy to add fields to DB without changing JJ format

---

## Milestone 1.2: op_heads Watcher + Attribution Processing

**Goal**: Watch for JJ operations and compute line-level attribution asynchronously.

### Tasks
- [ ] Implement op_heads file watcher (`.jj/repo/op_heads/heads`)
- [ ] Read provenance_id from operation descriptions ("aiki:12345")
- [ ] **Update DB record with jj_operation_id** (link JJ op to DB)
- [ ] Extract changed files from operations
- [ ] Compute line-level diffs using JJ's diff
- [ ] Build attribution index with confidence tracking
- [ ] Write unit tests for attribution computation
- [ ] Write integration tests with real operations
- [ ] Include functionality in real claude integration test

### op_heads Watcher Implementation

```rust
pub struct OpHeadsWatcher {
    repo_path: PathBuf,
    event_tx: mpsc::Sender<OpHeadEvent>,
}

pub enum OpHeadEvent {
    OperationDetected {
        op_id: OperationId,
        provenance: ProvenanceRecord,
        changed_files: Vec<PathBuf>,
    },
}

impl OpHeadsWatcher {
    pub async fn watch(&self) -> Result<()> {
        let op_heads_path = self.repo_path
            .join(".jj")
            .join("repo")
            .join("op_heads")
            .join("heads");

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;

        watcher.watch(&op_heads_path, RecursiveMode::NonRecursive)?;

        println!("👁️  Watching op_heads for JJ operations");

        loop {
            match rx.recv() {
                Ok(event) => {
                    if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        self.handle_op_head_change().await?;
                    }
                }
                Err(_) => break,
            }
        }

        Ok(())
    }

    async fn handle_op_head_change(&self) -> Result<()> {
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        let repo = workspace.repo_loader().load_at_head()?;
        let op_id = repo.op_id().clone();

        // Read provenance_id from operation description
        let provenance_id = self.read_provenance_id(&repo, &op_id)?;

        // Update provenance record with jj_operation_id
        if let Some(prov_id) = provenance_id {
            self.db.update_jj_operation_id(prov_id, &op_id.hex())?;

            // Get the full provenance record for processing
            let provenance = self.db.get_provenance(prov_id)?;

            // Extract changed files
            let changed_files = self.extract_changed_files(&repo)?;

            println!("📝 Operation {} - {} edited {} (provenance:{})",
                op_id.hex(),
                provenance.agent.agent_type,
                provenance.file_path.display(),
                prov_id);

            // Send for attribution processing
            self.event_tx.send(OpHeadEvent::OperationDetected {
                op_id,
                provenance,
                changed_files,
            }).await?;
        } else {
            println!("📝 Operation {} - no aiki provenance", op_id.hex());
        }

        Ok(())
    }

    fn read_provenance_id(
        &self,
        repo: &ReadonlyRepo,
        op_id: &OperationId
    ) -> Result<Option<i64>> {
        let operation = repo.operation();
        let metadata = operation.metadata();
        let description = metadata.description.as_deref().unwrap_or("");

        // Check for aiki provenance format: "aiki:12345"
        if description.starts_with("aiki:") {
            let id_str = &description[5..];
            if let Ok(id) = id_str.parse::<i64>() {
                return Ok(Some(id));
            }
        }

        // No aiki provenance (manual operation or other tool)
        Ok(None)
    }
}
```

### Attribution Processing

```rust
pub struct AttributionProcessor {
    db: Connection,
    repo_path: PathBuf,
}

impl AttributionProcessor {
    pub async fn process_operation(&self, event: OpHeadEvent) -> Result<()> {
        let OpHeadEvent::OperationDetected {
            op_id,
            provenance,
            changed_files,
        } = event;

        // For each file, determine which agent edited it
        for file in changed_files {
            // Check our provenance DB for recent edits to this file
            let recent_edits = self.db.get_recent_edits_for_file(&file)?;

            // Compute line-level attribution
            self.process_file_attribution(
                &file,
                &op_id,
                &recent_edits,
            ).await?;
        }

        Ok(())
    }

    async fn process_file_attribution(
        &self,
        file: &Path,
        op_id: &OperationId,
        recent_edits: &[ProvenanceRecord],
    ) -> Result<()> {
        // Load repo and compute diff
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        let repo = workspace.repo_loader().load_at(op_id)?;

        let view = repo.view();
        let wc_commit_id = view.get_wc_commit_id(&WorkspaceId::default())?;
        let commit = repo.store().get_commit(wc_commit_id)?;

        if commit.parent_ids().is_empty() {
            return Ok(());
        }

        let parent_id = &commit.parent_ids()[0];
        let parent = repo.store().get_commit(parent_id)?;

        // Get file contents
        let current_content = self.read_file_from_tree(&commit.tree()?, file)?;
        let parent_content = self.read_file_from_tree(&parent.tree()?, file)?;

        // Compute diff
        let diff = similar::TextDiff::from_lines(&parent_content, &current_content);

        // Attribute each changed line
        for change in diff.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Insert => {
                    if let Some(line_num) = change.new_index() {
                        // Find which agent made this edit
                        let agent = self.determine_agent_for_line(
                            file,
                            line_num,
                            recent_edits,
                        )?;

                        self.update_line_attribution(
                            file,
                            line_num,
                            &agent,
                            op_id,
                        )?;
                    }
                }
                similar::ChangeTag::Delete => {
                    if let Some(line_num) = change.old_index() {
                        self.delete_line_attribution(file, line_num)?;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn determine_agent_for_line(
        &self,
        file: &Path,
        line_num: usize,
        recent_edits: &[ProvenanceRecord],
    ) -> Result<AgentInfo> {
        // Find the most recent edit to this file
        if let Some(edit) = recent_edits.first() {
            return Ok(edit.agent.clone());
        }

        // No recent edit found, default to Unknown
        Ok(AgentInfo {
            agent_type: AgentType::Unknown,
            confidence: AttributionConfidence::Unknown,
            detection_method: DetectionMethod::Unknown,
            // ...
        })
    }
}
```

### Attribution Database Schema

```sql
-- Provenance records (written by hook handler, updated by op_heads watcher)
CREATE TABLE provenance_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    agent_type TEXT NOT NULL,
    confidence TEXT NOT NULL,
    detection_method TEXT NOT NULL,
    session_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,  -- "Edit" or "Write"
    timestamp TEXT NOT NULL,
    change_summary TEXT,      -- JSON with old/new strings
    jj_commit_id TEXT,        -- JJ commit ID from snapshot (filled by hook)
    jj_operation_id TEXT UNIQUE  -- JJ operation ID (filled by op_heads watcher)
);

CREATE INDEX idx_prov_file ON provenance_records(file_path);
CREATE INDEX idx_prov_timestamp ON provenance_records(timestamp);
CREATE INDEX idx_prov_session ON provenance_records(session_id);
CREATE INDEX idx_prov_commit ON provenance_records(jj_commit_id);
CREATE INDEX idx_prov_operation ON provenance_records(jj_operation_id);

-- Line-level attribution
CREATE TABLE line_attribution (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    agent_type TEXT NOT NULL,
    confidence TEXT NOT NULL,
    detection_method TEXT NOT NULL,
    op_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    UNIQUE(file_path, line_number)
);

CREATE INDEX idx_line_file ON line_attribution(file_path);
CREATE INDEX idx_line_op ON line_attribution(op_id);
CREATE INDEX idx_line_agent ON line_attribution(agent_type);
```

### Success Criteria
- ✅ op_heads watcher detects operations instantly (<5ms)
- ✅ Reads provenance_id from JJ operation descriptions ("aiki:12345")
- ✅ **Updates DB record with jj_operation_id** (bidirectional link)
- ✅ Retrieves full provenance from DB using provenance_id
- ✅ Attributes lines to correct agents
- ✅ Confidence preserved through attribution
- ✅ Runs async without blocking
- ✅ DB stays in sync with JJ operation log

---

## Milestone 1.3: Provenance Query API & CLI

**Goal**: Provide commands to query and view provenance data.

### Commands

#### `aiki status`
```bash
$ aiki status

Repository: /Users/dev/project
Active Tracking:
  ✓ Claude Code hooks installed
  ✓ Immediate snapshots (on every edit)

Recent Activity (last hour):
  10m ago: Claude Code edited auth.py (hook) ✓✓✓
  11m ago: Claude Code edited utils.py (hook) ✓✓✓
  25m ago: Claude Code edited config.py (hook) ✓✓✓

Total edits today: 47
Total operations: 47 (one per edit)
```

#### `aiki history`
```bash
$ aiki history --limit 5

2024-01-15 10:30:15  Claude Code edit: auth.py (abc123)
  Session: session-xyz
  Confidence: ✓✓✓ High (hook)

2024-01-15 10:30:10  Claude Code edit: utils.py (def456)
  Session: session-xyz
  Confidence: ✓✓✓ High (hook)

2024-01-15 10:25:45  Claude Code edit: config.py (ghi789)
  Session: session-abc
  Confidence: ✓✓✓ High (hook)
```

#### `aiki blame <file>`
```bash
$ aiki blame auth.py

45: Claude Code ✓✓✓ (hook)     def verify_token(token: str) -> bool:
46: Claude Code ✓✓✓ (hook)         """Verify JWT token validity."""
47: Claude Code ✓✓✓ (hook)         try:
48: Claude Code ✓✓✓ (hook)             decoded = jwt.decode(token, SECRET_KEY)
49: Claude Code ✓✓✓ (hook)             return decoded.get("exp") > time.time()
```

#### `aiki stats`
```bash
$ aiki stats

Detection Statistics (last 7 days):

Claude Code (hook):  892 edits (100%) - ✓✓✓ High confidence

Sessions tracked: 47
Files modified: 213
Average edits per session: 19

Overall: 100% attribution accuracy ✓
```

### Query API
```rust
pub struct ProvenanceQuery {
    db: Connection,
}

impl ProvenanceQuery {
    pub fn get_active_tracking_status(&self) -> Result<TrackingStatus>;
    pub fn get_recent_activity(&self, hours: usize) -> Result<Vec<Activity>>;
    pub fn get_file_blame(&self, file: &Path) -> Result<Vec<LineAttribution>>;
    pub fn get_detection_stats(&self, days: usize) -> Result<DetectionStats>;
    pub fn get_operation_summary(&self, op_id: &str) -> Result<OperationSummary>;
}
```

### Success Criteria
- ✅ CLI shows Claude Code attribution clearly
- ✅ Confidence indicators visible (always High for hooks)
- ✅ Session grouping visible in output
- ✅ All commands support `--json` flag
- ✅ Queries complete in <100ms

---

## Testing Strategy

### Unit Tests
- Hook handler JSON parsing
- Provenance record creation
- Snapshot aggregation logic
- Attribution computation
- Query API functionality

### Integration Tests
```rust
#[test]
fn test_claude_hook_integration() {
    // Mock Claude Code PostToolUse JSON
    // Run hook handler
    // Verify provenance recorded with High confidence
}

#[test]
fn test_session_tracking() {
    // Create multiple edits with same session_id
    // Verify they're grouped together
    // Query session history
}

#[test]
fn test_snapshot_aggregation() {
    // Create Claude Code edits (via hook)
    // Trigger snapshot
    // Verify aggregated metadata correct
}

#[test]
fn test_attribution_accuracy() {
    // Create edits via hooks
    // Trigger snapshot and attribution
    // Query line attribution
    // Verify correct Claude Code attribution per line
}
```

### End-to-End Tests
1. **Claude Code single session workflow:**
   - Start with Claude Code running
   - Make multiple edits via Edit/Write tools
   - Verify hook captures 100% accurately
   - Check JJ snapshot has correct provenance
   - Query via `aiki blame` shows all lines attributed

2. **Multi-session workflow:**
   - Complete first Claude Code session with edits
   - Start new Claude Code session
   - Make more edits to same files
   - Verify both sessions tracked separately
   - Query stats show session distribution correctly

### Performance Tests
- Hook handler: <25ms execution (get commit + DB write + spawn)
- JJ log (auto-snapshot + get commit_id): ~10ms
- DB write: ~5-10ms (single write with all data)
- JJ describe: happens async in background
- Operation detection: <5ms from op_heads change
- DB update from watcher: <10ms per operation
- Query performance: <100ms
- Zero idle CPU usage (no background polling)

---

## Dependencies

```toml
[dependencies]
clap = { version = "4.5", features = ["derive", "cargo"] }
anyhow = "1.0"
jj-lib = "0.35.0"
toml = "0.8"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
tokio = { version = "1", features = ["full"] }
notify = "6.0"
rusqlite = "0.31"
prettytable-rs = "0.10"
colored = "2.0"
similar = "2.0"
which = "5.0"
```

### System Requirements
- Rust 1.70+
- JJ 0.35+ (via jj-lib)
- Claude Code (for hook-based attribution)
- Cross-platform (macOS, Linux, Windows)

---

## Success Metrics

### Completion Criteria
- [ ] Claude Code hook integration working (100% accuracy)
- [ ] Hook handler completes in <100ms (snapshot + spawn)
- [ ] `jj snapshot` captures exact state (no race conditions)
- [ ] `jj describe` runs async in background
- [ ] op_heads watcher populates DB from JJ operations
- [ ] Line-level attribution working
- [ ] All CLI commands functional
- [ ] Session tracking and grouping functional
- [ ] Integration tests passing
- [ ] Documentation updated

### User Experience Goals
1. Completely transparent - no commands needed
2. **100% accuracy for Claude Code**
3. Clear confidence indicators (always High for hooks)
4. Session grouping visible in CLI output
5. **Perfect timeline** - one JJ operation per edit
6. Zero configuration required (hooks auto-installed)
7. Works cross-platform (macOS, Linux, Windows)

### Technical Goals
- ✅ 100% accuracy for Claude Code (hook-based)
- ✅ Dramatically simple architecture (no process detection)
- ✅ Event-driven (no background polling)
- ✅ Fast implementation (2-3 weeks)
- ✅ Cross-platform support
- ✅ Perfect provenance timeline (one op per edit)
- ✅ Immediate feedback (no batching delay)
- ✅ Proves architecture for Phase 3 (multi-agent)

---

## Architecture Comparison

| Aspect | Original (5-layer FD) | Hook-Based (Snapshot) |
|--------|----------------------|-----------------------|
| Claude Code accuracy | 85-95% | **100%** ✓ |
| Complexity | Very High | **Minimal** ✓ |
| Hook overhead | N/A | **~50-100ms** ✓ |
| Race conditions | N/A | **None** ✓ |
| Captures exact state | No | **Yes** (`jj snapshot`) ✓ |
| Data flow | Complex (polling) | **Simple** (event-driven) ✓ |
| Source of truth | Multiple (DB + files) | **JJ operations** ✓ |
| Setup | Complex | **Auto config** ✓ |
| Maintenance | High (OS-specific) | **Very Low** ✓ |
| Development time | 4-6 weeks | **2-3 weeks** ✓ |
| Platform support | macOS only | **Cross-platform** ✓ |

**Decision: Use snapshot-based hook approach for Phase 1, defer multi-agent to Phase 3**

---

## Installation & Setup

### Installation
```bash
# Build aiki
cargo build --release

# Install binaries
cargo install --path . --bin aiki
cargo install --path . --bin aiki-hook-handler

# Initialize in repository
cd my-project
jj git init --colocate
aiki init
```

### What `aiki init` Does
1. Verifies JJ repository exists
2. Creates `.aiki/` directory
3. **Installs Claude Code hooks** (`.claude/settings.json`)
4. Creates provenance database
5. Starts op_heads watcher daemon

### Starting the System
```bash
# All daemons start automatically with `aiki init`

# Or start manually:
aiki daemon start
```

### Checking System Status
```bash
# View tracking status
aiki status

# View detection statistics
aiki stats

# View recent activity
aiki history --limit 10
```

---

## Next Phase

Upon completion of Phase 1:
- ✓ **100% accurate Claude Code attribution** (hook-based)
- ✓ Transparent tracking (no commands needed)
- ✓ **Immediate JJ snapshots** (perfect timeline, one per edit)
- ✓ Line-level attribution with High confidence
- ✓ Session tracking and grouping
- ✓ Complete provenance query CLI
- ✓ Cross-platform support
- ✓ **Proven architecture** for future expansion

This enables **Phase 2: Multi-Editor Hook Support**:
- Intelligent editor detection (Claude Code, Cursor, Windsurf)
- Automatic hook configuration based on git history analysis
- Unified provenance tracking across all AI editors
- Per-editor statistics and filtering

Followed by **Phase 3: Autonomous Review & Self-Correction Loop**:
- Review triggering via op_heads events (one per edit!)
- Background review workers
- Autonomous feedback loop with Claude Code
- Self-correction iteration tracking
- High-confidence provenance enables precise review attribution
- Perfect timeline enables detailed iteration analysis

**Key Insight:** Hook-based detection gives us perfect accuracy with dramatically simpler implementation. Immediate snapshots provide perfect timeline granularity for autonomous review. Phase 1 proves the architecture, Phase 3 extends to multi-agent scenarios.
