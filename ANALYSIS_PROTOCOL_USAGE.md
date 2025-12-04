# Analysis: protocol.rs Usage in acp.rs

## Current State

### What protocol.rs Exposes
```rust
// Re-exported from agent-client-protocol crate
pub use agent_client_protocol::SessionNotification;

// Custom types for ACP proxy
pub struct JsonRpcMessage { ... }
pub struct ClientInfo { name, title?, version? }
pub struct AgentInfo { name, title?, version? }
pub struct InitializeRequest { client_info?, protocol_version?, capabilities? }
pub struct InitializeResponse { agent_info?, protocol_version?, agent_capabilities?, auth_methods? }
```

### What acp.rs Currently Uses

**Imports from protocol.rs:**
```rust
use crate::acp::protocol::{
    InitializeRequest,     // ✅ USED
    InitializeResponse,    // ✅ USED
    JsonRpcMessage,        // ✅ USED
    SessionNotification,   // ✅ USED
};
```

**NOT imported from protocol.rs:**
- `ClientInfo` - ❌ **DUPLICATED** in `MetadataMessage` enum
- `AgentInfo` - ❌ **NOT USED** (only used indirectly via InitializeResponse)

### The Duplication Problem

**protocol.rs has:**
```rust
pub struct ClientInfo {
    pub name: String,
    pub title: Option<String>,
    pub version: Option<String>,
}
```

**acp.rs duplicates it in MetadataMessage:**
```rust
enum MetadataMessage {
    ClientInfo {
        name: String,
        version: Option<String>,  // Missing title field!
    },
    // ...
}
```

**Problems:**
1. ✅ **Type mismatch**: protocol::ClientInfo has `title` field, MetadataMessage::ClientInfo doesn't
2. ✅ **Maintenance burden**: Changes to protocol::ClientInfo won't propagate
3. ✅ **Semantic confusion**: Two types with the same name but different shapes
4. ✅ **Data loss**: When we extract protocol::ClientInfo, we discard the `title` field

## Issues Found

### Issue 1: ClientInfo Duplication

**Location**: `cli/src/commands/acp.rs:25-28`

**Current Code:**
```rust
enum MetadataMessage {
    ClientInfo {
        name: String,
        version: Option<String>,
    },
    // ...
}
```

**Where it's used:**
1. Sent from IDE→Agent thread (line 231):
   ```rust
   metadata_tx_clone.send(MetadataMessage::ClientInfo {
       name: name.clone(),
       version: version.clone(),
   })
   ```

2. Received in Agent→IDE thread (line 361):
   ```rust
   MetadataMessage::ClientInfo { name, version } => {
       client_name = Some(name);
       client_version = version;
   }
   ```

**The Fix:**
Replace the custom struct with protocol::ClientInfo:
```rust
use crate::acp::protocol::ClientInfo;

enum MetadataMessage {
    ClientInfo(ClientInfo),  // Use protocol type directly!
    // ...
}
```

### Issue 2: AgentInfo Not Captured Fully

**Location**: `cli/src/commands/acp.rs:30`

**Current Code:**
```rust
enum MetadataMessage {
    AgentVersion(String),  // Only captures version!
    // ...
}
```

**Problem**: We extract `agent_info` from InitializeResponse but only send the version string (line 398):
```rust
if let Some(agent_info) = init_resp.agent_info {
    if let Some(version) = agent_info.version {
        agent_version = Some(version.clone());
        // Discarding agent_info.name and agent_info.title!
    }
}
```

**The Fix:**
Store the full AgentInfo:
```rust
use crate::acp::protocol::AgentInfo;

enum MetadataMessage {
    AgentInfo(AgentInfo),  // Store full agent info!
    // ...
}
```

## Recommended Refactoring

### Step 1: Import protocol types
```rust
use crate::acp::protocol::{
    AgentInfo,             // ADD
    ClientInfo,            // ADD
    InitializeRequest,
    InitializeResponse,
    JsonRpcMessage,
    SessionNotification,
};
```

### Step 2: Update MetadataMessage enum
```rust
#[derive(Debug, Clone)]
enum MetadataMessage {
    /// Client (IDE) information detected from initialize request
    ClientInfo(ClientInfo),  // CHANGED: Use protocol type
    
    /// Agent information detected from initialize response
    AgentInfo(AgentInfo),  // CHANGED: Use protocol type, capture full info
    
    /// Working directory from session/new or session/load
    WorkingDirectory(PathBuf),
    
    /// Session ID from session/prompt request (for PostResponse tracking)
    PromptRequest {
        request_id: serde_json::Value,
        session_id: String,
    },
    
    /// Clear response accumulator for a session (on new prompt)
    ClearAccumulator { session_id: String },
}
```

### Step 3: Update sender (IDE→Agent thread)

**Before:**
```rust
metadata_tx_clone.send(MetadataMessage::ClientInfo {
    name: name.clone(),
    version: version.clone(),
})
```

**After:**
```rust
metadata_tx_clone.send(MetadataMessage::ClientInfo(client_info))
```

### Step 4: Update receiver (Agent→IDE thread)

**Before:**
```rust
MetadataMessage::ClientInfo { name, version } => {
    client_name = Some(name);
    client_version = version;
}
```

**After:**
```rust
MetadataMessage::ClientInfo(client_info) => {
    client_name = Some(client_info.name.clone());
    client_version = client_info.version.clone();
    // Now we also have access to client_info.title if needed!
}
```

### Step 5: Update agent info handling

**Before:**
```rust
if let Some(agent_info) = init_resp.agent_info {
    if let Some(version) = agent_info.version {
        agent_version = Some(version.clone());
        // ...
    }
}
```

**After:**
```rust
if let Some(agent_info) = init_resp.agent_info {
    // Send full agent info, not just version
    let _ = metadata_tx.send(MetadataMessage::AgentInfo(agent_info.clone()));
}
```

### Step 6: Update state variables

**Before:**
```rust
let mut client_name: Option<String> = None;
let mut client_version: Option<String> = None;
let mut agent_version: Option<String> = None;
```

**After:**
```rust
let mut client_info: Option<ClientInfo> = None;
let mut agent_info: Option<AgentInfo> = None;
```

Then when passing to functions:
```rust
// Before
&client_name,
&client_version,
&agent_version,

// After
client_info.as_ref().map(|c| c.name.as_str()),
client_info.as_ref().and_then(|c| c.version.as_deref()),
agent_info.as_ref().and_then(|a| a.version.as_deref()),
```

## Benefits of Refactoring

1. **Type Safety**: No more manual field extraction and duplication
2. **Completeness**: We preserve all fields (title, name, version) from protocol
3. **Maintainability**: Changes to protocol types automatically propagate
4. **Clarity**: Same type means same semantics everywhere
5. **Future-proofing**: If protocol adds fields, we get them for free

## Breaking Changes?

**None** - This is an internal refactor. The external behavior stays the same:
- Still capture client_info from initialize request
- Still capture agent_info from initialize response
- Still pass name/version to provenance functions

## Testing Strategy

After refactoring:
1. Run existing tests: `cargo test --test test_acp_session_flow`
2. Manually test with real agent to verify:
   - Client detection still works
   - Agent version detection still works
   - Provenance still records correctly
3. Check that client_info.title is now available (new capability!)

## Conclusion

**We are NOT using protocol.rs to the fullest**. We're:
- Duplicating ClientInfo in MetadataMessage
- Only capturing agent version, not full AgentInfo
- Losing the `title` field from both ClientInfo and AgentInfo

The recommended refactoring eliminates duplication and makes better use of the well-typed protocol structures.
