# Phase 2: Cursor Support - Implementation Plan

## Overview

Expand AI provenance tracking beyond Claude Code to support Cursor. Use git commit history analysis to intelligently detect if Cursor is in use and automatically configure appropriate hooks.

**Key Innovation**: Analyze git history to detect Cursor AI coding patterns and automatically install the right hooks, making setup zero-config for users.

**Architecture**: Extends Phase 1's SQLite-free architecture using JJ commit descriptions. Same lightweight `[aiki]...[/aiki]` metadata format (~120 bytes per change).

**Note**: Windsurf support is planned for Phase 9, positioned before enterprise features.

## Goals

1. **Intelligent Editor Detection** - Analyze git history to identify if Cursor is being used
2. **Cursor Integration** - Add hook support for Cursor's AI features  
3. **Automatic Hook Installation** - `aiki init` automatically detects and configures Cursor
4. **Multi-Editor Support** - Track provenance from Claude Code and Cursor simultaneously

## Architecture: Detection Strategy

### Editor Detection Design

**Confidence Levels**:
- **Confirmed** (100%) - Editor config files present (`.cursor/`, `.cursorrules`)
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

## Milestone 2.1: Intelligent Editor Detection

**Goal**: Implement git history analysis to detect Cursor usage.

### Tasks

1. Create `editor_detector.rs` module
2. Implement filesystem checks for editor config files
3. Implement git log parsing for pattern detection
4. Add confidence scoring algorithm
5. Integrate into `aiki init` workflow
6. Add unit tests for detection logic

### Integration into `aiki init`

When user runs `aiki init`:

1. **Detect Editors**: Run detection on repository
2. **Report Findings**: Show which editors were detected
   ```
   Detected AI editors:
     • Claude Code (Confirmed)
     • Cursor (Likely)
   ```
3. **Install Hooks**: Automatically configure hooks for detected editors
4. **Confirm**: User sees what was installed

### Success Criteria

- ✅ Detects Claude Code from `.claude/` directory
- ✅ Detects Cursor from `.cursor/` directory or `.cursorrules`
- ✅ Detects editors from git commit patterns
- ✅ Returns confidence levels for each editor
- ✅ `aiki init` uses detection results automatically
- ✅ Detection completes in < 1 second

## Milestone 2.2: Cursor Hook Integration

**Goal**: Add Cursor-specific hook support for provenance tracking.

### Tasks

1. Research Cursor's hook/extension API
2. Design Cursor hook configuration format
3. Implement hook handler for Cursor events
4. Add Cursor to `AgentType` enum
5. Test Cursor provenance recording
6. Document Cursor setup in README

### Cursor Hook Configuration

Cursor hooks will follow similar pattern to Claude Code:

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

### Alternative: Git Hook-Based Tracking

If Cursor doesn't provide native hooks, fall back to Git hook-based detection:

- Use `prepare-commit-msg` hook to analyze diffs
- Infer Cursor usage from commit patterns
- Lower confidence (Medium vs High)
- Still stores metadata in JJ commit descriptions

### Success Criteria

- ✅ Cursor hook configuration installed by `aiki init`
- ✅ Cursor edits trigger `aiki record-change --cursor`
- ✅ Provenance metadata embedded in JJ commit descriptions
- ✅ `aiki authors` shows Cursor contributors
- ✅ `aiki blame` attributes lines to Cursor
- ✅ Git commits include `Co-authored-by: Cursor <cursor@cursor.sh>`

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

## Milestone 2.4: Hook Management CLI

**Goal**: Add commands to manage editor hooks.

### Commands

```bash
aiki hooks status               # Show which editors have hooks installed
aiki hooks install <editor>     # Manually install hooks for specific editor
aiki hooks remove <editor>      # Remove hooks for specific editor
aiki hooks list                 # List all available editor integrations
aiki hooks doctor               # Diagnose hook issues
```

### Hook Status Output

```bash
$ aiki hooks status

Hook Status:
  Claude Code:
    Status: ✓ Active
    Last Activity: 5 minutes ago
    Changes Tracked: 42 (last 7 days)
    Config: .claude/settings.json

  Cursor:
    Status: ✓ Active  
    Last Activity: 1 hour ago
    Changes Tracked: 18 (last 7 days)
    Config: .cursor/aiki-hooks.json
```

### Hook Doctor Output

```bash
$ aiki hooks doctor

Diagnosing hook configuration...

✓ Claude Code hooks: Healthy
✓ Cursor hooks: Healthy
✓ JJ repository: Initialized
✓ Provenance format: Valid

No issues detected.
```

### Success Criteria

- ✅ `aiki hooks status` shows real-time hook state
- ✅ Manual hook installation works for both editors
- ✅ Hook removal cleanly uninstalls configuration
- ✅ Doctor command identifies common issues
- ✅ User-friendly error messages with fix suggestions

## Testing Strategy

### Unit Tests

- Editor detection logic (filesystem + git patterns)
- Confidence scoring algorithm
- Hook configuration generation
- Provenance record parsing

### Integration Tests

- `aiki init` detects multiple editors
- Hooks install correctly for detected editors
- Provenance records embedded in JJ commits
- Multi-editor queries return correct results

### End-to-End Tests

- Full workflow: init → edit with Cursor → query → blame
- Both editors used in same repository
- Git commit includes co-authors for both editors

## Success Metrics

### Completion Criteria

- ✅ Cursor detection works with 90%+ accuracy
- ✅ Cursor provenance tracking at 100% accuracy (hook-based)
- ✅ All CLI commands support multi-editor scenarios
- ✅ Zero-config experience for users
- ✅ Documentation complete and tested
- ✅ All tests passing

### User Experience Goals

- `aiki init` "just works" with no manual configuration
- Users see which editors were detected
- Provenance queries work seamlessly with multiple editors
- Clear attribution in `blame` output

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
- **Phase 3**: Cryptographic commit signing for tamper-proof attribution
- **Phase 4**: Autonomous review & self-correction loop
