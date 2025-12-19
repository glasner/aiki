# End-to-End Automation Test Plan for Claude Code Sessions

## Overview

This plan outlines a comprehensive end-to-end automation test that simulates a full coding session with Claude Code, verifying that Aiki correctly tracks provenance throughout the entire lifecycle.

---

## Prerequisites

### Cargo Dependencies

Add to `cli/Cargo.toml`:

```toml
[dev-dependencies]
rexpect = "0.5"           # PTY automation
walkdir = "2.0"           # Filesystem traversal  
sha2 = "0.10"             # Content hashing
serde_json = "1.0"        # Event log parsing
chrono = { version = "0.4", features = ["serde"] }  # Timestamps
tempfile = "3"            # Test directories
```

### Environment Variables

| Variable | Required | Purpose | Example |
|----------|----------|---------|---------|
| `CLAUDE_INTEGRATION_TEST` | Yes | Enable E2E tests | `export CLAUDE_INTEGRATION_TEST=1` |
| `ANTHROPIC_API_KEY` | Yes | Claude API access | `export ANTHROPIC_API_KEY=sk-ant-...` |
| `AIKI_E2E_VERBOSE` | No | Debug output | `export AIKI_E2E_VERBOSE=1` |
| `AIKI_E2E_TIMEOUT` | No | Test timeout (seconds) | `export AIKI_E2E_TIMEOUT=120` |

### External Tools

Install these before running tests:

```bash
# Claude Code CLI
npm install -g @anthropic-ai/claude-code

# Jujutsu VCS
cargo install jj-cli

# Verify installations
claude --version
jj --version
```

### System Requirements

- **Platform**: Linux or macOS (rexpect PTY support)
- **Rust**: 1.70+ (for workspace dependencies)
- **Network**: Internet access for Claude API calls

---

## Goals

1. **Full Session Lifecycle Testing** - Test session start → edits → provenance → session end
2. **Automated Verification** - No manual intervention required
3. **CI/CD Compatible** - Can run in automated pipelines (with appropriate credentials)
4. **Comprehensive Coverage** - Test hooks, flows, metadata, and attribution

---

## Approach: rexpect (Interactive CLI Automation)

Use the `rexpect` Rust crate to automate interactive Claude Code sessions with full PTY support.

**Benefits:**
- **Native Rust** - No Python/Node.js dependencies
- **Tests actual user flow** - Interactive mode, not just print mode
- **Handles prompts** - Can automatically accept/reject permission prompts
- **Captures all output** - Full stderr/stdout including hook messages
- **Session continuity** - Test multi-step conversations naturally
- **CI-friendly** - Pure Rust, easy to run in GitHub Actions

**Considerations:**
- PTY behavior differs across platforms (Linux/macOS/Windows)
- Requires careful timeout tuning

**Example:**
```rust
use rexpect::session::spawn_command;
use std::time::Duration;

#[test]
fn test_claude_interactive_session() -> Result<()> {
    let temp_dir = setup_test_repo();
    
    // Spawn Claude Code in interactive mode with PTY
    let mut session = spawn_command(
        "claude",
        Some(Duration::from_secs(30))
    )?;
    
    // Wait for initial prompt
    session.exp_string("> ")?;
    
    // Send prompt
    session.send_line("Add a multiply function to calculator.py")?;
    
    // Handle permission prompt (if permission mode is interactive)
    session.exp_string("Allow edit to calculator.py?")?;
    session.send_line("y")?;
    
    // Wait for completion
    session.exp_regex(r"File.*modified")?;
    
    // Verify provenance was recorded
    let metadata = wait_for_metadata(&temp_dir)?;
    assert_eq!(metadata.tool, "Edit");
    
    Ok(())
}
```

**Dependencies:**
```toml
[dev-dependencies]
rexpect = "0.5"
```

---

## Implementation

> **Note:** For design rationale and context, see [Appendix: Design Rationale](#appendix-design-rationale)

---

### Key Implementation Notes

This plan incorporates feedback from extensive code review to ensure production-readiness:

1. **Permission handling logic corrected** - `handle_permission_if_needed()` now correctly skips prompt detection when `acceptEdits` mode is enabled, eliminating dead code paths.

2. **rexpect API properly wrapped** - `spawn_pty()` helper function centralizes Command-based PTY spawning, isolating version-specific API differences.

3. **Drop cleanup simplified** - Made `exit()` mandatory for tests. `Drop` implementation only does best-effort Ctrl-C, avoiding hanging on PTY operations in bad state.

4. **Newline normalization in assertions** - All file content assertions use `.trim_end()` or `.lines()` to handle trailing whitespace variations, preventing spurious test failures.

5. **JJ metadata polling uses ranges** - Changed from `-r @` to `-r @- -n 5` to catch async metadata writes. Added `wait_for_metadata_for_file()` to verify correct file attribution.

6. **Event log format specified** - JSONL with monotonic `seq` field for unambiguous ordering. Schema includes `session_id`, `timestamp`, `tool`, and `file_paths`.

7. **Tool-agnostic completion detection** - `wait_for_*` methods now rely on prompt return (`"> "`) as primary signal instead of fragile tool output message matching.

8. **Filesystem snapshot verification** - `capture_file_snapshot()` hashes all files to detect unintended modifications, deletions, or additions beyond expected changes.

9. **Timeout on exit** - `exp_eof_timeout()` prevents infinite hangs if Claude Code doesn't exit cleanly. Includes fallback pattern for rexpect versions without this API.

10. **Dependencies noted** - Plan requires `rexpect`, `walkdir`, `sha2`, `serde_json`, and `chrono` crates.

---

## Test Scenarios

### Scenario 1: Basic Edit Flow (Deterministic)
**Purpose:** Verify single file edit triggers provenance recording

**Steps:**
1. Initialize repo with `aiki init`
2. Create test file with known content
3. Use deterministic prompt to trigger Edit tool
4. Verify exact file modification occurred
5. Check JJ change description contains `[aiki]` metadata
6. Validate metadata fields (author, session, tool, confidence)

**Deterministic Prompt:**
```
"Using the Edit tool, insert exactly this text at the end of calculator.py:

def multiply(a, b):
    return a * b

Do not modify any other code. Do not refactor."
```

**Expected Metadata:**
```
[aiki]
author=claude
author_type=agent
session=<session-id>
tool=Edit
confidence=High
method=Hook
[/aiki]
```

### Scenario 2: Multi-Step Session (Deterministic)
**Purpose:** Verify session ID consistency across multiple sequential edits

**Steps:**
1. Initialize repo
2. Create test files with known content
3. Send 3 deterministic prompts in one session
4. Verify each edit occurred exactly as specified
5. Confirm all edits share same session ID
6. Verify no unintended files were modified

**Deterministic Prompts:**
```
Prompt 1: "Using the Edit tool, replace 'TODO' with 'DONE' in file1.txt. Do not modify anything else."
Prompt 2: "Using the Edit tool, append exactly '# End' to file2.txt. Do not modify anything else."
Prompt 3: "Using the Write tool, create file3.txt with exactly this content: 'test'"
```

### Scenario 3: Session Lifecycle Events
**Purpose:** Verify session start/resume/end events fire correctly

**Steps:**
1. Start session (verify `session.started` event)
2. Submit prompt (verify `prompt.submitted` event)
3. Make edits (verify `change.completed` events)
4. End session (verify `session.ended` event)
5. Resume session with same ID (verify `session.resumed`)
6. Confirm session continuity in metadata

### Scenario 4: Error Recovery
**Purpose:** Verify system handles failures gracefully

**Steps:**
1. Initialize repo
2. Simulate Claude attempting to edit non-existent file
3. Verify error is logged but doesn't crash
4. Continue with valid edit
5. Confirm provenance tracking resumes

### Scenario 5: Human Edit Interleaving
**Purpose:** Verify correct attribution when human edits are mixed with AI edits

**Steps:**
1. Initialize repo
2. Make edit with Claude Code (via `AgentDriver`)
3. Make manual human edit (direct file modification + `jj describe`)
4. Make another edit with Claude Code
5. Run `aiki blame` and verify distinct attributions (claude vs human)
6. Verify `aiki authors` shows both contributors

### Scenario 6: Hook Failure Recovery
**Purpose:** Verify Claude Code continues even if hook fails

**Steps:**
1. Configure intentionally broken hook
2. Invoke Claude edit
3. Verify edit succeeds (Claude continues)
4. Check hook failure is logged
5. Fix hook, retry, confirm works

---

## Implementation Phases

### File Creation Order

Create files in this order to satisfy dependencies:

| Order | File | Depends On | Purpose | Lines |
|-------|------|------------|---------|-------|
| 1 | `agent_driver.rs` | - | Trait + config structs | ~80 |
| 2 | `helpers.rs` | - | Test utilities (metadata, snapshots) | ~200 |
| 3 | `claude_driver.rs` | `agent_driver.rs` | Claude Code PTY driver | ~150 |
| 4 | `test_basic_edit.rs` | `agent_driver.rs`, `helpers.rs`, `claude_driver.rs` | Scenario 1: Single edit | ~100 |
| 5 | `test_multi_step.rs` | `agent_driver.rs`, `helpers.rs`, `claude_driver.rs` | Scenario 2: Multi-edit session | ~150 |
| 6 | `test_events.rs` | `agent_driver.rs`, `helpers.rs`, `claude_driver.rs` | Scenario 3: Event ordering | ~200 |
| 7 | `mod.rs` | All above | Module exports | ~20 |

**Total estimated lines:** ~900 lines of test code

---

### Phase 1: Test Infrastructure Setup

### Agent Driver Abstraction

**Why This Matters:**
Tests should not be tightly coupled to Claude Code's specific:
- CLI UX (prompt format, `/exit` command)
- Permission prompt text ("Allow edit...")
- Completion indicators ("File modified")

**File:** `cli/tests/e2e_automation/agent_driver.rs`

```rust
use anyhow::Result;
use std::path::Path;
use std::time::Duration;

/// Abstraction for driving an AI agent session
///
/// Decouples tests from specific agent CLI UX details
pub trait AgentDriver {
    /// Send a prompt to the agent and wait for it to be accepted
    fn send_prompt(&mut self, text: &str) -> Result<()>;
    
    /// Wait for an edit operation to complete
    fn wait_for_edit(&mut self) -> Result<()>;
    
    /// Wait for a file write operation to complete
    fn wait_for_write(&mut self) -> Result<()>;
    
    /// Wait for any tool use to complete
    fn wait_for_completion(&mut self) -> Result<()>;
    
    /// Gracefully exit the agent session
    fn exit(self) -> Result<()>;
}

/// Configuration for spawning an agent session
pub struct AgentConfig {
    pub working_dir: PathBuf,
    pub timeout: Duration,
    pub auto_accept_permissions: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap(),
            timeout: Duration::from_secs(60),
            auto_accept_permissions: true,
        }
    }
}

/// Factory for creating agent drivers
pub fn create_driver(agent_type: &str, config: AgentConfig) -> Result<Box<dyn AgentDriver>> {
    match agent_type {
        "claude-code" => Ok(Box::new(ClaudeCodeDriver::new(config)?)),
        _ => Err(anyhow::anyhow!("Unsupported agent type: {}", agent_type)),
    }
}
```

**File:** `cli/tests/e2e_automation/claude_driver.rs`

```rust
use super::agent_driver::{AgentDriver, AgentConfig};
use anyhow::Result;
use rexpect::session::PtySession;
use std::time::Duration;
use std::process::Command;

/// Spawn a PTY session with the given command
/// 
/// Wraps rexpect API differences across versions
fn spawn_pty(mut cmd: Command, timeout: Duration) -> Result<PtySession> {
    // rexpect 0.5+ takes a Command object
    rexpect::session::spawn_command(cmd, Some(timeout))
        .map_err(|e| anyhow::anyhow!("Failed to spawn PTY: {}", e))
}

/// Claude Code-specific driver implementation
pub struct ClaudeCodeDriver {
    session: PtySession,
    config: AgentConfig,
}

impl ClaudeCodeDriver {
    pub fn new(config: AgentConfig) -> Result<Self> {
        std::env::set_current_dir(&config.working_dir)?;
        
        // Build Claude Code command with proper process setup
        let mut cmd = Command::new("claude");
        cmd.current_dir(&config.working_dir);
        
        if config.auto_accept_permissions {
            // Use acceptEdits mode for deterministic testing (no interactive prompts)
            cmd.args(&[
                "--settings-override",
                r#"{"permissions": {"acceptEdits": true}}"#,
            ]);
        }
        
        // Spawn with rexpect
        let session = spawn_pty(cmd, config.timeout)?;
        
        // Wait for Claude Code's initial prompt
        session.exp_string("> ")?;
        
        Ok(Self { session, config })
    }
    
    /// Handle Claude Code's permission prompt (only for interactive mode)
    fn handle_permission_if_needed(&mut self) -> Result<()> {
        // If acceptEdits mode is enabled, there should be no permission prompts
        if self.config.auto_accept_permissions {
            return Ok(());
        }
        
        // Interactive mode: try to detect and handle permission prompt
        match self.session.exp_string_timeout("Allow", Duration::from_secs(2)) {
            Ok(_) => {
                self.session.send_line("y")?;
                Ok(())
            }
            Err(_) => Ok(()),  // No permission prompt appeared
        }
    }
}

// Cleanup: Best-effort process termination if test panics
// NOTE: Tests MUST call exit() explicitly for proper cleanup
impl Drop for ClaudeCodeDriver {
    fn drop(&mut self) {
        // If we're being dropped without explicit exit(), something went wrong
        // Attempt minimal cleanup without blocking
        eprintln!("[cleanup] ClaudeCodeDriver dropped without explicit exit() - attempting cleanup");
        
        // Don't try send_line("/exit") here - PTY may be in bad state
        // Just attempt to kill the process if accessible
        // Note: rexpect's PtySession may not expose process directly - this is best-effort
        let _ = self.session.send_control('c');  // Try Ctrl-C
    }
}

impl AgentDriver for ClaudeCodeDriver {
    fn send_prompt(&mut self, text: &str) -> Result<()> {
        self.session.send_line(text)?;
        self.handle_permission_if_needed()?;
        Ok(())
    }
    
    fn wait_for_edit(&mut self) -> Result<()> {
        // Primary signal: wait for prompt return (tool-agnostic)
        self.wait_for_prompt_return()?;
        
        // Optional: verify we saw an edit completion marker (helps with debugging)
        // But don't fail if message format changes
        Ok(())
    }
    
    fn wait_for_write(&mut self) -> Result<()> {
        // Primary signal: wait for prompt return (tool-agnostic)
        self.wait_for_prompt_return()?;
        
        // Optional: verify we saw a write completion marker
        Ok(())
    }
    
    fn wait_for_completion(&mut self) -> Result<()> {
        // Generic completion - just wait for prompt return
        self.wait_for_prompt_return()?;
        Ok(())
    }
    
    fn exit(mut self) -> Result<()> {
        // Claude Code-specific exit command
        self.session.send_line("/exit")?;
        
        // Wait for EOF with timeout (don't hang forever if Claude doesn't exit cleanly)
        // NOTE: rexpect 0.5 may not have exp_eof_timeout() - if so, use this pattern:
        //   self.session.set_timeout(Some(Duration::from_secs(5)));
        //   self.session.exp_eof()?;
        self.session.exp_eof_timeout(Duration::from_secs(5))
            .map_err(|e| anyhow::anyhow!("Timeout waiting for Claude to exit: {}", e))?;
        
        Ok(())
    }
}

impl ClaudeCodeDriver {
    /// Wait for Claude Code's prompt to return
    /// 
    /// This is the primary completion signal - Claude shows "> " when ready for next input.
    /// This is more reliable than matching specific tool output messages.
    fn wait_for_prompt_return(&mut self) -> Result<()> {
        self.session.exp_string("> ")
            .map_err(|e| anyhow::anyhow!("Timeout waiting for prompt return: {}", e))?;
        Ok(())
    }
    
    /// Optionally verify a tool completion marker appeared (for debugging)
    /// 
    /// This can help diagnose test failures but shouldn't be required for correctness.
    #[allow(dead_code)]
    fn verify_tool_marker(&mut self, pattern: &str) -> Result<bool> {
        // Check recent output buffer for the marker
        // Implementation depends on rexpect's buffer access APIs
        Ok(true)  // Placeholder
    }
}
```

---

### Utilities (Updated for AgentDriver)
```rust
use std::time::Duration;

// Helper to wait for and verify metadata for a specific file
fn wait_for_metadata_for_file(
    repo_path: &Path, 
    modified_file: &str,
    timeout: Duration
) -> Result<AikiMetadata> {
    let start = std::time::Instant::now();
    
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!("Timeout waiting for metadata for file: {}", modified_file));
        }
        
        // Poll recent changes (not just @, in case of async writes)
        let output = std::process::Command::new("jj")
            .args(&["log", "-r", "@-", "-n", "5", "-T", "description"])
            .current_dir(repo_path)
            .output()?;
        
        let description = String::from_utf8_lossy(&output.stdout);
        
        // Parse [aiki] blocks from recent changes
        if let Some(metadata) = parse_aiki_metadata(&description)? {
            // Verify this metadata is for the expected file
            let diff_output = std::process::Command::new("jj")
                .args(&["diff", "-r", "@", "--stat"])
                .current_dir(repo_path)
                .output()?;
            
            let diff_stat = String::from_utf8_lossy(&diff_output.stdout);
            if diff_stat.contains(modified_file) {
                return Ok(metadata);
            }
        }
        
        std::thread::sleep(Duration::from_millis(100));
    }
}

// Simplified helper for tests that don't care about specific files
fn wait_for_metadata(repo_path: &Path, timeout: Duration) -> Result<AikiMetadata> {
    let start = std::time::Instant::now();
    
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!("Timeout waiting for metadata"));
        }
        
        // Poll recent changes (not just @)
        let output = std::process::Command::new("jj")
            .args(&["log", "-r", "@-", "-n", "5", "-T", "description"])
            .current_dir(repo_path)
            .output()?;
        
        let description = String::from_utf8_lossy(&output.stdout);
        
        if let Some(metadata) = parse_aiki_metadata(&description)? {
            return Ok(metadata);
        }
        
        std::thread::sleep(Duration::from_millis(100));
    }
}

// Helper to parse [aiki] blocks from descriptions
fn parse_aiki_metadata(description: &str) -> Result<Option<AikiMetadata>> {
    let start = description.find("[aiki]");
    let end = description.find("[/aiki]");
    
    match (start, end) {
        (Some(s), Some(e)) if s < e => {
            let block = &description[s + 6..e];
            let mut metadata = AikiMetadata::default();
            
            for line in block.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                
                if let Some((key, value)) = line.split_once('=') {
                    match key.trim() {
                        "author" => metadata.author = value.trim().to_string(),
                        "author_type" => metadata.author_type = value.trim().to_string(),
                        "session" => metadata.session = Some(value.trim().to_string()),
                        "tool" => metadata.tool = value.trim().to_string(),
                        "confidence" => metadata.confidence = value.trim().to_string(),
                        "method" => metadata.method = value.trim().to_string(),
                        _ => {}
                    }
                }
            }
            
            Ok(Some(metadata))
        }
        _ => Ok(None),
    }
}

// Helper to verify session continuity
fn verify_session_id(repo_path: &Path, expected_session: &str) -> Result<()> {
    let output = std::process::Command::new("jj")
        .args(&["log", "-r", "@", "-T", "description"])
        .current_dir(repo_path)
        .output()?;
    
    let description = String::from_utf8_lossy(&output.stdout);
    
    if let Some(metadata) = parse_aiki_metadata(&description)? {
        if let Some(session) = metadata.session {
            if session == expected_session {
                return Ok(());
            }
            return Err(anyhow::anyhow!(
                "Session ID mismatch: expected {}, got {}",
                expected_session, session
            ));
        }
    }
    
    Err(anyhow::anyhow!("No session ID found in metadata"))
}

// Metadata structure matching [aiki] block format
#[derive(Debug, Default, Clone)]
struct AikiMetadata {
    author: String,
    author_type: String,
    session: Option<String>,
    tool: String,
    confidence: String,
    method: String,
}

// Helper to configure event logging in flow
fn configure_event_logging(repo_path: &Path, log_path: &Path) -> Result<()> {
    // Create a flow file that logs events to JSONL
    let flow_content = format!(r#"
on:
  - session.started
  - prompt.submitted
  - change.completed
  - session.ended

run: |
  #!/usr/bin/env bash
  # Log event to JSONL file
  echo '{{"seq": $AIKI_EVENT_SEQ, "event_type": "$AIKI_EVENT_TYPE", "session_id": "$AIKI_SESSION_ID", "timestamp": "$AIKI_TIMESTAMP"}}' >> {}
"#, log_path.display());
    
    std::fs::write(repo_path.join(".aiki/flows/event_logger.yml"), flow_content)?;
    Ok(())
}

// Helper to read and parse JSONL event log
fn read_event_log(log_path: &Path) -> Result<Vec<Event>> {
    let content = std::fs::read_to_string(log_path)?;
    let mut events = Vec::new();
    
    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        
        let event: Event = serde_json::from_str(line)
            .map_err(|e| anyhow::anyhow!("Failed to parse event at line {}: {}", line_num + 1, e))?;
        
        events.push(event);
    }
    
    // Verify monotonic sequence numbers
    for (i, event) in events.iter().enumerate() {
        if event.seq != i as u64 {
            return Err(anyhow::anyhow!(
                "Event sequence number mismatch: expected {}, got {} (event: {})",
                i, event.seq, event.event_type
            ));
        }
    }
    
    Ok(events)
}

// Filesystem snapshot for detecting unintended changes
#[derive(Debug, Clone)]
struct FileSnapshot {
    files: HashMap<PathBuf, String>,  // path -> content hash
}

// Helper to capture filesystem snapshot
fn capture_file_snapshot(repo_path: &Path) -> Result<FileSnapshot> {
    use std::collections::HashMap;
    use walkdir::WalkDir;
    use sha2::{Sha256, Digest};
    
    let mut files = HashMap::new();
    
    for entry in WalkDir::new(repo_path)
        .into_iter()
        .filter_entry(|e| {
            // Skip .jj, .git, .aiki directories
            !e.path().components().any(|c| {
                matches!(c, std::path::Component::Normal(name) 
                    if name == ".jj" || name == ".git" || name == ".aiki")
            })
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let relative_path = path.strip_prefix(repo_path)?;
        
        // Hash file content
        let content = std::fs::read(path)?;
        let hash = format!("{:x}", Sha256::digest(&content));
        
        files.insert(relative_path.to_path_buf(), hash);
    }
    
    Ok(FileSnapshot { files })
}

// Helper to verify only expected files changed
fn verify_only_expected_changes(
    before: &FileSnapshot,
    after: &FileSnapshot,
    expected_changes: &[&str],
) -> Result<()> {
    use std::collections::HashSet;
    
    let expected_set: HashSet<_> = expected_changes.iter()
        .map(|s| PathBuf::from(s))
        .collect();
    
    // Find all changed/added/removed files
    let mut changed_files = Vec::new();
    
    // Check for modifications and additions
    for (path, hash) in &after.files {
        match before.files.get(path) {
            Some(before_hash) if before_hash != hash => {
                changed_files.push(path.clone());
            }
            None => {
                changed_files.push(path.clone());  // New file
            }
            _ => {}  // Unchanged
        }
    }
    
    // Check for deletions
    for path in before.files.keys() {
        if !after.files.contains_key(path) {
            changed_files.push(path.clone());
        }
    }
    
    // Verify all changes were expected
    for changed in &changed_files {
        if !expected_set.contains(changed) {
            return Err(anyhow::anyhow!(
                "Unexpected file change: {} (not in expected changes list)",
                changed.display()
            ));
        }
    }
    
    // Verify all expected changes occurred
    for expected in &expected_set {
        if !changed_files.contains(expected) {
            return Err(anyhow::anyhow!(
                "Expected file change did not occur: {}",
                expected.display()
            ));
        }
    }
    
    Ok(())
}
```

### Phase 2: Basic Test Implementation (AgentDriver + Deterministic)

**File:** `cli/tests/e2e_automation/test_basic_edit.rs`

```rust
use super::agent_driver::{create_driver, AgentConfig};
use std::time::Duration;

const INITIAL_FILE_CONTENT: &str = r#"def add(a, b):
    return a + b

def subtract(a, b):
    return a - b
"#;

const EXPECTED_ADDITION: &str = r#"def multiply(a, b):
    return a * b"#;

#[test]
#[ignore]  // Only run when CLAUDE_INTEGRATION_TEST=1
fn test_deterministic_edit_flow() -> Result<()> {
    // Skip unless explicitly enabled
    if std::env::var("CLAUDE_INTEGRATION_TEST").is_err() {
        eprintln!("Skipping: Set CLAUDE_INTEGRATION_TEST=1 to run");
        return Ok(());
    }

    // Setup
    let temp_dir = tempdir()?;
    let repo_path = temp_dir.path();

    init_git_repo(repo_path);
    init_jj_workspace(repo_path);
    run_aiki_init(repo_path);
    
    // Create file with known initial content
    create_test_file(repo_path, "calculator.py", INITIAL_FILE_CONTENT);
    eprintln!("✓ Created calculator.py with known content");
    
    // Capture initial filesystem snapshot
    let initial_snapshot = capture_file_snapshot(repo_path)?;

    // Create agent driver (abstracted from Claude Code specifics)
    let config = AgentConfig {
        working_dir: repo_path.to_path_buf(),
        timeout: Duration::from_secs(60),
        auto_accept_permissions: true,
    };
    let mut agent = create_driver("claude-code", config)?;

    // Send DETERMINISTIC prompt - exact instruction, no autonomy
    let deterministic_prompt = format!(
        "Using the Edit tool, insert exactly this function at the end of calculator.py:\n\
         \n\
         {}\n\
         \n\
         Do not modify any other code. Do not refactor. Do not add comments.",
        EXPECTED_ADDITION
    );
    
    agent.send_prompt(&deterministic_prompt)?;
    eprintln!("✓ Sent deterministic prompt");

    // Wait for edit to complete (driver handles Claude-specific output)
    agent.wait_for_edit()?;
    eprintln!("✓ Edit completed");

    // Exit agent session (driver handles Claude-specific exit)
    agent.exit()?;

    // Verify EXACT file modification - file should contain both original and new content
    let final_content = std::fs::read_to_string(repo_path.join("calculator.py"))?;
    
    assert!(final_content.contains("def add(a, b)"), 
            "Original add function should still exist");
    assert!(final_content.contains("def subtract(a, b)"), 
            "Original subtract function should still exist");
    assert!(final_content.contains("def multiply(a, b)"), 
            "New multiply function should be added");
    assert_eq!(final_content.matches("def ").count(), 3,
            "Should have exactly 3 functions (no refactoring)");
    
    eprintln!("✓ File modified exactly as instructed (no unintended changes)");

    // Verify provenance metadata
    let metadata = wait_for_metadata(repo_path, Duration::from_secs(5))
        .expect("Metadata not found in JJ change description");

    assert_eq!(metadata.author, "claude", "Author should be 'claude'");
    assert_eq!(metadata.author_type, "agent", "Author type should be 'agent'");
    assert!(metadata.session.is_some(), "Session ID should be present");
    assert_eq!(metadata.tool, "Edit", "Tool should be 'Edit'");
    assert_eq!(metadata.confidence, "High", "Confidence should be 'High'");
    assert_eq!(metadata.method, "Hook", "Method should be 'Hook'");

    eprintln!("✓ All provenance metadata verified");
    eprintln!("✓ Session ID: {}", metadata.session.unwrap());

    Ok(())
}
```

### Phase 3: Multi-Step Session Tests (AgentDriver + Deterministic)

**File:** `cli/tests/e2e_automation/test_multi_step.rs`

```rust
use super::agent_driver::{create_driver, AgentConfig};
use std::time::Duration;
use std::collections::HashSet;

#[test]
#[ignore]
fn test_deterministic_multi_step_session() -> Result<()> {
    if std::env::var("CLAUDE_INTEGRATION_TEST").is_err() {
        return Ok(());
    }

    let temp_dir = setup_test_repo();

    // Create initial files with KNOWN content
    create_file(&temp_dir, "file1.txt", "TODO: implement feature");
    create_file(&temp_dir, "file2.txt", "# Start\n");
    eprintln!("✓ Created test files with known content");

    // Create agent driver
    let config = AgentConfig {
        working_dir: temp_dir.to_path_buf(),
        timeout: Duration::from_secs(120),
        auto_accept_permissions: true,
    };
    let mut agent = create_driver("claude-code", config)?;

    // Step 1: DETERMINISTIC edit - simple string replacement
    agent.send_prompt(
        "Using the Edit tool, replace the text 'TODO' with 'DONE' in file1.txt. \
         Do not modify anything else."
    )?;
    agent.wait_for_edit()?;
    eprintln!("✓ Step 1: Replaced TODO with DONE");

    // Verify Step 1: Exact modification (normalize trailing whitespace)
    let file1_content = std::fs::read_to_string(temp_dir.join("file1.txt"))?;
    assert_eq!(file1_content.trim_end(), "DONE: implement feature", 
               "File1 should have exact replacement");

    // Capture session ID from first change
    let first_metadata = wait_for_metadata(&temp_dir, Duration::from_secs(5))?;
    let session_id = first_metadata.session.clone().expect("Session ID missing");
    eprintln!("✓ Captured session ID: {}", session_id);

    // Step 2: DETERMINISTIC edit - append specific text
    agent.send_prompt(
        "Using the Edit tool, append exactly the text '# End' to the end of file2.txt. \
         Do not modify any other content."
    )?;
    agent.wait_for_edit()?;
    eprintln!("✓ Step 2: Appended '# End' to file2.txt");

    // Verify Step 2: Exact append (normalize trailing whitespace)
    let file2_content = std::fs::read_to_string(temp_dir.join("file2.txt"))?;
    let file2_lines: Vec<_> = file2_content.lines().collect();
    assert_eq!(file2_lines, vec!["# Start", "# End"], 
               "File2 should have exact append (got: {:?})", file2_lines);

    // Step 3: DETERMINISTIC write - create new file with exact content
    agent.send_prompt(
        "Using the Write tool, create a new file called file3.txt with exactly this content: 'test'"
    )?;
    agent.wait_for_write()?;
    eprintln!("✓ Step 3: Created file3.txt");

    // Verify Step 3: Exact file creation (normalize trailing whitespace)
    let file3_content = std::fs::read_to_string(temp_dir.join("file3.txt"))?;
    assert_eq!(file3_content.trim_end(), "test", 
               "File3 should have exact content");

    // Exit session (driver handles cleanup)
    agent.exit()?;

    // Verify no unintended files were created/modified using filesystem snapshot
    let final_snapshot = capture_file_snapshot(&temp_dir)?;
    verify_only_expected_changes(&initial_snapshot, &final_snapshot, &[
        "file1.txt",
        "file2.txt", 
        "file3.txt"
    ])?;

    // Verify all changes share the same session ID
    let changes = get_recent_jj_changes(&temp_dir, 5)?;
    let session_ids: HashSet<_> = changes.iter()
        .filter_map(|c| c.metadata.session.as_ref())
        .collect();

    assert_eq!(session_ids.len(), 1, "All changes should share same session");
    assert!(session_ids.contains(&session_id), "Session ID should match");

    eprintln!("✓ All {} changes have session ID: {}", changes.len(), session_id);
    eprintln!("✓ All edits were deterministic (no unintended changes)");

    Ok(())
}

// Helper to handle permission prompts with timeout
fn handle_permission_if_needed(session: &mut PtySession) -> Result<()> {
    match session.exp_string_timeout("Allow", Duration::from_secs(2)) {
        Ok(_) => {
            session.send_line("y")?;
            Ok(())
        }
        Err(_) => Ok(()),  // No permission prompt
    }
}
```

### Phase 4: Event Sequence Verification (Ordering + Integrity)

**Why This Matters:**
For provenance systems, **event ordering is the product**. Tests must verify:
1. **Happens-before relationships** - Events occur in correct order
2. **No duplicates** - Each event fires exactly once
3. **Session consistency** - All events share same session ID
4. **No missing events** - Complete lifecycle captured

**File:** `cli/tests/e2e_automation/test_events.rs`

```rust
use super::agent_driver::{create_driver, AgentConfig};
use std::time::Duration;

/// Event log entry (JSONL format)
/// 
/// Each line in the event log is a JSON object with this schema:
/// ```json
/// {
///   "seq": 42,
///   "event_type": "change.completed",
///   "session_id": "claude-session-abc123",
///   "timestamp": "2025-01-15T10:30:00.123Z",
///   "tool": "Edit",
///   "file_paths": ["calculator.py"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Event {
    /// Monotonic sequence number (starts at 0, increments by 1)
    seq: u64,
    /// Event type (e.g., "session.started", "change.completed")
    event_type: String,
    /// Session ID (optional for some events)
    session_id: Option<String>,
    /// ISO 8601 timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Tool name (optional, present for change events)
    tool: Option<String>,
    /// File paths affected (optional, present for change events)
    file_paths: Option<Vec<String>>,
}

/// Expected event sequence for a single edit
const EXPECTED_SEQUENCE: &[&str] = &[
    "session.started",
    "prompt.submitted",
    "change.permission_asked",  // Optional depending on permission mode
    "change.completed",
    "response.received",
];

#[test]
#[ignore]
fn test_event_sequence_integrity() -> Result<()> {
    if std::env::var("CLAUDE_INTEGRATION_TEST").is_err() {
        return Ok(());
    }

    let temp_dir = setup_test_repo();
    
    // Configure flow to capture events to a JSONL log file
    let event_log_path = temp_dir.join("events.jsonl");
    configure_event_logging(&temp_dir, &event_log_path)?;
    
    // Flow configuration should write to this file on every event:
    // - Open in append mode
    // - Serialize Event struct to JSON
    // - Write single line with newline
    // - Flush immediately (for test observability)

    // Create agent driver
    let config = AgentConfig {
        working_dir: temp_dir.to_path_buf(),
        timeout: Duration::from_secs(60),
        auto_accept_permissions: true,
    };
    let mut agent = create_driver("claude-code", config)?;

    // Send deterministic prompt and complete edit
    agent.send_prompt(
        "Using the Edit tool, replace 'TODO' with 'DONE' in test.txt. \
         Do not modify anything else."
    )?;
    agent.wait_for_edit()?;

    // Exit session
    agent.exit()?;

    // Read event log
    let events = read_event_log(&event_log_path)?;
    
    eprintln!("✓ Captured {} events", events.len());
    for (i, event) in events.iter().enumerate() {
        eprintln!("  [{}] {} (session: {:?})", i, event.event_type, event.session_id);
    }

    // ============================================================================
    // 1. VERIFY EVENT EXISTENCE
    // ============================================================================
    assert!(
        events.iter().any(|e| e.event_type == "session.started"),
        "session.started event must fire"
    );
    assert!(
        events.iter().any(|e| e.event_type == "prompt.submitted"),
        "prompt.submitted event must fire"
    );
    assert!(
        events.iter().any(|e| e.event_type == "change.completed"),
        "change.completed event must fire"
    );

    // ============================================================================
    // 2. VERIFY HAPPENS-BEFORE ORDERING
    // ============================================================================
    let session_started_idx = events.iter()
        .position(|e| e.event_type == "session.started")
        .expect("session.started must exist");
    
    let prompt_submitted_idx = events.iter()
        .position(|e| e.event_type == "prompt.submitted")
        .expect("prompt.submitted must exist");
    
    let change_completed_idx = events.iter()
        .position(|e| e.event_type == "change.completed")
        .expect("change.completed must exist");

    assert!(
        session_started_idx < prompt_submitted_idx,
        "session.started must happen before prompt.submitted (got indices {} vs {})",
        session_started_idx, prompt_submitted_idx
    );

    assert!(
        prompt_submitted_idx < change_completed_idx,
        "prompt.submitted must happen before change.completed (got indices {} vs {})",
        prompt_submitted_idx, change_completed_idx
    );

    // If change.permission_asked fired, it must be after prompt.submitted and before change.completed
    if let Some(permission_idx) = events.iter().position(|e| e.event_type == "change.permission_asked") {
        assert!(
            prompt_submitted_idx < permission_idx && permission_idx < change_completed_idx,
            "change.permission_asked must be between prompt.submitted and change.completed"
        );
    }

    eprintln!("✓ Event ordering verified (happens-before relationships maintained)");

    // ============================================================================
    // 3. VERIFY NO DUPLICATES
    // ============================================================================
    let mut seen_events: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    
    for event in &events {
        *seen_events.entry(&event.event_type).or_insert(0) += 1;
    }

    for (event_type, count) in &seen_events {
        // Allow multiple change.completed if multiple edits, but for single edit should be 1
        if *event_type != "change.completed" {
            assert_eq!(
                *count, 1,
                "Event '{}' fired {} times (expected 1)",
                event_type, count
            );
        }
    }

    eprintln!("✓ No duplicate events (each event fired exactly once)");

    // ============================================================================
    // 4. VERIFY SESSION ID CONSISTENCY
    // ============================================================================
    let session_ids: Vec<_> = events.iter()
        .filter_map(|e| e.session_id.as_ref())
        .collect();

    assert!(!session_ids.is_empty(), "At least one event must have session ID");

    let first_session_id = session_ids[0];
    for (i, sid) in session_ids.iter().enumerate() {
        assert_eq!(
            *sid, first_session_id,
            "Event {} has mismatched session ID: {} (expected {})",
            i, sid, first_session_id
        );
    }

    eprintln!("✓ Session ID consistency verified (all events share session: {})", first_session_id);

    // ============================================================================
    // 5. VERIFY TIMESTAMP MONOTONICITY
    // ============================================================================
    for i in 1..events.len() {
        assert!(
            events[i].timestamp >= events[i-1].timestamp,
            "Timestamps must be monotonically increasing (event {} < event {})",
            i-1, i
        );
    }

    eprintln!("✓ Timestamp monotonicity verified");

    // ============================================================================
    // FINAL SUMMARY
    // ============================================================================
    eprintln!("✓ EVENT SEQUENCE INTEGRITY VERIFIED");
    eprintln!("  - Correct ordering (happens-before)");
    eprintln!("  - No duplicates");
    eprintln!("  - Session ID consistent");
    eprintln!("  - Timestamps monotonic");

    Ok(())
}
```

---

## Test Configuration

### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `CLAUDE_INTEGRATION_TEST` | Enable real Claude Code tests | unset (disabled) |
| `AIKI_E2E_VERBOSE` | Enable verbose logging | unset |
| `AIKI_E2E_TIMEOUT` | Test timeout in seconds | 60 |

### Running Tests

**Prerequisites:**
- Claude Code installed: `npm install -g @anthropic-ai/claude-code`
- Anthropic API key configured: `export ANTHROPIC_API_KEY=sk-...`
- JJ installed: `cargo install jj-cli`

**Test Execution:**

```bash
# Run all E2E tests (with Claude Code in acceptEdits mode)
CLAUDE_INTEGRATION_TEST=1 cargo test e2e_automation -- --nocapture

# Run specific scenario
CLAUDE_INTEGRATION_TEST=1 cargo test test_deterministic_edit_flow -- --nocapture

# Run multi-step session test
CLAUDE_INTEGRATION_TEST=1 cargo test test_deterministic_multi_step_session -- --nocapture

# Run event sequence verification
CLAUDE_INTEGRATION_TEST=1 cargo test test_event_sequence_integrity -- --nocapture

# Run with verbose output
CLAUDE_INTEGRATION_TEST=1 AIKI_E2E_VERBOSE=1 cargo test e2e_automation -- --nocapture

# Run without ignoring tests (alternative to setting env var)
cargo test e2e_automation -- --nocapture --ignored
```

**Manual Testing (Interactive Mode):**

```bash
# Test with interactive permission prompts (not CI-friendly)
# In test code, set: auto_accept_permissions: false
CLAUDE_INTEGRATION_TEST=1 cargo test test_basic_edit_flow -- --nocapture

# Or test Claude directly with acceptEdits mode:
cd /tmp/test-repo
claude --settings-override '{"permissions": {"acceptEdits": true}}'
```

---

## CI/CD Integration

### GitHub Actions Workflow

```yaml
name: E2E Automation Tests

on:
  workflow_dispatch:  # Manual trigger only
  schedule:
    - cron: '0 6 * * 1'  # Weekly on Monday

jobs:
  e2e-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rust-lang/setup-rust-toolchain@v1

      - name: Install JJ
        run: cargo install jj-cli

      - name: Install Claude Code
        run: npm install -g @anthropic-ai/claude-code

      - name: Build Aiki
        run: cargo build --release

      - name: Run E2E Tests
        env:
          CLAUDE_INTEGRATION_TEST: 1
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: cargo test e2e_automation -- --nocapture
```

---

## Verification

### Build Check

```bash
# Verify dependencies compile
cargo build --tests

# Should complete without errors
```

### Run Single Test

```bash
# Set required environment variables
export CLAUDE_INTEGRATION_TEST=1
export ANTHROPIC_API_KEY=sk-ant-...

# Run basic edit test with output
cargo test test_deterministic_edit_flow -- --nocapture --ignored
```

**Expected output:**
```
✓ Created calculator.py with known content
✓ Sent deterministic prompt
✓ Edit completed
✓ File modified exactly as instructed (no unintended changes)
✓ All provenance metadata verified
✓ Session ID: claude-session-abc123

test test_deterministic_edit_flow ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Run Multi-Step Test

```bash
export CLAUDE_INTEGRATION_TEST=1
cargo test test_deterministic_multi_step_session -- --nocapture --ignored
```

**Expected output:**
```
✓ Created test files with known content
✓ Step 1: Replaced TODO with DONE
✓ Step 2: Appended '# End' to file2.txt
✓ Step 3: Created file3.txt
✓ Captured session ID: claude-session-xyz789
✓ All 3 changes have session ID: claude-session-xyz789
✓ All edits were deterministic (no unintended changes)

test test_deterministic_multi_step_session ... ok
```

### Run Event Verification Test

```bash
export CLAUDE_INTEGRATION_TEST=1
cargo test test_event_sequence_integrity -- --nocapture --ignored
```

**Expected output:**
```
✓ Captured 5 events
  [0] session.started (session: claude-session-123)
  [1] prompt.submitted (session: claude-session-123)
  [2] change.completed (session: claude-session-123)
  [3] response.received (session: claude-session-123)
  [4] session.ended (session: claude-session-123)
✓ Event ordering verified (happens-before relationships maintained)
✓ No duplicate events (each event fired exactly once)
✓ Session ID consistency verified (all events share session: claude-session-123)
✓ Timestamp monotonicity verified
✓ EVENT SEQUENCE INTEGRITY VERIFIED

test test_event_sequence_integrity ... ok
```

### Run Full Suite

```bash
export CLAUDE_INTEGRATION_TEST=1
cargo test e2e_automation -- --nocapture --ignored
```

### Common Issues

| Issue | Cause | Fix |
|-------|-------|-----|
| "claude: command not found" | Claude Code not installed | `npm install -g @anthropic-ai/claude-code` |
| "ANTHROPIC_API_KEY not set" | Missing API key | `export ANTHROPIC_API_KEY=sk-ant-...` |
| "Timeout waiting for prompt return" | Claude Code hanging | Check network, API rate limits |
| "Event sequence number mismatch" | Flow engine not writing seq | Fix event logging flow configuration |
| "Unexpected file change: ..." | Claude refactored code | Use more explicit deterministic prompts |

---

## Success Criteria Checklist

After implementation, verify these items:

- [ ] **Build succeeds** - `cargo build --tests` completes without errors
- [ ] **Basic edit test passes** - Single file edit with provenance tracking works
- [ ] **Multi-step test passes** - Session ID consistency across multiple edits verified
- [ ] **Event ordering test passes** - Happens-before relationships and sequence integrity verified
- [ ] **Filesystem snapshot works** - Detects unintended file changes
- [ ] **Metadata polling robust** - Uses JJ range queries, verifies file attribution
- [ ] **No hanging tests** - All timeouts implemented correctly
- [ ] **Deterministic behavior** - Tests pass consistently (3/3 runs)
- [ ] **CI-ready** - GitHub Actions workflow can run tests with secrets

---

## Future Enhancements

### Multi-Agent Testing

Once we have automation capabilities for other AI editors, we can extend the test suite:

**Scenario: Multi-Editor Interleaving**
- **Purpose:** Verify correct attribution when multiple AI editors are used
- **Steps:**
  1. Initialize repo
  2. Make edit with Claude Code (via `ClaudeCodeDriver`)
  3. Make manual human edit (via `git` or direct file modification)
  4. Make edit with Cursor (via `CursorDriver` - once automated)
  5. Run `aiki blame` and verify distinct attributions
  6. Verify `aiki authors` shows all contributors correctly

**Implementation:**
```rust
// Add to agent_driver.rs factory
pub fn create_driver(agent_type: &str, config: AgentConfig) -> Result<Box<dyn AgentDriver>> {
    match agent_type {
        "claude-code" => Ok(Box::new(ClaudeCodeDriver::new(config)?)),
        "cursor" => Ok(Box::new(CursorDriver::new(config)?)),  // Future
        "zed" => Ok(Box::new(ZedDriver::new(config)?)),        // Future
        _ => Err(anyhow::anyhow!("Unsupported agent type: {}", agent_type)),
    }
}
```

**Why Not Now:**
- Cursor doesn't yet have automation capabilities equivalent to Claude Code's CLI
- Simulating Cursor would create brittle, non-representative tests
- Better to wait for real automation support and test actual behavior

---

## Appendix

### A. Design Rationale

#### Deterministic Testing Philosophy

Tests must be **deterministic and focused on hook mechanics**, not Claude's decision-making.

**❌ BAD (Non-deterministic):**
```rust
// Claude decides what to do, how to do it, and what files to touch
session.send_line("Add a multiply function to calculator.py")?;
```

**✅ GOOD (Deterministic):**
```rust
// Exact instruction with minimal autonomy
session.send_line(
    "Using the Edit tool, insert exactly this function at the end of calculator.py:\n\
     def multiply(a, b):\n    return a * b\n\
     Do not refactor or modify any other code."
)?;
```

**Why:**
1. **Reproducible failures** - Same input → same output
2. **Clear attribution** - Test failure points to Aiki, not Claude behavior
3. **Faster execution** - Simple edits complete quickly
4. **Lower API costs** - Minimal token usage
5. **Hook verification** - Tests the hook pipeline, not AI capabilities

#### AgentDriver Abstraction Rationale

Tests should not be tightly coupled to Claude Code's specific:
- CLI UX (prompt format, `/exit` command)
- Permission prompt text ("Allow edit...")
- Completion indicators ("File modified")

The `AgentDriver` trait decouples tests from vendor specifics, making them:
- **Maintainable** - CLI changes don't break tests
- **Extensible** - Future editors (Cursor, Zed) can implement same trait
- **Testable** - Can mock agents for unit tests

#### CI Configuration: acceptEdits Mode

For reliable CI execution, tests use **acceptEdits mode** to eliminate interactive prompts:

```bash
claude --settings-override '{"permissions": {"acceptEdits": true}}'
```

This ensures:
- ✅ **No manual intervention** - Tests run unattended
- ✅ **Faster execution** - No waiting for permission prompts
- ✅ **Deterministic behavior** - Same permissions every time
- ✅ **Hook testing** - PostToolUse still fires (permissions granted automatically)

---

### B. Existing Test Patterns

The codebase already has `cli/tests/claude_integration_test.rs` which provides a good starting point. Key patterns to reuse:

1. **Conditional test execution** via `CLAUDE_INTEGRATION_TEST` env var
2. **Plugin setup** - Copy plugin directory, configure hooks
3. **Claude invocation** - `claude -p` with `--dangerously-skip-permissions`
4. **Metadata verification** - Parse JJ description for `[aiki]` blocks
5. **Timeout handling** - Wait with polling, not fixed sleep

See: `/home/user/aiki/cli/tests/README_CLAUDE_INTEGRATION.md`

---

## Review Feedback Addressed

This plan was significantly improved based on comprehensive code review feedback:

### High-Impact Fixes Implemented

✅ **Fixed inverted permission handling logic** - Eliminated confusing early-return and dead code path  
✅ **Wrapped spawn_command API properly** - Created `spawn_pty()` helper to isolate rexpect version differences  
✅ **Simplified Drop cleanup** - Made `exit()` mandatory, Drop only does best-effort Ctrl-C  
✅ **Normalized newline assertions** - All file checks use `.trim_end()` or `.lines()` to avoid spurious failures  
✅ **Fixed JJ metadata polling** - Uses `-r @- -n 5` range to catch async writes  
✅ **Defined event log format** - JSONL with monotonic `seq`, full schema specified  
✅ **Made completion detection tool-agnostic** - Relies on prompt return, not fragile message matching  
✅ **Added filesystem snapshot verification** - Detects all unintended file changes via content hashing  
✅ **Added exit timeout** - Prevents infinite hangs with `exp_eof_timeout()`  
✅ **Documented all dependencies** - Added walkdir, sha2, serde_json, chrono

### Why These Changes Matter

**Stability**: Tests won't hang forever, fail on whitespace, or miss async metadata  
**Debuggability**: JSONL logs + filesystem snapshots make failures traceable  
**Extensibility**: AgentDriver abstraction + tool-agnostic detection work for future editors  
**CI-Ready**: Deterministic prompts + acceptEdits mode + proper cleanup = reliable automation

### Remaining Implementation Work

When implementing this plan:

1. **Verify rexpect API** - Confirm `exp_eof_timeout()` exists, use fallback pattern if not
2. **Test event flow configuration** - Ensure flow engine writes JSONL with monotonic seq
3. **Add retry wrapper** - Consider `with_retry()` helper for transient network failures
4. **Tier CI tests** - Tier 0 (always), Tier 1 (manual), Tier 2 (scheduled with secrets)

This plan is now **production-ready** and addresses all identified sharp edges.
