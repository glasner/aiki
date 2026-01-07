# E2E Automation Tests for Claude Code Sessions

End-to-end tests that simulate full Claude Code sessions, verifying Aiki correctly tracks provenance throughout the entire lifecycle. Uses `rexpect` for interactive PTY automation.

---

## Prerequisites

### Cargo.toml Additions

```toml
# cli/Cargo.toml
[dev-dependencies]
rexpect = "0.5"
tempfile = "3"
chrono = { version = "0.4", features = ["serde"] }
```

### External Tools

| Tool | Install Command | Purpose |
|------|-----------------|---------|
| Claude Code | `npm install -g @anthropic-ai/claude-code` | Agent under test |
| JJ CLI | `cargo install jj-cli` | Version control backend |

### Environment Variables

| Variable | Required | Default | Purpose |
|----------|----------|---------|---------|
| `CLAUDE_INTEGRATION_TEST` | Yes | unset | Set to `1` to enable tests |
| `ANTHROPIC_API_KEY` | Yes | - | API authentication |
| `AIKI_E2E_VERBOSE` | No | unset | Enable debug output |
| `AIKI_E2E_TIMEOUT` | No | `60` | Test timeout in seconds |

---

## Files to Create

| Order | Path | Depends On | Purpose |
|-------|------|------------|---------|
| 1 | `cli/tests/e2e_automation/mod.rs` | - | Module exports |
| 2 | `cli/tests/e2e_automation/types.rs` | - | Shared types |
| 3 | `cli/tests/e2e_automation/agent_driver.rs` | types | Trait + factory |
| 4 | `cli/tests/e2e_automation/claude_driver.rs` | agent_driver | Claude implementation |
| 5 | `cli/tests/e2e_automation/helpers.rs` | types | Test utilities |
| 6 | `cli/tests/e2e_automation/test_basic_edit.rs` | helpers, agent_driver | Scenario 1 |
| 7 | `cli/tests/e2e_automation/test_multi_step.rs` | helpers, agent_driver | Scenario 2 |
| 8 | `cli/tests/e2e_automation/test_events.rs` | helpers, agent_driver | Scenario 3 |

---

## Implementation

### File 1: `cli/tests/e2e_automation/mod.rs`

```rust
//! E2E automation tests for Claude Code sessions
//!
//! Run with: CLAUDE_INTEGRATION_TEST=1 cargo test e2e_automation -- --nocapture

mod types;
mod agent_driver;
mod claude_driver;
mod helpers;

#[cfg(test)]
mod test_basic_edit;
#[cfg(test)]
mod test_multi_step;
#[cfg(test)]
mod test_events;

pub use agent_driver::{AgentDriver, AgentConfig, create_driver};
pub use types::{AikiMetadata, Event};
pub use helpers::*;
```

---

### File 2: `cli/tests/e2e_automation/types.rs`

```rust
//! Shared types for E2E tests

use chrono::{DateTime, Utc};

/// Parsed [aiki] metadata block from JJ change description
#[derive(Debug, Clone, PartialEq)]
pub struct AikiMetadata {
    pub author: String,
    pub author_type: String,
    pub session: Option<String>,
    pub tool: String,
    pub confidence: String,
    pub method: String,
}

/// Event log entry for lifecycle verification
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub event_type: String,
    pub session_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// JJ change with parsed metadata
#[derive(Debug, Clone)]
pub struct JjChange {
    pub change_id: String,
    pub description: String,
    pub metadata: Option<AikiMetadata>,
}
```

---

### File 3: `cli/tests/e2e_automation/agent_driver.rs`

```rust
//! Agent driver abstraction
//!
//! Decouples tests from specific agent CLI details (prompts, exit commands, etc.)

use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;

/// Abstraction for driving an AI agent session
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
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub working_dir: PathBuf,
    pub timeout: Duration,
    /// If true, use acceptEdits mode (no interactive permission prompts)
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
        "claude-code" => {
            let driver = super::claude_driver::ClaudeCodeDriver::new(config)?;
            Ok(Box::new(driver))
        }
        _ => Err(anyhow::anyhow!("Unsupported agent type: {}", agent_type)),
    }
}
```

---

### File 4: `cli/tests/e2e_automation/claude_driver.rs`

```rust
//! Claude Code-specific driver implementation

use super::agent_driver::{AgentDriver, AgentConfig};
use anyhow::{Context, Result};
use rexpect::session::PtySession;
use std::process::Command;
use std::time::Duration;

/// Claude Code driver using rexpect for PTY interaction
pub struct ClaudeCodeDriver {
    session: PtySession,
    config: AgentConfig,
}

impl ClaudeCodeDriver {
    pub fn new(config: AgentConfig) -> Result<Self> {
        // Build command with working directory
        let mut cmd = Command::new("claude");
        cmd.current_dir(&config.working_dir);
        
        // Add acceptEdits mode for deterministic testing
        if config.auto_accept_permissions {
            cmd.args(&[
                "--settings-override",
                r#"{"permissions": {"acceptEdits": true}}"#,
            ]);
        }
        
        // Spawn PTY session
        let mut session = rexpect::session::spawn_command(cmd, Some(config.timeout))
            .context("Failed to spawn Claude Code process")?;
        
        // Wait for initial prompt (Claude Code shows "> " when ready)
        session.exp_string("> ")
            .context("Timeout waiting for Claude Code initial prompt")?;
        
        Ok(Self { session, config })
    }
}

impl Drop for ClaudeCodeDriver {
    fn drop(&mut self) {
        // Try graceful exit
        let _ = self.session.send_line("/exit");
        
        // Wait briefly for clean shutdown
        let _ = self.session.exp_eof();
        
        // Note: rexpect handles process cleanup when PtySession drops
    }
}

impl AgentDriver for ClaudeCodeDriver {
    fn send_prompt(&mut self, text: &str) -> Result<()> {
        self.session.send_line(text)
            .context("Failed to send prompt to Claude Code")?;
        Ok(())
    }
    
    fn wait_for_edit(&mut self) -> Result<()> {
        // Claude Code outputs various messages on edit completion
        self.session.exp_regex(r"(File.*modified|Changes applied|Edit complete|Updated)")
            .context("Timeout waiting for edit completion")?;
        
        // Wait for prompt to return (ready for next input)
        self.session.exp_string("> ")
            .context("Timeout waiting for prompt after edit")?;
        
        Ok(())
    }
    
    fn wait_for_write(&mut self) -> Result<()> {
        self.session.exp_regex(r"(File.*created|File.*written|Write complete|Created)")
            .context("Timeout waiting for write completion")?;
        
        self.session.exp_string("> ")
            .context("Timeout waiting for prompt after write")?;
        
        Ok(())
    }
    
    fn wait_for_completion(&mut self) -> Result<()> {
        // Generic completion - matches any tool
        self.session.exp_regex(r"(File.*modified|File.*created|complete|Updated|Created)")
            .context("Timeout waiting for tool completion")?;
        
        self.session.exp_string("> ")
            .context("Timeout waiting for prompt")?;
        
        Ok(())
    }
    
    fn exit(mut self) -> Result<()> {
        self.session.send_line("/exit")
            .context("Failed to send exit command")?;
        
        // Wait for process to exit (with timeout via session config)
        self.session.exp_eof()
            .context("Timeout waiting for Claude Code to exit")?;
        
        Ok(())
    }
}
```

---

### File 5: `cli/tests/e2e_automation/helpers.rs`

```rust
//! Shared test utilities

use super::types::{AikiMetadata, Event, JjChange};
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::time::{Duration, Instant};

// ============================================================================
// REPO SETUP
// ============================================================================

/// Initialize a git repository in the given directory
pub fn init_git_repo(path: &Path) -> Result<()> {
    std::process::Command::new("git")
        .args(&["init"])
        .current_dir(path)
        .output()
        .context("Failed to run git init")?;
    
    // Configure git user for commits
    std::process::Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()?;
    
    std::process::Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(path)
        .output()?;
    
    Ok(())
}

/// Initialize JJ workspace colocated with git
pub fn init_jj_workspace(path: &Path) -> Result<()> {
    std::process::Command::new("jj")
        .args(&["git", "init", "--colocate"])
        .current_dir(path)
        .output()
        .context("Failed to run jj git init")?;
    
    Ok(())
}

/// Run aiki init to set up hooks and flows
pub fn run_aiki_init(path: &Path) -> Result<()> {
    std::process::Command::new("aiki")
        .args(&["init"])
        .current_dir(path)
        .output()
        .context("Failed to run aiki init")?;
    
    Ok(())
}

/// Create a test file with given content
pub fn create_test_file(path: &Path, filename: &str, content: &str) -> Result<()> {
    let file_path = path.join(filename);
    
    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    std::fs::write(&file_path, content)
        .with_context(|| format!("Failed to create test file: {}", file_path.display()))?;
    
    Ok(())
}

/// Setup a complete test repository (git + jj + aiki)
pub fn setup_test_repo() -> Result<tempfile::TempDir> {
    let temp_dir = tempfile::tempdir()
        .context("Failed to create temp directory")?;
    
    init_git_repo(temp_dir.path())?;
    init_jj_workspace(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;
    
    Ok(temp_dir)
}

// ============================================================================
// METADATA PARSING
// ============================================================================

/// Parse [aiki] metadata block from a JJ change description
pub fn parse_aiki_metadata(description: &str) -> Result<Option<AikiMetadata>> {
    // Find [aiki] block
    let start_marker = "[aiki]";
    let end_marker = "[/aiki]";
    
    let start = match description.find(start_marker) {
        Some(pos) => pos + start_marker.len(),
        None => return Ok(None),
    };
    
    let end = match description.find(end_marker) {
        Some(pos) => pos,
        None => return Err(anyhow!("Found [aiki] but no [/aiki] closing tag")),
    };
    
    let block = &description[start..end];
    
    // Parse key=value pairs
    let mut author = String::new();
    let mut author_type = String::new();
    let mut session = None;
    let mut tool = String::new();
    let mut confidence = String::new();
    let mut method = String::new();
    
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "author" => author = value.trim().to_string(),
                "author_type" => author_type = value.trim().to_string(),
                "session" => session = Some(value.trim().to_string()),
                "tool" => tool = value.trim().to_string(),
                "confidence" => confidence = value.trim().to_string(),
                "method" => method = value.trim().to_string(),
                _ => {} // Ignore unknown keys
            }
        }
    }
    
    Ok(Some(AikiMetadata {
        author,
        author_type,
        session,
        tool,
        confidence,
        method,
    }))
}

/// Wait for metadata to appear in the current JJ change
///
/// Polls JJ until metadata is found or timeout expires.
pub fn wait_for_metadata(repo_path: &Path, timeout: Duration) -> Result<AikiMetadata> {
    let start = Instant::now();
    
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!(
                "Timeout ({:?}) waiting for [aiki] metadata in JJ change",
                timeout
            ));
        }
        
        // Get current change description
        let output = std::process::Command::new("jj")
            .args(&["log", "-r", "@", "-T", "description", "--no-graph"])
            .current_dir(repo_path)
            .output()
            .context("Failed to run jj log")?;
        
        let description = String::from_utf8_lossy(&output.stdout);
        
        if let Some(metadata) = parse_aiki_metadata(&description)? {
            return Ok(metadata);
        }
        
        // Poll every 100ms
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Get recent JJ changes with parsed metadata
pub fn get_recent_jj_changes(repo_path: &Path, limit: usize) -> Result<Vec<JjChange>> {
    let output = std::process::Command::new("jj")
        .args(&[
            "log",
            "-r", &format!("@-{}::@", limit),
            "-T", r#"change_id ++ "\n---DESC---\n" ++ description ++ "\n---END---\n""#,
            "--no-graph",
        ])
        .current_dir(repo_path)
        .output()
        .context("Failed to run jj log")?;
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut changes = Vec::new();
    
    for block in output_str.split("---END---") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        
        let parts: Vec<&str> = block.splitn(2, "---DESC---").collect();
        if parts.len() != 2 {
            continue;
        }
        
        let change_id = parts[0].trim().to_string();
        let description = parts[1].trim().to_string();
        let metadata = parse_aiki_metadata(&description).ok().flatten();
        
        changes.push(JjChange {
            change_id,
            description,
            metadata,
        });
    }
    
    Ok(changes)
}

// ============================================================================
// FILE VERIFICATION
// ============================================================================

/// Check if a file contains a specific string
pub fn file_contains(repo_path: &Path, filename: &str, needle: &str) -> bool {
    let file_path = repo_path.join(filename);
    match std::fs::read_to_string(&file_path) {
        Ok(content) => content.contains(needle),
        Err(_) => false,
    }
}

/// Read file content
pub fn read_file(repo_path: &Path, filename: &str) -> Result<String> {
    let file_path = repo_path.join(filename);
    std::fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))
}

// ============================================================================
// EVENT LOG (for lifecycle testing)
// ============================================================================

/// Configure flow engine to log events to a file
pub fn configure_event_logging(repo_path: &Path, log_path: &Path) -> Result<()> {
    // Create flow config that writes events to log file
    let flow_config = format!(
        r#"
flows:
  - name: event_logger
    trigger:
      event: "*"
    actions:
      - type: log
        path: "{}"
        format: json
"#,
        log_path.display()
    );
    
    let config_path = repo_path.join(".aiki/flows/event_logger.yaml");
    std::fs::create_dir_all(config_path.parent().unwrap())?;
    std::fs::write(&config_path, flow_config)?;
    
    Ok(())
}

/// Read events from log file
pub fn read_event_log(log_path: &Path) -> Result<Vec<Event>> {
    let content = std::fs::read_to_string(log_path)
        .context("Failed to read event log")?;
    
    let mut events = Vec::new();
    
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        
        // Parse JSON event (simplified - adjust to actual format)
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            let event = Event {
                event_type: value["event_type"].as_str().unwrap_or("").to_string(),
                session_id: value["session_id"].as_str().map(|s| s.to_string()),
                timestamp: value["timestamp"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now),
            };
            events.push(event);
        }
    }
    
    Ok(events)
}

// ============================================================================
// TEST UTILITIES
// ============================================================================

/// Skip test if CLAUDE_INTEGRATION_TEST is not set
#[macro_export]
macro_rules! require_integration_test {
    () => {
        if std::env::var("CLAUDE_INTEGRATION_TEST").is_err() {
            eprintln!("Skipping: Set CLAUDE_INTEGRATION_TEST=1 to run");
            return Ok(());
        }
    };
}

pub use require_integration_test;
```

---

### File 6: `cli/tests/e2e_automation/test_basic_edit.rs`

```rust
//! Scenario 1: Basic deterministic edit flow
//!
//! Verifies single file edit triggers correct provenance recording.

use super::agent_driver::{create_driver, AgentConfig};
use super::helpers::*;
use anyhow::Result;
use std::time::Duration;

const INITIAL_CONTENT: &str = r#"def add(a, b):
    return a + b

def subtract(a, b):
    return a - b
"#;

const FUNCTION_TO_ADD: &str = r#"def multiply(a, b):
    return a * b"#;

#[test]
#[ignore] // Run with: CLAUDE_INTEGRATION_TEST=1 cargo test
fn test_deterministic_edit_flow() -> Result<()> {
    require_integration_test!();

    // ========================================================================
    // SETUP
    // ========================================================================
    let temp_dir = setup_test_repo()?;
    let repo_path = temp_dir.path();
    
    create_test_file(repo_path, "calculator.py", INITIAL_CONTENT)?;
    eprintln!("✓ Created calculator.py with known content");

    // ========================================================================
    // EXECUTE: Send deterministic prompt
    // ========================================================================
    let config = AgentConfig {
        working_dir: repo_path.to_path_buf(),
        timeout: Duration::from_secs(60),
        auto_accept_permissions: true,
    };
    let mut agent = create_driver("claude-code", config)?;

    // Deterministic prompt: exact instruction, no autonomy
    let prompt = format!(
        "Using the Edit tool, insert exactly this function at the end of calculator.py:\n\n\
         {}\n\n\
         Do not modify any other code. Do not refactor. Do not add comments.",
        FUNCTION_TO_ADD
    );
    
    agent.send_prompt(&prompt)?;
    eprintln!("✓ Sent deterministic prompt");

    agent.wait_for_edit()?;
    eprintln!("✓ Edit completed");

    agent.exit()?;
    eprintln!("✓ Session exited");

    // ========================================================================
    // VERIFY: File modification
    // ========================================================================
    let final_content = read_file(repo_path, "calculator.py")?;
    
    assert!(
        final_content.contains("def add(a, b)"),
        "Original add function should still exist"
    );
    assert!(
        final_content.contains("def subtract(a, b)"),
        "Original subtract function should still exist"
    );
    assert!(
        final_content.contains("def multiply(a, b)"),
        "New multiply function should be added"
    );
    assert_eq!(
        final_content.matches("def ").count(),
        3,
        "Should have exactly 3 functions (no refactoring occurred)"
    );
    
    eprintln!("✓ File modified exactly as instructed");

    // ========================================================================
    // VERIFY: Provenance metadata
    // ========================================================================
    let metadata = wait_for_metadata(repo_path, Duration::from_secs(5))?;

    assert_eq!(metadata.author, "claude", "Author should be 'claude'");
    assert_eq!(metadata.author_type, "agent", "Author type should be 'agent'");
    assert!(metadata.session.is_some(), "Session ID should be present");
    assert_eq!(metadata.tool, "Edit", "Tool should be 'Edit'");
    assert_eq!(metadata.confidence, "High", "Confidence should be 'High'");
    assert_eq!(metadata.method, "Hook", "Method should be 'Hook'");

    eprintln!("✓ Provenance metadata verified");
    eprintln!("  author={}", metadata.author);
    eprintln!("  session={}", metadata.session.as_ref().unwrap());
    eprintln!("  tool={}", metadata.tool);

    Ok(())
}
```

---

### File 7: `cli/tests/e2e_automation/test_multi_step.rs`

```rust
//! Scenario 2: Multi-step session with session ID consistency
//!
//! Verifies all edits in a single session share the same session ID.

use super::agent_driver::{create_driver, AgentConfig};
use super::helpers::*;
use anyhow::Result;
use std::collections::HashSet;
use std::time::Duration;

#[test]
#[ignore]
fn test_deterministic_multi_step_session() -> Result<()> {
    require_integration_test!();

    // ========================================================================
    // SETUP
    // ========================================================================
    let temp_dir = setup_test_repo()?;
    let repo_path = temp_dir.path();
    
    create_test_file(repo_path, "file1.txt", "TODO: implement feature")?;
    create_test_file(repo_path, "file2.txt", "# Start\n")?;
    eprintln!("✓ Created test files with known content");

    let config = AgentConfig {
        working_dir: repo_path.to_path_buf(),
        timeout: Duration::from_secs(120),
        auto_accept_permissions: true,
    };
    let mut agent = create_driver("claude-code", config)?;

    // ========================================================================
    // STEP 1: Simple string replacement
    // ========================================================================
    agent.send_prompt(
        "Using the Edit tool, replace the text 'TODO' with 'DONE' in file1.txt. \
         Do not modify anything else."
    )?;
    agent.wait_for_edit()?;
    
    let file1_content = read_file(repo_path, "file1.txt")?;
    assert_eq!(
        file1_content.trim(),
        "DONE: implement feature",
        "Step 1: file1.txt should have exact replacement"
    );
    eprintln!("✓ Step 1: Replaced TODO → DONE");

    // Capture session ID from first change
    let first_metadata = wait_for_metadata(repo_path, Duration::from_secs(5))?;
    let session_id = first_metadata.session.clone()
        .expect("Session ID missing from first change");
    eprintln!("✓ Captured session ID: {}", session_id);

    // ========================================================================
    // STEP 2: Append text
    // ========================================================================
    agent.send_prompt(
        "Using the Edit tool, append exactly the text '# End' to the end of file2.txt. \
         Do not modify any other content."
    )?;
    agent.wait_for_edit()?;
    
    let file2_content = read_file(repo_path, "file2.txt")?;
    assert!(
        file2_content.contains("# Start") && file2_content.contains("# End"),
        "Step 2: file2.txt should have both Start and End"
    );
    eprintln!("✓ Step 2: Appended '# End'");

    // ========================================================================
    // STEP 3: Create new file
    // ========================================================================
    agent.send_prompt(
        "Using the Write tool, create a new file called file3.txt with exactly this content: 'test'"
    )?;
    agent.wait_for_write()?;
    
    let file3_content = read_file(repo_path, "file3.txt")?;
    assert_eq!(
        file3_content.trim(),
        "test",
        "Step 3: file3.txt should have exact content"
    );
    eprintln!("✓ Step 3: Created file3.txt");

    agent.exit()?;

    // ========================================================================
    // VERIFY: No unintended files
    // ========================================================================
    let txt_files: Vec<_> = std::fs::read_dir(repo_path)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map_or(false, |ext| ext == "txt")
        })
        .collect();
    
    assert_eq!(
        txt_files.len(),
        3,
        "Should have exactly 3 .txt files (no extras created)"
    );
    eprintln!("✓ No unintended files created");

    // ========================================================================
    // VERIFY: Session ID consistency
    // ========================================================================
    let changes = get_recent_jj_changes(repo_path, 5)?;
    let session_ids: HashSet<_> = changes
        .iter()
        .filter_map(|c| c.metadata.as_ref())
        .filter_map(|m| m.session.as_ref())
        .collect();

    assert_eq!(
        session_ids.len(),
        1,
        "All changes should share the same session ID"
    );
    assert!(
        session_ids.contains(&session_id),
        "Session ID should match the one captured from first change"
    );

    eprintln!("✓ All {} changes share session ID: {}", changes.len(), session_id);

    Ok(())
}
```

---

### File 8: `cli/tests/e2e_automation/test_events.rs`

```rust
//! Scenario 3: Event sequence integrity verification
//!
//! Verifies lifecycle events fire in correct order with consistent session IDs.

use super::agent_driver::{create_driver, AgentConfig};
use super::helpers::*;
use super::types::Event;
use anyhow::Result;
use std::collections::HashMap;
use std::time::Duration;

#[test]
#[ignore]
fn test_event_sequence_integrity() -> Result<()> {
    require_integration_test!();

    // ========================================================================
    // SETUP
    // ========================================================================
    let temp_dir = setup_test_repo()?;
    let repo_path = temp_dir.path();
    
    // Configure event logging
    let event_log_path = repo_path.join("events.log");
    configure_event_logging(repo_path, &event_log_path)?;
    eprintln!("✓ Configured event logging to {}", event_log_path.display());
    
    create_test_file(repo_path, "test.txt", "TODO")?;

    let config = AgentConfig {
        working_dir: repo_path.to_path_buf(),
        timeout: Duration::from_secs(60),
        auto_accept_permissions: true,
    };
    let mut agent = create_driver("claude-code", config)?;

    // ========================================================================
    // EXECUTE
    // ========================================================================
    agent.send_prompt(
        "Using the Edit tool, replace 'TODO' with 'DONE' in test.txt. \
         Do not modify anything else."
    )?;
    agent.wait_for_edit()?;
    agent.exit()?;

    // ========================================================================
    // READ EVENTS
    // ========================================================================
    let events = read_event_log(&event_log_path)?;
    
    eprintln!("✓ Captured {} events:", events.len());
    for (i, event) in events.iter().enumerate() {
        eprintln!("  [{}] {} (session: {:?})", i, event.event_type, event.session_id);
    }

    // ========================================================================
    // VERIFY 1: Required events exist
    // ========================================================================
    let required_events = ["session.started", "prompt.submitted", "change.completed"];
    
    for required in &required_events {
        assert!(
            events.iter().any(|e| e.event_type == *required),
            "Required event '{}' must fire",
            required
        );
    }
    eprintln!("✓ All required events present");

    // ========================================================================
    // VERIFY 2: Happens-before ordering
    // ========================================================================
    let position = |event_type: &str| -> Option<usize> {
        events.iter().position(|e| e.event_type == event_type)
    };

    let session_started = position("session.started").expect("session.started must exist");
    let prompt_submitted = position("prompt.submitted").expect("prompt.submitted must exist");
    let change_completed = position("change.completed").expect("change.completed must exist");

    assert!(
        session_started < prompt_submitted,
        "session.started ({}) must happen before prompt.submitted ({})",
        session_started, prompt_submitted
    );
    assert!(
        prompt_submitted < change_completed,
        "prompt.submitted ({}) must happen before change.completed ({})",
        prompt_submitted, change_completed
    );
    eprintln!("✓ Event ordering correct (happens-before maintained)");

    // ========================================================================
    // VERIFY 3: No duplicate events
    // ========================================================================
    let mut event_counts: HashMap<&str, usize> = HashMap::new();
    for event in &events {
        *event_counts.entry(&event.event_type).or_insert(0) += 1;
    }

    for (event_type, count) in &event_counts {
        // change.completed may fire multiple times if multiple files edited
        if *event_type != "change.completed" {
            assert_eq!(
                *count, 1,
                "Event '{}' fired {} times (expected 1)",
                event_type, count
            );
        }
    }
    eprintln!("✓ No duplicate events");

    // ========================================================================
    // VERIFY 4: Session ID consistency
    // ========================================================================
    let session_ids: Vec<_> = events
        .iter()
        .filter_map(|e| e.session_id.as_ref())
        .collect();

    if !session_ids.is_empty() {
        let first = session_ids[0];
        for (i, sid) in session_ids.iter().enumerate() {
            assert_eq!(
                *sid, first,
                "Event {} has mismatched session ID: {} (expected {})",
                i, sid, first
            );
        }
        eprintln!("✓ Session ID consistent: {}", first);
    }

    // ========================================================================
    // VERIFY 5: Timestamp monotonicity
    // ========================================================================
    for i in 1..events.len() {
        assert!(
            events[i].timestamp >= events[i - 1].timestamp,
            "Timestamps must be monotonic (event {} < event {})",
            i - 1, i
        );
    }
    eprintln!("✓ Timestamps monotonically increasing");

    eprintln!("\n✓ EVENT SEQUENCE INTEGRITY VERIFIED");

    Ok(())
}
```

---

## Verification

### Build Check

```bash
cd cli
cargo build --tests
```

### Run Single Test

```bash
CLAUDE_INTEGRATION_TEST=1 cargo test test_deterministic_edit_flow -- --nocapture
```

### Run Full Suite

```bash
CLAUDE_INTEGRATION_TEST=1 cargo test e2e_automation -- --nocapture
```

### Expected Output (Basic Edit Test)

```
✓ Created calculator.py with known content
✓ Sent deterministic prompt
✓ Edit completed
✓ Session exited
✓ File modified exactly as instructed
✓ Provenance metadata verified
  author=claude
  session=abc123-def456
  tool=Edit
```

---

## CI/CD Integration

### GitHub Actions Workflow

Create `.github/workflows/e2e-tests.yml`:

```yaml
name: E2E Automation Tests

on:
  workflow_dispatch:  # Manual trigger
  schedule:
    - cron: '0 6 * * 1'  # Weekly on Monday 6am UTC

jobs:
  e2e-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    
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
          CLAUDE_INTEGRATION_TEST: "1"
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          cd cli
          cargo test e2e_automation -- --nocapture
```

---

## Success Criteria

### Must Have
- [ ] `test_deterministic_edit_flow` passes
- [ ] Provenance metadata correctly recorded with all fields
- [ ] Session ID present in metadata
- [ ] Tests run without manual intervention (acceptEdits mode)

### Should Have
- [ ] `test_deterministic_multi_step_session` passes
- [ ] Session ID consistent across all edits in session
- [ ] `test_event_sequence_integrity` passes (if event logging implemented)
- [ ] No orphaned Claude processes after test completion

### Nice to Have
- [ ] Human edit interleaving test
- [ ] Hook failure recovery test
- [ ] Performance benchmarks (<5s per edit)

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| API costs | Weekly CI runs, simple deterministic prompts |
| Rate limits | Retry with exponential backoff in CI |
| Flaky tests | Polling with timeout (not fixed sleep) |
| Auth failures | Skip gracefully with clear error message |
| Orphaned processes | Drop implementation kills Claude on panic |
| PTY differences | Test on Linux first (CI target) |

---

## Appendix

### A. Design Rationale

#### Why Deterministic Prompts?

Tests must verify **Aiki's hook mechanics**, not Claude's decision-making:

```rust
// ❌ Non-deterministic: Claude decides implementation
"Add a multiply function to calculator.py"

// ✅ Deterministic: Exact instruction
"Using the Edit tool, insert exactly this function..."
```

Benefits:
- Reproducible failures (same input → same output)
- Clear attribution (failure = Aiki bug, not Claude behavior)
- Faster execution (simple edits complete quickly)
- Lower API costs (minimal tokens)

#### Why AgentDriver Abstraction?

Tests should not couple to Claude Code's specific:
- CLI UX (prompt format, `/exit` command)
- Permission prompt text ("Allow edit...")
- Completion indicators ("File modified")

The trait allows future drivers for Cursor, Zed, etc.

#### Why acceptEdits Mode?

Interactive permission prompts break CI automation. The `--settings-override` flag enables deterministic, unattended execution while still exercising the PostToolUse hook path.

### B. Existing Test Patterns

The codebase has `cli/tests/claude_integration_test.rs` with useful patterns:

1. Conditional execution via `CLAUDE_INTEGRATION_TEST` env var
2. Plugin setup (copy hooks, configure)
3. Metadata parsing from JJ descriptions
4. Timeout handling with polling

### C. Future Enhancements

#### Multi-Agent Testing

Once Cursor/Zed have automation capabilities:

```rust
pub fn create_driver(agent_type: &str, config: AgentConfig) -> Result<Box<dyn AgentDriver>> {
    match agent_type {
        "claude-code" => Ok(Box::new(ClaudeCodeDriver::new(config)?)),
        "cursor" => Ok(Box::new(CursorDriver::new(config)?)),  // Future
        "zed" => Ok(Box::new(ZedDriver::new(config)?)),        // Future
        _ => Err(anyhow::anyhow!("Unsupported agent type")),
    }
}
```

#### Performance Benchmarks

Add timing assertions for hook latency targets:

```rust
let start = Instant::now();
agent.wait_for_edit()?;
let elapsed = start.elapsed();

assert!(
    elapsed < Duration::from_secs(5),
    "Edit should complete in <5s (took {:?})",
    elapsed
);
```
