# Phase 2: Multi-Editor Hook Support - Implementation Plan

## Overview

Expand AI provenance tracking beyond Claude Code to support Cursor and Windsurf (formerly Codeium Windsurf). Use git commit history analysis to intelligently detect which AI editors are in use and automatically configure appropriate hooks.

**Key Innovation**: Analyze git history to detect AI coding patterns and automatically install the right hooks, making setup zero-config for users.

## Goals

1. **Intelligent Editor Detection** - Analyze git history to identify which AI editors are being used
2. **Cursor Integration** - Add hook support for Cursor's AI features
3. **Windsurf Integration** - Add hook support for Windsurf's AI features
4. **Automatic Hook Installation** - `aiki init` automatically detects and configures all relevant editors
5. **Multi-Editor Support** - Track provenance from multiple AI editors simultaneously

## Architecture: Git History-Based Detection

### Detection Strategy

```rust
pub struct EditorDetector {
    repo_path: PathBuf,
}

impl EditorDetector {
    /// Detect which AI editors are in use by analyzing git history
    pub fn detect_editors(&self) -> Result<EditorDetectionResult> {
        // 1. Check for editor-specific markers in recent commits
        // 2. Look for characteristic file patterns
        // 3. Analyze commit message patterns
        // 4. Check for editor config files
    }
}

pub struct EditorDetectionResult {
    pub claude_code: EditorConfidence,
    pub cursor: EditorConfidence,
    pub windsurf: EditorConfidence,
}

pub enum EditorConfidence {
    Confirmed,    // Definitive evidence (config files present)
    Likely,       // Strong evidence (git history patterns)
    Possible,     // Weak evidence (similar patterns)
    NotDetected,  // No evidence found
}
```

### Detection Signals

#### Claude Code Detection
- **Config file**: `.claude/settings.json` exists
- **Git patterns**: Large diffs with specific formatting
- **Commit messages**: May contain session IDs

#### Cursor Detection
- **Config file**: `.cursor/` directory exists
- **Git patterns**: Cursor-specific file modifications
- **Commit author**: May include "Cursor" in author field
- **File patterns**: `.cursorrules` files

#### Windsurf Detection
- **Config file**: `.windsurf/` directory exists
- **Git patterns**: Windsurf-specific markers
- **Commit patterns**: AI-generated commit characteristics
- **File patterns**: Windsurf configuration files

## Milestone 2.1: Intelligent Editor Detection

**Goal**: Implement git history analysis to detect which AI editors are in use.

### Tasks
- [ ] Create `EditorDetector` module
- [ ] Implement git log parsing
- [ ] Add detection heuristics for each editor
- [ ] Add confidence scoring system
- [ ] Integrate detection into `aiki init`
- [ ] Add user confirmation for detected editors
- [ ] Write unit tests for detection logic
- [ ] Write integration tests with sample repos

### Editor Detector Implementation

```rust
// cli/src/editor_detector.rs

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct EditorDetector {
    repo_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Editor {
    ClaudeCode,
    Cursor,
    Windsurf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorConfidence {
    Confirmed,    // 100% - Config files present
    Likely,       // 70-90% - Strong git patterns
    Possible,     // 40-60% - Weak indicators
    NotDetected,  // 0% - No evidence
}

#[derive(Debug, Clone)]
pub struct EditorDetectionResult {
    pub claude_code: EditorConfidence,
    pub cursor: EditorConfidence,
    pub windsurf: EditorConfidence,
}

impl EditorDetector {
    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// Detect all AI editors in use
    pub fn detect_editors(&self) -> Result<EditorDetectionResult> {
        Ok(EditorDetectionResult {
            claude_code: self.detect_claude_code()?,
            cursor: self.detect_cursor()?,
            windsurf: self.detect_windsurf()?,
        })
    }

    /// Detect Claude Code usage
    fn detect_claude_code(&self) -> Result<EditorConfidence> {
        // Check for .claude directory
        if self.repo_path.join(".claude").exists() {
            return Ok(EditorConfidence::Confirmed);
        }

        // Check git history for Claude Code patterns
        if self.check_git_patterns_claude()? {
            return Ok(EditorConfidence::Likely);
        }

        Ok(EditorConfidence::NotDetected)
    }

    /// Detect Cursor usage
    fn detect_cursor(&self) -> Result<EditorConfidence> {
        // Check for .cursor directory
        if self.repo_path.join(".cursor").exists() {
            return Ok(EditorConfidence::Confirmed);
        }

        // Check for .cursorrules file
        if self.repo_path.join(".cursorrules").exists() {
            return Ok(EditorConfidence::Confirmed);
        }

        // Check git history
        if self.check_git_patterns_cursor()? {
            return Ok(EditorConfidence::Likely);
        }

        Ok(EditorConfidence::NotDetected)
    }

    /// Detect Windsurf usage
    fn detect_windsurf(&self) -> Result<EditorConfidence> {
        // Check for .windsurf directory
        if self.repo_path.join(".windsurf").exists() {
            return Ok(EditorConfidence::Confirmed);
        }

        // Check git history
        if self.check_git_patterns_windsurf()? {
            return Ok(EditorConfidence::Likely);
        }

        Ok(EditorConfidence::NotDetected)
    }

    /// Check git log for Claude Code patterns
    fn check_git_patterns_claude(&self) -> Result<bool> {
        // Look at recent commits (last 100)
        let output = Command::new("git")
            .arg("log")
            .arg("--format=%an|%ae|%s")
            .arg("-100")
            .current_dir(&self.repo_path)
            .output()?;

        if !output.status.success() {
            return Ok(false);
        }

        let log = String::from_utf8_lossy(&output.stdout);
        
        // Look for Claude-specific patterns
        // This is heuristic-based and may need refinement
        Ok(log.contains("claude") || log.contains("anthropic"))
    }

    /// Check git log for Cursor patterns
    fn check_git_patterns_cursor(&self) -> Result<bool> {
        let output = Command::new("git")
            .arg("log")
            .arg("--format=%an|%ae|%s")
            .arg("-100")
            .current_dir(&self.repo_path)
            .output()?;

        if !output.status.success() {
            return Ok(false);
        }

        let log = String::from_utf8_lossy(&output.stdout);
        
        // Look for Cursor-specific patterns
        Ok(log.contains("cursor") || log.contains("Cursor"))
    }

    /// Check git log for Windsurf patterns
    fn check_git_patterns_windsurf(&self) -> Result<bool> {
        let output = Command::new("git")
            .arg("log")
            .arg("--format=%an|%ae|%s")
            .arg("-100")
            .current_dir(&self.repo_path)
            .output()?;

        if !output.status.success() {
            return Ok(false);
        }

        let log = String::from_utf8_lossy(&output.stdout);
        
        // Look for Windsurf-specific patterns
        Ok(log.contains("windsurf") || log.contains("codeium"))
    }

    /// Get list of confirmed or likely editors
    pub fn get_detected_editors(&self, result: &EditorDetectionResult) -> Vec<Editor> {
        let mut editors = Vec::new();

        match result.claude_code {
            EditorConfidence::Confirmed | EditorConfidence::Likely => {
                editors.push(Editor::ClaudeCode);
            }
            _ => {}
        }

        match result.cursor {
            EditorConfidence::Confirmed | EditorConfidence::Likely => {
                editors.push(Editor::Cursor);
            }
            _ => {}
        }

        match result.windsurf {
            EditorConfidence::Confirmed | EditorConfidence::Likely => {
                editors.push(Editor::Windsurf);
            }
            _ => {}
        }

        editors
    }
}
```

### Integration into `aiki init`

```rust
// Update cli/src/main.rs init_command()

fn init_command() -> Result<()> {
    // ... existing init code ...

    // Detect AI editors in use
    println!("Detecting AI editors in use...");
    let detector = EditorDetector::new(&repo_root);
    let detection_result = detector.detect_editors()?;
    let detected_editors = detector.get_detected_editors(&detection_result);

    if detected_editors.is_empty() {
        println!("⚠️  No AI editors detected. You can manually configure hooks later.");
    } else {
        println!("✓ Detected editors:");
        for editor in &detected_editors {
            match editor {
                Editor::ClaudeCode => println!("  • Claude Code"),
                Editor::Cursor => println!("  • Cursor"),
                Editor::Windsurf => println!("  • Windsurf"),
            }
        }
    }

    // Install hooks for detected editors
    for editor in &detected_editors {
        match editor {
            Editor::ClaudeCode => {
                config::install_claude_code_hooks(&repo_root)?;
            }
            Editor::Cursor => {
                config::install_cursor_hooks(&repo_root)?;
            }
            Editor::Windsurf => {
                config::install_windsurf_hooks(&repo_root)?;
            }
        }
    }

    // ... rest of init ...
}
```

### Success Criteria
- ✅ Detects Claude Code from `.claude/` directory
- ✅ Detects Cursor from `.cursor/` or `.cursorrules`
- ✅ Detects Windsurf from `.windsurf/` directory
- ✅ Git history analysis provides fallback detection
- ✅ Confidence levels accurately reflect detection certainty
- ✅ User sees clear feedback about detected editors
- ✅ Works with zero editors, one editor, or multiple editors

## Milestone 2.2: Cursor Hook Integration

**Goal**: Add provenance tracking for Cursor AI edits.

### Tasks
- [ ] Research Cursor's extension/plugin API
- [ ] Determine if Cursor supports hooks like Claude Code
- [ ] If hooks available: Implement Cursor hook handler
- [ ] If hooks unavailable: Design alternative tracking mechanism
- [ ] Add Cursor-specific agent type to provenance data
- [ ] Add `install_cursor_hooks()` to config module
- [ ] Write tests for Cursor integration
- [ ] Document Cursor setup process

### Cursor Hook Configuration

**Note**: Cursor's hook mechanism needs to be researched. This is a proposed approach:

```json
// .cursor/settings.json (if Cursor supports hooks)
{
  "hooks": {
    "onEdit": {
      "command": "aiki record-change --editor=cursor",
      "events": ["ai-edit", "ai-generation"]
    }
  }
}
```

### Cursor Provenance Record

```rust
// Update AgentType enum
pub enum AgentType {
    ClaudeCode,
    Cursor,        // NEW
    Windsurf,      // NEW
    Unknown,
}
```

### Alternative: Git Hook-Based Tracking

If Cursor doesn't support native hooks, use git pre-commit hooks:

```bash
#!/bin/sh
# .git/hooks/pre-commit

# Check if this commit is from Cursor (heuristic-based)
if [ -f ".cursor/session" ]; then
    # Record change via aiki
    aiki record-cursor-session
fi
```

### Success Criteria
- ✅ Cursor AI edits are captured in provenance database
- ✅ Cursor sessions are tracked separately from Claude Code
- ✅ Works with Cursor's actual API/hook mechanism
- ✅ Minimal performance impact on Cursor workflow
- ✅ User documentation is clear and complete

## Milestone 2.3: Windsurf Hook Integration

**Goal**: Add provenance tracking for Windsurf AI edits.

### Tasks
- [ ] Research Windsurf's extension/plugin API
- [ ] Determine Windsurf's hook capabilities
- [ ] Implement Windsurf hook handler (if supported)
- [ ] Design alternative if native hooks unavailable
- [ ] Add `install_windsurf_hooks()` to config module
- [ ] Write tests for Windsurf integration
- [ ] Document Windsurf setup process

### Windsurf Hook Configuration

```json
// .windsurf/settings.json (proposed)
{
  "hooks": {
    "onAIEdit": {
      "command": "aiki record-change --editor=windsurf"
    }
  }
}
```

### Success Criteria
- ✅ Windsurf AI edits captured in provenance database
- ✅ Windsurf sessions tracked independently
- ✅ Integration works with Windsurf's actual API
- ✅ Documentation complete

## Milestone 2.4: Multi-Editor Query Support

**Goal**: Extend `aiki status` and other queries to show per-editor statistics.

### Tasks
- [ ] Add editor filtering to query API
- [ ] Update `aiki status` to show editor breakdown
- [ ] Add `aiki stats --by-editor` command
- [ ] Add `aiki history --editor=cursor` filtering
- [ ] Update database queries for efficient editor filtering
- [ ] Write tests for multi-editor queries

### Enhanced Status Output

```
$ aiki status

AI Activity Summary:
  Total AI changes: 247
  
  By Editor:
    • Claude Code: 156 changes (63%)
    • Cursor:      78 changes (32%)
    • Windsurf:    13 changes (5%)
  
  Recent Activity (last 24 hours):
    • Claude Code: 12 changes
    • Cursor:      5 changes
    • Windsurf:    2 changes
```

### Success Criteria
- ✅ Users can see breakdown by editor
- ✅ Filtering works across all query commands
- ✅ Performance remains fast with multi-editor data
- ✅ Statistics are accurate and useful

## Milestone 2.5: Hook Management CLI

**Goal**: Provide manual hook management commands for troubleshooting and advanced users.

### Tasks
- [ ] Implement `aiki hooks status` command
- [ ] Implement `aiki hooks install <editor>` command
- [ ] Implement `aiki hooks remove <editor>` command
- [ ] Implement `aiki hooks list` command
- [ ] Add hook health checking (verify hooks are working)
- [ ] Add detailed diagnostics for hook failures
- [ ] Write tests for all hook management commands
- [ ] Document hook management workflow

### Hook Status Command

```bash
$ aiki hooks status

Hook Status:
┌─────────────┬──────────────┬──────────────┬─────────────────────────┐
│ Editor      │ Hooks        │ Editor       │ Status                  │
│             │ Installed    │ Detected     │                         │
├─────────────┼──────────────┼──────────────┼─────────────────────────┤
│ Claude Code │ ✓            │ ✓            │ Active                  │
│ Cursor      │ ✓            │ ✗            │ Dormant                 │
│ Windsurf    │ ✗            │ ✗            │ Not installed           │
└─────────────┴──────────────┴──────────────┴─────────────────────────┘

Details:
  Claude Code:
    Hook file: .claude/settings.json
    Last activity: 2 minutes ago
    Changes tracked: 156
    
  Cursor:
    Hook file: .cursor/settings.json
    Last activity: Never
    Changes tracked: 0
    Note: Hooks installed but Cursor not detected running
    
  Windsurf:
    Not configured
    
Tip: Run 'aiki hooks install <editor>' to add hooks for an editor
```

### Hook Status Output Variants

**All hooks working:**
```bash
$ aiki hooks status
✓ All configured editors are working correctly

Claude Code: Active (156 changes tracked)
Cursor:      Active (78 changes tracked)
```

**Some hooks dormant:**
```bash
$ aiki hooks status
⚠️  Some hooks are dormant (installed but editor not in use)

Claude Code: ✓ Active
Cursor:      ○ Dormant (hooks installed, waiting for Cursor)
Windsurf:    ✗ Not installed
```

**Hooks broken:**
```bash
$ aiki hooks status
✗ Hook configuration issues detected

Claude Code: ✓ Active
Cursor:      ✗ Broken (hook file corrupt)
             Fix: Run 'aiki hooks install cursor --force'

Run 'aiki hooks doctor' for detailed diagnostics
```

### Hook Install Command

```bash
# Install hooks for specific editor
$ aiki hooks install claude
Analyzing repository...
✓ Claude Code detected (config at .claude/)
✓ Installed hooks at .claude/settings.json
✓ Hook handler: aiki record-change

Next step: Restart Claude Code to activate hooks

# Install with force (overwrites existing)
$ aiki hooks install cursor --force
⚠️  Overwriting existing Cursor hooks
✓ Installed hooks at .cursor/settings.json

# Install all detected editors
$ aiki hooks install --all
Detecting editors...
✓ Claude Code detected
✓ Cursor detected
✗ Windsurf not detected

Installing hooks:
  ✓ Claude Code
  ✓ Cursor

# Install even if not detected (manual override)
$ aiki hooks install windsurf --force
⚠️  Windsurf not detected in repository
Proceeding anyway (--force specified)
✓ Installed hooks at .windsurf/settings.json

Note: Hooks will activate when you start using Windsurf
```

### Hook Remove Command

```bash
# Remove hooks for specific editor
$ aiki hooks remove claude
⚠️  This will remove Claude Code hooks
   100% accurate tracking will be disabled
   File watcher will continue at ~70-80% accuracy
   
Proceed? (y/N): y

✓ Removed hooks from .claude/settings.json
✓ Provenance data preserved (156 changes)
  
Tip: Reinstall anytime with 'aiki hooks install claude'

# Remove with cleanup
$ aiki hooks remove cursor --clean
✓ Removed hooks from .cursor/settings.json
✓ Removed .cursor/settings.json (file is now empty)

# Remove all hooks
$ aiki hooks remove --all
⚠️  This will remove ALL editor hooks
   Tracking will fall back to lower-accuracy methods
   
Remove hooks for: Claude Code, Cursor? (y/N): y

✓ Removed Claude Code hooks
✓ Removed Cursor hooks
✓ Provenance data preserved
```

### Hook List Command

```bash
$ aiki hooks list
Available editors:
  claude       Claude Code (Anthropic)
  cursor       Cursor (Anysphere)
  windsurf     Windsurf (Codeium)
  
Installed hooks:
  ✓ claude
  ✓ cursor
  
Not installed:
  ○ windsurf
  
Use 'aiki hooks install <editor>' to add hooks
Use 'aiki hooks status' for detailed status
```

### Hook Doctor Command (Advanced Diagnostics)

```bash
$ aiki hooks doctor

Running hook diagnostics...

✓ Checking aiki installation
  Version: 0.1.0
  Location: /Users/user/.cargo/bin/aiki
  
✓ Checking jj installation
  Version: 0.22.0
  Location: /usr/local/bin/jj
  
✓ Checking repository
  Git: ✓ Initialized
  JJ:  ✓ Initialized (colocated)
  
✓ Checking hook files
  Claude Code:
    File: .claude/settings.json ✓ Exists
    Syntax: ✓ Valid JSON
    Command: "aiki record-change" ✓ Found in PATH
    Permissions: ✓ Executable
    
  Cursor:
    File: .cursor/settings.json ✓ Exists
    Syntax: ✗ Invalid JSON (line 12: unexpected token)
    Fix: Run 'aiki hooks install cursor --force'
    
✓ Checking database
  Location: .aiki/provenance/attribution.db
  Status: ✓ Accessible
  Records: 234
  
✓ Checking recent activity
  Last 24 hours: 12 changes
  Last change: 2 minutes ago (Claude Code)
  
Summary: 1 issue found
  Fix with: aiki hooks install cursor --force
```

### Implementation

```rust
// cli/src/hooks.rs

use anyhow::{Context, Result};
use std::path::Path;
use crate::editor_detector::{Editor, EditorDetector};
use crate::config;

pub struct HookManager {
    repo_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookStatus {
    Active,        // Installed + editor detected + recent activity
    Dormant,       // Installed but editor not active
    Broken,        // Installed but configuration invalid
    NotInstalled,  // Not installed
}

pub struct EditorHookStatus {
    pub editor: Editor,
    pub installed: bool,
    pub detected: bool,
    pub status: HookStatus,
    pub last_activity: Option<DateTime<Utc>>,
    pub changes_tracked: usize,
    pub config_path: PathBuf,
}

impl HookManager {
    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// Get status of all hooks
    pub fn status(&self) -> Result<Vec<EditorHookStatus>> {
        let detector = EditorDetector::new(&self.repo_path);
        let detection = detector.detect_editors()?;
        
        let mut statuses = Vec::new();
        
        // Check each editor
        statuses.push(self.check_editor_status(Editor::ClaudeCode, &detection)?);
        statuses.push(self.check_editor_status(Editor::Cursor, &detection)?);
        statuses.push(self.check_editor_status(Editor::Windsurf, &detection)?);
        
        Ok(statuses)
    }

    /// Install hooks for specific editor
    pub fn install(&self, editor: &Editor, force: bool) -> Result<()> {
        match editor {
            Editor::ClaudeCode => {
                if !force && self.hooks_exist(editor)? {
                    anyhow::bail!("Claude Code hooks already installed. Use --force to overwrite.");
                }
                config::install_claude_code_hooks(&self.repo_path)?;
            }
            Editor::Cursor => {
                if !force && self.hooks_exist(editor)? {
                    anyhow::bail!("Cursor hooks already installed. Use --force to overwrite.");
                }
                config::install_cursor_hooks(&self.repo_path)?;
            }
            Editor::Windsurf => {
                if !force && self.hooks_exist(editor)? {
                    anyhow::bail!("Windsurf hooks already installed. Use --force to overwrite.");
                }
                config::install_windsurf_hooks(&self.repo_path)?;
            }
        }
        
        Ok(())
    }

    /// Remove hooks for specific editor
    pub fn remove(&self, editor: &Editor, clean: bool) -> Result<()> {
        match editor {
            Editor::ClaudeCode => {
                config::remove_claude_code_hooks(&self.repo_path, clean)?;
            }
            Editor::Cursor => {
                config::remove_cursor_hooks(&self.repo_path, clean)?;
            }
            Editor::Windsurf => {
                config::remove_windsurf_hooks(&self.repo_path, clean)?;
            }
        }
        
        Ok(())
    }

    /// Check if hooks exist for editor
    fn hooks_exist(&self, editor: &Editor) -> Result<bool> {
        let config_path = match editor {
            Editor::ClaudeCode => self.repo_path.join(".claude/settings.json"),
            Editor::Cursor => self.repo_path.join(".cursor/settings.json"),
            Editor::Windsurf => self.repo_path.join(".windsurf/settings.json"),
        };
        
        Ok(config_path.exists())
    }

    /// Run comprehensive diagnostics
    pub fn doctor(&self) -> Result<DiagnosticReport> {
        // Check aiki installation
        // Check jj installation
        // Check repository status
        // Check each hook file
        // Check database
        // Check recent activity
        // Return detailed report
        todo!()
    }
}
```

### CLI Integration

```rust
// cli/src/main.rs

#[derive(Subcommand)]
enum Commands {
    Init,
    RecordChange,
    
    /// Manage AI editor hooks
    Hooks {
        #[command(subcommand)]
        command: HooksCommand,
    },
}

#[derive(Subcommand)]
enum HooksCommand {
    /// Show hook status for all editors
    Status,
    
    /// Install hooks for an editor
    Install {
        /// Editor to install hooks for (claude, cursor, windsurf, or 'all')
        editor: String,
        
        /// Force reinstall (overwrite existing)
        #[arg(long)]
        force: bool,
    },
    
    /// Remove hooks for an editor
    Remove {
        /// Editor to remove hooks from
        editor: String,
        
        /// Also remove config file if empty
        #[arg(long)]
        clean: bool,
    },
    
    /// List available editors
    List,
    
    /// Run diagnostics on hook configuration
    Doctor,
}
```

### Success Criteria
- ✅ `aiki hooks status` shows clear, actionable information
- ✅ Users can install/remove hooks for any editor
- ✅ Force flag allows fixing broken configurations
- ✅ Doctor command helps troubleshoot issues
- ✅ Removing hooks preserves existing provenance data
- ✅ Clear warnings when disabling 100% accurate tracking
- ✅ All commands have helpful error messages
- ✅ Works with zero, one, or multiple editors

### User Experience Examples

**Fresh repo, no editors detected:**
```bash
$ aiki hooks status
No AI editors detected

Run 'aiki hooks list' to see available editors
Run 'aiki hooks install <editor>' to manually configure
```

**After manual install:**
```bash
$ aiki hooks install claude
✓ Installed Claude Code hooks
  
Start using Claude Code to begin tracking changes
Run 'aiki hooks status' to verify activation
```

**Broken configuration:**
```bash
$ aiki hooks doctor
✗ Found 1 issue:
  Cursor hooks: Invalid JSON syntax
  
Fix: aiki hooks install cursor --force
```

## Testing Strategy

### Unit Tests
- Editor detection logic (each heuristic)
- Confidence level calculations
- Hook installation for each editor
- Database queries with editor filters

### Integration Tests
```rust
#[test]
fn test_detect_claude_code_from_config() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".claude")).unwrap();
    
    let detector = EditorDetector::new(temp.path());
    let result = detector.detect_editors().unwrap();
    
    assert_eq!(result.claude_code, EditorConfidence::Confirmed);
}

#[test]
fn test_detect_multiple_editors() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".claude")).unwrap();
    fs::create_dir_all(temp.path().join(".cursor")).unwrap();
    
    let detector = EditorDetector::new(temp.path());
    let result = detector.detect_editors().unwrap();
    
    assert_eq!(result.claude_code, EditorConfidence::Confirmed);
    assert_eq!(result.cursor, EditorConfidence::Confirmed);
}

#[test]
fn test_init_installs_multiple_hooks() {
    // Create repo with both Claude and Cursor markers
    // Run aiki init
    // Verify both .claude/settings.json and .cursor/settings.json exist
}
```

### End-to-End Tests
- Initialize in repo with Claude Code → only Claude hooks installed
- Initialize in repo with Cursor → only Cursor hooks installed
- Initialize in repo with both → both hooks installed
- Make edits with each editor → verify separate tracking
- Query stats → verify correct per-editor breakdown

## Dependencies

```toml
[dependencies]
# Existing dependencies...
# No new dependencies needed for Phase 2!
```

## Success Metrics

### Completion Criteria
- ✅ `aiki init` automatically detects editors (no user input needed)
- ✅ Cursor AI edits tracked with high confidence
- ✅ Windsurf AI edits tracked with high confidence
- ✅ Multi-editor repos work seamlessly
- ✅ All tests pass
- ✅ Documentation covers all three editors

### User Experience Goals
- **Zero-config detection**: Users don't specify editors, aiki figures it out
- **Clear feedback**: Users see what was detected and why
- **Opt-out available**: Users can disable specific editor tracking if desired
- **Fast execution**: Detection adds <100ms to init time

### Technical Goals
- **Accurate detection**: >95% accuracy in editor identification
- **Extensible architecture**: Easy to add new editors in future
- **Minimal overhead**: Hook handlers remain <25ms
- **Database efficiency**: Editor filtering doesn't slow queries

## Future Extensions (Phase 3+)

- **GitHub Copilot** integration
- **Tabnine** integration
- **Amazon CodeWhisperer** integration
- **Plugin system** for community-contributed editor integrations
- **Cloud sync** for provenance across machines
- **Team dashboards** showing AI usage patterns

## Next Phase

**Phase 3**: LLM-Based Review (previously Phase 2)
- Static analysis review
- Security vulnerability detection  
- Code quality suggestions
- Automated PR comments
