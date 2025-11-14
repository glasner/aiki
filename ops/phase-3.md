# Phase 3: Hook Management CLI

## Overview

Provide comprehensive hook management commands to help users understand, configure, and troubleshoot all hooks in their repository - both AI editor hooks (Claude Code, Cursor) and Git hooks (prepare-commit-msg).

**Key Innovation**: Unified interface for managing all Aiki hooks regardless of type, with intelligent diagnostics and repair capabilities.

## Goals

1. **Hook Status Visibility** - Show which hooks are installed and active
2. **Manual Hook Management** - Install/remove hooks for specific editors or Git
3. **Hook Diagnostics** - Detect and fix common hook configuration issues
4. **Multi-Hook Support** - Manage editor hooks and Git hooks through single interface

## Architecture

### Hook Types

Aiki manages two categories of hooks:

1. **Editor Hooks** - AI editor integrations
   - Claude Code: PostToolUse hooks in `.claude/settings.json`
   - Cursor: Hook config in `.cursor/aiki-hooks.json`
   - Windsurf: (Future - Phase 9)

2. **Git Hooks** - Repository-level hooks
   - `prepare-commit-msg`: Injects AI co-authors into commit messages
   - Uses templates from `cli/templates/`

### Components

- `HookManager` - Central hook management engine
- `HookType` enum - EditorHook, GitHook
- `HookStatus` - Active, Inactive, Broken, Missing
- `HookDiagnostics` - Health check and repair logic

## Commands

### `aiki hooks status`

Show comprehensive status of all hooks.

**Usage:**
```bash
aiki hooks status               # Show all hooks
aiki hooks status --editor      # Show only editor hooks
aiki hooks status --git         # Show only git hooks
```

**Example Output:**
```bash
$ aiki hooks status

Editor Hooks:
  Claude Code:
    Status: ✓ Active
    Location: .claude/settings.json
    Last Activity: 5 minutes ago
    Changes Tracked: 42 (last 7 days)

  Cursor:
    Status: ✓ Active
    Location: .cursor/aiki-hooks.json
    Last Activity: 1 hour ago
    Changes Tracked: 18 (last 7 days)

Git Hooks:
  prepare-commit-msg:
    Status: ✓ Active
    Location: .git/hooks/prepare-commit-msg
    Last Execution: 10 minutes ago
    Template: cli/templates/prepare-commit-msg.sh

Summary: All hooks healthy ✓
```

### `aiki hooks install`

Manually install hooks for specific editor or Git.

**Usage:**
```bash
aiki hooks install <target>     # Install specific hook
aiki hooks install --all        # Install all detected hooks
```

**Examples:**
```bash
$ aiki hooks install claude-code
✓ Installed Claude Code hooks in .claude/settings.json

$ aiki hooks install cursor
✓ Installed Cursor hooks in .cursor/aiki-hooks.json

$ aiki hooks install git
✓ Installed prepare-commit-msg hook in .git/hooks/

$ aiki hooks install --all
✓ Installed Claude Code hooks
✓ Installed Cursor hooks
✓ Installed Git hooks
```

### `aiki hooks remove`

Remove hooks for specific editor or Git.

**Usage:**
```bash
aiki hooks remove <target>      # Remove specific hook
aiki hooks remove --all         # Remove all hooks
```

**Examples:**
```bash
$ aiki hooks remove cursor
✓ Removed Cursor hooks from .cursor/aiki-hooks.json

$ aiki hooks remove git
✓ Removed prepare-commit-msg hook from .git/hooks/

$ aiki hooks remove --all
✓ Removed all Aiki hooks
```

### `aiki hooks list`

List all available hook integrations.

**Example Output:**
```bash
$ aiki hooks list

Available Hook Integrations:

Editor Hooks:
  claude-code       Claude Code AI editor (detected: ✓)
  cursor            Cursor AI editor (detected: ✓)
  windsurf          Windsurf AI editor (Phase 9)

Git Hooks:
  prepare-commit-msg  Inject AI co-authors into commits (supported: ✓)

Use 'aiki hooks install <name>' to install a specific hook.
```

### `aiki hooks doctor`

Diagnose and repair hook issues.

**Usage:**
```bash
aiki hooks doctor               # Diagnose all hooks
aiki hooks doctor --fix         # Automatically fix issues
aiki hooks doctor --editor      # Check only editor hooks
aiki hooks doctor --git         # Check only git hooks
```

**Example Output:**
```bash
$ aiki hooks doctor

Diagnosing hook configuration...

Editor Hooks:
  ✓ Claude Code hooks: Healthy
  ✗ Cursor hooks: Broken
    Issue: .cursor/aiki-hooks.json syntax error
    Fix: Run 'aiki hooks doctor --fix' to repair

Git Hooks:
  ✓ prepare-commit-msg: Healthy
  ⚠ Hook executable permission missing
    Fix: Run 'aiki hooks doctor --fix' to repair

JJ Repository:
  ✓ Initialized
  ✓ Provenance format valid

Found 2 issues. Run 'aiki hooks doctor --fix' to repair.
```

**With --fix:**
```bash
$ aiki hooks doctor --fix

Diagnosing and repairing hook configuration...

Editor Hooks:
  ✓ Claude Code hooks: Healthy
  ✓ Cursor hooks: Repaired (regenerated .cursor/aiki-hooks.json)

Git Hooks:
  ✓ prepare-commit-msg: Repaired (set executable permissions)

All issues fixed ✓
```

## Hook Diagnostics

### Checks Performed

**Editor Hooks:**
- Configuration file exists
- Configuration file has valid JSON/format
- Hook command is correct (`aiki record-change`)
- Hook is actually triggering (check recent activity)

**Git Hooks:**
- Hook file exists in `.git/hooks/`
- Hook has executable permissions (`chmod +x`)
- Hook template is up-to-date
- Hook references correct `aiki` commands
- Git is configured correctly

**Repository:**
- JJ repository initialized
- Provenance format valid in commit descriptions
- No corrupted metadata

### Common Issues and Fixes

| Issue | Detection | Fix |
|-------|-----------|-----|
| Missing hook config | File doesn't exist | Regenerate from template |
| Invalid JSON | Parse error | Regenerate config file |
| Wrong permissions | `!is_executable()` | `chmod +x` |
| Outdated template | Hash mismatch | Reinstall from latest template |
| Hook not triggering | No recent activity | Reinstall and test |
| Corrupted provenance | Parse error | Validate and repair |

## Implementation Design

### Hook Status Detection

```rust
pub struct HookStatus {
    pub hook_type: HookType,
    pub status: Status,
    pub location: PathBuf,
    pub last_activity: Option<DateTime>,
    pub issues: Vec<HookIssue>,
}

pub enum HookType {
    EditorHook(Editor),
    GitHook(GitHookName),
}

pub enum Status {
    Active,      // Installed and working
    Inactive,    // Installed but not triggering
    Broken,      // Installed but has errors
    Missing,     // Not installed
}

pub struct HookIssue {
    pub severity: Severity,
    pub description: String,
    pub fix: Option<String>,
}
```

### Hook Installation

**Editor Hooks:**
- Read template from `cli/templates/`
- Inject configuration into editor config file
- Validate JSON/format
- Test hook triggers

**Git Hooks:**
- Copy template from `cli/templates/prepare-commit-msg.sh`
- Install to `.git/hooks/prepare-commit-msg`
- Set executable permissions
- Preserve existing hooks if present (chain them)

### Hook Activity Tracking

Track last hook execution by reading:
- Editor hooks: Parse recent JJ commit descriptions for timestamps
- Git hooks: Check git reflog for commit timestamps

## Testing Strategy

### Unit Tests

- Hook status detection logic
- Hook installation/removal for each type
- Diagnostics detection for common issues
- Fix application for each issue type

### Integration Tests

- Install editor hooks and verify configuration
- Install git hooks and verify execution
- Remove hooks and verify cleanup
- Doctor detects and fixes issues

### End-to-End Tests

- Full workflow: install → use → diagnose → remove
- Multiple editors installed simultaneously
- Git hooks working with editor hooks

## Success Criteria

- ✅ `aiki hooks status` shows accurate state of all hooks
- ✅ Manual hook installation works for all hook types
- ✅ Hook removal cleanly uninstalls without breaking repo
- ✅ Doctor command detects 90%+ of common issues
- ✅ Doctor --fix successfully repairs detected issues
- ✅ User-friendly error messages with actionable fixes
- ✅ All tests passing
- ✅ Zero-config workflow still works (aiki init handles everything)

## User Experience Goals

- Users can see exactly which hooks are active
- Easy to manually install/remove specific hooks
- Clear diagnostics when hooks aren't working
- Automatic repair for common issues
- Doesn't interfere with existing git hooks

## Dependencies

No new dependencies required:
- `jj-lib` - Already used for JJ operations
- `serde`/`serde_json` - Already used for config parsing
- `anyhow` - Already used for error handling
- Standard library for file operations

## Future Extensions

- Support for custom hook templates
- Hook performance monitoring
- Hook execution logging
- Integration with IDE extensions

## Next Steps

After Phase 3 completion:
- **Phase 4**: Cryptographic commit signing for tamper-proof attribution
- **Phase 5**: Autonomous review & self-correction loop
