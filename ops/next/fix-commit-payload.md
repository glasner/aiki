---
status: draft
---

# Fix commit.message_started Event Payload

## Problem

The `commit.message_started` event has an inconsistent payload structure compared to all other Aiki events:

```rust
// ❌ CURRENT: commit.message_started uses agent_type directly
pub struct AikiCommitMessageStartedPayload {
    pub agent_type: AgentType,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub commit_msg_file: Option<PathBuf>,
}

// ✅ EXPECTED: Should use session like all other events
pub struct AikiCommitMessageStartedPayload {
    pub session: AikiSession,  // Contains agent_type + external_id + metadata
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    pub commit_msg_file: Option<PathBuf>,
}
```

## Impact

1. **Inconsistent API**: Flows expect `$event.session` but this event has `$event.agent_type`
2. **Missing session context**: Can't correlate commit events with active sessions
3. **No external_id**: Can't link commits back to the editor's session for full provenance
4. **Pattern violation**: Helper methods in `AikiEvent` have to special-case this event:

```rust
// cli/src/events/mod.rs
impl AikiEvent {
    pub fn agent_type(&self) -> AgentType {
        match self {
            // ... all other events use e.session.agent_type()
            Self::CommitMessageStarted(e) => e.agent_type,  // ❌ Special case
            Self::Unsupported => AgentType::Unknown,
        }
    }
}
```

## Root Cause

The `commit.message_started` event is triggered from Git's `prepare-commit-msg` hook via:

```bash
aiki event prepare-commit-msg <commit_msg_file> <commit_source> <commit_sha>
```

This runs in a separate process from the active agent session, so there's no session context available. The hook tries to infer the agent type from environment or falls back to `AgentType::Unknown`.

**Current implementation:**
- `cli/src/commands/event.rs` - Handles `prepare-commit-msg` subcommand
- Reads `.aiki/session.json` to get agent type
- Creates event with just `agent_type`, not full `AikiSession`

## Solution

### Option 1: Use Full Session (Recommended)

Load the full session from `.aiki/session.json` instead of just the agent type:

**Changes:**
1. `cli/src/events/commit_message_started.rs`:
   - Change `agent_type: AgentType` to `session: AikiSession`
   - Update docs

2. `cli/src/commands/event.rs`:
   - In `prepare-commit-msg` handler, load full `AikiSession` from `.aiki/session.json`
   - If no session file exists, create a minimal session with `agent_type: Unknown`

**Benefits:**
- Consistent API across all events
- Full session provenance in commits
- Can correlate commits with active sessions
- Simpler code (removes special cases)

**Minimal session fallback:**
```rust
let session = AikiSession::load_from_file(session_path)
    .unwrap_or_else(|_| AikiSession {
        agent_type: AgentType::Unknown,
        external_id: None,
        // ... other default fields
    });
```

### Option 2: Keep agent_type but Document Special Case

Document that `commit.message_started` is special because it runs outside session context.

**Changes:**
1. Add clear documentation in `cli/src/events/commit_message_started.rs`
2. Update flow docs to explain `$event.agent_type` vs `$event.session`

**Benefits:**
- Minimal code changes
- Acknowledges the architectural difference

**Drawbacks:**
- API remains inconsistent
- Special cases remain in helper methods
- Missing session correlation

## Recommendation

**Go with Option 1** - Use full session.

**Rationale:**
1. The session file exists when `prepare-commit-msg` runs (created by the active editor)
2. Full session provides better provenance (external_id links commit to editor session)
3. API consistency makes flows easier to write
4. Removes special-case code in `AikiEvent` helper methods

## Implementation Plan

### Step 1: Update Event Payload Structure

**File:** `cli/src/events/commit_message_started.rs`

```rust
use crate::editors::acp::AikiSession;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Payload for commit.message_started event
///
/// Fired by Git's prepare-commit-msg hook to allow modification of the commit message.
/// Typically used for adding co-author attributions from AI session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiCommitMessageStartedPayload {
    /// Session information (loaded from .aiki/session.json)
    pub session: AikiSession,
    
    /// Current working directory where the commit is happening
    pub cwd: PathBuf,
    
    /// Timestamp when the commit message preparation started
    pub timestamp: DateTime<Utc>,
    
    /// Path to the commit message file (COMMIT_EDITMSG)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_msg_file: Option<PathBuf>,
}
```

### Step 2: Update Event Handler

**File:** `cli/src/commands/event.rs`

Find the `prepare-commit-msg` handler and update to load full session:

```rust
// Current code (simplified):
let agent_type = load_agent_type_from_session(cwd)?;
let payload = AikiCommitMessageStartedPayload {
    agent_type,
    cwd,
    timestamp,
    commit_msg_file,
};

// New code:
let session = load_session_from_file(cwd)
    .unwrap_or_else(|_| AikiSession::minimal(AgentType::Unknown));
let payload = AikiCommitMessageStartedPayload {
    session,
    cwd,
    timestamp,
    commit_msg_file,
};
```

### Step 3: Update Helper Methods

**File:** `cli/src/events/mod.rs`

Remove special case from `agent_type()` method:

```rust
impl AikiEvent {
    pub fn agent_type(&self) -> AgentType {
        match self {
            // ... all other events
            Self::CommitMessageStarted(e) => e.session.agent_type(),  // ✅ Consistent
            Self::Unsupported => AgentType::Unknown,
        }
    }
}
```

### Step 4: Add Session Loading Helper

**File:** `cli/src/editors/acp/mod.rs` or appropriate location

```rust
impl AikiSession {
    /// Create a minimal session with just agent type
    /// Used as fallback when no active session exists
    pub fn minimal(agent_type: AgentType) -> Self {
        Self {
            agent_type,
            external_id: None,
            // ... other default fields
        }
    }
    
    /// Load session from .aiki/session.json
    pub fn load_from_file(cwd: &Path) -> Result<Self> {
        let session_path = cwd.join(".aiki").join("session.json");
        let content = std::fs::read_to_string(session_path)?;
        let session: Self = serde_json::from_str(&content)?;
        Ok(session)
    }
}
```

### Step 5: Update Tests

Update any tests that construct `AikiCommitMessageStartedPayload`:

```rust
// Before:
let payload = AikiCommitMessageStartedPayload {
    agent_type: AgentType::ClaudeCode,
    cwd: PathBuf::from("/tmp"),
    timestamp: Utc::now(),
    commit_msg_file: None,
};

// After:
let payload = AikiCommitMessageStartedPayload {
    session: AikiSession::minimal(AgentType::ClaudeCode),
    cwd: PathBuf::from("/tmp"),
    timestamp: Utc::now(),
    commit_msg_file: None,
};
```

### Step 6: Update Documentation

**File:** `ops/now/code-review.md` and any flow examples

Update event payload table to show `session` field instead of `agent_type`.

## Testing Strategy

1. **Unit tests**: Test `AikiSession::minimal()` and `load_from_file()`
2. **Integration test**: 
   - Create active session
   - Run `aiki event prepare-commit-msg`
   - Verify event has full session with external_id
3. **Fallback test**:
   - No active session
   - Run `aiki event prepare-commit-msg`
   - Verify event has minimal session with `AgentType::Unknown`

## Migration Notes

This is a **breaking change** for:
- Flows that reference `$event.agent_type` for `commit.message_started`
- Should be updated to `$event.session.agent_type` (though flows likely don't exist yet)

Since this is pre-1.0, breaking changes are acceptable. Document in CHANGELOG.

## Files to Modify

1. `cli/src/events/commit_message_started.rs` - Update payload struct
2. `cli/src/commands/event.rs` - Update `prepare-commit-msg` handler
3. `cli/src/events/mod.rs` - Remove special case in `agent_type()` method
4. `cli/src/editors/acp/mod.rs` - Add session loading helpers
5. Tests that construct `AikiCommitMessageStartedPayload`
6. Documentation (`ops/now/code-review.md`, etc.)

## Success Criteria

- ✅ All events use `session: AikiSession` field
- ✅ No special cases in `AikiEvent` helper methods
- ✅ Commits can be correlated with active sessions via external_id
- ✅ Graceful fallback when no session exists
- ✅ All tests pass
- ✅ Documentation updated
