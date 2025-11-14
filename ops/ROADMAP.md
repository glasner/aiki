# Aiki Product Roadmap

## Overview

Aiki follows a **twelve-phase development strategy**, where each phase validates assumptions before proceeding to the next. The roadmap builds from foundational infrastructure (CLI, JJ, provenance) through multi-editor support, hook management, cryptographic verification, and solving individual developer pain (autonomous review) to full multi-agent team orchestration and enterprise compliance.

**Foundation:** All phases build on complete provenance tracking via Jujutsu (JJ), capturing edit-level history, agent attribution, and iteration tracking that Git cannot provide.

---

## Phase 0: Initial CLI & JJ Setup

### Problem
Before implementing provenance tracking and autonomous review, we need a robust CLI infrastructure and reliable JJ integration. Developers need a simple way to initialize Aiki in their repositories and ensure JJ is properly configured.

### Solution
Build the foundational CLI application with JJ integration, configuration management, and repository setup capabilities.

### What We Build
- **CLI application structure** - Argument parsing, command routing, help system (✅ Completed)
- **JJ library integration** - Direct jj-lib crate usage (no external binary needed) (✅ Completed)
- **Repository initialization** - Colocated JJ/Git workspace setup (✅ Completed)
- **Configuration management** - TOML-based config loading and validation (✅ Completed)
- **Repository detection** - Validate Git/JJ repository state (✅ Completed)
- **Error handling** - User-friendly error messages and recovery suggestions (✅ Completed)

### Commands Delivered
```bash
aiki init              # Initialize Aiki in current repository (✅ Completed)
aiki --version         # Show version (✅ Completed)
aiki --help            # Show help (✅ Completed)
```

**Current Status:** Phase 0 completed with basic `aiki init` command using jj-lib.

### Value Delivered
- **Developer onboarding** - Simple setup process (`aiki init`)
- **Reliable foundation** - Robust JJ integration for all future phases
- **Configuration flexibility** - Per-repository and global settings
- **Troubleshooting** - Clear diagnostics when issues occur

### Technical Components
| Component | Complexity | Status |
|-----------|------------|--------|
| CLI framework (clap) | Low | ✅ Completed |
| jj-lib integration | Medium | ✅ Completed |
| Configuration parsing (TOML) | Low | ✅ Completed |
| Repository state detection | Low | ✅ Completed |
| Error handling | Low | ✅ Completed |

### Success Criteria
- ✅ `aiki init` successfully initializes a colocated JJ repository (using jj-lib)
- ✅ JJ operations can be executed via jj-lib crate (no external binary needed)
- ✅ Configuration can be loaded and validated
- ✅ Repository state (Git/JJ) can be queried reliably
- ✅ Works with Git worktrees and submodules
- ✅ All tests pass (27/27)
- ✅ Zero compiler warnings

**Note:** Using jj-lib v0.35.0 eliminates the need for external JJ binary installation, improving reliability and portability.

---

## Phase 1: Claude Code Provenance (Hook-Based)

### Problem
Developers using Claude Code have no visibility into which code changes the AI made. When issues arise, it's impossible to trace them back to specific Claude Code edits. Teams lack the foundation needed to test and validate autonomous review capabilities.

**Without provenance:**
- Can't attribute bugs to Claude Code
- Can't measure Claude Code quality over time
- Can't test autonomous review feedback loops
- Can't validate AI self-correction iterations
- Can't demonstrate which code was AI-generated vs human-written

### Solution
**Hook-based provenance tracking for Claude Code with 100% accuracy.** Leverage Claude Code's native PostToolUse hooks to capture exact edit information immediately as it happens.

**Key Insight:** Claude Code provides native hooks that tell us exactly what changed, eliminating all guesswork and achieving perfect attribution accuracy.

### What We Build
- **Claude Code hook integration** - PostToolUse hooks for Edit|Write tools (✅ Completed)
- **Hook handler binary** - Lightweight process to record provenance (✅ Completed)
- **JJ commit description metadata** - Embed `[aiki]...[/aiki]` blocks in commit descriptions (✅ Completed)
- **Edit-level attribution** - Map file changes to Claude Code with 100% confidence (✅ Completed)
- **Provenance persistence** - Store attribution in JJ commit descriptions (~120 bytes per change) (✅ Completed)
- **Session tracking** - Group related Claude Code edits together (✅ Completed)

**Architecture Decision:** No SQLite database - JJ commit descriptions are the single source of truth. This eliminates database maintenance, reduces storage overhead, and leverages JJ's native revset query engine for efficient filtering.

### Commands Delivered
```bash
aiki init             # Install Claude Code hooks + start tracking (✅ Completed)
aiki record-change    # Hook handler command (internal, called by hooks) (✅ Completed)
# Future commands (Phase 1.2):
aiki status           # Show Claude Code activity (Planned)
aiki history          # View complete provenance timeline (Planned)
aiki blame <file>     # Show which lines Claude Code edited (Planned)
aiki stats            # Show detection accuracy (Planned)
```

**Current Status (Phase 1.1):** Hook integration and provenance recording complete. Query commands planned for Phase 1.2.

### Example Output
```bash
$ aiki status
Repository: /Users/dev/project
JJ Status: Clean

Active Tracking:
  ✓ Claude Code hooks installed
  ✓ Provenance recording active

Recent Activity (last hour):
  10m ago: Claude Code edited auth.py (hook) ✓✓✓ High confidence
  25m ago: Claude Code edited utils.py (hook) ✓✓✓ High confidence
  45m ago: JJ snapshot created (15 edits)

$ aiki blame auth.py
45: Claude Code ✓✓✓ (10m ago, hook)    def verify_token(token: str) -> bool:
46: Claude Code ✓✓✓ (10m ago, hook)        """Verify JWT token validity."""
47: Claude Code ✓✓✓ (10m ago, hook)        try:
48: Claude Code ✓✓✓ (10m ago, hook)            decoded = jwt.decode(token, SECRET_KEY)

$ aiki stats
Detection Accuracy (last 7 days):
Hook-based (Claude Code): 892 edits (100%) - ✓✓✓ High confidence

Overall: 100% attribution accuracy ✓
```

### Value Delivered
- **100% attribution accuracy** - Claude Code hooks provide perfect information
- **Dramatically simplified** - No complex process detection needed
- **Fast implementation** - 2-3 weeks vs 4-6 weeks
- **Testing foundation** - Enables comprehensive testing of Phase 2 (autonomous review)
- **Session tracking** - Understand how Claude Code works over time
- **Confidence indicators** - All attributions marked as High confidence

### Technical Components
| Component | Complexity | Priority | Status |
|-----------|------------|----------|--------|
| Claude Code hook configuration | Low | High | ✅ Complete |
| Hook handler binary | Low | High | ✅ Complete |
| JJ commit description embedding | Low | High | ✅ Complete |
| Background threading for async updates | Low | High | ✅ Complete |
| Provenance serialization/deserialization | Low | High | ✅ Complete |
| CLI commands (query/blame) | Medium | High | 🔜 Phase 1.2 |

**Architecture Note:** Removed SQLite dependency. Using JJ commit descriptions as single source of truth (~120 bytes per change). Hook handler completes in ~7-8ms (target: <10ms).

### Hook Integration

**Claude Code PostToolUse Hook** - Automatically triggered after Edit/Write:
```json
// .claude/settings.json (created by aiki init)
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "aiki-hook-handler",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

**Hook receives exact edit information:**
- session_id - Claude Code session
- tool_name - Edit or Write

**Architecture Decision:** Store only what JJ doesn't know. File paths, diffs, and timestamps are queried from JJ's native APIs when needed. This keeps metadata lightweight (~120 bytes) and eliminates redundancy.

**No guessing. No detection. Perfect attribution.**

### Provenance Data Model

```rust
struct ProvenanceRecord {
    agent: AgentInfo,           // Contains agent_type, confidence, method
    session_id: String,         // Claude Code session ID
    tool_name: String,          // Edit, Write, etc.
    // Note: file_path, timestamp, diffs queried from JJ when needed
}

struct AgentInfo {
    agent_type: AgentType,      // ClaudeCode in Phase 1
    version: Option<String>,    // Claude Code version (if available)
    detected_at: DateTime,      // When agent was detected
    confidence: AttributionConfidence,  // Always High for hook-based
    detection_method: DetectionMethod,  // Always Hook for Phase 1
}

enum AgentType {
    ClaudeCode,
    Unknown,
}

enum AttributionConfidence {
    High,    // Hook-based (100% accurate)
    Medium,  // Future: File watching
    Low,     // Future: Process detection
}

enum DetectionMethod {
    Hook,            // PostToolUse hooks
    FileWatcher,     // Future: FSEvents
    ProcessDetection,// Future: lsof/ps
}
```

**Serialization Format (in JJ commit descriptions):**
```
[aiki]
agent=claude
session=550e8400-e29b-41d4-a716-446655440000
tool=edit
confidence=high
method=hook
[/aiki]
```

Size: ~120 bytes per change. Stored in JJ commit descriptions, queryable via JJ revsets.

### Success Criteria
- ✅ Hook integration works reliably with Claude Code
- ✅ 100% attribution accuracy for Claude Code edits
- ✅ Hook handler completes in <10ms (actual: ~7-8ms, doesn't slow down Claude)
- ✅ Commit description embedding works via background threading
- ✅ Provenance serialization/deserialization tested and validated
- ✅ All tests pass (41/41)
- ✅ Zero compiler warnings
- ⏳ Line-level attribution (planned for Phase 1.2 using JJ's FileAnnotator API)
- ⏳ Query CLI commands (planned for Phase 1.2)
- ✅ Enable Phase 2 testing with full provenance data

### Why This Enables Phase 2 Testing

**Testing autonomous review requires knowing:**
1. Which Claude Code session made the original code
2. Which session made the fix after review
3. How many iterations occurred
4. Whether Claude Code correctly responded to feedback

**With hook-based provenance, we can test:**
- Claude Code correctly reads review feedback
- Claude Code makes appropriate fixes
- Multi-iteration correction loops work
- Review quality improves over iterations
- All with 100% attribution accuracy

### Technical Notes
- Hook-based: No process detection, no file watching for Claude Code
- 100% accuracy: Hook tells us exactly what happened
- Fast: Implemented in ~2 weeks
- Simple: JSON config + lightweight binary + JJ commit descriptions
- Proven: Uses Claude Code's official hook system
- SQLite-free: JJ is single source of truth
- Lightweight: ~120 bytes per change
- Fast queries: JJ revsets (e.g., `description(glob:"*agent=claude*")`)
- Focused: Phase 1 is Claude Code only, Phase 2 adds other editors

**Implementation Complete (Phase 1.1):**
- Core hook integration: ✅
- Provenance recording: ✅
- Background threading: ✅
- All tests passing: ✅

**Next Steps:**
- Phase 1.2: Query commands + line-level attribution
- Phase 1.3: Expand `aiki authors --changes` flag support
- Phase 2: Multi-editor support (Cursor, Windsurf)

### Phase 1.3: Expand `aiki authors --changes` Flag (Future Enhancement)

**Current Implementation (Phase 1.1):**
```bash
aiki authors                    # Working copy (@) - default
aiki authors --changes=staged   # Git staging area
aiki authors --format=git       # Output format options
aiki authors --format=json
```

**Future Enhancements (Phase 1.3):**
```bash
# Specific change by ID
aiki authors --changes=abc123

# Multiple changes (comma-separated)
aiki authors --changes=abc123,def456

# JJ revset expressions
aiki authors --changes='trunk..@'      # All changes from trunk to working copy
aiki authors --changes='@-'            # Parent of working copy
aiki authors --changes='description(glob:"*feat*")'  # Changes matching description

# Combined with format options
aiki authors --changes='trunk..@' --format=git
```

**Implementation Tasks:**
- Parse single change IDs and query JJ for change metadata
- Support comma-separated list of change IDs
- Add JJ revset expression parser integration
- Handle invalid change IDs gracefully
- Update documentation with revset examples

---

## Phase 2: Cursor Support

**Architecture:** Phase 2 extends Phase 1's SQLite-free architecture to Cursor. Uses the same `[aiki]...[/aiki]` format (~120 bytes per change) in JJ commit descriptions. No new dependencies required.

---

## Phase 3: Hook Management CLI

### Problem
Users need visibility and control over all Aiki hooks - both AI editor hooks (Claude Code, Cursor) and Git hooks (prepare-commit-msg). When hooks aren't working, users need diagnostic tools to identify and fix issues quickly.

**Without hook management:**
- Can't see which hooks are installed or active
- Manual hook troubleshooting is time-consuming
- No way to selectively enable/disable hooks
- Difficult to diagnose why hooks aren't triggering
- Can't verify hooks are working correctly

### Solution
Provide comprehensive hook management commands with unified interface for editor hooks and Git hooks. Include intelligent diagnostics that detect and repair common issues automatically.

**Key Innovation:** Single CLI interface manages all hook types (editor + Git) with automatic health checks and repair capabilities.

### What We Build
- **Hook status visibility** - Show all installed hooks and their state
- **Manual hook management** - Install/remove specific hooks
- **Hook diagnostics** - Detect configuration issues, permission problems, outdated templates
- **Automatic repair** - Fix common issues with `--fix` flag
- **Activity tracking** - Show when hooks last executed
- **Multi-hook support** - Manage editor and Git hooks through unified interface

### Commands Delivered
```bash
aiki hooks status               # Show status of all hooks
aiki hooks status --editor      # Show only editor hooks
aiki hooks status --git         # Show only Git hooks
aiki hooks install <target>     # Install specific hook (claude-code, cursor, git)
aiki hooks install --all        # Install all detected hooks
aiki hooks remove <target>      # Remove specific hook
aiki hooks remove --all         # Remove all hooks
aiki hooks list                 # List available integrations
aiki hooks doctor               # Diagnose hook issues
aiki hooks doctor --fix         # Automatically repair issues
```

### Example Output
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

Found 2 issues. Run 'aiki hooks doctor --fix' to repair.
```

### Hook Types Managed

**Editor Hooks:**
- Claude Code: PostToolUse hooks in `.claude/settings.json`
- Cursor: Hook config in `.cursor/aiki-hooks.json`
- Windsurf: (Future - Phase 10)

**Git Hooks:**
- `prepare-commit-msg`: Injects AI co-authors into commit messages
- Uses templates from `cli/templates/`

### Diagnostics Performed

**Editor Hooks:**
- Configuration file exists and has valid format
- Hook command is correct
- Hook is actually triggering (check recent activity)

**Git Hooks:**
- Hook file exists in `.git/hooks/`
- Hook has executable permissions
- Hook template is up-to-date
- Hook references correct `aiki` commands

**Common Fixes:**
- Regenerate corrupted config files
- Set executable permissions on Git hooks
- Update outdated hook templates
- Repair invalid JSON/format

### Value Delivered
- **Hook visibility** - Users see exactly which hooks are active
- **Easy troubleshooting** - Doctor command identifies issues automatically
- **Quick repair** - `--fix` flag resolves common problems
- **Selective control** - Install/remove specific hooks as needed
- **Unified interface** - Single command set for all hook types

### Technical Components
| Component | Complexity | Priority |
|-----------|------------|----------|
| Hook status detection | Low | High |
| Hook installation/removal | Low | High |
| Diagnostics engine | Medium | High |
| Automatic repair logic | Medium | High |
| Activity tracking | Low | Medium |

### Success Criteria
- ✅ `aiki hooks status` shows accurate state of all hooks
- ✅ Manual hook installation works for all hook types
- ✅ Hook removal cleanly uninstalls without breaking repo
- ✅ Doctor detects 90%+ of common issues
- ✅ Doctor --fix successfully repairs detected issues
- ✅ User-friendly error messages with actionable fixes
- ✅ Works with both editor and Git hooks

---

## Phase 4: Cryptographic Commit Signing

### Problem
AI-generated code lacks cryptographic verification. Current provenance tracking (Phase 1) stores attribution in JJ change descriptions as plain text `[aiki]` metadata, but:
- No guarantee the metadata hasn't been tampered with
- Can't prove to auditors that attribution is authentic
- Enterprise compliance requirements demand cryptographic proof
- Supply chain security needs verifiable AI authorship

**Without signing:**
- Anyone can edit `[aiki]` blocks to fake AI attribution
- No cryptographic chain of custody for code changes
- Auditors can't verify claims about who (human or AI) wrote code
- Can't meet regulatory requirements for tamper-proof audit trails

### Solution
Leverage JJ's native commit signing to cryptographically sign all changes containing AI provenance. Use GPG, SSH, or GPG-SM to create tamper-proof attribution that satisfies enterprise compliance and audit requirements.

**Key Insight:** JJ already supports commit signing. Aiki enables it automatically during `aiki init` and signs all AI-attributed changes, creating an immutable cryptographic chain from AI tool → provenance metadata → signed commit.

### What We Build
- **Automatic signing configuration** - Enable JJ signing during `aiki init`
- **Key management integration** - Detect existing GPG/SSH keys or guide setup
- **Provenance + signature workflow** - Sign changes with `[aiki]` metadata automatically
- **Signature verification commands** - Verify AI attribution authenticity
- **Multi-backend support** - GPG, SSH, and GPG-SM signing methods
- **Compliance reporting** - Generate signed provenance reports for auditors

### Commands Delivered
```bash
aiki init --signing     # Initialize with commit signing enabled (auto-detects keys)
aiki sign setup         # Interactive key setup (GPG/SSH/GPG-SM)
aiki verify <change-id> # Verify signature + provenance for a change
aiki verify --all       # Verify all AI-attributed changes in repo
aiki audit-report       # Generate signed provenance report for compliance
```

### Configuration
Aiki will configure JJ signing in `.jj/repo/config.toml`:

```toml
# Aiki-managed signing configuration
[signing]
behavior = "own"        # Sign all commits you author when modified
backend = "gpg"         # or "ssh" or "gpgsm" based on detection
# key = "auto-detected" # Uses user.email by default

# Optional: SSH signing
# backend = "ssh"
# key = "~/.ssh/id_ed25519.pub"
# [signing.backends.ssh]
# allowed-signers = ".jj/allowed-signers"

# Optional: Enable signature display
[ui]
show-cryptographic-signatures = true
```

### Example Flow
```bash
$ aiki init --signing
Initializing Aiki...
✓ JJ repository initialized
✓ Detected GPG key: 4ED556E9729E000F (user@example.com)
✓ Configured commit signing (backend: gpg, behavior: own)
✓ Claude Code hooks installed
✓ All AI changes will be cryptographically signed

$ aiki record-change --claude-code
# (Called by Claude Code hook after Edit)
# Records provenance: agent=claude-code, session=abc123, tool=Edit
# JJ automatically signs the change with GPG key
# Result: Tamper-proof attribution

$ aiki verify abc123def
Change: abc123def456
✓ Signature valid (GPG key 4ED556E9729E000F)
✓ Provenance verified:
  Agent: Claude Code
  Session: claude-session-abc123
  Tool: Edit
  Confidence: High
  Signed by: user@example.com
  Signed at: 2025-01-15 14:32:10 UTC

Cryptographic verification: PASSED ✓

$ aiki audit-report --format=pdf
Generating compliance report...
✓ Analyzed 1,247 changes with AI attribution
✓ All signatures valid
✓ Report saved to: aiki-audit-2025-01-15.pdf

Report includes:
- Full provenance for all AI changes
- Cryptographic signature verification
- Timeline of AI contributions
- Agent attribution breakdown
```

### Value Delivered
- **Cryptographic proof** - Tamper-proof AI attribution (can't fake `[aiki]` metadata)
- **Compliance ready** - Meets SOX, PCI-DSS, ISO 27001 audit requirements
- **Supply chain security** - Verifiable authorship for AI-generated code
- **Enterprise confidence** - Cryptographically prove who (human or AI) wrote code
- **Automatic signing** - No manual intervention needed (configured once)
- **Audit reports** - Generate signed provenance reports for regulators
- **Multi-backend support** - Works with GPG, SSH, or GPG-SM (enterprise PKI)

### Technical Components
| Component | Complexity | Priority |
|-----------|------------|----------|
| JJ signing configuration (aiki init) | Low | High |
| Key detection and setup wizard | Medium | High |
| Automatic signing for AI changes | Low | High |
| Signature verification commands | Medium | High |
| Compliance report generation | Medium | Medium |
| Multi-backend support (GPG/SSH/GPG-SM) | Low | Medium |
| UI for signature display | Low | Low |

### Architecture Notes

**How Signing Works:**
1. Claude Code makes an edit (triggers PostToolUse hook)
2. `aiki record-change --claude-code` embeds `[aiki]` metadata in JJ change description
3. JJ automatically signs the change (per `signing.behavior = "own"`)
4. Result: Change with both provenance metadata AND cryptographic signature

**Signature Verification:**
- `aiki verify` checks:
  1. Is the JJ commit signature valid?
  2. Does the change contain `[aiki]` metadata?
  3. Does the signer match the expected author?
- Reports: PASSED (both valid) or FAILED (signature invalid or metadata missing)

**Key Management:**
- Auto-detect existing GPG/SSH keys during `aiki init --signing`
- If none found, launch interactive setup wizard (`aiki sign setup`)
- Support enterprise PKI via GPG-SM backend

**Performance:**
- Signing adds ~10-50ms per change (GPG) or ~1-5ms (SSH)
- Verification is fast (< 10ms for GPG, < 1ms for SSH)
- No impact on hook handler latency (signing happens after hook returns)

### Success Criteria
- ✅ `aiki init --signing` automatically configures JJ signing
- ✅ All AI changes automatically signed with detected keys
- ✅ `aiki verify` validates signatures + provenance
- ✅ Support GPG, SSH, and GPG-SM backends
- ✅ Audit reports generate signed provenance summaries
- ✅ Works on macOS, Linux, Windows
- ✅ Key setup wizard guides users without existing keys
- ✅ Zero performance impact on Claude Code hooks

### Why This Enables Enterprise Adoption

**Before Phase 3:**
- Provenance is plain text (can be edited)
- No cryptographic proof of attribution
- Auditors can't verify claims
- Fails enterprise compliance requirements

**After Phase 3:**
- Provenance is cryptographically signed
- Tamper-proof attribution (edit `[aiki]` → signature breaks)
- Auditors can verify with standard tools (gpg --verify)
- Meets SOX, PCI-DSS, ISO 27001 requirements
- Supply chain security (SLSA/SBOM compatible)

---

## Phase 5: Autonomous Review & Self-Correction Loop

### Problem
Developers waste significant time per feature fixing AI-generated code through manual iteration loops. AI commits blindly, humans discover issues through slow manual testing or CI failures.

**With provenance (Phase 1), we can now:**
- Test that agents correctly respond to review feedback
- Validate multi-iteration correction loops
- Measure review quality improvements
- Attribute fixes to specific agents

### Solution
Autonomous review feedback loop where AI validates and corrects its own work before reaching Git:
1. AI generates code (Cursor, Copilot, etc.)
2. AI attempts commit (normal `git commit` workflow)
3. Aiki intercepts (pre-commit hook)
4. Autonomous review runs (static analysis + AI review)
5. Results fed back to AI agent (**tracked via provenance**)
6. AI reads review results and fixes issues (**attributed via provenance**)
7. AI attempts commit again (repeats until passing or escalates)
8. Human sees only final, reviewed code

### What We Build
- **Git commit interception** - Pre-commit hook integration
- **Autonomous review engine** - Static analysis + AI review
- **Agent feedback loop** - Structured feedback to agents
- **Self-correction iteration** - Auto-retry until passing
- **Review provenance** - Track review iterations via Phase 1
- **Web UI enhancements** - View review history and iterations

### Value Delivered
- **Time savings** - Significant reduction in manual iteration cycles
- **Quality improvement** - Catch bugs pre-commit (type errors, security issues, complexity)
- **Agent attribution** - Know which AI made which edit (via Phase 1)
- **Clean Git history** - No failed CI commits or fix iterations
- **Testable feedback loops** - Validate agent responses via provenance

### Technical Components
| Component | Complexity |
|-----------|------------|
| Git hook integration | Medium |
| Autonomous review engine | High |
| Agent feedback protocol | Medium |
| Iteration control logic | Medium |
| Review provenance (Phase 1 integration) | Low |
| Web UI updates | Low |

### Example Flow (with Provenance)
```bash
# Agent makes changes
git add auth.py
git commit -m "Add caching"

# Aiki intercepts
Aiki: Reviewing changes...
  Agent detected: Cursor v0.42.0 (via provenance)
  Files changed: auth.py (lines 45-60)

Aiki Review:
  ❌ Type error: Missing return type on verify_token()
  ❌ Security: API key hardcoded in auth.py:45
  ⚠ Complexity: authenticate() cyclomatic 18 (limit: 10)

# Feedback sent to Cursor (tracked in provenance)
Cursor: Reading Aiki review...
  Fixing type error...
  Removing hardcoded API key...
  Refactoring authenticate()...

# Cursor attempts commit again
git commit -m "Add caching (reviewed)"

# Aiki reviews again (iteration 2, tracked via provenance)
Aiki: Reviewing changes...
  Agent: Cursor v0.42.0
  Iteration: 2
  
Aiki Review:
  ✓ All checks passed

# Commit succeeds
Provenance recorded:
  - Initial commit attempt by Cursor (failed review)
  - Review feedback generated
  - Fixes made by Cursor (iteration 2)
  - Final commit approved
```

---

## Phase 6: Multi-Agent Provenance (Fallback Detection)

### Problem
Developers use agents beyond Claude Code (Cursor, Copilot, custom tools, or manual edits), but Phase 1 only tracks Claude Code. Without provenance for these agents:
- Can't attribute bugs to Cursor/Copilot
- Can't compare agent quality across tools
- Can't track human vs AI edits
- Incomplete provenance picture

### Solution
Add fallback provenance detection for agents beyond Claude Code/Cursor/Windsurf using file watching + simplified process detection. Achieve 70-80% accuracy for agents without native hooks (e.g., Copilot, custom tools).

**Key Insight:** This is optional—Phase 1 + 2 provide full value for hook-based editors (Claude Code, Cursor, Windsurf). Phase 4 extends to agents without hook support.

### What We Build
- **File watcher** - Detect file changes via FSEvents (macOS)
- **Simplified 3-layer detection** - lsof, active process heuristic, unknown fallback
- **Multi-agent attribution** - Track hook-based editors (100%) + others (70-80%)
- **Unified provenance** - Same JJ commit description format for all agents
- **Confidence indicators** - Show hook-based vs fallback detection

**Architecture:** Extends the same `[aiki]...[/aiki]` format with different confidence levels (High for hooks, Medium for file watching, Low for process detection).

### Commands Enhanced
```bash
aiki status           # Show all active agents (not just Claude Code)
aiki history          # Complete multi-agent timeline
aiki blame <file>     # Attribution for all agents
aiki agents           # List all detected agents (Cursor, Copilot, etc.)
aiki stats            # Accuracy breakdown by detection method
```

### Example Output
```bash
$ aiki status
Active Agents:
  Claude Code (hook) ✓✓✓ - 15 edits today
  Cursor (lsof) ✓✓ - 3 edits today
  Unknown ? - 1 edit today

$ aiki stats
Detection Accuracy (last 7 days):
Hook-based (Claude Code): 892 edits (85.2%) - ✓✓✓ High
File watching (Cursor): 127 edits (12.1%) - ✓✓ Medium
Unknown: 9 edits (0.9%) - ? Unknown

Overall: 85% high confidence, 12% medium confidence
```

### Value Delivered
- **Complete attribution** - All agents tracked, not just Claude Code
- **Multi-agent comparison** - Compare Claude Code vs Cursor vs Copilot quality
- **Human vs AI** - Distinguish AI edits from manual changes
- **Confidence tracking** - Know which attributions are reliable

### Technical Components
| Component | Complexity |
|-----------|------------|
| File watcher (FSEvents) | Low |
| lsof-based detection | Low |
| Active process heuristic | Low |
| Multi-agent database schema | Low |
| Confidence distribution tracking | Low |

### Success Criteria
- ✅ Hook-based editors still 100% accurate (Claude Code, Cursor, Windsurf)
- ✅ Other agents 70-80% accurate (fallback detection for Copilot, etc.)
- ✅ Overall 85%+ attribution coverage
- ✅ Confidence levels clearly indicated in JJ commit descriptions
- ✅ Works on macOS (Linux/Windows later)

### Technical Notes
- File watching activates only for non-hook-based editors
- Much simpler than original multi-layer approach
- Graceful degradation from 100% (hooks) to 70-80% (lsof)
- Same JJ commit description format, different confidence levels
- Optional phase - full value without it for hook-based editor users

---

## Phase 7: Local Multi-Agent Coordination

### Problem
Multiple local AIs (Claude Code + Cursor + Copilot + custom agents) overwrite each other's changes. Each AI works independently on the same filesystem, unaware of others. Conflicts discovered late (at commit or code review), resulting in wasted AI work.

**Example:**
```
Claude Code adds caching to auth.py (lines 45-50)
Cursor autocomplete "optimizes" auth.py (lines 45-48)
→ Cursor unknowingly overwrites Claude Code's work
Developer attempts commit
→ Which changes should be kept?
```

### Solution
Sequential overwrite detection, auto-merge, and quarantine functionality for local multi-agent conflicts. Leverages provenance from Phase 1 + 2 + 4 to track which agent made which change.

### What We Build
- **Multi-agent detection** - Track concurrent local agent activity (uses Phase 1 + 2 + 4)
- **Sequential overwrite detection** - Identify when agents edit same files/lines
- **Complete timeline** - Show all agent activity in chronological order (query JJ commit descriptions)
- **Auto-merge compatible changes** - Merge non-conflicting edits automatically on rebase
- **Quarantine conflicts** - Push clean code, defer conflict resolution

### Value Delivered
- Eliminate local agent conflicts
- Smart rebase on remote changes
- Quarantine functionality (push clean code, resolve conflicts later)
- Local multi-agent provenance tracking (via JJ commit descriptions)

---

## Phase 8: PR Review for Non-Aiki Agents

### Problem
Cloud-based AI agents (Copilot Workspace, Devin, Sweep) generate PRs from isolated environments where Aiki daemon cannot be installed. These PRs bypass all Aiki quality gates, creating inconsistent quality across the team.

### Solution
GitHub/GitLab webhook integration to run autonomous review on all PRs, regardless of source.

### What We Build
- **GitHub/GitLab webhook integration** - Monitor all PRs created
- **PR autonomous review** - Same review engine as Phase 2, applied to PRs
- **GitHub bot comments** - Detailed review results as PR comments
- **PR labels and status checks** - `aiki-review-passed/failed` labels
- **Metrics dashboard** - Track cloud agent quality over time

### Value Delivered
- Consistent quality across all agents (local and cloud)
- Cloud agent PRs reviewed automatically
- No agent cooperation required (works via webhooks)
- Teams get uniform quality standards

---

## Phase 9: Shared JJ Brain & Team Coordination

### Problem
Even with local coordination (Phase 6) and PR review (Phase 7), developers with Aiki work independently. No visibility into what other developers' agents are working on until push/merge. Conflicts discovered late, resulting in wasted work.

### Solution
Distributed JJ repository mirroring for team-wide pre-merge conflict detection and coordination.

### What We Build

**Jujutsu OSS Contributions:**
- **Distributed JJ repository mirroring** - Sync JJ repos across team
- **Performance optimizations** - Scale JJ to team coordination
- **Enhanced conflict detection algorithms** - Better multi-agent conflict awareness
- **API improvements** - Support remote coordination use cases

**Aiki Features:**
- **Shared JJ Brain** - Centralized coordination repository
- **Pre-merge conflict detection** - Warn before conflicts occur
- **Repository-wide provenance** - See all agent activity across team (query JJ commit descriptions)
- **Team activity dashboard** - Real-time view of who's working on what (via JJ revsets)

### Value Delivered
- Team-wide real-time coordination
- Pre-merge conflict awareness
- Repository-wide provenance tracking (via JJ commit descriptions with `[aiki]` metadata)
- Prevent wasted work from conflicts

---

## Phase 10: Windsurf Support

### Problem
Windsurf is another AI-powered code editor gaining traction, but it lacks provenance tracking integration with Aiki. Teams using Windsurf alongside Claude Code and Cursor need unified attribution across all their AI tools.

### Solution
Extend Aiki's hook-based provenance architecture to Windsurf, using the same `[aiki]...[/aiki]` metadata format. Achieve 100% attribution accuracy for Windsurf edits through native hook integration.

**Key Insight:** Like Claude Code and Cursor, Windsurf can provide native hooks or API integration, enabling perfect attribution without guesswork.

### What We Build
- **Windsurf hook integration** - PostToolUse or equivalent hooks for Windsurf
- **Unified provenance format** - Same `[aiki]` metadata structure
- **Agent type detection** - Add `AgentType::Windsurf` enum variant
- **Co-author formatting** - `Windsurf <windsurf@windsurf.com>` in Git trailers

### Commands Enhanced
```bash
# All existing commands work with Windsurf
aiki authors                    # Shows Windsurf authors in working copy
aiki blame src/file.rs          # Attributes lines to Windsurf
aiki verify <change-id>         # Verifies Windsurf-signed changes
```

### Example Output
```bash
$ aiki authors --format=git --changes=staged
Co-authored-by: Claude Code <claude-code@anthropic.ai>
Co-authored-by: Cursor <cursor@cursor.sh>
Co-authored-by: Windsurf <windsurf@windsurf.com>

$ aiki blame auth.py
abc12345 (ClaudeCode   session-123  High  )    1| def authenticate():
def67890 (Cursor       session-456  High  )    2|     user = get_user()
xyz98765 (Windsurf     session-789  High  )    3|     return validate(user)
```

### Value Delivered
- **Complete editor coverage** - Claude Code, Cursor, and Windsurf all tracked
- **Unified attribution** - Same provenance format across all editors
- **Enterprise readiness** - Full AI tool visibility before compliance features
- **Flexible adoption** - Teams can use any combination of supported editors

### Technical Components
| Component | Complexity | Priority |
|-----------|------------|----------|
| Windsurf hook integration | Low | High |
| AgentType enum extension | Low | High |
| Hook handler updates | Low | High |
| Documentation | Low | Medium |

### Architecture Notes
- Reuses existing hook infrastructure from Phase 1 (Claude Code) and Phase 2 (Cursor)
- Same `[aiki]...[/aiki]` format (~120 bytes per change)
- No new dependencies or architectural changes
- Windsurf hooks follow same pattern as Claude Code PostToolUse

### Success Criteria
- ✅ Windsurf hook integration works reliably
- ✅ 100% attribution accuracy for Windsurf edits
- ✅ All three editors (Claude Code, Cursor, Windsurf) tracked simultaneously
- ✅ `aiki authors` shows all three editor types
- ✅ Git commits include co-authors for all three editors
- ✅ All tests pass with Windsurf support

---

## Phase 11: Enterprise Compliance

### Problem
Enterprise organizations have regulatory requirements for code changes (SOX, PCI-DSS, ISO 27001, etc.). Current AI tools lack:
- Audit trails for all code changes
- Mandatory human review for sensitive code paths
- Custom policies per codebase
- Compliance demonstration for auditors

### Solution
Enterprise governance layer with path-based policies, mandatory review gates, and complete audit trails. Leverages provenance from Phase 1 for immutable audit trails.

### What We Build
- **Path-based policy engine** - Different rules per code path
- **Mandatory review gates** - Enforce human approvals for sensitive paths
- **Custom review models** - Company-specific standards and policies
- **Compliance reporting** - SOX, PCI-DSS, ISO 27001 reports
- **Immutable audit trails** - Complete provenance via JJ commit descriptions
- **Multi-level approval workflows** - 2+ approvers for high-risk changes

### Value Delivered
- Enterprise governance for AI development
- Demonstrable compliance for auditors
- Custom policies per team/project
- Complete audit trails with full provenance (immutable JJ commit history with `[aiki]` metadata)
- Risk-based review workflows
- Regulatory confidence (SOX, PCI-DSS, ISO 27001)

**Architecture Note:** JJ's immutable commit graph provides tamper-proof audit trails. All provenance data in `[aiki]` blocks is part of commit history, making it impossible to retroactively alter attributions.

---

## Phase 12: Native Agent Integration

### Problem
AI agents want deeper collaboration than passive observation. Current approach (Phases 1-10) observes agents post-facto. Agents can't:
- Check for conflicts before starting work
- Get incremental feedback during execution
- Capture and verify intent upfront
- Participate actively in coordination

### Solution
Agent SDK for real-time feedback, intent capture, and active participation in coordination.

### What We Build
- **Aiki SDK** - Libraries for agent frameworks (Python, TypeScript, Rust)
- **Real-time feedback API** - Agents get review feedback during execution
- **Intent capture and verification** - Validate goals before starting
- **Pre-work conflict awareness** - Check for conflicts before editing
- **Agent trust scoring** - Track quality over time via JJ commit history queries
- **Continuous learning** - Agents query their own history to improve (via JJ revsets)

### Value Delivered
- Agents get feedback **during execution** (not after)
- Intent verification upfront
- Conflict awareness before starting work
- Higher quality through real-time guidance
- Agent self-improvement via history access (query `[aiki]` metadata in JJ)
- Trust scoring informs agent behavior (analyze past changes via JJ revsets)

### Important Note
**Phases 1-10 deliver full value WITHOUT vendor cooperation.** Phase 11 is an optional enhancement, not a requirement for success.

---

## Phase Dependencies

```
Phase 0 (CLI/JJ) ✅
    ↓
Phase 1 (Claude Code Provenance) ✅ ← Foundation complete, SQLite-free
    ↓
Phase 2 (Cursor Support) 🔜 ← Extends Phase 1 architecture
    ↓
Phase 3 (Hook Management CLI) ← Unified hook management + diagnostics
    ↓
Phase 4 (Cryptographic Signing) ← Tamper-proof attribution
    ↓
Phase 5 (Autonomous Review) ← Tests enabled by Phase 1
    ↓
Phase 6 (Multi-Agent: Fallback Detection)
    ↓
Phase 7 (Local Multi-Agent Coordination) ← Uses Phase 1+2+6 provenance
    ↓
Phase 8 (PR Review)
    ↓
Phase 9 (Shared JJ Brain) ← Team provenance via JJ commit descriptions
    ↓
Phase 10 (Windsurf Support) ← Additional editor before enterprise
    ↓
Phase 11 (Enterprise Compliance) ← Immutable audit trails via JJ + Phase 4 signing
    ↓
Phase 12 (Agent SDK) ← Trust scoring via JJ revsets
```

**Key Insights:** 
- Phase 1 (Provenance) provides the SQLite-free foundation (~120 bytes per change in JJ commit descriptions)
- Phase 2 (Cursor) and Phase 10 (Windsurf) extend to additional editors using same architecture
- Phase 3 (Hook Management) provides unified interface for all hook types with diagnostics
- Phase 4 (Cryptographic Signing) adds tamper-proof verification layer for enterprise compliance
- All subsequent phases query provenance via JJ revsets (no database needed)
- JJ's immutable commit graph + cryptographic signatures provide audit-ready trails for compliance
