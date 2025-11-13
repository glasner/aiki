# Phase 1: Claude Code Provenance (Hook-Based) - Implementation Plan

## JJ Terminology Guide

**IMPORTANT**: Aiki uses Jujutsu (jj) which has different terminology from Git:

- **Change**: The atomic unit in jj (analogous to a git commit, but mutable)
- **Change ID**: Stable identifier that persists across rewrites (e.g., `28be28a352aca126`)
- **Commit ID**: Content-based hash that changes when a change is rewritten
- **Change Description**: Where we store `[aiki]` metadata (mutable field on a change)

**Throughout this codebase:**
- We use **"change"** to refer to jj's atomic unit of version control
- We use **"commit"** only when referring to Git commits or jj operation log commits
- **Change IDs are our primary identifier** (not commit IDs, which are transient)
- Metadata is stored in **change descriptions** using `[aiki]...[/aiki]` blocks

**Key distinction**: In Git, commits are immutable. In jj, changes are mutable and have stable change_ids that persist across rewrites. This is fundamental to Aiki's architecture.

---

## Overview

Phase 1 establishes hook-based provenance tracking exclusively for Claude Code. This phase focuses on answering the question: **"Which code changes did Claude Code make?"** with 100% accuracy.

**Scope**: Claude Code attribution via PostToolUse hooks, transparent tracking with automatic JJ integration, edit-level attribution, and provenance CLI.

**Key Architecture Decision:** Use Claude Code's native PostToolUse hooks for 100% accurate attribution. Metadata stored directly in JJ change descriptions - no SQLite database needed.

**Data Storage:** All provenance metadata embedded in JJ change descriptions using `[aiki]...[/aiki]` format. JJ is the single source of truth for all data (metadata, file paths, diffs, timestamps, change IDs).

**Multi-Agent Support:** Architecture proven with Claude Code. Phase 2 extends to Cursor and Windsurf using same pattern.

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
│    3. Build lightweight ProvenanceRecord:                   │
│       → Only metadata jj doesn't know:                      │
│         • agent=claude-code                                 │
│         • session=<session_id>                              │
│         • tool=Edit|Write                                   │
│         • confidence=High                                   │
│         • method=Hook                                       │
│         • timestamp=<ISO8601>                               │
│    4. Run shell command: jj describe -m "[aiki]...[/aiki]"  │
│       → Adds metadata to current working copy change        │
│    5. Run shell command: jj new                             │
│       → Creates new empty change for next edit              │
│       → Previous change now has metadata + file changes     │
│    6. Return (total: ~180-200ms, synchronous)               │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        ↓ (metadata embedded in change description)
                        │
                        ↓ (new empty change created for next edit)
                        │
┌─────────────────────────────────────────────────────────────┐
│  JJ Change Graph (Single Source of Truth)                   │
│    ├─ Change description contains [aiki]...[/aiki] block    │
│    ├─ File paths: query via "jj log -r <id> --summary"      │
│    ├─ Diffs: query via "jj diff -r <id>"                    │
│    ├─ Change ID: stable identifier (persists across rewrites)│
│    └─ Ready for revset queries (future):                    │
│       → jj log -r 'description("agent=claude-code")'        │
│       → jj log -r 'description("session=xyz")'              │
└─────────────────────────────────────────────────────────────┘

Future: jj's native `annotate` API provides line-level attribution
```

**Why this architecture is optimal:**
- **100% accuracy** - Claude Code tells us exactly what happened
- **Simplicity** - Shell commands instead of complex jj-lib API
- **One change per edit** - Each tool use gets its own dedicated change
- **Easy debugging** - Can run `jj describe` and `jj new` manually
- **Dramatically simple** - No SQLite, no process detection, no file watching
- **Single source of truth** - JJ commit graph contains all data
- **No sync issues** - Metadata can't drift from commits
- **Reasonable performance** - Hook executes in ~180-200ms (synchronous)
- **Lightweight** - Only ~120 bytes of metadata per commit
- **No duplication** - File paths, diffs, timestamps queried from JJ when needed
- **Flexible** - Can add new metadata fields without migration
- **Ready for revsets** - Future: `jj log -r 'description("agent=claude-code")'`
- **Cross-platform** - Hooks work on macOS, Linux, Windows
- **Low maintenance** - No database, no OS-specific code
- **Proven architecture** - Ready to extend to other AI agents (Phase 2)

---

## Milestone 1.1: Claude Code Hook Integration (Primary Detection)

**Goal**: Implement Claude Code PostToolUse hook handler for 100% accurate attribution.

### Tasks
- [x] Create Claude Code hook configuration (`.claude/settings.json`)
- [x] Implement `aiki record-change` command (no --claude-code flag needed)
- [x] Parse PostToolUse JSON payload
- [x] Extract session ID and tool name (file path/changes queried from JJ)
- [x] Record provenance with High confidence in JJ change descriptions
- [x] Add hook installation to `aiki init`
- [x] Implement provenance serialization to `[aiki]...[/aiki]` format
- [x] Write unit tests for record-change handler (41/41 tests passing)
- [x] Write integration tests with mock hook data

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
// Called via: aiki record-change --claude-code

use anyhow::Result;
use serde::Deserialize;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

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
}

pub fn record_change(agent_type: AgentType) -> Result<()> {
    // 1. Read JSON from stdin
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin.read_to_string(&mut buffer)?;

    // 2. Parse hook data
    let hook_data: HookInput = serde_json::from_str(&buffer)?;

    // 3. Build provenance record
    let provenance = ProvenanceRecord {
        agent: AgentInfo {
            agent_type,
            version: None,
            detected_at: chrono::Utc::now(),
            confidence: AttributionConfidence::High,
            detection_method: DetectionMethod::Hook,
        },
        session_id: hook_data.session_id.clone(),
        tool_name: hook_data.tool_name.clone(),
    };

    // 4. Convert to [aiki] description format
    let description = provenance.to_description();

    // 5. Run jj describe to add metadata to current change
    run_jj_describe(Path::new(&hook_data.cwd), &description)?;

    // 6. Run jj new to create a new change for next edit
    run_jj_new(Path::new(&hook_data.cwd))?;

    // 7. Return (total: ~180-200ms)
    Ok(())
}

fn run_jj_describe(cwd: &Path, description: &str) -> Result<()> {
    let output = Command::new("jj")
        .args(["describe", "-m", description])
        .current_dir(cwd)
        .output()?;

    if !output.status.success() {
        eprintln!("Warning: jj describe failed: {}", 
                  String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn run_jj_new(cwd: &Path) -> Result<()> {
    let output = Command::new("jj")
        .args(["new"])
        .current_dir(cwd)
        .output()?;

    if !output.status.success() {
        eprintln!("Warning: jj new failed: {}", 
                  String::from_utf8_lossy(&output.stderr));
    }
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
    Medium,  // 70-80% - Future: other detection methods
    Low,     // 40-60% - Future: heuristic detection
    Unknown, // No detection succeeded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    Hook,    // PostToolUse hook (Claude Code, Cursor, etc.)
    Unknown, // Future: other detection methods
}

/// Simplified provenance record - stores only what JJ doesn't know
/// File paths, diffs, timestamps, and change IDs come from JJ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    agent: AgentInfo,
    session_id: String,
    tool_name: String,  // "Edit" or "Write"
}
```

**Metadata format in change description:**
```
[aiki]
agent=claude-code
session=claude-session-xyz
tool=Edit
confidence=High
method=Hook
[/aiki]
```

**Querying data from JJ:**
- File paths: `jj log -r <change_id> --summary`
- Diffs: `jj diff -r <change_id>`
- Timestamp: `jj log -r <change_id> -T 'committer.timestamp()'`
- Metadata: Parse `[aiki]` block from description

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

### Success Criteria (Milestone 1.1 Complete ✅)
- ✅ Hook handler receives and parses Claude Code JSON
- ✅ JJ change_id retrieved via jj-lib (stable identifier across rewrites)
- ✅ **Change description updated** with `[aiki]...[/aiki]` metadata (~120 bytes)
- ✅ Hook completes in <10ms (actual: ~7-8ms via background threading)
- ✅ No race conditions - change description updates happen via background thread
- ✅ Provenance metadata embedded in JJ change descriptions (no separate DB)
- ✅ Session IDs tracked in metadata
- ✅ Change details (file paths, diffs) queried from JJ when needed (not stored in metadata)
- ✅ Works with both Edit and Write tools
- ✅ Graceful failure (doesn't break Claude Code if handler errors)
- ✅ All tests pass (41/41)


### Technical Notes (Milestone 1.1 Implementation)
- Hook handler is a subcommand (`aiki record-change`) not a separate binary
- **Two-step process**:
  1. Parse hook data and create ProvenanceRecord (~1-2ms)
  2. Spawn background thread to update change description with `[aiki]` block (~5-6ms async)
- No SQLite database - JJ change descriptions are single source of truth
- Metadata format: `[aiki]\nagent=...\nsession=...\ntool=...\nconfidence=...\nmethod=...\n[/aiki]`
- Size: ~120 bytes per change
- Returns immediately after spawning background thread (~7-8ms total)
- Exit code 0 = success (Claude continues normally)
- Exit code handling ensures Claude Code isn't blocked by errors
- Background thread handles change description updates asynchronously
- **Automatic snapshot** - JJ creates changes automatically when files change
- Uses jj-lib for direct JJ operations (no external binary needed)
- Provenance stored in change descriptions, queryable via JJ revsets
- File paths, diffs, timestamps retrieved from JJ APIs when needed (not stored redundantly)
- **Race-condition free** - Background thread ensures safe async updates
- **Flexible format** - Easy to extend `[aiki]` block with additional fields

---

## Milestone 1.2: Line-Level Attribution (Using JJ's Native API)

**Goal**: Implement `aiki blame` command using jj's native `annotate` API.

**Status**: ✅ **COMPLETE** - Fully implemented using JJ's FileAnnotator API with change description metadata parsing.

### Tasks
- [x] Implement `aiki blame <file>` command
- [x] Use jj-lib's `FileAnnotator` API for line-level attribution
- [x] Parse change descriptions to extract agent metadata
- [x] Cross-reference jj's blame output with provenance metadata
- [x] Display enriched blame with agent info and confidence
- [x] Write unit tests for blame parsing
- [x] Write integration tests with real commits
- [x] Include functionality in end-to-end tests

### Actual Implementation (Completed)

The implementation uses JJ's `FileAnnotator` API to get line-by-line change attribution, then parses the `[aiki]...[/aiki]` blocks from change descriptions to extract agent metadata.

**Key architecture decisions:**
1. Uses JJ's repo heads (latest changes) instead of working copy commit
2. No separate attribution database - all data comes from JJ directly
3. Parses provenance metadata on-demand from change descriptions
4. Handles human commits gracefully (shows as "Unknown" agent)

**Implementation files:**
- `cli/src/blame.rs` - Core blame implementation (180 lines)
- `cli/src/provenance.rs` - Added `from_description()` parser for `[aiki]` blocks
- `cli/src/main.rs` - Added blame CLI command and workspace detection
- `cli/src/jj.rs` - Added `git_import()` for initial Git history import
- `cli/tests/blame_tests.rs` - Integration test

**Example output:**
```bash
$ aiki blame src/provenance.rs
a288d49e (Unknown      -            -     )    1| use anyhow::{Context, Result};
7f50e063 (Unknown      -            -     )    2| use chrono::{DateTime, Utc};
7f50e063 (Unknown      -            -     )    3| use serde::{Deserialize, Serialize};
```

**Core implementation:**
```rust
pub struct BlameCommand {
    repo_path: PathBuf,
}

pub struct LineAttribution {
    pub line_number: usize,
    pub line_text: String,
    pub change_id: String,
    pub commit_id: String,
    pub agent_type: AgentType,
    pub confidence: Option<AttributionConfidence>,
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
}

impl BlameCommand {
    pub fn blame_file(&self, file_path: &Path) -> Result<Vec<LineAttribution>> {
        // Load JJ workspace
        let workspace = Workspace::load(&settings, &self.repo_path, ...)?;
        let repo = workspace.repo_loader().load_at_head()?;
        
        // CRITICAL: Search repo heads (latest changes) instead of working copy
        let heads = repo.view().heads();
        
        // Find head that contains the file
        let mut commit_to_use = None;
        for head_id in heads.iter() {
            let commit = repo.store().get_commit(head_id)?;
            let tree = commit.tree()?;
            if tree.path_value(repo_path)?.is_present() {
                commit_to_use = Some(commit);
                break;
            }
        }
        
        // Use FileAnnotator to get line-by-line attribution
        let mut file_annotator = FileAnnotator::from_commit(&commit_to_use, repo_path)?;
        file_annotator.compute(repo.as_ref(), &revset_expr)?;
        let file_annotation = file_annotator.to_annotation();
        
        // Parse provenance from change descriptions
        for (commit_id_result, line_text) in file_annotation.lines() {
            let commit = repo.store().get_commit(&commit_id)?;
            let description = commit.description(); // Gets CHANGE description
            
            // Parse [aiki]...[/aiki] block from description
            let provenance = ProvenanceRecord::from_description(description)
                .unwrap_or(None); // Treat as human commit if no metadata
            
            // Build LineAttribution...
        }
    }
}
```

### Success Criteria (All Met ✅)
- ✅ Uses JJ's native FileAnnotator API for line attribution
- ✅ Parses `[aiki]...[/aiki]` metadata from change descriptions
- ✅ Works with repo heads (change-based model) not working copy
- ✅ Handles human commits gracefully (shows "Unknown")
- ✅ Clean output formatting (no debug messages)
- ✅ Integration test verifies full workflow
- ✅ All 69 tests passing
- ✅ Git import happens during `aiki init`

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
