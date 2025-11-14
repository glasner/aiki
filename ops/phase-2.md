# Phase 2: Cursor Support - Implementation Plan

## Overview

Expand AI provenance tracking beyond Claude Code to support Cursor with global hook installation.

**Architecture**: Extends Phase 1's SQLite-free architecture using JJ commit descriptions. Same lightweight `[aiki]...[/aiki]` metadata format (~120 bytes per change).

**Note**: Windsurf support is planned for Phase 10, positioned before enterprise features.

**External Resources**:
- https://cursor.com/docs/agent/hooks

## Goals

1. **Cursor Integration** - Add hook support for Cursor's AI features  
2. **Global Hook Installation** - `aiki hooks install` installs hooks for all supported editors
3. **Multi-Editor Support** - Track provenance from Claude Code and Cursor simultaneously

## Milestone 2.1: Cursor Hook Installation

**Goal**: Add Cursor hook support with global installation (matching Claude Code pattern).

### Tasks

1. ✅ Add Cursor to `AgentType` enum (already done)
2. Implement Cursor vendor handler (`vendors/cursor.rs`)
3. Add Cursor hook installation to `install_cursor_hooks_global()`
4. Test Cursor provenance recording
5. Add unit tests for Cursor hook installation
6. Document Cursor setup in README

### Global Hook Installation

`aiki hooks install` installs global hooks for both editors:

- **Claude Code**: `~/.claude/settings.json`
- **Cursor**: `~/.cursor/hooks.json`

Both hooks use the unified command:
```bash
aiki hooks handle --agent cursor --event <EventName>
```

### Cursor Hook Configuration

**Configuration File**: `~/.cursor/hooks.json`

```json
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [{
      "command": "aiki hooks handle --agent cursor --event beforeSubmitPrompt"
    }],
    "afterFileEdit": [{
      "command": "aiki hooks handle --agent cursor --event afterFileEdit"
    }]
  }
}
```

### Cursor Vendor Handler

Create `cli/src/vendors/cursor.rs`:

```rust
pub fn handle(event_name: &str) -> Result<()> {
    let payload: CursorPayload = super::read_stdin_json()?;
    
    let aiki_event_type = match event_name {
        "beforeSubmitPrompt" => AikiEventType::Start,
        "afterFileEdit" => AikiEventType::PostChange,
        _ => return Ok(()), // Forward compatibility
    };
    
    let event = AikiEvent::new(aiki_event_type, AgentType::Cursor, ...)
        .with_session_id(payload.session_id)
        .with_metadata("tool_name", "Edit")
        .with_metadata("vendor_event", event_name);
    
    event_bus::dispatch(event)
}
```

### Cursor Provenance Record

AgentType already includes Cursor:

```rust
pub enum AgentType {
    ClaudeCode,
    Cursor,      // ✅ Already exists
    Unknown,
}
```

Provenance format:
```
[aiki]
agent=cursor
session=cursor-session-abc123
tool=Edit
confidence=High
method=Hook
[/aiki]
```

### Success Criteria

- ✅ `aiki hooks install` installs both Claude Code and Cursor hooks globally
- ✅ Cursor hook configuration created in `~/.cursor/hooks.json`
- ✅ Cursor edits trigger `aiki hooks handle --agent cursor --event afterFileEdit`
- ✅ Provenance metadata embedded in JJ change descriptions
- ✅ `aiki authors` shows Cursor contributors
- ✅ `aiki blame` attributes lines to Cursor
- ✅ Git commits include `Co-authored-by: Cursor <cursor@cursor.sh>`
- ✅ Works alongside Claude Code hooks without conflicts

## Milestone 2.2: Multi-Editor Query Support

**Status**: ✅ Complete

**Goal**: Enhance CLI commands to support querying multiple editors.

### Tasks

1. ✅ Update `aiki authors` to show all detected editors
2. ✅ Update `aiki blame` to distinguish between editors (Display trait)
3. ✅ Add filtering by editor type (--agent flag on blame only)
4. ✅ Update output formatting for multi-editor display
5. ✅ Add tests for multi-editor scenarios

### Enhanced Output Examples

```bash
$ aiki authors --format=git --changes=staged
Co-authored-by: Claude Code <claude-code@anthropic.ai>
Co-authored-by: Cursor <cursor@cursor.sh>

$ aiki blame auth.py
abc12345 (ClaudeCode   session-123  High  )    1| def authenticate():
def67890 (Cursor       session-456  High  )    2|     user = get_user()
abc12345 (ClaudeCode   session-123  High  )    3|     return validate(user)
```

### Query Implementation Using JJ Revsets

Query provenance using JJ's revset engine:

```bash
# All Claude Code changes
jj log -r 'description(glob:"*agent=claude-code*")'

# All Cursor changes
jj log -r 'description(glob:"*agent=cursor*")'

# All AI changes (any editor)
jj log -r 'description(glob:"*[aiki]*")'
```

### Success Criteria

- ✅ `aiki authors` shows all editors that contributed
- ✅ `aiki blame` distinguishes between Claude Code and Cursor
- ✅ Queries complete in < 100ms for typical repos
- ✅ Output is human-readable and parseable
- ✅ Works with both editors installed simultaneously

## Testing Strategy

### Unit Tests

- Cursor hook configuration generation (Milestone 2.1)
- Provenance record parsing for Cursor (Milestone 2.1)
- Multi-editor query logic (Milestone 2.2)

### Integration Tests

- `aiki hooks install` installs Cursor hooks globally (Milestone 2.1)
- Hooks work correctly for both editors (Milestone 2.1)
- Provenance records embedded in JJ changes (Milestone 2.1)
- Multi-editor queries return correct results (Milestone 2.2)

### End-to-End Tests

- Full workflow: install → edit with Cursor → query → blame
- Both editors used in same repository
- Git commit includes co-authors for both editors

## Success Metrics

### Completion Criteria

- ✅ Cursor hooks install and work correctly (Milestone 2.1)
- ✅ Cursor provenance tracking at 100% accuracy (hook-based, Milestone 2.1)
- ✅ All CLI commands support multi-editor scenarios (Milestone 2.2)
- ✅ Documentation complete and tested
- ✅ All tests passing

### User Experience Goals

- **Milestone 2.1**: `aiki hooks install` installs both editor hooks globally
- **Milestone 2.2**: Provenance queries work seamlessly with multiple editors
- **Milestone 2.2**: Clear attribution in `blame` output distinguishing editors

### Technical Goals

- Reuses Phase 1 architecture (JJ change descriptions)
- No performance degradation with multiple editors
- Extensible for future editor support (Phase 10: Windsurf)
- Clean separation of concerns (vendor handlers vs event system vs querying)

## Dependencies

No new dependencies required beyond Phase 1:
- `jj-lib` - Already used for JJ operations
- `serde`/`serde_json` - Already used for config parsing
- `anyhow` - Already used for error handling

## Future Extensions

### Phase 10: Windsurf Support

- Apply same pattern as Cursor
- Add `AgentType::Windsurf`
- Create `vendors/windsurf.rs`
- Add to global hooks installation

### Additional Editors

Framework is extensible to any editor that provides:
- Hooks or event APIs
- JSON payload to stdin
- Session/tool metadata

## Next Steps

After Phase 2 completion:
- **Phase 3**: Hook Management CLI (unified management for all hooks)
- **Phase 4**: Cryptographic commit signing for tamper-proof attribution
- **Phase 5**: Autonomous review & self-correction loop
