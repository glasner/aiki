10
# Aiki Product Roadmap

## Overview

Aiki follows a **nineteen-phase development strategy**, where each phase validates assumptions before proceeding to the next. The roadmap builds from foundational infrastructure (CLI, JJ, provenance) through multi-editor support, hook management, cryptographic verification, user edit detection, solving individual developer pain (autonomous review), to full multi-agent team orchestration and enterprise compliance.

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
- **Testing foundation** - Enables comprehensive testing of Phase 7 (Autonomous Review Flow)
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

## Phase 3: CLI Streamlining & Health Diagnostics

**Status:** ✅ Complete

### Problem
Users need a simple way to verify Aiki is configured correctly and diagnose issues when hooks aren't working. The CLI had redundant commands (`hooks status`, `hooks doctor`, `hooks list`) that could be streamlined for better UX.

**Without comprehensive diagnostics:**
- No single command to check "is Aiki working?"
- Difficult to diagnose configuration issues
- Unclear which hooks are installed
- No guidance on fixing broken setups

### Solution
Streamline CLI to essential commands and provide comprehensive health checking via `aiki doctor`. Remove redundant commands and consolidate diagnostics into one top-level command.

**Key Innovation:** Single `aiki doctor` command checks repository setup, global hooks, and local configuration - providing actionable guidance for any issues.

### What We Built
- **`aiki doctor` command** - Comprehensive health check at main CLI level
- **Repository checks** - JJ workspace, Git repo, Aiki directory
- **Global hooks verification** - Git, Claude Code, Cursor installations
- **Local configuration validation** - Git core.hooksPath setup
- **Streamlined CLI** - Removed `hooks status`, `hooks doctor`, `hooks list`
- **Clear actionable output** - ✓/✗/⚠ symbols with fix suggestions

### Commands Delivered
```bash
aiki doctor         # Check Aiki health (repo + hooks + config)
aiki doctor --fix   # Automatically repair detected issues (foundation for future fixes)
aiki hooks install  # Install all global hooks (simplified)
```

**Removed Commands:**
- ❌ `aiki hooks status` - Functionality integrated into `doctor`
- ❌ `aiki hooks doctor` - Moved to top-level `aiki doctor`
- ❌ `aiki hooks list` - Unnecessary (only 2 editors, obvious from install)

### Example Output
```bash
$ aiki doctor

Checking Aiki health...

Repository:
  ✓ JJ workspace initialized
  ✓ Git repository detected
  ✓ Aiki directory exists

Global Hooks:
  ✓ Git hooks installed (~/.aiki/githooks/)
  ✓ Claude Code hooks configured
  ✓ Cursor hooks configured

Local Configuration:
  ✓ Git core.hooksPath configured

✓ All checks passed! Aiki is healthy.

$ aiki doctor  # With issues

Checking Aiki health...

Repository:
  ✗ JJ workspace not found
    → Run: aiki init
  ✓ Git repository detected
  ✗ Aiki directory missing
    → Run: aiki init

Global Hooks:
  ✗ Git hooks missing
    → Run: aiki hooks install
  ⚠ Claude Code hooks not configured
    → Run: aiki hooks install
  ⚠ Cursor hooks not configured
    → Run: aiki hooks install

Local Configuration:
  ✗ Git core.hooksPath not set
    → Run: aiki init

Found 4 issue(s).

Run 'aiki doctor --fix' to automatically fix issues.
```

### Diagnostics Performed

**Repository Health:**
- JJ workspace exists (`.jj/` directory)
- Git repository present (`.git/` directory)
- Aiki directory exists (`.aiki/` directory)

**Global Hooks:**
- Git hooks installed (`~/.aiki/githooks/`)
- Claude Code hooks configured (`~/.claude/settings.json`)
- Cursor hooks configured (`~/.cursor/hooks.json`)

**Local Configuration:**
- Git `core.hooksPath` points to Aiki hooks
- Previous hooks preserved (if any existed)

### Value Delivered
- **Single health check** - One command to verify entire setup
- **Clear diagnostics** - Visual indicators (✓/✗/⚠) for each component
- **Actionable guidance** - Specific commands to fix each issue
- **Simpler CLI** - 5 user-facing commands instead of 8
- **Better UX** - `doctor` is more discoverable at main level
- **Foundation for future** - `--fix` flag ready for automatic repairs

### Technical Components
| Component | Complexity | Status |
|-----------|------------|--------|
| Doctor command infrastructure | Low | ✅ Complete |
| Repository health checks | Low | ✅ Complete |
| Global hooks detection | Low | ✅ Complete |
| Local config validation | Low | ✅ Complete |
| User-friendly output formatting | Low | ✅ Complete |
| Automatic repair logic | Medium | 🔜 Future (foundation in place) |

### Success Criteria
- ✅ `aiki doctor` checks all critical components
- ✅ Clear visual output with ✓/✗/⚠ symbols
- ✅ Actionable fix suggestions for each issue
- ✅ CLI streamlined to essential commands
- ✅ All 90 tests passing
- ✅ No regressions in existing functionality
- ✅ Works on macOS, Linux, Windows

### Why This Matters
**Before Phase 3:**
- 8 user-facing commands (confusing)
- `hooks status` only checked hooks (not repo/config)
- `hooks doctor` buried under subcommand
- `hooks list` redundant (only 2 editors)

**After Phase 3:**
- 5 streamlined commands (clearer)
- `doctor` checks everything (repo + hooks + config)
- Top-level command (more discoverable)
- CLI focused on essential workflows

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

## Phase 5: Internal Flow Engine

### Problem
Aiki's core functionality (provenance embedding, session tracking, JJ integration) is hardcoded in Rust, making it difficult to test, modify, and extend. We need a declarative system to define Aiki's behavior and enable Phase 7 (Autonomous Review Flow).

**With provenance (Phase 1-4), we can now:**
- Trigger workflows on specific agent events (PostFileChange, PreCommit, Start, Stop)
- Access rich event metadata (agent type, file paths, change IDs)
- Build automation that reacts to AI coding events

### Solution
Build a minimal flow engine focused on **internal flows** only:
1. **System flows** - Refactor existing Rust code into mandatory flows (provenance, session tracking)
2. **Default flows** - Ship built-in flows users can customize (autonomous review)
3. **Single flow file** - Users edit `.aiki/flow.yaml` for customization

**Scope:** Built-in flows only. No user-defined flows, no external flows, no registries.

### What We Build

**Flow Types:**
- **System flows** - Mandatory, built into binary, power Aiki core (provenance, JJ integration)
- **Default flows** - Optional, built into binary, user can customize (autonomous review)

**Flow Engine:**
- Event system (PostFileChange, PreCommit, Start, Stop)
- YAML parser and executor
- Sequential and parallel execution
- Conditionals (`if/then/else`)
- Step references and aliases

**User Experience:**
```yaml
# .aiki/flow.yaml (single file, edited by user)
name: My Workflow
version: 1

PreCommit:
  - flow: aiki/autonomous-review   # Include built-in default flow
  - shell: pytest --fast             # Add custom inline steps
    on_failure: block
```

### Refactoring Existing Functionality

| Current Rust Code | Becomes System Flow |
|-------------------|---------------------|
| Provenance embedding (hooks.rs) | `aiki/system/provenance` |
| Session tracking | `aiki/system/session-tracking` |
| JJ integration | `aiki/system/jj-integration` |

**Example refactor:**
```yaml
# aiki/system/provenance (built into Aiki binary)
PostFileChange:
  - jj:
      - describe
      - --no-edit
      - -m
      - |
        [aiki]
        agent=$agent
        session=$session_id
        tool=$event.tool_name
        [/aiki]
    on_failure: block
```

### Value Delivered
- **Cleaner architecture** - Aiki's behavior in declarative flows, not scattered Rust
- **Dog-fooding** - Aiki uses its own flow system internally
- **Enables Phase 6** - Autonomous review can be built as a default flow
- **Easier testing** - Test flows, not Rust code
- **User customization** - Inline steps in `.aiki/flow.yaml`

### Technical Components
| Component | Complexity |
|-----------|------------|
| Event system architecture | Medium |
| YAML parser and validator | Low |
| Flow execution engine | Medium |
| Conditional execution | Medium |
| Parallel execution (DAG) | Medium |
| System flow refactoring | Medium |

### Success Criteria
- ✅ System flows power Aiki core (provenance, session tracking)
- ✅ Default flows ship with Aiki (autonomous review)
- ✅ Users can add inline steps to `.aiki/flow.yaml`
- ✅ Flows execute on events (PostFileChange, PreCommit, Start, Stop)
- ✅ Phase 6 can build on this foundation

### Why This Enables Future Phases
- **Phase 6**: Autonomous review built as default flow
- **Phase 7**: Complete event system for all Git and agent hooks
- **Phase 8**: Users write their own flows in `.aiki/flows/`
- **Phase 9**: External flow ecosystem with bundled binaries
- **Rapid iteration**: Change workflows without Aiki releases

---

## Phase 5.X: Event System Hardening with Automatic Response Translation

### Problem
Current event system has several gaps:
- Inconsistent naming for agent events (before/after vs pre/post)
- Handlers return generic `Result<()>` with no rich feedback capability
- Both Claude Code and Cursor support JSON responses but in different formats
- Flows/handlers shouldn't need to know about editor-specific response formats
- Need automatic translation layer between generic responses and editor-specific JSON

### Solution
Standardize agent event naming, add generic response system, implement automatic translation layer between generic responses and editor-specific JSON formats.

**Key Design Principle:** Handlers and flows return generic, editor-agnostic responses. The event dispatcher automatically translates them to editor-specific JSON formats.

### What We Build

**1. Event Naming Standardization**
- Rename `Start` → `SessionStart` (agent event only)
- Keep `PostFileChange` as-is (already uses Post prefix)
- Keep `PrepareCommitMessage` unchanged (Git hook name)
- Update flow field: `start` → `session_start`

**2. Generic Response System** (`HookResponse` in `handlers.rs`)
- `success()` - Simple success response
- `success_with_message()` - Success with user-visible message
- `success_with_metadata()` - Success with key-value metadata
- `failure()` - Failure with user and optional agent messages
- Builder methods: `with_metadata()`, `with_agent_message()`

**3. Automatic Translation Layer** (in `commands/event.rs`)
- Detect editor type (Claude Code, Cursor, Unknown)
- Translate generic `HookResponse` to editor-specific JSON
- Output JSON to stdout, exit with appropriate code

### Architecture Flow
```
Handler → HookResponse (generic) 
  ↓
Translation Layer (detect editor type)
  ↓
Editor-Specific JSON + Exit Code
```

### Response Translation Examples

| Generic Response | Claude Code JSON | Cursor JSON |
|-----------------|------------------|-------------|
| `success()` | *(no JSON, exit 0)* | *(no JSON, exit 0)* |
| `success_with_message("✅ Done")` | `{"userMessage": "✅ Done"}` | `{"user_message": "✅ Done"}` |
| `failure("Error", Some("Context"))` | `{"userMessage": "Error", "agentMessage": "Context"}` | `{"user_message": "Error", "agent_message": "Context"}` |
| `success().with_metadata([("k","v")])` | `{"metadata": [["k","v"]]}` | `{"metadata": {"k":"v"}}` |

### Value Delivered
- **Separation of concerns** - Handlers focus on logic, not output format
- **Easy to add editors** - Just add new translation function
- **Testing** - Test handlers with generic responses, test translations separately
- **Maintainability** - Change editor format without touching handlers
- **Future-proof** - Add new response fields without breaking handlers

### Technical Components
| Component | Complexity | Status |
|-----------|------------|--------|
| HookResponse struct in handlers.rs | Low | ✅ Complete |
| Event naming standardization | Low | ✅ Complete |
| Translation layer (detect + translate) | Medium | ✅ Complete |
| Handler response updates | Low | ✅ Complete |
| Event bus return type change | Low | ✅ Complete |
| Flow type updates | Low | ✅ Complete |

### Success Criteria
- ✅ Agent events use Pre/Post naming (`SessionStart`, `PreFileChange`, `PostFileChange`)
- ✅ Git hooks keep official names (`PrepareCommitMessage` unchanged)
- ✅ `HookResponse` defined in `handlers.rs`
- ✅ Handlers return generic `HookResponse` (no editor knowledge)
- ✅ Automatic translation to Claude Code JSON format
- ✅ Automatic translation to Cursor JSON format
- ✅ User messages shown in editor UI
- ✅ Agent messages provide context to AI
- ✅ Metadata properly formatted per editor
- ✅ Backward compatible (exit code fallback when no messages)
- ✅ All tests passing

### Files Modified
1. `cli/src/handlers.rs` - Added `HookResponse` struct, updated handlers
2. `cli/src/events.rs` - Renamed `Start` → `SessionStart`
3. `cli/src/commands/event.rs` - Added translation layer
4. `cli/src/event_bus.rs` - Changed return type to `Result<HookResponse>`
5. `cli/src/flows/types.rs` - Renamed `start` → `session_start`
6. `cli/src/flows/core/flow.yaml` - Updated `Start:` → `SessionStart:`

**Status:** ✅ Complete

---

## Phase 6: ACP Support via Bidirectional Proxy

### Problem
ACP (Agent Client Protocol) is becoming the standard for IDE-agent communication. IDEs like Zed, Neovim, and (future) VSCode/JetBrains use ACP to communicate with AI coding agents. Without ACP support, Aiki can't:
- Track provenance for agents running in ACP-compatible IDEs
- Observe tool calls from agents in real-time
- Inject context or modify prompts for autonomous review
- Support the growing ecosystem of ACP-compatible editors

**Current limitations:**
- Only hook-based provenance (Claude Code standalone, Cursor)
- Can't track agents running inside Zed, Neovim, or other ACP IDEs
- Missing opportunity to intercept and enhance agent communication
- No visibility into ACP-based agent workflows

### Solution
Build a bidirectional ACP proxy (`aiki acp`) that sits between ACP-compatible IDEs and AI agents. The proxy:
- Auto-detects the client (IDE) from `InitializeRequest.clientInfo.name`
- Validates agent type against `AgentType` enum
- Observes agent → IDE messages for provenance tracking
- Provides foundation for future prompt modification and context injection

**Key Insight:** ACP's `InitializeRequest` provides client identification, so we can automatically detect which IDE is being used without manual configuration.

### What We Build
- **`aiki acp` command** - Transparent bidirectional proxy for ACP communication
- **Client auto-detection** - Extract IDE name from `InitializeRequest.clientInfo`
- **Agent type validation** - Ensure agent matches `AgentType` enum
- **Message observation** - Capture tool calls for provenance tracking
- **IDE configuration** - Auto-configure Zed/Neovim via `aiki hooks install`
- **Executable derivation** - Default binary path from agent type (customizable via `--bin`)

### Commands Delivered
```bash
aiki acp <agent-type> [--bin <path>] [-- <agent-args>...]

# Examples (executable derived from agent-type):
aiki acp claude-code
aiki acp gemini
aiki acp cursor

# Examples (custom executable):
aiki acp claude-code --bin /custom/path/to/claude-code
aiki acp gemini --bin gemini-cli-beta

# Examples (passing args to agent):
aiki acp gemini -- --verbose --model gemini-2.0
aiki acp cursor --bin /usr/local/bin/cursor -- --debug
```

### Example Output
```bash
$ aiki hooks install
✓ Git hooks installed
✓ Claude Code hooks configured
✓ Cursor hooks configured
✓ ACP proxy configured for supported IDEs

IDE Configuration:
  Updated ~/.config/zed/settings.json:
    - claude: Uses 'aiki acp claude-code'
    - gemini: Uses 'aiki acp gemini'
    - cursor: Uses 'aiki acp cursor'

Restart your IDE to activate.

$ aiki doctor
ACP Proxy:
  ✓ 'aiki' command found in PATH
  ✓ Zed: Claude configured to use 'aiki acp claude-code'
  ✓ Zed: Gemini configured to use 'aiki acp gemini'
  ✓ Zed: Cursor configured to use 'aiki acp cursor'
```

### Value Delivered
- **Multi-IDE support** - Works with Zed, Neovim, and future ACP-compatible IDEs
- **Auto-detection** - No manual IDE configuration needed
- **Transparent proxy** - Zero latency overhead, invisible to agents
- **Provenance foundation** - Enables tracking for ACP-based agents
- **Future-proof** - Foundation for prompt modification and autonomous review in IDEs
- **Simple configuration** - `aiki hooks install` sets up everything

### Architecture
```
IDE (Zed/Neovim/etc) ←→ aiki acp ←→ Agent (claude-code/gemini/etc)
                            ↓              ↓
                       Modify prompts  Observe tool_call
                       Inject context  Record provenance
                       Auto-detect IDE from InitializeRequest
```

### Technical Components
| Component | Complexity | Priority |
|-----------|------------|----------|
| ACP protocol types (JSON-RPC, InitializeRequest) | Low | High |
| Bidirectional stdio proxy | Medium | High |
| Client auto-detection | Low | High |
| Agent type validation | Low | High |
| Message observation (tool calls) | Medium | High |
| IDE auto-configuration (Zed) | Medium | Medium |
| Doctor validation | Low | Medium |
| Provenance integration | Medium | Future |

### Implementation Notes

**ACP Client Detection:**
```rust
// Extract from InitializeRequest
if let Ok(init_req) = serde_json::from_value::<InitializeRequest>(msg.params.clone()) {
    if let Some(client_info) = init_req.client_info {
        client_name = Some(client_info.name);  // "zed", "neovim", etc.
    }
}
```

**Agent Executable Derivation:**
```rust
fn derive_executable(agent_type: &str) -> String {
    match agent_type {
        "gemini" => "gemini-cli".to_string(),
        other => other.to_string(),  // claude-code → claude-code
    }
}
```

**Zed Configuration:**
```json
{
  "agent_servers": {
    "claude": {
      "env": {"CLAUDE_CODE_EXECUTABLE": "aiki"},
      "args": ["acp", "claude-code"]
    }
  }
}
```

### Success Criteria
- ✅ `aiki acp <agent-type>` command exists and works
- ✅ Validates `agent-type` against `AgentType` enum (errors on invalid types)
- ✅ Bidirectional message forwarding (transparent, zero overhead)
- ✅ Auto-detects client (IDE) from `InitializeRequest.clientInfo.name`
- ✅ Detects tool_call notifications from agents
- ✅ Records provenance with `client_name` (IDE) and `agent_type` (from enum)
- ✅ `aiki hooks install` configures IDEs automatically
- ✅ `aiki doctor` validates ACP setup
- ✅ Works with all ACP-compatible IDEs
- ✅ Works with all agents in `AgentType` enum

### Why This Enables Future Phases
- **Autonomous review in IDEs** - Can inject review feedback into prompts
- **Context enhancement** - Pass previous session data to agents
- **Policy enforcement** - Block or modify dangerous operations
- **IDE-agnostic workflows** - Same flows work across Zed, Neovim, VSCode, etc.
- **Broader ecosystem** - Support any ACP-compatible tool

### Timeline
- ACP protocol types: 1 day
- `aiki acp` command + bidirectional proxy: 2-3 days
- Provenance integration (client_name, agent_type): 1 day
- IDE auto-configuration: 1 day
- Doctor validation: 1 day
- Testing: 2 days

**Total: ~2 weeks**

**Detailed Plan:** See `ops/phase-6.md`

---

## Phase 7: User Edit Detection & Separation

### Problem

When an AI agent session starts or during AI operations, users may manually edit files. Currently, these user edits can be incorrectly attributed to the AI agent, leading to false attribution, provenance corruption, and trust issues.

**Three problematic scenarios:**
1. **Between SessionStart and first PostFileChange**: User edits between session init and first AI edit
2. **Concurrent different files**: User and AI edit different files simultaneously  
3. **Concurrent same file**: User and AI edit the same file (hardest case)

### Solution

Implement **tiered detection** based on available edit information from each integration:

| Integration | Edit Details Available |
|-------------|----------------------|
| **ACP** | ✅ ToolCallContent with old/new text diffs |
| **Claude Code** | ✅ old_string/new_string |
| **Cursor** | ✅ edits[] array with old_string/new_string |

**Detection strategy:**
1. **Capture edit details** in AikiPostChangeEvent (all integrations provide this!)
2. **Compare expected vs actual** diffs in PostFileChange flow
3. **Separate cleanly** when user edited different files using `jj restore`
4. **Warn** when user edited same file (requires manual `jj split --interactive`)

### What We Build

**Event structure enhancement:**
```rust
pub struct AikiPostChangeEvent {
    // ... existing fields ...
    pub edit_details: Option<Vec<EditDetail>>,
}

pub struct EditDetail {
    pub file_path: String,
    pub old_text: String,
    pub new_text: String,
    pub line_range: Option<(usize, usize)>,
}
```

**Flow functions:**
- `self.check_for_user_edits` - Compare expected AI edits with actual working copy
- `self.separate_user_edits` - Use `jj restore` to split user files into separate change

**Enhanced PostFileChange flow:**
```yaml
PostFileChange:
  # Detect user edits
  - let: user_edit_check = self.check_for_user_edits
  
  # Separate different-file edits automatically
  - if: $user_edit_check.has_different_files == true
    then:
      - let: result = self.separate_user_edits
  
  # Warn about same-file edits
  - if: $user_edit_check.has_same_file_edits == true
    then:
      - log: "⚠️  User edited same files as AI - run 'jj split --interactive'"
  
  # Record AI provenance
  - let: metadata = self.build_metadata
  - jj: metaedit --message "$metadata.message"
  - jj: new
```

### Value Delivered

1. **Accurate attribution**: User edits are never falsely attributed to AI
2. **Automatic separation**: Different-file concurrent edits are split into distinct changes
3. **Clear warnings**: Same-file conflicts are detected and user is guided to manual resolution
4. **Trust in provenance**: `aiki blame` shows correct authorship
5. **Works everywhere**: Full support across ACP, Claude Code, and Cursor

### Technical Components

**New modules:**
- `cli/src/flows/core/check_user_edits.rs` - Detection logic
- `cli/src/flows/core/separate_user_edits.rs` - Separation logic

**Modified files:**
- `cli/src/events.rs` - Add `edit_details` field and `EditDetail` struct
- `cli/src/commands/acp.rs` - Extract edit details from ToolCallContent
- `cli/src/vendors/claude_code.rs` - Extract from old_string/new_string
- `cli/src/vendors/cursor.rs` - Extract from edits[] array
- `cli/src/flows/core/flow.yaml` - Add detection and separation steps

### Success Criteria

1. ✅ Different-file user edits are automatically separated into distinct changes
2. ✅ Same-file user edits are detected and warned about
3. ✅ AI provenance is only recorded for AI-edited files
4. ✅ Works across all three integrations (ACP, Claude Code, Cursor)
5. ✅ All tests pass with no regressions

### Limitations

1. **Same-file concurrent edits**: Cannot auto-separate when user and AI edit same file simultaneously
2. **Heuristic detection**: Uses line counts and content matching (may have false positives)
3. **Performance**: Adds `jj diff` calls (minor overhead)

### Timeline

**Estimated: 9-13 hours**
- Event structure: 1-2 hours
- Detection logic: 2-3 hours  
- Separation logic: 2-3 hours
- Flow integration: 1 hour
- Testing: 2-3 hours
- Documentation: 1 hour

**Detailed Plan:** See `ops/phase-7.md`

---

## Phase 8: The Aiki Way (aiki/default Flow)

### Problem

We've built powerful primitives (flows, events, provenance), but users need event types to build intelligent automation. The flow system (Phase 5) needs PrePrompt and PostResponse events to enable context injection, validation, and task management.

**What's missing:**
1. **PrePrompt event** - No way to inject context before agent sees prompt
2. **PostResponse event** - No way to validate after agent responds
3. **Task system** - No structured way to track work across sessions
4. **Flow composition** - No way to reuse flows via `includes:`

### Solution

Extend the flow system with core event types and capabilities that enable intelligent AI workflows. This phase delivers the foundational primitives that unlock all subsequent phases (9-12).

**The three core extensions:**
1. **PrePrompt event** - Inject skills, architecture docs, and task context
2. **PostResponse event & Task System** - Validate builds, create tasks from errors, auto-close completed work
3. **Flow composition** - Reuse flows via `includes:` directive

### What We Build

**Implementation in two milestones:**

**Milestone 1: Core Extensions (4 weeks)**
- MessageBuilder shared syntax (week 1)
- PrePrompt event type (week 1)
- PostResponse event type & Task System (weeks 2-3)
  - Event-sourced task storage on JJ `aiki/tasks` branch
  - CLI commands: `aiki task ready/create/start/close`
  - Auto-closing via PostToolUse
  - Attempt-based stuck detection
- Flow composition via `includes:` directive (week 2)

**Milestone 2: Multi-Stage Pipeline (1-2 weeks)**
- Session state tracking (edited files, affected repos)
- PostResponse hook for automatic builds
- Error parsing (TypeScript, Rust, ESLint)
- Pattern detection (missing error handling, etc.)
- Gentle reminder system (non-blocking suggestions)

**Key architectural decisions:**
- Task system uses event sourcing (immutable event log on JJ branch)
- Content-addressed task IDs prevent duplicate task creation
- Tasks auto-close when PostToolUse detects fixes
- Attempt-based stuck detection (3+ failed attempts)

### Commands Delivered

```bash
# Task management
aiki task ready                     # List ready tasks
aiki task create                    # Create new task
aiki task start <id>                # Start working on task
aiki task close <id>                # Close completed task

# Flow installation
aiki flows install aiki/default     # Install this flow
aiki flows show aiki/default        # Show flow details
```

### Value Delivered

**For developers:**
- **Context injection** - PrePrompt injects skills and architecture docs automatically
- **Structured task tracking** - Tasks auto-created from build failures, tracked across sessions
- **Agent-driven workflow** - AI queries and closes tasks automatically via CLI
- **Zero errors left behind** - Milestone 2 builds on this to catch problems immediately

**For Aiki:**
- **Unlocks all subsequent phases** - Phases 9-12 all depend on PrePrompt/PostResponse/Tasks
- **Proves the flow system** - Demonstrates Phase 5's power with real event types
- **Dogfood opportunity** - Use task system to build Aiki itself
- **Foundation for automation** - Core primitives for intelligent AI workflows

### Technical Components

| Component | Complexity | Priority | Timeline |
|-----------|------------|----------|----------|
| MessageBuilder shared syntax | Low | High | Week 1 (Days 1-3) |
| PrePrompt event | Medium | High | Week 1 (Days 4-5) |
| PostResponse event | Medium | High | Week 2 (Day 1) |
| Task system (event-sourced) | High | High | Week 2-3 |
| Flow composition (`includes:`) | Medium | High | Week 2 |
| Multi-stage pipeline (Milestone 2) | Medium | High | Week 5 |

### Success Criteria

**Milestone 1:**
- ✅ MessageBuilder parses short form (`action: "string"`) and explicit form (`action: { prepend: [...], append: [...] }`)
- ✅ PrePrompt event fires before agent sees prompt
- ✅ PostResponse event fires after agent responds
- ✅ Task system creates/queries/starts/closes tasks
- ✅ Tasks stored as events on JJ `aiki/tasks` branch
- ✅ CLI commands work: `aiki task ready/create/start/close`
- ✅ PostToolUse auto-closes tasks when fixes detected
- ✅ Attempt-based stuck detection works (3+ failed attempts)
- ✅ Flow composition works (`includes:` directive)
- ✅ Content-addressed task IDs prevent duplicates

**Milestone 2:**
- ✅ Session state tracking works (edited files, affected repos)
- ✅ Builds run automatically after AI responses
- ✅ Error parsing works (TypeScript, Rust, ESLint)
- ✅ Pattern detection works (missing error handling, etc.)
- ✅ All patterns dogfooded during Aiki development

### Why This Matters

**This unlocks everything.** Phase 5 gave us the flow engine. Phase 8 adds the event types and task system that make flows truly powerful. Phases 9-12 all build on PrePrompt, PostResponse, and the task system. Without this foundation, intelligent automation isn't possible.

### Timeline

**Estimated: 3-5 weeks**
- Milestone 1: 2-3 weeks
- Milestone 2: 1-2 weeks

**Before Phase 8:**
- Users have flow primitives but no guidance
- Everyone reinvents the same patterns
- Power users succeed, others struggle

**After Phase 8:**
- One command: `aiki flows install aiki/default`
- Proven patterns out of the box
- Learn by example, customize later

**Detailed Plan:** See `ops/the-aiki-way.md`

---

## Phase 9: Doc Management Action Type

### Problem

Flows need a way to create, update, and query structured documentation within the `.aiki/` directory. While Phase 8 delivers core patterns like architecture caching and task management, these features require persistent document storage that flows can interact with programmatically.

**Without doc management:**
- No way to cache architecture discoveries in flows
- Task documentation must be manual
- Session notes can't be auto-generated
- Architecture patterns can't be stored and queried

### Solution

Implement the `doc_management` action type that allows flows to create, update, append to, and query markdown documents. This enables Phase 8's architecture caching, task docs, and other documentation patterns.

**Key capabilities:**
- **Create** - Create new documents (error if exists)
- **Update** - Overwrite entire documents
- **Append** - Add content to end of documents
- **Query** - Read document content into variables

All operations are restricted to the `.aiki/` directory for security, with path traversal detection and atomic writes.

### What We Build

**Core doc_management action with four operations:**

```yaml
# Create new document
doc_management:
  operation: create
  path: .aiki/arch/structure/backend/index.md
  content: |
    # Backend Architecture
    Discovered patterns...

# Update existing document
doc_management:
  operation: update
  path: .aiki/tasks/current/status.md
  content: "Status: In Progress"

# Append to document
doc_management:
  operation: append
  path: .aiki/sessions/notes.md
  content: |
    - Completed auth implementation

# Query document into variable
- doc_management:
    operation: query
    path: .aiki/arch/structure/backend/index.md
    variable: backend_arch

- if: $backend_arch contains "OAuth2"
  then:
    prompt: "Remember we use OAuth2 for auth"
```

**Security features:**
- Path validation (must be within `.aiki/`)
- Path traversal detection (blocks `..` attempts)
- Automatic directory creation
- Atomic writes (temp file + rename)

### Commands Delivered

No new CLI commands - this is a flow action type only. Flows use it programmatically:

```yaml
PostResponse:
  # Architecture caching example
  - doc_management:
      operation: append
      path: .aiki/arch/patterns/discovered.md
      content: |
        ## Pattern: $pattern_name
        $pattern_description
```

### Value Delivered

**For Phase 8 milestones:**
- **Enables Milestone 2** - Architecture docs cached via doc_management
- **Enables Milestone 5** - Task docs created and updated automatically
- **Enables session notes** - Track work across sessions in `.aiki/sessions/`

**For flow authors:**
- **Persistent state** - Store data across flow executions
- **Queryable docs** - Load and check existing documentation
- **Safe operations** - Security built-in, no path traversal risks

### Technical Components

| Component | Complexity | Priority | Timeline |
|-----------|------------|----------|----------|
| Doc management action parser | Low | High | 1 day |
| Path validation & security | Medium | High | 1 day |
| Create/update/append operations | Low | High | 1 day |
| Query operation with variables | Low | High | 1 day |
| Atomic write implementation | Low | High | 1 day |
| Unit & integration tests | Medium | High | 2 days |

### Success Criteria

- ✅ Can create new documents from flows
- ✅ Can update existing documents
- ✅ Can append to documents
- ✅ Can query document content into variables
- ✅ Parent directories created automatically
- ✅ Path validation prevents security issues
- ✅ Path traversal attempts blocked
- ✅ Atomic writes prevent corruption
- ✅ Clear error messages for invalid operations

### Why This Enables Phase 8

Phase 8's "Aiki Way" patterns depend on doc_management:

1. **Architecture Caching (Milestone 2)** - Needs to store discovered patterns
2. **Task Documentation (Milestone 5)** - Needs to create/update task docs
3. **Session Notes** - Needs to append session progress
4. **Skills** - May need to query existing documentation

Without doc_management, these features would need separate, redundant implementations.

### Timeline

**Estimated: 1 week**
- Implementation: 3-4 days
- Testing: 2 days  
- Documentation: 1 day

**Detailed Plan:** See `ops/phase-9.md`

---

## Phase 10: Auto Architecture Documentation

### Problem

AI agents repeatedly explore the same codebases, reading 20+ files to understand patterns that haven't changed. This wastes time, tokens, and creates inconsistent mental models across sessions.

**Current pain points:**
- Agent re-discovers "we use Zod for validation" every session
- Same 15 files read to understand backend structure
- Exploration expensive (time + tokens)
- No memory of previous discoveries
- Different explanations each time

### Solution

Automatically detect when an agent explores a directory (5+ files read), summarize the discovered patterns, and cache them in a shadow directory structure. Future sessions inject cached docs via PrePrompt instead of re-exploring.

**Key capabilities:**
- **Exploration detection** - Recognize when agent is learning architecture
- **Auto-summarization** - Extract patterns from file contents
- **Shadow directory** - Store discoveries in `.aiki/arch/structure/`
- **Staleness tracking** - Regenerate when underlying files change
- **PrePrompt injection** - Load cached docs automatically

### What We Build

**1. Exploration Detection**

Monitor PostToolUse events to detect exploration patterns:

```rust
// Detect exploration: 5+ files read in same directory within session
if session.files_read_in("src/backend/") > 5 {
    trigger_summarization("src/backend/");
}
```

**2. Auto-Summarization**

Generate architecture docs from discovered patterns:

```yaml
PostResponse:
  - if: self.exploration_detected("src/backend/")
    then:
      - let: summary = self.summarize_directory("src/backend/")
      - doc_management:
          operation: create
          path: .aiki/arch/structure/backend/index.md
          content: |
            # Backend Architecture
            
            $summary
            
            Generated: $timestamp
            Source files: $files_analyzed
```

**3. Shadow Directory Structure**

Cache follows source structure:

```
src/backend/
├── controllers/
├── models/
└── services/

.aiki/arch/structure/backend/
├── index.md              # Overview
├── controllers.md        # Controllers summary
├── models.md            # Models summary
└── services.md          # Services summary
```

**4. Staleness Detection**

Track source file changes and mark cached docs stale:

```yaml
PostFileChange:
  - if: $event.file_path starts_with "src/backend/"
    then:
      - doc_management:
          operation: update
          path: .aiki/arch/structure/backend/.stale
          content: "true"
```

**5. PrePrompt Auto-Injection**

Load cached architecture docs before each prompt:

```yaml
PrePrompt:
  - let: working_dir = self.get_working_directory()
  - let: arch_doc = ".aiki/arch/structure/" + $working_dir + "/index.md"
  
  - if: file_exists($arch_doc) AND NOT is_stale($arch_doc)
    then:
      - doc_management:
          operation: query
          path: $arch_doc
          variable: arch_content
      
      - prompt:
          prepend: |
            # Cached Architecture
            
            $arch_content
```

### Commands Delivered

```bash
# Show cached architecture
aiki arch show src/components
# Output: Displays cached architecture doc for src/components

# Force regeneration
aiki arch refresh src/components
# Triggers new exploration and summarization

# Clear all cached architecture
aiki arch clear
# Removes all .aiki/arch/structure/ docs

# List all cached directories
aiki arch list
# Shows which directories have cached docs

# Check staleness
aiki arch status
# Shows which cached docs are stale
```

### Value Delivered

**For developers:**
- **10x faster context loading** - Read cached doc instead of exploring 20+ files
- **Consistent mental models** - Same architecture explanation every session
- **Token savings** - Avoid re-reading same files repeatedly
- **Automatic updates** - Docs regenerate when code changes

**For Aiki:**
- **Killer feature** - Unique value proposition vs raw AI coding
- **Dogfood opportunity** - Use for Aiki's own development
- **Composable primitive** - Works with other patterns (skills, tasks)

### Technical Components

| Component | Complexity | Priority | Timeline |
|-----------|------------|----------|----------|
| Exploration detection | Medium | High | 2 days |
| Directory summarization | High | High | 3 days |
| Shadow directory management | Low | High | 1 day |
| Staleness tracking | Medium | High | 2 days |
| PrePrompt injection | Low | High | 1 day |
| CLI commands | Low | Medium | 2 days |

### Success Criteria

- ✅ Exploration detection triggers after 5+ file reads
- ✅ Summaries accurately capture directory patterns
- ✅ Shadow directory structure mirrors source
- ✅ Stale docs regenerate when source changes
- ✅ PrePrompt injects cached docs automatically
- ✅ CLI commands work for manual management
- ✅ Token usage reduced by 50%+ for repeat exploration
- ✅ Summaries are accurate and useful

### Why This Enables Future Phases

Architecture caching is foundational for:
- **Skills Auto-Activation** - Skills reference cached architecture
- **Multi-Stage Pipeline** - Builds use architecture knowledge
- **Dev Docs** - Task docs link to architecture
- **Team sharing** - Cached docs can be committed to Git

### Timeline

**Estimated: 1-2 weeks**
- Exploration detection: 2 days
- Auto-summarization: 3 days
- Shadow directory + staleness: 3 days
- PrePrompt injection: 1 day
- CLI + testing: 2 days

**Detailed Plan:** See `ops/current/milestone-2.md` (to be created)

---

## Phase 11: Skills Auto-Activation

### Problem

AI agents forget project-specific guidelines and best practices between prompts. Developers waste time correcting the same mistakes: wrong error handling patterns, forgotten testing requirements, inconsistent code style.

**Current pain points:**
- Agent forgets "always use Zod for validation"
- Inconsistent error handling across sessions
- Testing guidelines ignored
- Code style drifts from conventions
- Manual reminders in every prompt

### Solution

Implement pattern-based skill activation that automatically injects relevant guidelines via PrePrompt based on context (keywords, files, content patterns).

**Key capabilities:**
- **Pattern matching engine** - Detect when skills are relevant
- **Skill configuration** - Define rules for activation
- **PrePrompt injection** - Auto-inject skill docs
- **Skill library** - Curated examples (backend, frontend, database)

### What We Build

**1. Pattern Matching Engine**

Match prompts/files against skill activation rules:

```rust
pub struct SkillRule {
    pub name: String,
    pub triggers: Vec<Trigger>,
    pub skill_path: PathBuf,
}

pub enum Trigger {
    KeywordInPrompt(String),
    FilePathPattern(String),
    ContentContains { file: String, pattern: String },
}
```

**2. Skill Configuration**

Define skills in `.aiki/skills/skill-rules.yaml`:

```yaml
skills:
  - name: backend-guidelines
    skill_path: .aiki/skills/backend.md
    triggers:
      - keyword_in_prompt: "backend"
      - keyword_in_prompt: "API"
      - file_path_pattern: "src/backend/**/*.ts"
  
  - name: testing-requirements
    skill_path: .aiki/skills/testing.md
    triggers:
      - keyword_in_prompt: "test"
      - file_path_pattern: "**/*.test.ts"
      - content_contains:
          file: "package.json"
          pattern: "vitest"
  
  - name: database-patterns
    skill_path: .aiki/skills/database.md
    triggers:
      - keyword_in_prompt: "database"
      - keyword_in_prompt: "query"
      - file_path_pattern: "src/db/**/*.ts"
```

**3. PrePrompt Flow**

Auto-inject matched skills:

```yaml
PrePrompt:
  - let: matched_skills = self.match_skills($event.prompt, $event.recent_files)
  
  - for: skill in $matched_skills
    then:
      - doc_management:
          operation: query
          path: $skill.path
          variable: skill_content
      
      - prompt:
          prepend: |
            # Skill: $skill.name
            
            $skill_content
```

**4. Example Skills**

Ship with curated examples:

**.aiki/skills/backend.md:**
```markdown
# Backend Guidelines

## Error Handling
- Always use custom error types
- Include error codes for client errors
- Log errors with context

## Validation
- Use Zod for all input validation
- Validate at API boundary
- Return 400 with clear messages

## Testing
- Unit tests for business logic
- Integration tests for API endpoints
- Mock external dependencies
```

### Commands Delivered

```bash
# List available skills
aiki skills list
# Shows all configured skills

# Show skill details
aiki skills show backend-guidelines
# Displays skill content and triggers

# Create new skill
aiki skills create my-skill
# Generates template skill file

# Test skill matching
aiki skills test "prompt text"
# Shows which skills would activate
```

### Value Delivered

**For developers:**
- **Consistent quality** - Guidelines never forgotten
- **Reduced corrections** - Fewer mistakes to fix
- **Context-aware help** - Right guidance at right time
- **Onboarding** - New team members learn conventions

**For Aiki:**
- **Differentiation** - Not just code completion
- **Customizable** - Teams define their own skills
- **Composable** - Works with architecture caching

### Technical Components

| Component | Complexity | Priority | Timeline |
|-----------|------------|----------|----------|
| Pattern matching engine | Medium | High | 3 days |
| Skill configuration parser | Low | High | 2 days |
| PrePrompt integration | Low | High | 1 day |
| Example skills | Low | High | 2 days |
| CLI commands | Low | Medium | 2 days |

### Success Criteria

- ✅ Skills activate based on keyword triggers (90%+ accuracy)
- ✅ File pattern matching works correctly
- ✅ Content-based triggers detect patterns
- ✅ PrePrompt injects matched skills
- ✅ Example skills are useful and accurate
- ✅ CLI commands work for skill management
- ✅ Skill matching is fast (<10ms)

### Timeline

**Estimated: 2-3 weeks**
- Pattern matching: 3 days
- Configuration format: 2 days
- PrePrompt integration: 1 day
- Example skills: 2 days
- Testing + CLI: 3 days

**Detailed Plan:** See `ops/current/milestone-3.md` (to be created)

---

## Phase 12: Process Management

### Problem

Modern development involves multiple long-running processes (backend servers, databases, queues). Developers manually start/stop these services and struggle to correlate logs with code changes.

**Current pain points:**
- Manual process management (`npm run dev`, `docker-compose up`)
- Lost terminal output when restarting
- Hard to correlate errors with recent changes
- No health monitoring
- Process failures go unnoticed

### Solution

Implement flow-based process management with automatic startup, log aggregation, health monitoring, and correlation with code changes.

**Key capabilities:**
- **Process action type** - Start/stop/status from flows
- **Process configuration** - Define services in `.aiki/processes.yaml`
- **Log aggregation** - Collect and query process logs
- **Health monitoring** - Detect failures and restart
- **Change correlation** - Link errors to recent edits

### What We Build

**1. Process Action Type**

Manage processes from flows:

```yaml
PostFileChange:
  # Restart backend when code changes
  - if: $event.file_path starts_with "src/backend/"
    then:
      - process.restart: backend
      - process.wait_healthy: backend
  
SessionStart:
  # Start all services
  - process.start_all:
      parallel: true
  
SessionEnd:
  # Stop all services
  - process.stop_all
```

**2. Process Configuration**

Define services in `.aiki/processes.yaml`:

```yaml
processes:
  backend:
    command: npm run dev
    cwd: ./backend
    env:
      NODE_ENV: development
    health_check:
      type: http
      url: http://localhost:3000/health
      interval: 5s
    restart_on_failure: true
  
  database:
    command: docker-compose up postgres
    health_check:
      type: tcp
      port: 5432
      interval: 2s
  
  queue:
    command: npm run queue
    depends_on:
      - database
```

**3. Log Aggregation**

Collect logs with metadata:

```rust
pub struct ProcessLog {
    pub process: String,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub change_id: Option<String>,  // Correlate with JJ change
}
```

**4. Health Monitoring**

Detect failures and restart:

```yaml
ProcessHealthCheck:
  - if: $process.health == "unhealthy"
    then:
      - log: "Process $process.name is unhealthy, restarting..."
      - process.restart: $process.name
      - task.create:
          objective: "Investigate $process.name crash"
          evidence:
            - source: process_logs
              logs: $process.recent_errors
```

### Commands Delivered

```bash
# Start process
aiki process start backend
# Starts backend service

# Stop process
aiki process stop backend
# Stops backend service

# Show logs
aiki process logs backend --errors
# Shows error logs for backend

# Show all process status
aiki process status
# Lists all processes with health status

# Correlate errors with changes
aiki process errors --since-change <change-id>
# Shows errors since specific JJ change
```

### Value Delivered

**For developers:**
- **Observable systems** - All logs in one place
- **Automatic restarts** - Services recover from crashes
- **Change correlation** - Know which edit broke what
- **Task integration** - Crashes create investigation tasks

**For Aiki:**
- **Full-stack support** - Not just code editing
- **Differentiation** - Unique process management
- **Composable** - Works with tasks, architecture

### Technical Components

| Component | Complexity | Priority | Timeline |
|-----------|------------|----------|----------|
| Process action type | Medium | High | 3 days |
| Process manager | High | High | 4 days |
| Log aggregation | Medium | High | 3 days |
| Health monitoring | Medium | High | 2 days |
| CLI commands | Low | Medium | 2 days |

### Success Criteria

- ✅ Can start/stop processes from flows
- ✅ Process configuration works correctly
- ✅ Logs aggregated with timestamps
- ✅ Health checks detect failures
- ✅ Automatic restarts work
- ✅ Change correlation is accurate
- ✅ CLI commands work for management

### Timeline

**Estimated: 2 weeks**
- Process action + manager: 4 days
- Log aggregation: 3 days
- Health monitoring: 2 days
- Change correlation: 2 days
- Testing + CLI: 3 days

**Detailed Plan:** See `ops/current/milestone-5.md` (to be created)

---

## Phase 13: Autonomous Review Flow

### Problem
Developers waste significant time fixing AI-generated code through manual iteration loops. AI commits blindly, humans discover issues through slow manual testing or CI failures.

**With flows (Phase 5), we can now:**
- Build autonomous review as a declarative flow (not hardcoded in Rust)
- Let users customize review logic via YAML
- Compose review checks from multiple flows
- Block commits that fail quality gates

### Solution
Ship a built-in `aiki/autonomous-review` flow that users can include in their `.aiki/flow.yaml`. The flow runs on PreCommit events and blocks commits that fail static analysis, security scans, or other checks.

**User workflow:**
1. User adds `aiki/autonomous-review` to their flow.yaml
2. AI agent edits code and attempts commit
3. PreCommit event triggers
4. Review flow runs (static analysis, security scans, etc.)
5. If checks fail, commit is blocked with feedback
6. AI agent sees failure, makes corrections, tries again
7. Repeat until checks pass
8. Clean commit lands in Git

### What We Build
**This is just a flow** - No Rust changes needed beyond Phase 5:

```yaml
# Built-in flow: aiki/autonomous-review
# Location: Built into Aiki or shipped as default flow
name: Autonomous Review
version: 1.0.0
description: Automated code review with static analysis and security checks

requires:
  semgrep: ">=1.0"
  ruff: ">=0.1"
  mypy: ">=1.0"

PreCommit:
  # Run all checks in parallel for speed
  - parallel:
      - shell: semgrep --config=auto --json .
        alias: security
      - shell: ruff check --output-format=json .
        alias: lint
      - shell: mypy --json-report .
        alias: types
  
  # Aggregate results and block if any failed
  - if: $security.failed OR $lint.failed OR $types.failed
    then:
      - log: "❌ Review failed:"
      - if: $security.failed
        then:
          - log: "  Security issues found"
      - if: $lint.failed
        then:
          - log: "  Linting errors found"
      - if: $types.failed
        then:
          - log: "  Type errors found"
      - shell: exit 1
        on_failure: block
    else:
      - log: "✅ All checks passed"
```

**User customization:**
```yaml
# .aiki/flow.yaml
name: My Workflow
version: 1

includes:
  - aiki/autonomous-review    # Use built-in review
  - company/security-policy   # Add company-specific checks

PreCommit:
  # Runs after included flows
  - shell: pytest --fast       # Add custom test suite
    on_failure: block
```

### Value Delivered
- **Zero Rust code** - Entire review system is a YAML flow
- **Infinitely customizable** - Users modify flow.yaml, not Rust
- **Composable** - Mix built-in review with custom checks
- **Fast iteration** - Change review logic without rebuilding Aiki
- **Quality gates** - Block bad commits before they reach Git
- **Clean history** - No failed CI commits

### Technical Components
| Component | Complexity |
|-----------|------------|
| Built-in `aiki/autonomous-review` flow | Medium |
| Bundle semgrep/ruff/mypy binaries | Medium |
| Flow composition examples | Low |
| Documentation | Low |

**Note:** Everything else is already in Phase 5 (flows, PreCommit events, blocking, conditionals, parallel execution).

### Example: Team Customization
```yaml
# Company overrides built-in review with stricter checks
name: Company Code Review
version: 1.0.0

requires:
  semgrep: ">=1.0"
  sonarqube-cli: ">=4.0"

PreCommit:
  - parallel:
      - shell: semgrep --config=company-rules .
      - shell: sonar-scanner
      - shell: custom-compliance-check
  
  - if: $previous_step.failed
    then:
      - http:
          url: $SLACK_WEBHOOK
          body:
            text: "🚨 Compliance failure blocked commit"
      - shell: exit 1
        on_failure: block
```

### Success Criteria
- ✅ `aiki/autonomous-review` flow ships with Aiki (or as default flow)
- ✅ Flow bundles semgrep, ruff, mypy binaries for all platforms
- ✅ PreCommit blocking works (commit rejected if checks fail)
- ✅ Users can customize by editing `.aiki/flow.yaml`
- ✅ Teams can build custom review flows using same pattern
- ✅ Documentation shows composition examples

### Why This Matters
This demonstrates the power of Phase 5: **complex features become configuration, not code**. Autonomous review is ~200 lines of YAML instead of thousands of lines of Rust. Users can fork, customize, and share their own review flows without touching Aiki internals.

---

## Phase 14: Zed Extension (One-Click Setup & Status UI)

### Problem
While Phase 6 provides ACP proxy support, setup requires manual CLI steps and users have no visual feedback about Aiki's status. This creates friction:
- Users must run `aiki init` manually
- No visual indication that Aiki is active and working
- Review results only visible via CLI commands
- Configuration requires editing JSON files
- No discoverability for Aiki features within Zed

**Current user flow (CLI-only):**
```bash
$ aiki init
# Edit ~/.config/zed/settings.json manually
# Restart Zed
# No visual feedback that it's working
```

### Solution
Build a thin Zed extension that provides one-click setup and visual status UI. The extension sits ABOVE the ACP proxy (Phase 6) and delegates all logic to the `aiki` CLI tool.

**Key Principle:** The extension is a UI/UX layer only. All real work happens in the `aiki` CLI.

### Architecture
```
┌─────────────────────────────────────┐
│  Zed Extension (UI Layer)           │
│  - Command palette                  │
│  - Status bar                       │
│  - Settings UI                      │
└───────────┬─────────────────────────┘
            │ Delegates to CLI
            ↓
┌─────────────────────────────────────┐
│  aiki CLI (All Logic)               │
│  - aiki init                        │
│  - aiki acp (proxy)                 │
└─────────────────────────────────────┘
```

### What We Build
- **One-click installation** - Run `aiki init` from command palette
- **Status bar indicator** - Shows "Aiki ✓" or "Aiki ⚠"
- **Command palette commands** - Access Aiki features without CLI
- **Settings UI** - Configure Aiki from Zed's settings panel
- **Health check UI** - Display `aiki doctor` results

### User Experience

**Installation:**
```
Cmd+Shift+P → "Install Aiki Extension"
  ↓
Extension runs: aiki init
  ↓
Status bar shows: "Aiki ✓"
  ↓
Done! AI changes now tracked.
```

**Status Indicators:**
```
Aiki ○  - Not initialized
Aiki ◐  - Installed but not running  
Aiki ✓  - Running, all checks passing
Aiki ⚠  - Error or health check failed
```

### Value Delivered
- **Zero-friction setup** - Install extension, click Initialize, done
- **Visual feedback** - Always know if Aiki is working
- **Discoverability** - Find features via command palette
- **Professional UX** - Feels native to Zed
- **Lower barrier** - Non-technical users can use Aiki

### Technical Components
| Component | Complexity | Priority |
|-----------|------------|----------|
| Zed extension scaffold | Low | High |
| Command palette integration | Low | High |
| Status bar indicator | Low | High |
| CLI delegation | Low | High |
| Settings UI | Medium | Medium |
| Extension marketplace submission | Low | Medium |

### Success Criteria
- ✅ Extension installable from Zed marketplace
- ✅ One-click "Initialize Repository" from command palette
- ✅ Status bar shows Aiki status (○/◐/✓/⚠)
- ✅ `aiki doctor` results displayed in panel
- ✅ All logic delegated to `aiki` CLI (no duplication)
- ✅ Works on macOS, Linux, Windows

### Timeline
- Extension scaffold + commands: 1-2 days
- Status bar indicator: 1 day
- CLI delegation: 1 day
- Settings UI: 1-2 days
- Testing + marketplace: 2 days

**Total: ~1.5 weeks**

**Detailed Plan:** See `ops/phase-9.md`

### Why This Matters
**Before:** Users must run 5 terminal commands  
**After:** Click "Install Extension", click "Initialize"  

**Impact:** 10x easier onboarding, professional UX, marketplace visibility

---

## Phase 15: Comprehensive Event System (All Git & Agent Hooks)

### Problem
Phase 5 introduced the flow system with 4 core events (SessionStart, PreFileChange, PostFileChange, PrepareCommitMessage), but Git provides 20+ hooks and agents (like Claude Code) provide 10+ lifecycle hooks. Users need access to the full event lifecycle to build sophisticated workflows.

**Current limitations:**
- Only 4 events supported (SessionStart, PreFileChange, PostFileChange, PrepareCommitMessage)
- Can't hook into pre-commit, post-commit, pre-push, etc.
- Can't react to agent lifecycle events (SessionStart, SessionEnd, Stop)
- Can't integrate with Git's full workflow (rebase, merge, checkout)
- Teams can't build complete CI/CD-like workflows locally

### Solution
Expand the event system to support all Git client-side hooks and all Claude Code agent hooks. Map them to flow events with consistent naming and context.

### What We Build

**Git Hook Events:**
```yaml
# Complete Git client-side hook coverage
PreCommit:          # pre-commit - before commit message editor
PrepareCommitMessage: # prepare-commit-msg - modify message before editor
CommitMessage:      # commit-msg - validate/modify message after editor  
PostCommit:         # post-commit - after commit completes
PreRebase:          # pre-rebase - before rebase starts
PostCheckout:       # post-checkout - after checkout completes
PostMerge:          # post-merge - after merge completes
PrePush:            # pre-push - before pushing to remote
PostRewrite:        # post-rewrite - after commit-rewriting command
PreMergeCommit:     # pre-merge-commit - before merge commit created
PreAutoGC:          # pre-auto-gc - before garbage collection
ReferenceTransaction: # reference-transaction - when refs are updated
```

**Agent Lifecycle Events:**
```yaml
# Claude Code / AI agent hooks
SessionStart:       # Agent session begins
SessionEnd:         # Agent session ends
UserPromptSubmit:   # Before user prompt is sent to agent
PreToolUse:         # Before agent executes a tool
PostToolUse:        # After agent executes a tool (current PostFileChange)
Notification:       # Agent sends notification
Stop:               # Agent is stopped/interrupted
SubagentStart:      # Subagent/task begins
SubagentStop:       # Subagent/task completes
PreCompact:         # Before context compaction
```

**Event Context Variables:**

Each event provides relevant context as `$event.*` variables:

```yaml
# PrepareCommitMessage event
$event.commit_msg_file    # Path to COMMIT_EDITMSG
$event.commit_source      # "message", "template", "merge", "squash", "commit"
$event.commit_sha         # SHA-1 of commit being amended (if applicable)

# PostCommit event  
$event.commit_sha         # SHA of newly created commit
$event.commit_message     # Full commit message

# PrePush event
$event.remote             # Remote name (e.g., "origin")
$event.remote_url         # Remote URL
$event.local_ref          # Local ref being pushed
$event.remote_ref         # Remote ref being updated

# PostCheckout event
$event.prev_head          # Previous HEAD ref
$event.new_head           # New HEAD ref
$event.checkout_type      # "branch" or "file"

# UserPromptSubmit event
$event.prompt             # User's prompt text
$event.context_files      # Files in context

# PreToolUse/PostToolUse events
$event.tool_name          # Tool being executed
$event.tool_args          # Tool arguments (JSON)
$event.file_path          # File being modified (if applicable)
```

### Example: Complete Pre-Push Workflow

```yaml
# .aiki/flow.yaml
name: Complete Development Workflow
version: 1

# Before commit message editor opens
PreCommit:
  - shell: cargo fmt --check
  - shell: cargo clippy
  - let: tests = shell("cargo test")
    on_failure: stop

# Modify commit message before editor
PrepareCommitMessage:
  - let: coauthors = self.generate_coauthors
  - commit_message:
      append_trailer: $coauthors

# Validate commit message after editor closes  
CommitMessage:
  - shell: |
      if ! grep -q "^[A-Z]" "$event.commit_msg_file"; then
        echo "Error: Commit message must start with capital letter"
        exit 1
      fi
    on_failure: stop

# After commit succeeds
PostCommit:
  - log: "✅ Commit $event.commit_sha created"
  - shell: echo "$event.commit_message" >> .aiki/commit-log.txt

# Before pushing to remote
PrePush:
  - log: "Pushing to $event.remote ($event.remote_url)"
  - shell: cargo test --release    # Full test suite before push
  - shell: cargo build --release   # Ensure release builds
    on_failure: stop

# After merge completes
PostMerge:
  - shell: cargo update            # Update dependencies after merge
  - log: "Merge complete, dependencies updated"
```

### Example: Agent Lifecycle Integration

```yaml
# .aiki/flow.yaml
name: AI Development Workflow
version: 1

# When agent session starts
SessionStart:
  - log: "🤖 AI session starting"
  - shell: git fetch origin        # Sync with remote
  - shell: cargo check             # Verify project builds

# Before user prompt is submitted
UserPromptSubmit:
  - log: "User prompt: $event.prompt"
  - shell: echo "$event.prompt" >> .aiki/prompt-history.txt

# Before agent uses a tool
PreToolUse:
  - log: "Tool: $event.tool_name on $event.file_path"
  - shell: cp "$event.file_path" ".aiki/backups/$(basename $event.file_path).bak"

# After agent modifies files (existing PostFileChange)
PostToolUse:
  - let: description = self.build_description
  - jj: describe -m "$description"

# When agent session ends
SessionEnd:
  - log: "🤖 AI session ending"
  - shell: cargo fmt               # Final format
  - jj: commit -m "End of AI session"
```

### Example: Complex Git Workflow

```yaml
# Handle rebases
PreRebase:
  - log: "Starting rebase, backing up current state"
  - jj: bookmark create backup-$(date +%s)

PostRewrite:
  - log: "Commits rewritten, updating metadata"
  - shell: ./scripts/update-change-ids.sh

# Handle checkouts
PostCheckout:
  - log: "Switched from $event.prev_head to $event.new_head"
  - if: $event.checkout_type == "branch"
    then:
      - shell: cargo check    # Verify build after branch switch
```

### Commands Delivered

```bash
# No new commands - hooks automatically installed
$ aiki init    # Installs all Git hooks

# Events dispatched automatically by Git
$ git commit              # Triggers: PreCommit → PrepareCommitMessage → CommitMessage → PostCommit
$ git push                # Triggers: PrePush
$ git checkout main       # Triggers: PostCheckout
$ git merge feature       # Triggers: PreMergeCommit → PostMerge
$ git rebase main         # Triggers: PreRebase → PostRewrite

# Events dispatched by agent hooks
# (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, SessionEnd)
# Automatically triggered by agent lifecycle
```

### Value Delivered
- **Complete hook coverage** - Access to full Git and agent lifecycle
- **Sophisticated workflows** - Build CI/CD-like pipelines locally
- **Consistent interface** - All hooks use same flow YAML syntax
- **Event context** - Rich `$event.*` variables for each hook
- **Backward compatible** - Existing flows continue to work
- **No configuration needed** - `aiki init` installs all hooks

### Technical Components

| Component | Complexity |
|-----------|------------|
| Git hook templates (20+ hooks) | Medium |
| Event type definitions | Medium |
| Event context extraction | Medium |
| Hook → Event mapping | Low |
| Agent hook integration | Medium |
| Documentation for all events | High |
| Event context variable resolution | Low (reuses Phase 5) |

### Implementation Notes

**Git Hook Installation:**
```bash
# .git/hooks/pre-commit
#!/bin/sh
export AIKI_HOOK_NAME="pre-commit"
aiki event pre-commit

# .git/hooks/pre-push  
#!/bin/sh
export AIKI_HOOK_NAME="pre-push"
export AIKI_REMOTE="$1"
export AIKI_REMOTE_URL="$2"
# Read stdin for ref info
while read local_ref local_sha remote_ref remote_sha; do
    export AIKI_LOCAL_REF="$local_ref"
    export AIKI_REMOTE_REF="$remote_ref"
done
aiki event pre-push
```

**Event Dispatching:**
```rust
// cli/src/events.rs
pub enum AikiEvent {
    // Existing
    Start(AikiStartEvent),
    PostFileChange(AikiPostChangeEvent),
    PrepareCommitMessage(AikiPrepareCommitMessageEvent),
    
    // New Git events
    PreCommit(AikiPreCommitEvent),
    CommitMessage(AikiCommitMessageEvent),
    PostCommit(AikiPostCommitEvent),
    PrePush(AikiPrePushEvent),
    PostCheckout(AikiPostCheckoutEvent),
    PostMerge(AikiPostMergeEvent),
    // ... 12 more Git events
    
    // New agent events  
    SessionStart(AikiSessionStartEvent),
    SessionEnd(AikiSessionEndEvent),
    UserPromptSubmit(AikiUserPromptSubmitEvent),
    PreToolUse(AikiPreToolUseEvent),
    // ... 6 more agent events
}
```

**Flow YAML Schema:**
```yaml
# cli/src/flows/types.rs
pub struct Flow {
    // Existing
    pub start: Vec<Action>,
    pub post_change: Vec<Action>,
    pub prepare_commit_message: Vec<Action>,
    
    // New Git hooks
    pub pre_commit: Vec<Action>,
    pub commit_message: Vec<Action>,
    pub post_commit: Vec<Action>,
    pub pre_push: Vec<Action>,
    pub post_checkout: Vec<Action>,
    pub post_merge: Vec<Action>,
    pub pre_rebase: Vec<Action>,
    pub post_rewrite: Vec<Action>,
    pub pre_merge_commit: Vec<Action>,
    pub pre_auto_gc: Vec<Action>,
    pub reference_transaction: Vec<Action>,
    
    // New agent hooks
    pub session_start: Vec<Action>,
    pub session_end: Vec<Action>,
    pub user_prompt_submit: Vec<Action>,
    pub pre_tool_use: Vec<Action>,
    pub post_tool_use: Vec<Action>,
    pub notification: Vec<Action>,
    pub stop: Vec<Action>,
    pub subagent_start: Vec<Action>,
    pub subagent_stop: Vec<Action>,
    pub pre_compact: Vec<Action>,
}
```

### Success Criteria
- ✅ All Git client-side hooks supported (20+ hooks)
- ✅ All Claude Code agent hooks supported (10+ hooks)
- ✅ Each event provides rich `$event.*` context variables
- ✅ `aiki init` installs all Git hooks automatically
- ✅ Existing flows (SessionStart, PreFileChange, PostFileChange, PrepareCommitMessage) continue working
- ✅ Documentation covers all events with examples
- ✅ Users can build complete local CI/CD workflows
- ✅ Agent lifecycle fully integrated with flows

### Why This Enables Future Phases
- **Phase 8**: User-defined flows can hook into any lifecycle event
- **Phase 9**: External flows can leverage full event system
- **Autonomous workflows**: Complete control over entire development lifecycle
- **Team workflows**: Standardize Git workflows across organization
- **Quality gates**: Enforce checks at every stage (commit, push, merge, etc.)

---

## Phase 16: User-Defined Flows

### Problem
Phase 5 and 6 provide built-in flows, but users need to write their own reusable flows without duplicating inline steps across `.aiki/flow.yaml`.

### Solution
Enable users to create their own flows in `.aiki/flows/` directory and compose them together.

### What We Build
- **Flow directories** - `.aiki/flows/my/` for user flows
- **Flow composition** - `includes:` to reference other flows
- **Step references** - Reference flow results in conditionals
- **Flow-level variables** - Define reusable variables in flows

### Example
```yaml
# .aiki/flows/my/review.yaml
name: My Custom Review
version: 1.0.0

PreCommit:
  - shell: semgrep --config=custom-rules .
  - shell: mypy --strict .
  - if: $previous_step.failed
    then:
      - shell: exit 1
        on_failure: block

# .aiki/flow.yaml
name: My Workflow
version: 1

includes:
  - aiki/autonomous-review    # Built-in
  - my/review                 # User-defined

PreCommit:
  - shell: echo "All reviews done"
```

### Value Delivered
- **Reusable flows** - DRY across projects
- **Organization patterns** - Teams share flows via Git
- **Flow composition** - Build complex workflows from simple pieces

### Technical Components
| Component | Complexity |
|-----------|------------|
| Flow directory loading | Low |
| Flow composition (includes) | Medium |
| Step reference resolution | Low |
| Flow-level variables | Low |

### Success Criteria
- ✅ Users can create flows in `.aiki/flows/my/`
- ✅ `includes:` composes multiple flows
- ✅ Flows can reference each other's results
- ✅ No external flows yet (local only)

---

## Phase 17: External Flow Ecosystem

### Problem
Vendors want to distribute complete, working flows with bundled binaries. Users want to install flows from vendors without manual setup.

### Solution
Enable flow ecosystem with bundled binaries, lazy loading, and distribution.

### What We Build
- **WASM-based custom functions** - Flow authors write Rust, compile to WASM, distribute pre-compiled
- **Native compilation (optional)** - Power users can compile flows to native for 1.5-3x speed boost
- **Bundled binaries** - `bin/<platform>/` for native tools (e.g., semgrep)
- **Lazy loading** - Auto-download flows on first use
- **Symlink management** - `~/.aiki/bin/` for all flow binaries
- **External dependencies** - `requires:` for tools like Docker
- **Flow distribution** - Tarballs for sharing flows
- **Flow caching** - `~/.aiki/cache/flows/` for downloaded flows

### Example: WASM-based Custom Functions

**Flow author writes custom Rust function:**
```rust
// vendor/complexity-analyzer/src/lib.rs
use aiki_flow_sdk::prelude::*;

#[aiki_function]
pub fn analyze_complexity(context: &Context) -> Result<String> {
    // Read file path from event context
    let file_path = context.event_vars.get("file_path")
        .ok_or_else(|| anyhow::anyhow!("Missing event.file_path"))?;
    
    let content = context.read_file(file_path)?;
    let complexity = calculate_complexity(&content);
    Ok(complexity.to_string())
}
```

**Compile to WASM:**
```bash
$ cargo build --target wasm32-wasi --release
# Produces: target/wasm32-wasi/release/complexity_analyzer.wasm
```

**Flow structure:**
```
vendor/complexity-analyzer/
├── flow.yaml
├── src/
│   └── lib.rs              # Source (optional in distribution)
└── bin/
    └── wasm/
        └── complexity_analyzer.wasm  # Pre-compiled WASM (~500KB-2MB)
```

**Flow YAML uses the WASM function with `let:` syntax:**
```yaml
# vendor/complexity-analyzer/flow.yaml
name: Complexity Analyzer
version: 1.0.0

PreCommit:
  # Call WASM function - receives full $event context automatically
  - let: complexity = vendor/analyzer.analyze_complexity
  
  - if: $complexity > 10
    then:
      - log: "Warning: High complexity detected: $complexity"
      - shell: exit 1
        on_failure: block
    else:
      - log: "✅ Complexity acceptable: $complexity"
```

**User includes the flow:**
```yaml
# .aiki/flow.yaml
name: My Workflow
version: 1

includes:
  - aiki/autonomous-review
  - vendor/complexity-analyzer     # Auto-downloads WASM on first use

PreCommit:
  - flow: vendor/complexity-analyzer
```

**Or call WASM function directly in user flow:**
```yaml
# .aiki/flow.yaml
PreCommit:
  # Direct function call - same syntax as built-in functions
  - let: complexity = vendor/analyzer.analyze_complexity
  - let: security = vendor/scanner.scan_security
  
  - if: $complexity > 10 OR $security.failed
    then:
      - log: "❌ Quality checks failed"
      - shell: exit 1
        on_failure: block
```

### Example: Native Tools + WASM Functions

**Flow bundles both native binaries and WASM:**
```
vendor/security-scan/
├── flow.yaml
├── bin/
│   ├── darwin-arm64/
│   │   └── semgrep          # Native binary for macOS ARM
│   ├── darwin-x86_64/
│   │   └── semgrep          # Native binary for macOS Intel
│   ├── linux-x86_64/
│   │   └── semgrep          # Native binary for Linux
│   └── wasm/
│       └── custom_rules.wasm  # Custom Rust logic as WASM
```

**Flow YAML mixes native binaries and WASM functions:**
```yaml
PostFileChange:
  - shell: semgrep --config=auto $event.file_path  # Uses native binary
  - let: result = vendor/scanner.validate_custom_rules  # Uses WASM function
  
  - if: $result.failed
    then:
      - log: "❌ Custom rule validation failed"
```

### The `let:` Syntax for External Functions

External WASM functions use the same `let:` syntax as built-in Aiki functions (from Phase 5.1):

```yaml
# Built-in Aiki function
- let: description = aiki/provenance.build_description

# External WASM function - same syntax!
- let: complexity = vendor/analyzer.analyze_complexity
```

**Key design principles:**
- **No `args:` block needed** - Functions receive full `$event` context automatically
- **Namespace determines implementation** - `aiki/` = built-in Rust, `vendor/` or `my/` = WASM
- **Transparent routing** - Aiki handles WASM vs native vs built-in automatically
- **Same variable storage** - All functions store results as `$var_name`, `$var_name.exit_code`, etc.

**How WASM functions receive context:**
```rust
// WASM function signature
pub fn analyze_complexity(context: &Context) -> Result<String> {
    // Read from event context
    let file_path = context.event_vars.get("file_path")?;
    let agent = context.event_vars.get("agent")?;
    
    // All $event.* variables available
    // Plus any variables set by previous steps
}
```

**Benefits:**
- Users learn one syntax for all functions
- Built-in and external functions are interchangeable
- Easy migration path (add namespace prefix to call external version)
- Consistent error handling and variable storage

See [`ops/milestone-5.1.md`](milestone-5.1.md) for complete `let:` syntax specification.

### Commands Delivered
```bash
aiki flows list          # Show all flows (built-in, installed, cached)
aiki flows install       # Install all flows from .aiki/flow.yaml
aiki flows compile       # (Optional) Compile WASM flows to native for speed
aiki flows cleanup       # Remove unused cached flows
aiki doctor              # Validate flows and dependencies
```

**Example: Optional native compilation for power users:**
```bash
# Default: Use WASM (fast install, cross-platform)
$ aiki flows install
✓ vendor/complexity-analyzer v1.0.0 (WASM, ~500KB)
  First run: ~50ms, warm runs: ~20ms

# Power user: Compile to native for better performance
$ aiki flows compile vendor/complexity-analyzer --release
Compiling vendor/complexity-analyzer to native...
✓ Compiled in 8 seconds
✓ Performance: 50ms → 15ms (3x faster)
```

### Value Delivered
- **Custom logic in flows** - Flow authors write Rust functions, not just shell scripts
- **Fast installation** - WASM binaries download in <1 second (vs 5-30 seconds compiling Rust)
- **Small disk footprint** - WASM flows use ~1-2 MB (vs 50-200 MB for Rust with deps)
- **Sandboxed execution** - WASM runtime isolates flow code for security
- **Cross-platform** - Single WASM binary works on all platforms
- **Optional native speed** - Power users can compile for 1.5-3x performance boost
- **Vendor ecosystem** - Third parties ship complete flows with custom logic
- **Flow marketplace** - (Future) Central registry for discovering flows

### Technical Components
| Component | Complexity | Notes |
|-----------|------------|-------|
| WASM runtime integration | Medium | Embed `wasmtime` or `wasmer` |
| WASM function calling | Medium | FFI, memory management, WASI support |
| Optional native compilation | Medium | Dynamic library loading via `libloading` |
| Performance optimization | Low | Cache compiled WASM modules |
| Binary bundling (native tools) | Medium | Platform-specific binaries in `bin/<platform>/` |
| Platform detection | Low | Auto-detect architecture |
| Symlink management | Low | Create symlinks in `~/.aiki/bin/` |
| Lazy loading | Medium | Auto-download flows on first use |
| Flow caching | Low | Store in `~/.aiki/cache/flows/` |
| CLI commands | Low | `list`, `install`, `compile`, `cleanup` |

### Success Criteria
- ✅ Flow authors can write custom Rust functions and compile to WASM
- ✅ WASM flows install in <1 second (pre-compiled binaries)
- ✅ WASM flows run with 10-50% overhead vs native (acceptable for most use cases)
- ✅ Power users can compile WASM flows to native for 1.5-3x speedup
- ✅ Flows can bundle both WASM functions and native binaries (e.g., semgrep)
- ✅ `~/.aiki/bin` symlinks to all flow binaries
- ✅ Flows auto-download on first use
- ✅ `aiki flows install` installs all referenced flows
- ✅ `aiki flows compile` compiles WASM to native (optional)
- ✅ `aiki doctor` validates flow health and dependencies
- ✅ WASM runtime sandboxes flow code for security

Side-by-Side Comparison

| Phase | Rust Dynamic Lib | WASM | Winner |
|-------|-----------------|------|--------|
| **Install (first time)** | 5-30 sec | 200-850ms | **WASM (20-60x faster)** |
| **Disk usage** | 50-200 MB | 1-2 MB | **WASM (100x smaller)** |
| **First run (cold)** | 20-140ms | 50-300ms | **Rust (2-3x faster)** |
| **Subsequent runs (warm)** | 10-100ms | 15-155ms | **Rust (1.5x faster)** |
| **Update flow** | 5-30 sec | 200-850ms | **WASM (20-60x faster)** |
| **Memory overhead** | ~5-20 MB (loaded lib) | ~10-30 MB (WASM runtime) | **Rust (2x better)** |

---

## Real-World Scenarios

### Scenario 1: Developer with 10 external flows

**Rust:**
- Install all flows: **50-300 seconds** (compile each)
- Disk usage: **500 MB - 2 GB** (dependencies, compiled libs)
- Runtime performance: **Excellent** (native speed)

**WASM:**
- Install all flows: **2-8 seconds** ✅
- Disk usage: **10-20 MB** ✅
- Runtime performance: **Good** (10-50% slower)

**Winner: WASM** - Much better UX for installation

---

### Scenario 2: High-frequency flow (runs on every PostFileChange)

**Rust:**
- First run: 20-140ms
- Next 1000 runs: **10-100ms each** = 10-100 seconds total
- **Total: 10-100 seconds for 1000 runs**

**WASM:**
- First run: 50-300ms
- Next 1000 runs: **15-155ms each** = 15-155 seconds total
- **Total: 15-155 seconds for 1000 runs**

**Winner: Rust** - 1.5x faster over many runs

---

### Scenario 3: CI/CD environment (fresh install each run)

**Rust:**
- Install flows: 50-300 seconds
- Run flows: 10-100ms
- **Total: 50-300 seconds**

**WASM:**
- Install flows: 2-8 seconds
- Run flows: 15-155ms
- **Total: 2-8 seconds** ✅

**Winner: WASM** - 20-60x faster total time

---

## Phase 18: Multi-Agent Provenance (Fallback Detection)

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

## Phase 19: Local Multi-Agent Coordination

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

## Phase 20: PR Review for Non-Aiki Agents

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

## Phase 21: Shared JJ Brain & Team Coordination

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

## Phase 22: Windsurf Support

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

## Phase 23: Enterprise Compliance

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
- **Centralized hook management** - Deploy and manage AI editor hooks across all developer machines from central dashboard (similar to Cursor's enterprise cloud distribution and Claude Code's team management)

### Value Delivered
- Enterprise governance for AI development
- Demonstrable compliance for auditors
- Custom policies per team/project
- Complete audit trails with full provenance (immutable JJ commit history with `[aiki]` metadata)
- Risk-based review workflows
- Regulatory confidence (SOX, PCI-DSS, ISO 27001)
- Simplified deployment and maintenance of AI editor hooks across entire organization

**Architecture Note:** JJ's immutable commit graph provides tamper-proof audit trails. All provenance data in `[aiki]` blocks is part of commit history, making it impossible to retroactively alter attributions.

---

## Phase 24: Native Agent Integration

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
**Phases 1-11 deliver full value WITHOUT vendor cooperation.** Phase 12 is an optional enhancement, not a requirement for success.

---

## Phase Dependencies

```
Phase 0 (CLI/JJ) ✅
    ↓
Phase 1 (Claude Code Provenance) ✅ ← Foundation complete, SQLite-free
    ↓
Phase 2 (Cursor Support) 🔜 ← Extends Phase 1 architecture
    ↓
Phase 3 (CLI Streamlining & Health Diagnostics) ✅ ← aiki doctor command
    ↓
Phase 4 (Cryptographic Signing) ← Tamper-proof attribution
    ↓
Phase 5 (Internal Flow Engine) ← Event-driven workflow system
    ↓
Phase 6 (ACP Support) ← Generic ACP proxy for IDE-agent communication
    ↓
Phase 7 (Autonomous Review Flow) ← Built on Phase 5 flows
    ↓
Phase 8 (Zed Extension) ← One-click setup & status UI
    ↓
Phase 9 (Comprehensive Event System) ← All Git & agent hooks supported
    ↓
Phase 10 (User-Defined Flows) ← Users write reusable flows
    ↓
Phase 11 (External Flow Ecosystem) ← WASM functions + bundled binaries
    ↓
Phase 12 (Multi-Agent: Fallback Detection)
    ↓
Phase 13 (Local Multi-Agent Coordination) ← Uses Phase 1+2+12 provenance
    ↓
Phase 14 (PR Review for Non-Aiki Agents)
    ↓
Phase 15 (Shared JJ Brain & Team Coordination) ← Team provenance via JJ commit descriptions
    ↓
Phase 16 (Windsurf Support) ← Additional editor before enterprise
    ↓
Phase 17 (Enterprise Compliance) ← Immutable audit trails via JJ + Phase 4 signing
    ↓
Phase 18 (Native Agent Integration) ← Agent SDK with trust scoring
```

**Key Insights:** 
- Phase 1 (Provenance) provides the SQLite-free foundation (~120 bytes per change in JJ commit descriptions)
- Phase 2 (Cursor) and Phase 16 (Windsurf) extend to additional editors using same architecture
- Phase 3 (CLI Streamlining) provides `aiki doctor` for health diagnostics
- Phase 4 (Cryptographic Signing) adds tamper-proof verification layer for enterprise compliance
- Phase 6 (ACP Support) enables IDE-agnostic provenance tracking
- Phase 8 (Zed Extension) provides polished UX for zero-friction onboarding
- All subsequent phases query provenance via JJ revsets (no database needed)
- JJ's immutable commit graph + cryptographic signatures provide audit-ready trails for compliance
