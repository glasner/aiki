# File-Based Session Tracking

## Overview

This document describes how Aiki tracks session state across hook invocations using simple session files. Each session gets its own file in `.aiki/sessions/`, enabling fast existence checks with atomic creation to prevent race conditions.

## Motivation

Hook invocations are stateless processes - each hook call spawns a new process, handles the event, and exits. For features like SessionStart detection in Cursor, we need to track state across invocations.

**Options considered:**
1. **Per-session files** (`.aiki/sessions/{session_id}`) - ✅ **Chosen**
2. **Single state file** (`.aiki/cursor_state.json`) - Fast but doesn't support concurrent sessions
3. **JJ-based tracking** (`aiki/sessions` branch) - Full event history but 25-50x slower
4. **Fire event every time** - No state needed but loses semantic meaning

**Why file-based tracking wins:**
- ✅ Extremely fast (~0.1ms read, ~1-2ms write)
- ✅ Supports concurrent sessions (multiple Cursor/Claude Code windows)
- ✅ Atomic creation prevents race conditions (`O_EXCL` flag)
- ✅ Simple existence check: `file.exists()` = session seen before
- ✅ No locking required (per-session isolation)
- ✅ Scales to hundreds of sessions with no degradation
- ✅ Natural cleanup (periodic file deletion)

## Aiki Session ID Design

### Why UUID v5 (Deterministic Hashing)

Aiki generates its own canonical session IDs from the `(agent_type, external_session_id)` tuple provided by IDEs/agents. We use **UUID v5 (SHA-1 namespace-based)** for deterministic generation.

**Why UUID v5:**
- ✅ **Deterministic**: Same `(agent, external_id)` → always same UUID
- ✅ **Collision-resistant**: SHA-1 provides strong uniqueness guarantees
- ✅ **No sanitization needed**: Hash function makes any input filesystem-safe
- ✅ **Universal format**: Clean UUID works everywhere (files, logs, databases, provenance)
- ✅ **Simple**: No time synchronization, no random number generation

**Why not ULID:**
- ❌ Not deterministic (includes random bits each time)
- ❌ Time-based (we already track `created_at` in session file)
- ❌ Sortability not needed for session tracking

**Why not UUID v7:**
- ❌ Not deterministic (includes random component)
- ❌ Time-based (timestamp not useful for our use case)

**Format:** Standard UUID (36 chars with hyphens)
- Example: `a7c3e5f2-8d4b-5a9c-b1e3-f4567890abcd`
- Filename: `a7c3e5f28d4b5a9cb1e3f4567890abcd` (no hyphens)

**Validation:** Only reject empty external session IDs (semantically invalid). No length limits, no character restrictions - UUID v5 hashing handles everything safely.

### Implementation

```rust
use uuid::Uuid;

/// A canonical Aiki session identifier (UUID v5, deterministic)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AikiSessionId {
    id: Uuid,
    agent_type: AgentType,
    external_id: String,
}

impl AikiSessionId {
    /// Aiki session namespace UUID
    const NAMESPACE: Uuid = Uuid::from_bytes([
        0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1,
        0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
    ]);
    
    /// Create deterministic session ID from agent + external ID
    pub fn new(agent_type: AgentType, external_id: &str) -> Result<Self> {
        if external_id.is_empty() {
            return Err(AikiError::InvalidSessionId(
                "Session ID cannot be empty".to_string()
            ));
        }
        
        // Hash: sha1(namespace + "agent:external_id")
        let name = format!("{}:{}", agent_type.to_metadata_string(), external_id);
        let uuid = Uuid::new_v5(&Self::NAMESPACE, name.as_bytes());
        
        Ok(Self {
            id: uuid,
            agent_type,
            external_id: external_id.to_string(),
        })
    }
    
    /// Get UUID as string: "a7c3e5f2-8d4b-5a9c-b1e3-f4567890abcd"
    pub fn as_str(&self) -> String {
        self.id.to_string()
    }
    
    /// Get filesystem filename: "a7c3e5f28d4b5a9cb1e3f4567890abcd"
    pub fn as_filename(&self) -> String {
        self.id.simple().to_string()
    }
    
    pub fn agent_type(&self) -> AgentType {
        self.agent_type
    }
    
    pub fn external_id(&self) -> &str {
        &self.external_id
    }
}
```

**Dependency:** `uuid = { version = "1.6", features = ["v5"] }`

### Examples

```rust
// Same input → same UUID (deterministic)
let id1 = AikiSessionId::new(AgentType::Claude, "claude-session-123")?;
let id2 = AikiSessionId::new(AgentType::Claude, "claude-session-123")?;
assert_eq!(id1.as_str(), id2.as_str());

// Different inputs → different UUIDs
let id3 = AikiSessionId::new(AgentType::Cursor, "cursor-xyz")?;
assert_ne!(id1.as_str(), id3.as_str());

// Agent type provides namespacing
let claude = AikiSessionId::new(AgentType::Claude, "session-123")?;
let cursor = AikiSessionId::new(AgentType::Cursor, "session-123")?;
assert_ne!(claude.as_str(), cursor.as_str());

// All inputs are safe (hashing handles sanitization)
AikiSessionId::new(AgentType::Claude, "../../etc/passwd")?;  // Safe!
AikiSessionId::new(AgentType::Cursor, "session with spaces")?;  // Safe!
AikiSessionId::new(AgentType::Claude, "日本語")?;  // Safe!
```

## Pattern: Per-Session Files

### Core Concept

Each session gets its own file identified by the Aiki session ID (UUID). Atomic file creation (`create_new()`) guarantees that only one process successfully creates the file, ensuring SessionStart fires exactly once per session.

**Directory structure:**
```
.aiki/
└── sessions/
    ├── a7c3e5f28d4b5a9cb1e3f4567890abcd  # Cursor session 1
    ├── b8d4f6a39e5c6b0d2f4g5678901bcde  # Claude session 1
    └── c9e5g7b40f6d7c1e3g5h678902cdef  # Cursor session 2
```

**Session file format (key=value, matches [aiki] metadata blocks):**
```
[aiki-session]
uuid=a7c3e5f2-8d4b-5a9c-b1e3-f4567890abcd
agent=cursor
external_id=cursor-abc123xyz
conversation_id=conv-456
created_at=1736956800
[/aiki-session]
```

**Purpose:** Store debugging metadata to map from Aiki UUID back to agent/external_id

### File Operations

```rust
impl AikiSessionId {
    /// Write session file with metadata (atomic creation)
    pub fn write_session_file(&self, repo_path: &Path, conversation_id: Option<&str>) -> Result<bool> {
        let sessions_dir = repo_path.join(".aiki/sessions");
        let session_path = sessions_dir.join(self.as_filename());
        
        // Check if already exists
        if session_path.exists() {
            return Ok(false); // Session already recorded
        }
        
        // Atomic create
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)  // Fail if exists (O_EXCL)
            .open(&session_path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                return Ok(false); // Race: another process created it
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Create directory and retry
                fs::create_dir_all(&sessions_dir)?;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&session_path)?
            }
            Err(e) => return Err(e.into()),
        };
        
        // Write metadata in [aiki-session] format
        writeln!(file, "[aiki-session]")?;
        writeln!(file, "uuid={}", self.id)?;
        writeln!(file, "agent={}", self.agent_type.to_metadata_string())?;
        writeln!(file, "external_id={}", self.external_id)?;
        if let Some(conv_id) = conversation_id {
            writeln!(file, "conversation_id={}", conv_id)?;
        }
        writeln!(file, "created_at={}", Utc::now().timestamp())?;
        writeln!(file, "[/aiki-session]")?;
        
        Ok(true)  // New session created (~1-2ms total)
    }
}
```
## Use Case: Session Start Tracking

### Problem Statement

**Cursor's beforeSubmitPrompt hook** fires on *every* prompt submission, but we only want to fire `SessionStart` event once per conversation:

- ❌ Fire SessionStart every time → Loses semantic meaning
- ✅ Track session_id and fire SessionStart only for new sessions

**Challenge:** Hook invocations are stateless processes, so we need persistent storage.

### Solution: Session Files

Track session starts by creating session files in `.aiki/sessions/{uuid_filename}` where the filename is the deterministic Aiki session ID (UUID v5 hash of agent + external_id).

**Session file format:** [aiki-session] metadata block
```
[aiki-session]
uuid=a7c3e5f2-8d4b-5a9c-b1e3-f4567890abcd
agent=cursor
external_id=cursor-abc123xyz
conversation_id=conv-456
created_at=1736956800
[/aiki-session]
```

### Implementation

**Design Decisions:**

1. **Atomic check-and-create pattern:** Session tracking uses a single `record_session_start()` function that atomically creates session files and returns whether it's a new session. This eliminates the race condition from the check-then-act pattern.

2. **Error propagation over degradation:** The library propagates errors to callers rather than implementing graceful degradation internally. This provides better observability, flexibility, and separation of concerns—hooks implement their own degradation policies based on context.

**File:** `cli/src/session_tracking.rs`

```rust
use anyhow::Result;
use std::path::Path;
use uuid::Uuid;

use crate::error::AikiError;
use crate::provenance::AgentType;

// Note: Add to cli/src/error.rs:
// #[error("Invalid session ID: {0}")]
// InvalidSessionId(String),

/// Record a session start event by creating a session file
///
/// Uses atomic file creation (O_EXCL) to ensure exactly one SessionStart
/// event fires per Aiki session ID, even with concurrent hook invocations.
///
/// Takes the external session ID from the IDE/agent and generates a deterministic
/// Aiki session ID (UUID v5) from the (agent_type, external_session_id) tuple.
///
/// Returns `Ok(true)` if the session file was created (new session), `Ok(false)` 
/// if it already existed (lost race), or `Err` on filesystem errors.
///
/// # Errors
///
/// Returns errors for:
/// - `InvalidSessionId` - External session ID is empty
/// - I/O errors - `PermissionDenied`, disk full, readonly filesystem, etc.
///
/// **Caller responsibility:** Handle errors gracefully to avoid blocking hook execution.
pub fn record_session_start(
    repo_path: &Path,
    agent_type: AgentType,
    external_session_id: &str,
    conversation_id: Option<&str>,
) -> Result<bool> {
    // Generate deterministic Aiki session ID
    let session_id = AikiSessionId::new(agent_type, external_session_id)?;
    
    // Write session file with metadata (atomic creation)
    session_id.write_session_file(repo_path, conversation_id)
}

/// Check if a session exists (read-only)
///
/// Returns true if this session has been recorded before, false otherwise.
///
/// **Note:** This function is primarily for testing and diagnostics. For session
/// tracking in hooks, use `record_session_start()` directly instead of checking
/// existence first (which would introduce a race condition).
///
/// Performance: ~0.1ms (single stat syscall)
pub fn has_session(
    repo_path: &Path,
    agent_type: AgentType,
    external_session_id: &str,
) -> Result<bool> {
    let session_id = AikiSessionId::new(agent_type, external_session_id)?;
    let session_path = repo_path
        .join(".aiki/sessions")
        .join(session_id.as_filename());
    
    Ok(session_path.exists())  // Fast: ~0.1ms (stat syscall)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn setup_test_repo() -> TempDir {
        // Session tracking only uses .aiki/sessions/ directory
        // No JJ repository needed (tests filesystem operations only)
        tempfile::tempdir().unwrap()
    }
    
    #[test]
    fn test_record_and_query_session() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();
        
        // Initially no sessions
        assert!(!has_session(repo_path, AgentType::Cursor, "test-session-1").unwrap());
        
        // Record first session
        assert!(record_session_start(
            repo_path,
            AgentType::Cursor,
            "test-session-1",
            Some("conv-1"),
        ).unwrap());
        
        // Now session exists
        assert!(has_session(repo_path, AgentType::Cursor, "test-session-1").unwrap());
        
        // Different session doesn't exist
        assert!(!has_session(repo_path, AgentType::Cursor, "test-session-2").unwrap());
        
        // Record second session
        assert!(record_session_start(
            repo_path,
            AgentType::Cursor,
            "test-session-2",
            Some("conv-2"),
        ).unwrap());
        
        // Both sessions exist
        assert!(has_session(repo_path, AgentType::Cursor, "test-session-1").unwrap());
        assert!(has_session(repo_path, AgentType::Cursor, "test-session-2").unwrap());
    }
    
    #[test]
    fn test_multiple_records_same_session() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();
        
        // Record same session twice (should be idempotent)
        let first = record_session_start(
            repo_path,
            AgentType::Cursor,
            "test-session-1",
            Some("conv-1"),
        ).unwrap();
        assert!(first); // First call creates file
        
        let second = record_session_start(
            repo_path,
            AgentType::Cursor,
            "test-session-1",
            Some("conv-1"),
        ).unwrap();
        assert!(!second); // Second call returns false (already exists)
        
        // Session exists
        assert!(has_session(repo_path, AgentType::Cursor, "test-session-1").unwrap());
        
        // Only one session file created
        let sessions_dir = repo_path.join(".aiki/sessions");
        let session_count = fs::read_dir(sessions_dir)
            .unwrap()
            .count();
        assert_eq!(session_count, 1);
    }
    
    #[test]
    fn test_concurrent_session_creation() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        
        let temp_dir = setup_test_repo();
        let repo_path = Arc::new(temp_dir.path().to_path_buf());
        
        // Simulate two processes trying to create same session simultaneously
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = vec![];
        
        for _ in 0..2 {
            let repo_path = Arc::clone(&repo_path);
            let barrier = Arc::clone(&barrier);
            
            let handle = thread::spawn(move || {
                barrier.wait();  // Synchronize start
                record_session_start(
                    &repo_path,
                    AgentType::Cursor,
                    "test-session-concurrent",
                    Some("conv-1"),
                )
            });
            
            handles.push(handle);
        }
        
        // Both threads complete successfully (one creates, one returns false)
        let results: Vec<bool> = handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect();
        
        // Exactly one returned true (created), one returned false (already exists)
        assert_eq!(results.iter().filter(|&&r| r).count(), 1);
        
        // Session exists
        assert!(has_session(&repo_path, AgentType::Cursor, "test-session-concurrent").unwrap());
        
        // Only one session file
        let sessions_dir = repo_path.join(".aiki/sessions");
        let session_count = fs::read_dir(sessions_dir)
            .unwrap()
            .count();
        assert_eq!(session_count, 1);
    }
    
    #[test]
    fn test_deterministic_session_ids() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();
        
        // Same (agent, external_id) should be idempotent
        assert!(record_session_start(
            repo_path,
            AgentType::Claude,
            "session-123",
            None,
        ).unwrap());
        
        // Second call with same params returns false
        assert!(!record_session_start(
            repo_path,
            AgentType::Claude,
            "session-123",
            None,
        ).unwrap());
        
        // Different agent, same external_id = different session
        assert!(record_session_start(
            repo_path,
            AgentType::Cursor,
            "session-123",
            None,
        ).unwrap());
        
        // Should have 2 session files
        let sessions_dir = repo_path.join(".aiki/sessions");
        let session_count = fs::read_dir(sessions_dir).unwrap().count();
        assert_eq!(session_count, 2);
    }
}
```

### Integration with Cursor

**File:** `cli/src/vendors/cursor.rs`

```rust
/// Handle a Cursor event
pub fn handle(event_name: &str) -> Result<()> {
    let payload: CursorPayload = super::read_stdin_json()?;
    
    // Build event from payload
    let aiki_event = match event_name {
        "beforeSubmitPrompt" => {
            let repo_path = std::env::current_dir()?;
            
            // Record session start atomically (~1-2ms)
            // Generates deterministic Aiki session ID from (Cursor, payload.session_id)
            // Returns Ok(true) if new session, Ok(false) if existing, Err on failure
            let is_new_session = match crate::session_tracking::record_session_start(
                &repo_path,
                AgentType::Cursor,
                &payload.session_id,
                Some(&payload.conversation_id),
            ) {
                Ok(is_new) => is_new,
                Err(e) => {
                    // Graceful degradation: log error but don't block hook execution
                    // Transient failures (permissions, disk full) shouldn't prevent prompts
                    eprintln!("Warning: Session tracking failed: {}", e);
                    eprintln!("Firing SessionStart event anyway (graceful degradation)");
                    true  // Treat as new session
                }
            };
            
            if is_new_session {
                // Fire SessionStart event only if we successfully created the session file
                // (or if session tracking failed and we're degrading gracefully)
                let session_start = build_session_start_event(&payload);
                let session_response = event_bus::dispatch(session_start)?;
                
                // If SessionStart blocks, return early
                if session_response.is_blocking() {
                    let cursor_response = translate_before_submit_prompt(&session_response);
                    cursor_response.print_json();
                    std::process::exit(cursor_response.exit_code);
                }
            }
            
            // Always fire PrePrompt
            build_pre_prompt_event(payload)
        }
        "beforeMCPExecution" | "beforeShellExecution" => build_pre_file_change_event(payload),
        "afterFileEdit" => build_post_file_change_event(payload),
        "stop" => build_post_response_event(payload),
        _ => AikiEvent::Unsupported,
    };
    
    // Dispatch event and exit with translated response
    let aiki_response = event_bus::dispatch(aiki_event)?;
    let cursor_response = translate_response(aiki_response, event_name);
    
    cursor_response.print_json();
    std::process::exit(cursor_response.exit_code);
}

/// Build SessionStart event from beforeSubmitPrompt payload
fn build_session_start_event(payload: &CursorPayload) -> AikiEvent {
    AikiEvent::SessionStart(AikiStartEvent {
        agent_type: AgentType::Cursor,
        session_id: Some(payload.session_id.clone()),
        cwd: PathBuf::from(&payload.working_directory),
        timestamp: chrono::Utc::now(),
    })
}
```

### Flow

```
┌─────────────────────────────────────────────────────────┐
│ Cursor: User submits first prompt                      │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ beforeSubmitPrompt hook fires                           │
│ session_id: "cursor-abc123"                            │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ record_session_start(Cursor, "cursor-abc123")          │
│ → Generate UUID: a7c3e5f28d4b5a9cb1e3f4567890abcd     │
│ → Atomically create .aiki/sessions/a7c3e5f28d4b...     │
│ → Write [aiki-session] metadata block                  │
│ → Returns: true (new session, ~1-2ms)                  │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ Dispatch SessionStart event                             │
│ → Run SessionStart flows                               │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ Dispatch PrePrompt event                                │
│ → Run PrePrompt flows                                  │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ Return response to Cursor                               │
└─────────────────────────────────────────────────────────┘

                     ... (user gets response) ...

┌─────────────────────────────────────────────────────────┐
│ Cursor: User submits second prompt (same conversation) │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ beforeSubmitPrompt hook fires                           │
│ session_id: "cursor-abc123" (same)                     │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ record_session_start(Cursor, "cursor-abc123")          │
│ → Generate same UUID: a7c3e5f28d4b5a9cb1e3f4567890abcd │
│ → Try to create .aiki/sessions/a7c3e5f28d4b...         │
│ → File already exists (AlreadyExists error)            │
│ → Returns: false (existing session, ~0.1ms)            │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ Skip SessionStart (is_new_session = false)             │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ Dispatch PrePrompt event only                           │
└─────────────────────────────────────────────────────────┘
```

## Concurrency Handling

### Scenario 1: Different Sessions (Multiple Windows)

```
Time  Cursor Window 1           Cursor Window 2
──────────────────────────────────────────────────
0ms   beforeSubmitPrompt       beforeSubmitPrompt
      session_id=abc123        session_id=def456
      
1ms   record_session_start()   record_session_start()
      → create abc123          → create def456
      → Success ✅ (true)      → Success ✅ (true)
      
2ms   Fire SessionStart        Fire SessionStart
      (for abc123)             (for def456)
```

**Result:** Both sessions fire SessionStart correctly ✅ (they're independent sessions)

### Scenario 2: Same Session (Race Condition)

```
Time  Hook Process A            Hook Process B
──────────────────────────────────────────────────
0ms   Prompt 1: session=abc123 Prompt 2: session=abc123
      (extremely rapid prompts)
      
1ms   record_session_start()   record_session_start()
      → try create abc123      → try create abc123
      
2ms   → Success ✅ (true)      → AlreadyExists (false)
      
3ms   Fire SessionStart ✅     Skip SessionStart ✅
```

**Result:** Only first prompt fires SessionStart ✅ (atomic file creation prevents duplicate)

**Why this works:**
- `OpenOptions::create_new()` uses `O_EXCL` flag
- Operating system guarantees atomic file creation
- Second process gets `AlreadyExists` error (handled gracefully)
- No file locking or complex sync needed

## Maintenance

### Auto-Cleanup on Session End

**Design:** Delete session file when emitting `SessionEnd` event (when session naturally ends without autoreply).

**SessionEnd Event Design:**
- `PostResponse` handler checks if there's an autoreply to send
- If **no autoreply**: Emit `SessionEnd` event (handler cleans up session file)
- If **autoreply**: Keep session active (don't emit SessionEnd, don't clean up)

**Why this is correct:**
- ✅ **SessionEnd means the conversation is truly over** - no more interaction expected
- ✅ **Autoreply continues the session** - user will see response, provide feedback, session continues
- ✅ **Clean lifecycle:** `SessionStart` → (multiple prompts/responses) → `SessionEnd`
- ✅ **Mirrors SessionStart semantics** - bookends the session lifecycle

**Edge cases:**
- **Session with autoreply**: File remains (correct - session continues, user will respond)
- **Session crashes without PostResponse**: File remains (acceptable - will be reused on next start)
- **Multiple windows**: Each session cleans up independently when it ends
- **Rapid restart**: New session creates new file (correct behavior)

**Event Lifecycle Examples:**

1. **Simple session (no autoreply):**
   ```
   SessionStart → PrePrompt → PostFileChange → PostResponse → SessionEnd
   ```
   Session file created on SessionStart, deleted on SessionEnd.

2. **Session with autoreply:**
   ```
   SessionStart → PrePrompt → PostFileChange → PostResponse (autoreply) → PrePrompt → PostResponse → SessionEnd
   ```
   Session file created on SessionStart, kept alive through autoreply, deleted on final SessionEnd.

3. **Long-running session:**
   ```
   SessionStart → [multiple prompts/responses] → PostResponse → SessionEnd
   ```
   Session file persists throughout, deleted when conversation naturally ends.

**Implementation:** See [SessionEnd Event Implementation](#sessionend-event-implementation) section below for detailed code.

### Storage Growth

**Session file format:** [aiki-session] metadata block (~150 bytes)
```
[aiki-session]
uuid=a7c3e5f2-8d4b-5a9c-b1e3-f4567890abcd
agent=cursor
external_id=cursor-abc123xyz
conversation_id=conv-456
created_at=1736956800
[/aiki-session]
```

**With auto-cleanup:**
- Active sessions only (typically 1-5 files at a time)
- ~150 bytes per session file (metadata block)
- **Total:** ~750 bytes (5 concurrent sessions)
- **Conclusion:** Storage is negligible, bounded by concurrent sessions

**Without auto-cleanup (if PostResponse never fires):**
- Worst case: 10 sessions/day × 365 days = 3,650 orphaned files/year
- ~150 bytes × 3,650 = ~548 KB/year
- **Conclusion:** Even without cleanup, storage growth is minimal (~0.5 MB/year)

## Future Use Cases

This pattern can be extended for other stateful tracking needs:

### 1. Rate Limiting / Quota Tracking
```
.aiki/quota/
  ├── session-abc123-2025-01-15.quota   # Per-session, per-day
  └── session-def456-2025-01-15.quota
```

**Format:**
```
tool=Edit
count=5
window=2025-01-15T10:00:00Z
```

### 2. Multi-Agent Coordination Locks
```
.aiki/locks/
  ├── src__auth.rs.lock   # File path with / → __
  └── src__db.rs.lock
```

**Format:**
```
file=src/auth.rs
agent=cursor
session_id=abc123
acquired_at=2025-01-15T10:00:00Z
```

### 3. Feature Flags / Configuration
```
.aiki/config/
  ├── auto_sign.flag
  └── require_approval.flag
```

**Format:**
```
value=true
changed_at=2025-01-15T10:00:00Z
```

## Comparison to Other Approaches

| Aspect | File Markers | Single State File | JJ Branch | SQLite |
|--------|--------------|-------------------|-----------|--------|
| **Read latency** | ✅ ~0.1ms | ✅ ~0.5ms | ❌ ~25ms | ✅ ~1ms |
| **Write latency** | ✅ ~1-2ms | ✅ ~1.6ms | ❌ ~50ms | ✅ ~2ms |
| **Concurrent sessions** | ✅ Perfect | ❌ Overwrites | ✅ Yes | ✅ Yes |
| **Race conditions** | ✅ Atomic (O_EXCL) | ⚠️ Needs locking | ✅ JJ handles | ✅ ACID |
| **History** | ❌ No | ❌ No | ✅ Full log | 🟡 If designed |
| **Queryability** | ❌ Manual | ❌ Manual | ✅ Revsets | ✅ SQL |
| **Complexity** | ✅ Trivial | ✅ Simple | ⚠️ Medium | ⚠️ Medium |
| **Debugging** | ✅ `ls .aiki/sessions/` | ✅ `cat state.json` | ✅ `jj log` | 🟡 SQL client |
| **Storage growth** | ✅ Linear | ✅ Fixed | ⚠️ Linear | ✅ Bounded |
| **Integration** | ✅ Standalone | ✅ Standalone | ✅ JJ-native | ⚠️ External |

**Recommendation:**
- **Session tracking:** Use file sessions (optimal for fast, concurrent, stateless checks)
- **Task system:** Use JJ branch + SQLite cache (rich metadata, queryable, event-sourced)
- **Provenance:** Use JJ change descriptions (tight coupling with code changes)

## Error Handling

### Design Philosophy

**Session tracking errors are propagated to callers** who implement graceful degradation policies. This provides:
- ✅ Clear separation of concerns (library vs policy)
- ✅ Better observability (hook knows when degraded)
- ✅ Flexibility (different hooks, different policies)

### Missing Sessions Directory

If `.aiki/sessions/` doesn't exist:
- ✅ First hook invocation attempts one-time auto-creation
- ✅ Returns `SessionDirectoryNotFound` error if auto-creation fails
- ✅ Hook handles error gracefully (fires SessionStart anyway)
- ✅ `aiki doctor` detects and fixes the missing directory

**Example:**
```rust
match record_session_start(&repo, &session_id, ...) {
    Ok(is_new) => is_new,
    Err(e) => {
        eprintln!("Warning: Session tracking failed: {}", e);
        eprintln!("Firing SessionStart event anyway (graceful degradation)");
        true  // Treat as new session
    }
}
```

### Disk Full / Permissions

If session file creation fails (disk full, permissions):
- ✅ `record_session_start()` returns error with context
- ✅ Hook logs warning to stderr with error details
- ✅ Hook treats as new session (fires SessionStart anyway)
- ✅ Hook execution continues (doesn't block user prompts)

**Graceful degradation at hook level:**
```rust
let is_new_session = match record_session_start(&repo, &session_id, ...) {
    Ok(is_new) => is_new,
    Err(e) => {
        // Log error but don't block hook execution
        eprintln!("Warning: Session tracking failed: {}", e);
        true  // Treat as new session
    }
};

if is_new_session {
    fire_session_start()?;
}
```

**Why this design:**
- Transient filesystem issues shouldn't prevent users from working
- SessionStart may fire multiple times in degraded mode (acceptable trade-off)
- Original error is preserved in logs for debugging
- Hook can add metrics/telemetry for degraded mode
- Different hooks can implement different policies

### Concurrent Creation (Same Session)

Atomic file creation handles this naturally:
- First process: `create_new()` succeeds → Fire SessionStart
- Second process: `create_new()` returns `AlreadyExists` → Skip SessionStart
- No race condition, no duplicate events

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_session_lifecycle() {
    let repo = setup_test_repo();
    let repo_path = repo.path();
    
    // 1. Initially no sessions
    assert!(!has_session(repo_path, AgentType::Cursor, "test-123").unwrap());
    
    // 2. Record session
    record_session_start(repo_path, AgentType::Cursor, "test-123", None).unwrap();
    
    // 3. Session now exists
    assert!(has_session(repo_path, AgentType::Cursor, "test-123").unwrap());
    
    // 4. Different session doesn't exist
    assert!(!has_session(repo_path, AgentType::Cursor, "test-456").unwrap());
}

#[test]
fn test_idempotent_recording() {
    let repo = setup_test_repo();
    let repo_path = repo.path();
    
    // Record twice
    record_session_start(repo_path, AgentType::Cursor, "test-123", None).unwrap();
    record_session_start(repo_path, AgentType::Cursor, "test-123", None).unwrap();
    
    // Only one session file
    let session_count = fs::read_dir(repo_path.join(".aiki/sessions")).unwrap().count();
    assert_eq!(session_count, 1);
}

#[test]
fn test_concurrent_creation() {
    use std::sync::{Arc, Barrier};
    use std::thread;
    
    let temp_dir = setup_test_repo();
    let repo_path = Arc::new(temp_dir.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = vec![];
    
    // Two threads create same session simultaneously
    for _ in 0..2 {
        let repo_path = Arc::clone(&repo_path);
        let barrier = Arc::clone(&barrier);
        
        let handle = thread::spawn(move || {
            barrier.wait();
            record_session_start(&repo_path, AgentType::Cursor, "test-123", None)
        });
        
        handles.push(handle);
    }
    
    // Both complete successfully (one creates, one sees AlreadyExists)
    for handle in handles {
        handle.join().unwrap().unwrap();
    }
    
    // Exactly one session file created
    let session_count = fs::read_dir(repo_path.join(".aiki/sessions")).unwrap().count();
    assert_eq!(session_count, 1);
}
```

### Integration Tests

```rust
#[test]
fn test_cursor_session_start_fires_once() {
    let repo = setup_test_repo();
    let repo_path = repo.path();
    install_aiki_hooks(&repo);
    
    // First prompt: Should fire SessionStart + PrePrompt
    simulate_cursor_hook(&repo, "beforeSubmitPrompt", json!({
        "sessionId": "test-123",
        "workingDirectory": repo_path,
        "prompt": "Hello",
        "conversation_id": "conv-1",
    }));
    
    assert!(has_session(repo_path, AgentType::Cursor, "test-123").unwrap());
    
    // Second prompt: Should fire only PrePrompt
    simulate_cursor_hook(&repo, "beforeSubmitPrompt", json!({
        "sessionId": "test-123",
        "workingDirectory": repo_path,
        "prompt": "How are you?",
        "conversation_id": "conv-1",
    }));
    
    // Still only one session file
    let session_count = fs::read_dir(repo_path.join(".aiki/sessions")).unwrap().count();
    assert_eq!(session_count, 1);
}
```

## Implementation Checklist

- [ ] Add dependency to `Cargo.toml`
  - [ ] `uuid = { version = "1.6", features = ["v5"] }`
- [ ] Update `cli/src/error.rs`
  - [ ] Add `InvalidSessionId(String)` error variant
  - [ ] Error message: "Invalid session ID: {0}"
- [ ] Update `aiki init` command
  - [ ] Create `.aiki/sessions/` directory during initialization
  - [ ] Add to init output: "Session tracking initialized"
- [ ] Update `aiki doctor` command
  - [ ] Check if `.aiki/sessions/` directory exists
  - [ ] Create directory if missing with fix suggestion
  - [ ] Report directory health (permissions, writability)
  - [ ] Count and display number of active sessions
- [ ] Create `cli/src/session_tracking.rs`
  - [ ] `AikiSessionId` struct with UUID v5 generation
  - [ ] `AikiSessionId::new()` - deterministic ID from (agent, external_id)
  - [ ] `AikiSessionId::write_session_file()` - atomic creation with metadata
  - [ ] `record_session_start()` - public API wrapper
  - [ ] `has_session()` - read-only existence check (for tests)
  - [ ] Unit tests:
    - [ ] Deterministic UUID generation (same input → same output)
    - [ ] Agent namespacing (Claude "session-123" ≠ Cursor "session-123")
    - [ ] Idempotency (second call returns false)
    - [ ] Concurrency (atomic creation handles race conditions)
    - [ ] Special characters handled safely (path traversal, etc.)
    - [ ] Session file format ([aiki-session] block)
- [ ] Update `cli/src/vendors/cursor.rs`
  - [ ] Update `beforeSubmitPrompt` to use new session tracking API
  - [ ] Pass `AgentType::Cursor` as first parameter
  - [ ] Implement graceful degradation for session tracking errors
  - [ ] Fire SessionStart for new sessions only
- [ ] Update `cli/src/vendors/claude_code.rs` (when implemented)
  - [ ] Add session tracking to appropriate hooks
  - [ ] Pass `AgentType::Claude` as first parameter
- [ ] Add integration tests
  - [ ] Test SessionStart fires once per Aiki session ID
  - [ ] Test multiple windows/agents with same external ID (different UUIDs)
  - [ ] Test rapid prompts (same session, deterministic UUID)
  - [ ] Test concurrent session creation race condition
  - [ ] Test graceful degradation when `.aiki/sessions/` deleted (auto-creation)
- [ ] Update `cli/src/lib.rs`
  - [ ] Export `session_tracking` module
- [ ] Documentation
  - [ ] ✅ Update ops/current/session-tracking.md with UUID v5 design
  - [ ] Update CLAUDE.md with session tracking pattern
  - [ ] Add session tracking example to README
  - [ ] Document .aiki/sessions/ directory structure
  - [ ] Document Aiki session ID format (UUID v5, deterministic)
  - [ ] Document session file format ([aiki-session] metadata)

## Doctor Integration

The `aiki doctor` command should verify session tracking health:

```rust
// In cli/src/commands/doctor.rs

fn check_session_tracking(repo_path: &Path) -> DiagnosticResult {
    let sessions_dir = repo_path.join(".aiki/sessions");
    
    // Check if directory exists
    if !sessions_dir.exists() {
        return DiagnosticResult::Warning {
            check: "Session tracking directory",
            message: ".aiki/sessions/ directory is missing",
            fix: "Creating directory now...",
            action: || {
                fs::create_dir_all(&sessions_dir)?;
                println!("✓ Created .aiki/sessions/ directory");
                Ok(())
            }
        };
    }
    
    // Check if directory is writable
    let test_session = sessions_dir.join(".doctor-test");
    match fs::File::create(&test_session) {
        Ok(_) => {
            let _ = fs::remove_file(&test_session);
            DiagnosticResult::Ok {
                check: "Session tracking directory",
                message: format!("Writable ({} sessions)", count_sessions(&sessions_dir))
            }
        }
        Err(e) => {
            DiagnosticResult::Error {
                check: "Session tracking directory",
                message: format!("Directory exists but not writable: {}", e),
                fix: "Check filesystem permissions on .aiki/sessions/"
            }
        }
    }
}

fn count_sessions(sessions_dir: &Path) -> usize {
    fs::read_dir(sessions_dir)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0)
}
```

**Example output:**

```bash
$ aiki doctor
Checking Aiki installation health...

✓ JJ repository initialized
✓ Git hooks installed
✓ Session tracking directory: Writable (42 sessions)
✓ Provenance recording: Working

All checks passed!
```

**When directory is missing:**

```bash
$ aiki doctor
Checking Aiki installation health...

✓ JJ repository initialized
⚠ Session tracking directory: Missing
  → Creating directory now...
  ✓ Created .aiki/sessions/ directory
✓ Git hooks installed
✓ Provenance recording: Working

Fixed 1 issue automatically.
```

## Future Work (Deferred)

Diagnostic and manual cleanup commands deferred until requested by users:

- [ ] Add `aiki session list` command (show active sessions with timestamps)
- [ ] Add `aiki session stats` command (show session count/storage usage)
- [ ] Add `aiki session prune` command (manual cleanup of orphaned files from crashes)
- [ ] Consider `aiki doctor --fix-all` to auto-clean orphaned session files

**Note:** Auto-cleanup on PostResponse (implemented) handles the common case.
These commands are for diagnostics and handling edge cases (crashes, debugging).

## SessionEnd Event Implementation

### Event Definition

**Location:** `cli/src/events.rs`

```rust
/// Session end event (emitted when agent session ends without autoreply)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiSessionEndEvent {
    pub agent_type: AgentType,
    pub session_id: String,  // Required - always present by this point
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}
```

Add to `AikiEvent` enum:
```rust
pub enum AikiEvent {
    SessionStart(AikiStartEvent),
    PrePrompt(AikiPrePromptEvent),
    PreFileChange(AikiPreFileChangeEvent),
    PostFileChange(AikiPostFileChangeEvent),
    PostResponse(AikiPostResponseEvent),
    SessionEnd(AikiSessionEndEvent),  // NEW
    PrepareCommitMessage(AikiPrepareCommitMessageEvent),
    Unsupported,
}
```

### Event Dispatch

**Location:** `cli/src/event_bus.rs`

```rust
pub fn dispatch(event: AikiEvent) -> Result<HookResponse> {
    match event {
        AikiEvent::SessionStart(e) => handlers::handle_session_start(e),
        AikiEvent::PrePrompt(e) => handlers::handle_pre_prompt(e),
        AikiEvent::PreFileChange(e) => handlers::handle_pre_file_change(e),
        AikiEvent::PostFileChange(e) => handlers::handle_post_file_change(e),
        AikiEvent::PostResponse(e) => handlers::handle_post_response(e),
        AikiEvent::SessionEnd(e) => handlers::handle_session_end(e),  // NEW
        AikiEvent::PrepareCommitMessage(e) => handlers::handle_prepare_commit_message(e),
        AikiEvent::Unsupported => Ok(HookResponse::new()),
    }
}
```

### Event Handler

**Location:** `cli/src/handlers.rs`

```rust
/// Handle SessionEnd event
///
/// Fires when an agent session ends without autoreply. Allows flows to react
/// to session completion (e.g., analytics, notifications, final validations).
///
/// This handler also cleans up the session file after executing SessionEnd flows.
pub fn handle_session_end(event: AikiSessionEndEvent) -> Result<HookResponse> {
    // Load core flow
    let flow = Flow::load_core(&event.cwd)?;
    
    // Initialize execution state
    let mut state = ExecutionState::new(&event.cwd);
    state.set_variable("event.agent", event.agent_type.to_metadata_string());
    state.set_variable("event.session_id", event.session_id.clone());
    
    // Execute SessionEnd handlers
    let results = FlowEngine::execute_statements(&flow.session_end, &mut state)?;
    
    // Collect failures (SessionEnd doesn't have autoreply)
    let mut response = HookResponse::new();
    for result in results {
        if let Some(failure) = result.failure {
            response.failures.push(failure);
        }
    }
    
    // Clean up session file (after flows have executed)
    if let Err(e) = session_tracking::remove_session(
        &event.cwd,
        event.agent_type,
        &event.session_id,
    ) {
        eprintln!("Warning: Failed to clean up session file: {}", e);
    }
    
    Ok(response)
}
```

### Flow Type Update

**Location:** `cli/src/flows/types.rs`

Add `session_end` field to `Flow` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    #[serde(rename = "SessionStart", default)]
    pub session_start: Vec<FlowStatement>,
    
    #[serde(rename = "PrePrompt", default)]
    pub pre_prompt: Vec<FlowStatement>,
    
    #[serde(rename = "PreFileChange", default)]
    pub pre_file_change: Vec<FlowStatement>,
    
    #[serde(rename = "PostFileChange", default)]
    pub post_file_change: Vec<FlowStatement>,
    
    #[serde(rename = "PostResponse", default)]
    pub post_response: Vec<FlowStatement>,
    
    /// SessionEnd event handler (fires when session ends without autoreply)
    #[serde(rename = "SessionEnd", default)]
    pub session_end: Vec<FlowStatement>,  // NEW
    
    #[serde(rename = "PrepareCommitMessage", default)]
    pub prepare_commit_message: Vec<FlowStatement>,
}
```

### PostResponse Handler Update

**Location:** `cli/src/handlers.rs` - Update existing `handle_post_response()`

```rust
pub fn handle_post_response(event: AikiPostResponseEvent) -> Result<HookResponse> {
    // Load core flow
    let flow = Flow::load_core(&event.cwd)?;
    
    // Initialize execution state
    let mut state = ExecutionState::new(&event.cwd);
    state.set_variable("event.agent", event.agent_type.to_metadata_string());
    if let Some(ref session_id) = event.session_id {
        state.set_variable("event.session_id", session_id.clone());
    }
    state.set_variable("event.response", event.response.clone());
    
    // Execute PostResponse handlers
    let results = FlowEngine::execute_statements(&flow.post_response, &mut state)?;
    
    // Collect autoreply actions and failures
    let mut response = HookResponse::new();
    for result in results {
        if let Some(autoreply_text) = result.autoreply {
            response.context.push_str(&autoreply_text);
            if !response.context.ends_with('\n') {
                response.context.push('\n');
            }
        }
        if let Some(failure) = result.failure {
            response.failures.push(failure);
        }
    }
    
    // If no autoreply, emit SessionEnd event
    // (SessionEnd handler will clean up the session file)
    if response.context.is_empty() {
        if let Some(session_id) = event.session_id {
            // Emit SessionEnd event (flows can react to session ending)
            let session_end_event = AikiSessionEndEvent {
                agent_type: event.agent_type,
                session_id: session_id.clone(),
                cwd: event.cwd.clone(),
                timestamp: Utc::now(),
            };
            
            // Dispatch SessionEnd through event bus
            // Note: handle_session_end() will clean up the session file
            let session_end_response = event_bus::dispatch(session_end_event.into())?;
            
            // Merge SessionEnd failures into PostResponse response
            response.failures.extend(session_end_response.failures);
        }
    }
    
    Ok(response)
}
```

### Example Flow Usage

**File:** `.aiki/flows/core.yml`

```yaml
SessionEnd:
  - Log:
      message: "Session {event.session_id} ended for {event.agent}"
  
  - Bash:
      command: |
        echo "Session analytics: completed at $(date)"
        # Could send to analytics service, update stats, etc.
```

### Testing

**Location:** `cli/tests/test_session_end.rs` (new file)

```rust
#[test]
fn test_session_end_fires_without_autoreply() {
    let temp_dir = setup_test_repo();
    
    // PostResponse with no autoreply should fire SessionEnd
    let event = AikiPostResponseEvent {
        agent_type: AgentType::Cursor,
        session_id: Some("session-123".to_string()),
        cwd: temp_dir.path().to_path_buf(),
        timestamp: Utc::now(),
        response: "Done!".to_string(),
        modified_files: vec![],
    };
    
    let response = handle_post_response(event)?;
    
    // Should have cleaned up session file
    let session_file = temp_dir.path()
        .join(".aiki/sessions")
        .join(AikiSessionId::new(AgentType::Cursor, "session-123")?.as_filename());
    assert!(!session_file.exists());
}

#[test]
fn test_session_end_not_fired_with_autoreply() {
    let temp_dir = setup_test_repo();
    
    // Create flow with autoreply
    write_flow(temp_dir.path(), "PostResponse:\n  - Autoreply: { message: 'Continue?' }");
    
    // Record session
    record_session_start(temp_dir.path(), AgentType::Cursor, "session-123")?;
    
    let event = AikiPostResponseEvent {
        agent_type: AgentType::Cursor,
        session_id: Some("session-123".to_string()),
        cwd: temp_dir.path().to_path_buf(),
        timestamp: Utc::now(),
        response: "Done!".to_string(),
        modified_files: vec![],
    };
    
    let response = handle_post_response(event)?;
    
    // Should have autoreply
    assert!(!response.context.is_empty());
    
    // Session file should still exist (session continues)
    let session_file = temp_dir.path()
        .join(".aiki/sessions")
        .join(AikiSessionId::new(AgentType::Cursor, "session-123")?.as_filename());
    assert!(session_file.exists());
}
```

## Design Decisions

### D1: Aiki Session ID Format

**Decision:** Use UUID v5 (deterministic, SHA-1 namespace-based)

**Rationale:**
- ✅ Deterministic: Same `(agent_type, external_session_id)` always produces same UUID
- ✅ No sanitization needed: Hash function makes any input filesystem-safe
- ✅ Collision-resistant: SHA-1 provides strong uniqueness guarantees
- ✅ Agent namespacing: Different agents with same external ID get different UUIDs
- ✅ Universal format: Clean UUID works everywhere (files, logs, databases, provenance)

**Rejected alternatives:**
- ULID: Not deterministic (includes random bits)
- UUID v7: Not deterministic (includes random component)
- Prefix + sanitize: Complex sanitization logic, redundant strings

### D2: Session File Metadata

**Decision:** Store debugging metadata in [aiki-session] blocks

**Format:**
```
[aiki-session]
uuid=a7c3e5f2-8d4b-5a9c-b1e3-f4567890abcd
agent=cursor
external_id=cursor-abc123xyz
conversation_id=conv-456
created_at=1736956800
[/aiki-session]
```

**Rationale:**
- ✅ Consistent with [aiki] provenance format
- ✅ Easy to parse with same logic
- ✅ Debugging: Can map from UUID back to agent/external_id
- ✅ Conversation tracking: Optional conversation_id for context
- ❌ No `last_seen`: Adds complexity, not needed for current use case

### D3: Session ID Validation

**Decision:** Only validate non-empty, no length limits

**Rationale:**
- ✅ UUID v5 hashing makes any input safe (no path traversal, special chars, etc.)
- ✅ Empty check ensures semantic validity
- ❌ No length limits: Avoids arbitrary restrictions that could break valid use cases
- ❌ No character checks: Hashing handles everything

### D4: Session Lifecycle

**Decision:** Track by `session_id` (not `conversation_id`)

**Rationale:**
- SessionStart semantics: "Agent started a new session"
- Cursor's `conversation_id` may not be stable across IDE restarts
- More granular tracking (can see session restarts within same conversation)

### D5: Session File Cleanup

**Decision:** Auto-cleanup on SessionEnd event (when PostResponse has no autoreply)

**Rationale:**
- ✅ **Semantic correctness** - SessionEnd only fires when session truly ends (no autoreply)
- ✅ **Automatic** - No manual cleanup needed
- ✅ **Bounded storage** - Active sessions only (completed sessions cleaned up immediately)
- ✅ **Simple** - Delete on natural session end in SessionEnd handler
- ✅ **Flow integration** - Flows can react to SessionEnd (analytics, notifications, etc.)
- ❌ Session crashes without PostResponse leave orphaned files (acceptable - will be reused or manually cleaned via `aiki doctor` when needed)

## Success Criteria

- [ ] Cursor fires SessionStart exactly once per session_id
- [ ] Subsequent prompts in same session fire only PrePrompt
- [ ] Session tracking works with multiple Cursor windows (different sessions)
- [ ] Race conditions handled correctly (atomic file creation)
- [ ] Performance: <1ms overhead per hook invocation
- [ ] Graceful degradation if session file creation fails
- [ ] No file locking complexity required

## Related Documentation

- [Milestone 1.4: Task System](./milestone-1.4-task-system.md) - JJ-based event tracking for rich metadata
- [CLAUDE.md](../../CLAUDE.md) - JJ vs Git terminology
- [Phase 4 Architecture](../phase-4.md) - Event dispatch system
- [ACP Protocol Specification](https://agentclientprotocol.com/protocol/schema) - Session/update event schema
