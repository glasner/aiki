# Phase 2: Cursor Support - Implementation Plan

## Overview

Expand AI provenance tracking beyond Claude Code to support Cursor. Start with simple hook installation (Milestone 2.1), then add intelligent detection to make it zero-config (Milestone 2.2).

**Architecture**: Extends Phase 1's SQLite-free architecture using JJ commit descriptions. Same lightweight `[aiki]...[/aiki]` metadata format (~120 bytes per change).

**Note**: Windsurf support is planned for Phase 10, positioned before enterprise features.

**External Resources**:
- https://cursor.com/docs/agent/hooks

## Goals

1. **Cursor Integration** - Add hook support for Cursor's AI features  
2. **Intelligent Editor Detection** - Analyze repo and git history to identify editors in use (Milestone 2.2)
3. **Automatic Hook Installation** - `aiki init` automatically detects and configures all editors (Milestone 2.2)
4. **Multi-Editor Support** - Track provenance from Claude Code and Cursor simultaneously

## Milestone 2.1: Basic Cursor Hook Installation

**Goal**: Add Cursor hook support with same simple installation as Claude Code (always installed by default).

### Tasks

1. Add Cursor to `AgentType` enum
2. Design and implement Cursor hook configuration
3. Update `aiki init` to install both Claude Code and Cursor hooks by default
4. Test Cursor provenance recording
5. Add unit tests for Cursor hook installation
6. Document Cursor setup in README

### Integration into `aiki init`

When user runs `aiki init`:

1. **Install Hooks**: Install hooks for both Claude Code and Cursor (just like we do for Claude Code now)
2. **Report**: Show what was installed
   ```
   Configured AI editors:
     • Claude Code - hooks installed
     • Cursor - hooks installed
   ```

### Cursor Hook Configuration

Cursor hooks follow the same pattern as Claude Code - always installed by default.

**Configuration File**: `.cursor/aiki-hooks.json`

```json
{
  "hooks": {
    "onEdit": {
      "command": "aiki record-change --cursor",
      "events": ["file.save", "ai.complete"]
    }
  }
}
```

### Cursor Provenance Record

Extend `AgentType` enum:

```rust
pub enum AgentType {
    ClaudeCode,
    Cursor,      // NEW
    Unknown,
}
```

Provenance format remains the same:
```
[aiki]
agent=cursor
session=cursor-session-abc123
tool=edit
confidence=high
method=hook
[/aiki]
```


### Success Criteria

- ✅ `aiki init` installs both Claude Code and Cursor hooks by default
- ✅ Cursor hook configuration created in `.cursor/aiki-hooks.json`
- ✅ Cursor edits trigger `aiki record-change --cursor`
- ✅ Provenance metadata embedded in JJ commit descriptions
- ✅ `aiki authors` shows Cursor contributors
- ✅ `aiki blame` attributes lines to Cursor
- ✅ Git commits include `Co-authored-by: Cursor <cursor@cursor.sh>`
- ✅ Works alongside Claude Code hooks without conflicts

## Milestone 2.2: Intelligent Editor Detection

**Goal**: Add smart detection to only install hooks for editors actually in use, making setup zero-config.

### Tasks

1. Create `editor_detector.rs` module
2. Implement filesystem checks for editor config files
3. Implement git log parsing for pattern detection
4. Add confidence scoring algorithm
5. Update `aiki init` to use detection results
6. Add unit tests for detection logic
7. Update documentation

### Detection Strategy

**Confidence Levels**:
- **Confirmed** (100%) - Editor config files present (`.claude/`, `.cursor/`, `.cursorrules`)
- **Likely** (70-90%) - Strong patterns in git history
- **Possible** (40-60%) - Weak indicators
- **NotDetected** (0%) - No evidence

**Detection Methods**:

1. **Filesystem Checks**:
   - Claude Code: Look for `.claude/settings.json`
   - Cursor: Look for `.cursor/` directory or `.cursorrules` file

2. **Git History Analysis**:
   - Parse recent commits (last 100) for editor signatures
   - Check commit authors, messages for editor names
   - Identify characteristic commit patterns

3. **Pattern Matching**:
   - Claude Code: "claude", "Claude Code" in commits
   - Cursor: "cursor", "Cursor" in commits

### Components

- `EditorDetector` - Main detection engine
- `Editor` enum - ClaudeCode, Cursor
- `EditorConfidence` - Confidence level tracker
- `EditorDetectionResult` - Results struct

### Updated `aiki init` Flow

When user runs `aiki init`:

1. **Detect Editors**: Run detection on repository (filesystem + git history)
2. **Install Hooks**: Only install hooks for detected editors
3. **Report**: Show what was detected and installed
   ```
   Detected and configured AI editors:
     • Claude Code (Confirmed) - hooks installed
     • Cursor (Likely) - hooks installed
   
   Skipped (not detected):
     • Windsurf
   ```

### Success Criteria

- ✅ Detects Claude Code from `.claude/` directory
- ✅ Detects Cursor from `.cursor/` directory or `.cursorrules`
- ✅ Detects editors from git commit patterns
- ✅ Returns confidence levels for each editor
- ✅ `aiki init` only installs hooks for detected editors
- ✅ Detection completes in < 1 second
- ✅ False positive rate < 5%
- ✅ Works with repos that have only one editor
- ✅ Works with repos that have multiple editors

## Milestone 2.3: Multi-Editor Query Support

**Goal**: Enhance CLI commands to support querying multiple editors.

### Tasks

1. Update `aiki authors` to show all detected editors
2. Update `aiki blame` to distinguish between editors
3. Add filtering by editor type
4. Update output formatting for multi-editor display
5. Add tests for multi-editor scenarios

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
- Editor detection logic - filesystem + git patterns (Milestone 2.2)
- Confidence scoring algorithm (Milestone 2.2)
- Provenance record parsing (Milestone 2.1)
- Multi-editor query logic (Milestone 2.3)

### Integration Tests

- `aiki init` installs Cursor hooks by default (Milestone 2.1)
- `aiki init` detects multiple editors and installs selectively (Milestone 2.2)
- Hooks work correctly for both editors (Milestone 2.1)
- Provenance records embedded in JJ commits (Milestone 2.1)
- Multi-editor queries return correct results (Milestone 2.3)

### End-to-End Tests

- Full workflow: init → edit with Cursor → query → blame
- Both editors used in same repository
- Git commit includes co-authors for both editors
- Detection-based installation only installs detected editors (Milestone 2.2)

## Success Metrics

### Completion Criteria

- ✅ Cursor hooks install and work correctly (Milestone 2.1)
- ✅ Cursor provenance tracking at 100% accuracy (hook-based, Milestone 2.1)
- ✅ Editor detection works with 90%+ accuracy (Milestone 2.2)
- ✅ All CLI commands support multi-editor scenarios (Milestone 2.3)
- ✅ Zero-config experience after detection is added (Milestone 2.2)
- ✅ Documentation complete and tested
- ✅ All tests passing

### User Experience Goals

- **Milestone 2.1**: `aiki init` installs both editor hooks by default
- **Milestone 2.2**: `aiki init` "just works" - only installs hooks for detected editors
- **Milestone 2.2**: Users see which editors were detected and why
- **Milestone 2.3**: Provenance queries work seamlessly with multiple editors
- **Milestone 2.3**: Clear attribution in `blame` output distinguishing editors

### Technical Goals

- Reuses Phase 1 architecture (JJ commit descriptions)
- No performance degradation with multiple editors
- Extensible for future editor support (Phase 9: Windsurf)
- Clean separation of concerns (detection vs tracking vs querying)

## Dependencies

No new dependencies required beyond Phase 1:
- `jj-lib` - Already used for JJ operations
- `serde`/`serde_json` - Already used for config parsing
- `anyhow` - Already used for error handling

## Future Extensions

### Phase 9: Windsurf Support

- Apply same pattern as Cursor
- Reuse detection infrastructure
- Add `AgentType::Windsurf`
- Extend hook management

### Additional Editors

Framework is extensible to any editor that provides:
- Hooks or event APIs
- Identifiable commit patterns
- Configuration files

## Next Steps

After Phase 2 completion:
- **Phase 3**: Hook Management CLI (unified management for editor and Git hooks)
- **Phase 4**: Cryptographic commit signing for tamper-proof attribution
- **Phase 5**: Autonomous review & self-correction loop
