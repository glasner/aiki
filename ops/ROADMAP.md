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

**Status:** ✅ COMPLETE - SQLite-free architecture validated

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
- Phase 2: Multi-editor support (Cursor, Windsurf)

---

## Phase 2: Multi-Editor Support (Cursor, Windsurf)

**Status:** 🔜 NEXT - Extend proven architecture to additional editors

**Dependencies:** Phase 0 ✅ + Phase 1 (Provenance) ✅

**Architecture:** Phase 2 extends Phase 1's SQLite-free architecture to Cursor and Windsurf. All editors use the same `[aiki]...[/aiki]` format (~120 bytes per change) in JJ commit descriptions. No new dependencies required.

## Phase 3: Autonomous Review & Self-Correction Loop

**Status:** FUTURE - Solves the iteration problem

**Dependencies:** Phase 0 ✅ + Phase 1 (Provenance) ✅ + Phase 2 (Multi-Editor) ✅

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

## Phase 4: Multi-Agent Provenance (Fallback Detection)

**Dependencies:** Phase 3 success

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

**Next Phase:** Phase 5 (Local Multi-Agent Coordination)

---

## Phase 5: Local Multi-Agent Coordination

**Dependencies:** Phase 4 success

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

**Next Phase:** Phase 6 (PR Review for Non-Aiki Agents)

---

## Phase 6: PR Review for Non-Aiki Agents

**Dependencies:** Phase 3 (Autonomous Review) success

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

**Next Phase:** Phase 7 (Shared JJ Brain & Team Coordination)

---

## Phase 7: Shared JJ Brain & Team Coordination

**Dependencies:** Phase 5 (Local Multi-Agent Coordination) success, **JJ OSS contributions required**

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
- **Repository-wide provenance** - See all agent activity across team (query JJ commit descriptions)
- **Team activity dashboard** - Real-time view of who's working on what (via JJ revsets)

### Value Delivered
- Team-wide real-time coordination
- Pre-merge conflict awareness
- Repository-wide provenance tracking (via JJ commit descriptions with `[aiki]` metadata)
- Prevent wasted work from conflicts

**Next Phase:** Phase 8 (Enterprise Compliance)

---

## Phase 8: Enterprise Compliance

**Dependencies:** Phase 7 (Shared JJ Brain) success

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

**Next Phase:** Phase 9 (Native Agent Integration)

---

## Phase 9: Native Agent Integration

**Status:** ASPIRATIONAL - Requires vendor partnerships

**Dependencies:** Phases 1-8 success

### Problem
AI agents want deeper collaboration than passive observation. Current approach (Phases 1-8) observes agents post-facto. Agents can't:
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
**Phases 1-8 deliver full value WITHOUT vendor cooperation.** Phase 9 is an optional enhancement, not a requirement for success.

---

## Phase Dependencies

```
Phase 0 (CLI/JJ) ✅
    ↓
Phase 1 (Claude Code Provenance) ✅ ← Foundation complete, SQLite-free
    ↓
Phase 2 (Multi-Editor: Cursor, Windsurf) 🔜 ← Extends Phase 1 architecture
    ↓
Phase 3 (Autonomous Review) ← Tests enabled by Phase 1
    ↓
Phase 4 (Multi-Agent: Fallback Detection)
    ↓
Phase 5 (Local Multi-Agent Coordination) ← Uses Phase 1+2+4 provenance
    ↓
Phase 6 (PR Review)
    ↓
Phase 7 (Shared JJ Brain) ← Team provenance via JJ commit descriptions
    ↓
Phase 8 (Enterprise Compliance) ← Immutable audit trails via JJ
    ↓
Phase 9 (Agent SDK) ← Trust scoring via JJ revsets
```

**Key Insights:** 
- Phase 1 (Provenance) provides the SQLite-free foundation (~120 bytes per change in JJ commit descriptions)
- All subsequent phases query provenance via JJ revsets (no database needed)
- JJ's immutable commit graph provides tamper-proof audit trails for compliance
