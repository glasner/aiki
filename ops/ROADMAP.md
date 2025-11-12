# Aiki Product Roadmap

## Overview

Aiki follows a **seven-phase development strategy**, where each phase validates assumptions before proceeding to the next. The roadmap builds from foundational infrastructure (CLI, JJ, provenance) through solving individual developer pain (autonomous review) to full multi-agent team orchestration and enterprise compliance.

**Foundation:** All phases build on complete provenance tracking via Jujutsu (JJ), capturing edit-level history, agent attribution, and iteration tracking that Git cannot provide.

---

## Phase 0: Initial CLI & JJ Setup

**Status:** ✅ FOUNDATION COMPLETE - Required for all subsequent phases

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

**Next Phase:** Phase 1 (Provenance Tracking & Agent Attribution)

---

## Phase 1: Claude Code Provenance (Hook-Based)

**Status:** 🔨 NEXT - Foundation for autonomous review

**Dependencies:** Phase 0 ✅

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
- **Claude Code hook integration** - PostToolUse hooks for Edit|Write tools
- **Hook handler binary** - Lightweight process to record provenance
- **JJ operation metadata** - Periodic snapshots with aggregated provenance
- **Edit-level attribution** - Map file changes to Claude Code with 100% confidence
- **Provenance persistence** - Store attribution in SQLite + JJ operations
- **Session tracking** - Group related Claude Code edits together

### Commands Delivered
```bash
aiki init             # Install Claude Code hooks + start tracking
aiki status           # Show Claude Code activity
aiki history          # View complete provenance timeline
aiki blame <file>     # Show which lines Claude Code edited
aiki stats            # Show detection accuracy (should be 100%!)
```

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
| Component | Complexity | Priority |
|-----------|------------|----------|
| Claude Code hook configuration | Low | High |
| Hook handler binary | Low | High |
| Provenance database (SQLite) | Low | High |
| Periodic JJ snapshots | Medium | High |
| Attribution processor | Medium | High |
| CLI commands | Low | High |

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
- file_path - Which file was edited
- old_string - What was there before
- new_string - What it changed to
- session_id - Claude Code session
- tool_name - Edit or Write

**No guessing. No detection. Perfect attribution.**

### Provenance Data Model

```rust
struct ProvenanceRecord {
    agent: AgentInfo,           // Always ClaudeCode in Phase 1
    file_path: PathBuf,         // Exact file from hook
    session_id: String,         // Claude Code session
    timestamp: DateTime,        // When edit occurred
    change_summary: ChangeSummary, // old_string → new_string
    confidence: High,           // Always High (hook-based)
    detection_method: Hook,     // Always Hook
}

struct AgentInfo {
    agent_type: ClaudeCode,     // Only ClaudeCode in Phase 1
    version: Option<String>,    // Claude Code version
    confidence: High,           // 100% confidence
}
```

### Success Criteria
- ✅ Hook integration works reliably with Claude Code
- ✅ 100% attribution accuracy for Claude Code edits
- ✅ Hook handler completes in <100ms (doesn't slow down Claude)
- ✅ Periodic JJ snapshots aggregate recent edits
- ✅ Line-level attribution working
- ✅ All CLI commands functional
- ✅ JJ operation log manageable (<50 ops/day)
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
- Fast: 2-3 weeks to implement vs 4-6 weeks for complex detection
- Simple: JSON config + lightweight binary
- Proven: Uses Claude Code's official hook system
- Focused: Phase 1 is Claude Code only, Phase 3 adds other agents

**Next Phase:** Phase 2 (Autonomous Review & Self-Correction Loop)

---

## Phase 2: Autonomous Review & Self-Correction Loop

**Status:** THE WEDGE - Solves the burning problem

**Dependencies:** Phase 0 ✅ + Phase 1 (Provenance) ✅

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

**Next Phase:** Phase 3 (Local Multi-Agent Coordination)

---

## Phase 3: Multi-Agent Provenance (Fallback Detection)

**Dependencies:** Phase 2 success

### Problem
Developers use agents beyond Claude Code (Cursor, Copilot, custom tools, or manual edits), but Phase 1 only tracks Claude Code. Without provenance for these agents:
- Can't attribute bugs to Cursor/Copilot
- Can't compare agent quality across tools
- Can't track human vs AI edits
- Incomplete provenance picture

### Solution
Add fallback provenance detection for non-Claude Code agents using file watching + simplified process detection. Achieve 70-80% accuracy for agents without native hooks.

**Key Insight:** This is optional—Phase 1 + 2 provide full value for Claude Code users. Phase 3 extends to multi-agent scenarios.

### What We Build
- **File watcher** - Detect file changes via FSEvents (macOS)
- **Simplified 3-layer detection** - lsof, active process heuristic, unknown fallback
- **Multi-agent attribution** - Track Claude Code (100%) + others (70-80%)
- **Unified provenance** - Single database for all agents
- **Confidence indicators** - Show hook-based vs fallback detection

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
- ✅ Claude Code still 100% accurate (hook-based)
- ✅ Other agents 70-80% accurate (fallback detection)
- ✅ Overall 85%+ attribution coverage
- ✅ Confidence levels clearly indicated
- ✅ Works on macOS (Linux/Windows later)

### Technical Notes
- File watching activates only for non-Claude Code edits
- Much simpler than original multi-layer approach
- Graceful degradation from 100% (hooks) to 70-80% (lsof)
- Optional phase - full value without it for Claude Code users

**Next Phase:** Phase 4 (Local Multi-Agent Coordination)

---

## Phase 4: Local Multi-Agent Coordination

**Dependencies:** Phase 3 success

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
Sequential overwrite detection, auto-merge, and quarantine functionality for local multi-agent conflicts. Leverages provenance from Phase 1 + 3 to track which agent made which change.

### What We Build
- **Multi-agent detection** - Track concurrent local agent activity (uses Phase 1 + 3)
- **Sequential overwrite detection** - Identify when agents edit same files/lines
- **Complete timeline** - Show all agent activity in chronological order
- **Auto-merge compatible changes** - Merge non-conflicting edits automatically on rebase
- **Quarantine conflicts** - Push clean code, defer conflict resolution

### Value Delivered
- Eliminate local agent conflicts
- Smart rebase on remote changes
- Quarantine functionality (push clean code, resolve conflicts later)
- Local multi-agent provenance tracking (via Phase 1 + 3)

**Next Phase:** Phase 5 (PR Review for Non-Aiki Agents)

---

## Phase 4: PR Review for Non-Aiki Agents

**Dependencies:** Phase 3 success

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

**Next Phase:** Phase 5 (Shared JJ Brain & Team Coordination)

---

## Phase 5: Shared JJ Brain & Team Coordination

**Dependencies:** Phase 4 success, **JJ OSS contributions required**

### Problem
Even with local coordination (Phase 3) and PR review (Phase 4), developers with Aiki work independently. No visibility into what other developers' agents are working on until push/merge. Conflicts discovered late, resulting in wasted work.

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
- **Repository-wide provenance** - See all agent activity across team (Phase 1)
- **Team activity dashboard** - Real-time view of who's working on what

### Value Delivered
- Team-wide real-time coordination
- Pre-merge conflict awareness
- Repository-wide provenance tracking (Phase 1)
- Prevent wasted work from conflicts

**Next Phase:** Phase 6 (Enterprise Compliance)

---

## Phase 6: Enterprise Compliance

**Dependencies:** Phase 5 success

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
- **Immutable audit trails** - Complete provenance with tamper-proof logging (Phase 1)
- **Multi-level approval workflows** - 2+ approvers for high-risk changes

### Value Delivered
- Enterprise governance for AI development
- Demonstrable compliance for auditors
- Custom policies per team/project
- Complete audit trails with full provenance (Phase 1)
- Risk-based review workflows
- Regulatory confidence (SOX, PCI-DSS, ISO 27001)

**Next Phase:** Phase 7 (Native Agent Integration)

---

## Phase 7: Native Agent Integration

**Status:** ASPIRATIONAL - Requires vendor partnerships

**Dependencies:** Phases 1-6 success

### Problem
AI agents want deeper collaboration than passive observation. Current approach (Phases 1-6) observes agents post-facto. Agents can't:
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
- **Agent trust scoring** - Track quality over time, provide scores to agents (via Phase 1)
- **Continuous learning** - Agents query their own history to improve (via Phase 1)

### Value Delivered
- Agents get feedback **during execution** (not after)
- Intent verification upfront
- Conflict awareness before starting work
- Higher quality through real-time guidance
- Agent self-improvement via history access (Phase 1)
- Trust scoring informs agent behavior (Phase 1)

### Important Note
**Phases 1-6 deliver full value WITHOUT vendor cooperation.** Phase 7 is an optional enhancement, not a requirement for success.

---

## Phase Dependencies

```
Phase 0 (CLI/JJ) ✅
    ↓
Phase 1 (Provenance) 🔨 ← Foundation for testing
    ↓
Phase 2 (Autonomous Review) ← Tests enabled by Phase 1
    ↓
Phase 3 (Multi-Agent Coordination) ← Uses Phase 1 provenance
    ↓
Phase 4 (PR Review)
    ↓
Phase 5 (Shared JJ Brain) ← Team provenance via Phase 1
    ↓
Phase 6 (Enterprise Compliance) ← Audit trails via Phase 1
    ↓
Phase 7 (Agent SDK) ← Trust scoring via Phase 1
```

**Key Insight:** Phase 1 (Provenance) is the foundation that enables testing, attribution, and audit trails for all subsequent phases.
