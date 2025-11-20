# Phase 6 Implementation Summary

## Overview

Phase 6 implements ACP (Agent Communication Protocol) support via a bidirectional proxy, allowing Aiki to intercept and observe communication between IDEs and AI agents.

## What Was Implemented

### 1. ACP Protocol Types (`cli/src/acp/`)

Created protocol definitions for JSON-RPC message handling:
- `JsonRpcMessage` - Core JSON-RPC 2.0 message structure
- `ClientInfo` - Client (IDE) information from initialize requests
- `InitializeRequest` - Initialization handshake parameters
- `SessionUpdate` - Session update notifications
- `ToolCall` - Tool call notification structure

### 2. Bidirectional Proxy Command (`cli/src/commands/acp.rs`)

Implemented `aiki acp <agent-type> [--bin <path>] [-- <agent-args>...]` command:
- **Agent type validation** - Validates against `AgentType` enum (claude-code, cursor)
- **Executable derivation** - Maps agent type to binary (e.g., gemini → gemini-cli)
- **Custom binary support** - Optional `--bin` flag for custom paths
- **Agent arguments** - Pass-through arguments via `-- <args>`
- **Client detection** - Auto-detects IDE from `InitializeRequest.clientInfo.name`
- **Bidirectional forwarding** - Transparent message relay in both directions
- **Future-ready** - Stubs for prompt modification and provenance recording

### 3. Provenance Extensions

Extended provenance data model to track both agent and client:
- Added `client_name: Option<String>` to `ProvenanceRecord`
- Added `client_name` to `AikiPostChangeEvent`
- Updated serialization format to include `client=<ide>` in `[aiki]` blocks
- Updated all event constructors (claude_code, cursor vendors)
- Maintained backward compatibility (client_name is optional)

### 4. CLI Integration

- Added `Acp` command variant to main CLI
- Exported `acp` module from `commands/mod.rs`
- Registered command dispatch in `main.rs`

### 5. Testing

- All existing tests pass with new `client_name` field
- Provenance serialization/deserialization tests updated
- Command help and validation tested

## File Changes

### New Files
- `cli/src/acp/mod.rs` - ACP module exports
- `cli/src/acp/protocol.rs` - JSON-RPC and ACP protocol types
- `cli/src/commands/acp.rs` - Bidirectional proxy command

### Modified Files
- `cli/src/main.rs` - Added acp module and Acp command variant
- `cli/src/commands/mod.rs` - Exported acp module
- `cli/src/provenance.rs` - Added client_name field
- `cli/src/events.rs` - Added client_name to AikiPostChangeEvent
- `cli/src/vendors/claude_code.rs` - Set client_name to None
- `cli/src/vendors/cursor.rs` - Set client_name to None
- `cli/src/verify.rs` - Updated test with client_name
- `cli/src/flows/*.rs` - Updated test events
- `cli/src/test_must_use.rs` - Updated test events

## Usage

### Basic Usage
```bash
# Proxy claude-code agent
aiki acp claude-code

# Proxy cursor agent
aiki acp cursor

# Custom binary path
aiki acp claude-code --bin /custom/path/to/claude-code

# Pass arguments to agent
aiki acp gemini -- --verbose --model gemini-2.0
```

### IDE Configuration (Future)

The proxy is designed to be configured in IDE settings:

**Zed** (`~/.config/zed/settings.json`):
```json
{
  "agent_servers": {
    "claude": {
      "command": "aiki",
      "args": ["acp", "claude-code"]
    }
  }
}
```

## Architecture

```
IDE (Zed/Neovim/etc) ←→ aiki acp ←→ Agent (claude-code/gemini/etc)
                            ↓              ↓
                       Auto-detect IDE  Observe tool_call
                       (from InitReq)   (from session/update)
```

**Thread Model:**
- Thread 1: IDE → Agent (intercepts, can modify)
- Thread 2: Agent → IDE (observes, records)
- Main thread: Waits for agent process

## Key Design Decisions

### 1. Subcommand Pattern
- Used `aiki acp` instead of separate binary
- Consistent with Aiki CLI design
- Single binary to install

### 2. Auto-Detection
- Client (IDE) name detected from `InitializeRequest.clientInfo.name`
- Agent type specified explicitly for provenance
- No manual IDE configuration needed in future phases

### 3. Optional client_name
- Backward compatible with existing hook-based detection
- Hook-based detection sets `client_name: None`
- ACP-based detection will populate client_name
- Allows gradual migration

### 4. Agent Type Validation
- Validates against `AgentType` enum
- Provides clear error messages
- Lists supported agents

## Future Enhancements (Not in Phase 6)

The foundation is in place for:
- **Prompt modification** - Inject context into user prompts
- **Provenance recording** - Record tool_call notifications
- **Context injection** - Pass previous session data
- **Policy enforcement** - Block dangerous operations
- **Autonomous review** - Add review feedback to prompts
- **IDE auto-configuration** - `aiki init` configures IDE settings
- **Doctor validation** - `aiki doctor` validates ACP setup

## Testing Strategy

### Manual Testing
```bash
# Test agent type validation
./target/release/aiki acp invalid-agent
# Error: Unknown agent type: 'invalid-agent'. Supported values: 'claude-code', 'cursor'

# Test help
./target/release/aiki acp --help

# Test with mock agent (echo server)
echo '{"jsonrpc":"2.0","method":"initialize","params":{"clientInfo":{"name":"test"}}}' | aiki acp claude-code --bin cat
```

### Integration Testing (Future)
- Test with real IDE (Zed)
- Test bidirectional message flow
- Test provenance recording
- Test client detection

## Success Criteria

✅ `aiki acp <agent-type>` command exists and works  
✅ Validates `agent-type` against `AgentType` enum  
✅ Bidirectional message forwarding (transparent)  
✅ Auto-detects client from `InitializeRequest.clientInfo.name`  
✅ Provenance structure supports `client_name`  
✅ All tests pass  
✅ Builds successfully  

## Known Limitations

1. **No provenance recording yet** - Stub exists but not implemented
2. **No prompt modification yet** - Foundation in place
3. **No IDE auto-configuration** - Requires manual IDE setup
4. **No doctor validation** - Can't verify ACP setup yet
5. **Limited agent types** - Only claude-code and cursor validated
6. **No session state** - Each invocation is stateless

## Next Steps

To complete Phase 6:
1. Implement `handle_session_update()` for provenance recording
2. Add IDE auto-configuration in `aiki init`
3. Add ACP validation in `aiki doctor`
4. Add more agent types to enum (gemini, windsurf, aider, cline)
5. Test with real IDE (Zed integration)
6. Document IDE setup instructions

## Related Documentation

- `ops/phase-6.md` - Phase 6 plan
- `CLAUDE.md` - JJ vs Git terminology, error handling patterns
- `cli/src/acp/protocol.rs` - Protocol type definitions
- `cli/src/commands/acp.rs` - Proxy implementation
