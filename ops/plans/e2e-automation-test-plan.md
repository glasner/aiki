# End-to-End Automation Test Plan for Claude Code Sessions

## Overview

This plan outlines a comprehensive end-to-end automation test that simulates a full coding session with Claude Code, verifying that Aiki correctly tracks provenance throughout the entire lifecycle.

---

## Goals

1. **Full Session Lifecycle Testing** - Test session start → edits → provenance → session end
2. **Automated Verification** - No manual intervention required
3. **CI/CD Compatible** - Can run in automated pipelines (with appropriate credentials)
4. **Comprehensive Coverage** - Test hooks, flows, metadata, and attribution

---

## Approach Options

### Option A: Claude Agent SDK (Python) - Recommended

Use the Python Claude Agent SDK for programmatic control with hooks for event capture.

**Pros:**
- Full control over session lifecycle
- Built-in hooks for PreToolUse/PostToolUse capture
- Session resume/fork capabilities
- Structured output handling

**Cons:**
- Requires Python environment
- Additional dependency

**Implementation:**
```python
from claude_agent_sdk import ClaudeSDKClient, ClaudeAgentOptions

options = ClaudeAgentOptions(
    permission_mode="acceptEdits",
    cwd="/path/to/test/repo",
    hooks={
        "PreToolUse": [...],
        "PostToolUse": [...]
    }
)

async with ClaudeSDKClient(options=options) as client:
    await client.query("Add a subtract function to calculator.py")
    async for message in client.receive_response():
        # Capture and verify
```

### Option B: Claude Code CLI (Bash/Rust)

Use `claude -p` (print mode) for non-interactive execution.

**Pros:**
- Simple setup
- Already have existing test (`claude_integration_test.rs`)
- No additional dependencies

**Cons:**
- Less control over session
- Harder to capture intermediate events
- Limited hook introspection

**Implementation:**
```bash
claude -p "Add a subtract function" \
  --output-format json \
  --dangerously-skip-permissions
```

### Option C: Hybrid Approach - Recommended for Comprehensive Testing

Combine both: Use CLI for simple scenarios, SDK for complex multi-step sessions.

---

## Test Scenarios

### Scenario 1: Basic Edit Flow (Minimal)
**Purpose:** Verify single file edit triggers provenance recording

**Steps:**
1. Initialize repo with `aiki init`
2. Create test file (e.g., `calculator.py`)
3. Invoke Claude to add a function
4. Verify file was modified
5. Check JJ change description contains `[aiki]` metadata
6. Validate metadata fields (author, session, tool, confidence)

**Expected Metadata:**
```
[aiki]
author=claude
author_type=agent
session=<session-id>
tool=Edit
confidence=High
method=Hook
[/aiki]
```

### Scenario 2: Multi-File Edit Session
**Purpose:** Verify session tracking across multiple file edits

**Steps:**
1. Initialize repo
2. Create multiple source files
3. Ask Claude to refactor across files (e.g., "Extract common logic")
4. Verify each file edit has separate provenance
5. Confirm all edits share same session ID
6. Check change descriptions for each modified file

### Scenario 3: Session Lifecycle Events
**Purpose:** Verify session start/resume/end events fire correctly

**Steps:**
1. Start session (verify `session.started` event)
2. Submit prompt (verify `prompt.submitted` event)
3. Make edits (verify `change.completed` events)
4. End session (verify `session.ended` event)
5. Resume session with same ID (verify `session.resumed`)
6. Confirm session continuity in metadata

### Scenario 4: Error Recovery
**Purpose:** Verify system handles failures gracefully

**Steps:**
1. Initialize repo
2. Simulate Claude attempting to edit non-existent file
3. Verify error is logged but doesn't crash
4. Continue with valid edit
5. Confirm provenance tracking resumes

### Scenario 5: Multi-Editor Interleaving
**Purpose:** Verify correct attribution when multiple editors used

**Steps:**
1. Initialize repo
2. Make edit with Claude Code
3. Make manual human edit
4. Make edit with Cursor (simulated)
5. Run `aiki blame` and verify distinct attributions
6. Verify `aiki authors` shows all contributors

### Scenario 6: Hook Failure Recovery
**Purpose:** Verify Claude Code continues even if hook fails

**Steps:**
1. Configure intentionally broken hook
2. Invoke Claude edit
3. Verify edit succeeds (Claude continues)
4. Check hook failure is logged
5. Fix hook, retry, confirm works

---

## Implementation Phases

### Phase 1: Test Infrastructure Setup

**Files to create:**
- `cli/tests/e2e_automation/mod.rs` - Module organization
- `cli/tests/e2e_automation/helpers.rs` - Shared test utilities
- `cli/tests/e2e_automation/scenarios.rs` - Test scenario definitions

**Utilities needed:**
```rust
// Helper to invoke Claude Code with capture
fn invoke_claude_code(prompt: &str, repo_path: &Path) -> Result<ClaudeOutput>

// Helper to wait for and verify metadata
fn wait_for_metadata(repo_path: &Path, expected: &MetadataFields) -> Result<()>

// Helper to parse [aiki] blocks from descriptions
fn parse_aiki_metadata(description: &str) -> Result<AikiMetadata>

// Helper to verify session continuity
fn verify_session_id(repo_path: &Path, expected_session: &str) -> Result<()>
```

### Phase 2: Basic Test Implementation

**File:** `cli/tests/e2e_automation/test_basic_edit.rs`

```rust
#[test]
fn test_basic_edit_flow() {
    // Setup
    let temp_dir = tempdir().unwrap();
    let repo_path = temp_dir.path();

    init_git_repo(repo_path);
    init_jj_workspace(repo_path);
    run_aiki_init(repo_path);
    create_test_file(repo_path, "calculator.py", CALCULATOR_CONTENT);

    // Execute
    let output = invoke_claude_code(
        "Add a multiply function to calculator.py",
        repo_path
    ).expect("Claude Code invocation failed");

    // Verify file modification
    assert!(file_contains(repo_path, "calculator.py", "multiply"));

    // Verify provenance metadata
    let metadata = wait_for_metadata(repo_path, Duration::from_secs(5))
        .expect("Metadata not found");

    assert_eq!(metadata.author, "claude");
    assert_eq!(metadata.author_type, "agent");
    assert!(metadata.session.is_some());
    assert_eq!(metadata.tool, "Edit");
    assert_eq!(metadata.confidence, "High");
    assert_eq!(metadata.method, "Hook");
}
```

### Phase 3: Multi-Step Session Tests

**File:** `cli/tests/e2e_automation/test_multi_step.rs`

```rust
#[test]
fn test_multi_file_refactoring_session() {
    let temp_dir = setup_test_repo();

    // Create initial files
    create_file(&temp_dir, "math/add.py", "def add(a, b): return a + b");
    create_file(&temp_dir, "math/sub.py", "def sub(a, b): return a - b");
    create_file(&temp_dir, "main.py", "from math.add import add\nfrom math.sub import sub");

    // Ask Claude to refactor
    let output = invoke_claude_code(
        "Create a unified math.py module that combines add and sub functions, and update main.py to import from it",
        &temp_dir
    ).expect("Claude refactoring failed");

    // Verify multiple files modified
    assert!(file_exists(&temp_dir, "math.py") || file_contains(&temp_dir, "math/__init__.py", "def"));

    // Verify all changes have same session ID
    let changes = get_recent_changes(&temp_dir, 5);
    let session_ids: HashSet<_> = changes.iter()
        .filter_map(|c| c.metadata.session.as_ref())
        .collect();

    assert_eq!(session_ids.len(), 1, "All changes should share same session");
}
```

### Phase 4: Event Verification Tests

**File:** `cli/tests/e2e_automation/test_events.rs`

```rust
#[test]
fn test_session_lifecycle_events() {
    let temp_dir = setup_test_repo();
    let event_log = Arc::new(Mutex::new(Vec::new()));

    // Setup event capture (via flow or hook output)
    configure_event_logging(&temp_dir, event_log.clone());

    // Start session
    let output = invoke_claude_code("Add a helper function", &temp_dir)?;

    // Verify event sequence
    let events = event_log.lock().unwrap();
    assert!(events.iter().any(|e| e.event_type == "session.started"));
    assert!(events.iter().any(|e| e.event_type == "prompt.submitted"));
    assert!(events.iter().any(|e| e.event_type == "change.completed"));
    // Note: session.ended may require explicit session termination
}
```

### Phase 5: SDK-Based Advanced Tests (Optional)

**File:** `cli/tests/e2e_automation_sdk/test_sdk_control.py`

```python
import pytest
from claude_agent_sdk import ClaudeSDKClient, ClaudeAgentOptions
import subprocess
import tempfile
import json

class TestSDKAutomation:
    @pytest.fixture
    def test_repo(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            # Initialize repo
            subprocess.run(["git", "init"], cwd=tmpdir, check=True)
            subprocess.run(["aiki", "init"], cwd=tmpdir, check=True)
            yield tmpdir

    @pytest.mark.asyncio
    async def test_full_session_with_hooks(self, test_repo):
        tool_calls = []

        async def capture_tool_use(input_data, tool_use_id, context):
            tool_calls.append({
                "tool": input_data.get("tool_name"),
                "input": input_data.get("tool_input"),
                "id": tool_use_id
            })
            return {}

        options = ClaudeAgentOptions(
            permission_mode="acceptEdits",
            cwd=test_repo,
            hooks={
                "PostToolUse": [
                    HookMatcher(matcher="", hooks=[capture_tool_use])
                ]
            }
        )

        async with ClaudeSDKClient(options=options) as client:
            await client.query("Create a hello.py file with a greet function")
            async for message in client.receive_response():
                pass

        # Verify tool calls
        assert len(tool_calls) > 0
        write_calls = [c for c in tool_calls if c["tool"] == "Write"]
        assert len(write_calls) >= 1

        # Verify Aiki metadata
        result = subprocess.run(
            ["jj", "log", "-r", "@", "-T", "description"],
            cwd=test_repo,
            capture_output=True,
            text=True
        )
        assert "[aiki]" in result.stdout
```

---

## Test Configuration

### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `CLAUDE_INTEGRATION_TEST` | Enable real Claude Code tests | unset (disabled) |
| `AIKI_E2E_VERBOSE` | Enable verbose logging | unset |
| `AIKI_E2E_TIMEOUT` | Test timeout in seconds | 60 |

### Running Tests

```bash
# Run all E2E tests (with Claude Code)
CLAUDE_INTEGRATION_TEST=1 cargo test e2e_automation -- --nocapture

# Run specific scenario
CLAUDE_INTEGRATION_TEST=1 cargo test test_basic_edit_flow -- --nocapture

# Run with verbose output
CLAUDE_INTEGRATION_TEST=1 AIKI_E2E_VERBOSE=1 cargo test e2e_automation -- --nocapture

# Run SDK-based tests (Python)
cd cli/tests/e2e_automation_sdk
pytest -v
```

---

## CI/CD Integration

### GitHub Actions Workflow

```yaml
name: E2E Automation Tests

on:
  workflow_dispatch:  # Manual trigger only
  schedule:
    - cron: '0 6 * * 1'  # Weekly on Monday

jobs:
  e2e-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rust-lang/setup-rust-toolchain@v1

      - name: Install JJ
        run: cargo install jj-cli

      - name: Install Claude Code
        run: npm install -g @anthropic-ai/claude-code

      - name: Build Aiki
        run: cargo build --release

      - name: Run E2E Tests
        env:
          CLAUDE_INTEGRATION_TEST: 1
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: cargo test e2e_automation -- --nocapture
```

---

## Success Criteria

### Must Have
- [ ] Basic edit flow test passes
- [ ] Provenance metadata correctly recorded
- [ ] Session ID consistent across edits
- [ ] Test can run in automated mode (no prompts)

### Should Have
- [ ] Multi-file edit test
- [ ] Session lifecycle event verification
- [ ] Error recovery test
- [ ] Verbose logging for debugging

### Nice to Have
- [ ] SDK-based Python tests
- [ ] Multi-editor interleaving test
- [ ] Performance benchmarks
- [ ] Visual test report

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| API costs | Run infrequently, use simple prompts |
| Rate limits | Add retry logic with exponential backoff |
| Flaky tests | Use polling with timeouts, not sleep |
| Auth failures | Clear error messages, skip gracefully |
| Hook timing | Wait for metadata with timeout, not fixed delay |

---

## Next Steps

1. **Decide on approach** - CLI-only vs Hybrid (CLI + SDK)
2. **Create test infrastructure** - Helper functions, fixtures
3. **Implement basic test** - Single edit scenario
4. **Expand coverage** - Multi-file, session lifecycle
5. **Add CI integration** - GitHub Actions workflow
6. **Document usage** - README for running tests

---

## Appendix: Existing Test Patterns

The codebase already has `cli/tests/claude_integration_test.rs` which provides a good starting point. Key patterns to reuse:

1. **Conditional test execution** via `CLAUDE_INTEGRATION_TEST` env var
2. **Plugin setup** - Copy plugin directory, configure hooks
3. **Claude invocation** - `claude -p` with `--dangerously-skip-permissions`
4. **Metadata verification** - Parse JJ description for `[aiki]` blocks
5. **Timeout handling** - Wait with polling, not fixed sleep

See: `/home/user/aiki/cli/tests/README_CLAUDE_INTEGRATION.md`
