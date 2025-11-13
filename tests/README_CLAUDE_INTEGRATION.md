# Claude Code Integration Test

This directory contains a **real integration test** that invokes Claude Code CLI to make actual edits and verifies that the Aiki provenance tracking works end-to-end.

## What This Test Does

Unlike the simulated tests, this test:

1. ✅ Sets up a real Git/JJ repository
2. ✅ Installs the Aiki plugin configuration
3. ✅ Creates a Python file
4. ✅ **Invokes real Claude Code CLI** with `-p` (print mode)
5. ✅ Asks Claude to edit the file (add a subtract function)
6. ✅ Verifies Claude actually modified the file
7. ✅ Checks that the PostToolUse hook triggered
8. ✅ Validates provenance was recorded in the database
9. ✅ Confirms attribution is accurate (ClaudeCode, High confidence)

## Prerequisites

### 1. Install Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. Authenticate with Claude

You need an active **Claude Pro** or **Claude Max** subscription:

```bash
claude
# Follow authentication prompts
```

### 3. Build Aiki

```bash
cd /Users/glasner/code/aiki/cli
cargo build --release
```

## Running the Test

### Enable Integration Test Mode

The test is **disabled by default** to avoid requiring Claude Code for normal CI/CD. Enable it with an environment variable:

```bash
CLAUDE_INTEGRATION_TEST=1 cargo test test_real_claude_code_integration -- --nocapture
```

### What You'll See

```
🧪 Starting real Claude Code integration test
📁 Test directory: /var/folders/.../tmp.XXXXXX
✓ Git repository initialized
✓ Plugin directory copied
✓ aiki init completed
✓ Initial file created: calculator.py
🤖 Invoking Claude Code to edit calculator.py...
✓ Claude Code executed successfully
📝 Edited file content:
# Calculator module

def add(a, b):
    return a + b

def subtract(a, b):
    return a - b

✓ File contains 'subtract' function
✓ Provenance database exists
✓ Found 1 provenance record(s)
📊 Provenance record:
   File: /var/folders/.../calculator.py
   Agent: ClaudeCode
   Tool: Edit
   Confidence: High

✅ Real Claude Code integration test passed!
   ✓ Claude Code CLI invoked successfully
   ✓ File was edited by Claude
   ✓ PostToolUse hook triggered
   ✓ Provenance recorded in database
   ✓ Attribution is 100% accurate
```

## How It Works

### Claude Code Non-Interactive Mode

The test uses `claude -p` (print mode) to run Claude without the interactive TUI:

```bash
claude -p "Add a subtract function to calculator.py" \
  --output-format json \
  --dangerously-skip-permissions
```

**Flags:**
- `-p` / `--print`: Non-interactive mode (exits after completion)
- `--output-format json`: Structured output for parsing
- `--dangerously-skip-permissions`: Auto-accept edits (for testing only!)

### Plugin Installation

The test copies `claude-code-plugin/` to the test directory and runs `aiki init`, which creates `.claude/settings.json`:

```json
{
  "extraKnownMarketplaces": {
    "aiki": {
      "source": {
        "source": "directory",
        "path": "./claude-code-plugin"
      }
    }
  },
  "enabledPlugins": {
    "aiki@aiki": true
  }
}
```

This tells Claude Code to load the Aiki plugin, which registers the PostToolUse hook.

### Hook Execution Flow

1. Claude Code edits `calculator.py` using the `Edit` tool
2. PostToolUse hook triggers: `aiki record-change --claude-code`
3. `record-change` captures:
   - Working copy state (via JJ snapshot)
   - File path
   - Changes (old/new content)
   - Session ID
   - Timestamp
4. Writes provenance to `.aiki/provenance/attribution.db`
5. Links to JJ commit via `jj describe`

## Troubleshooting

### Test Skipped

If you see: `Skipping test: Set CLAUDE_INTEGRATION_TEST=1 to enable`

**Solution:** Run with the environment variable:
```bash
CLAUDE_INTEGRATION_TEST=1 cargo test test_real_claude_code_integration -- --nocapture
```

### Claude Code Not Installed

If you see: `Skipping test: Claude Code CLI not installed`

**Solution:** Install Claude Code:
```bash
npm install -g @anthropic-ai/claude-code
claude --version  # Verify installation
```

### Hook Not Triggered

If database exists but has no records:

1. Check `.claude/settings.json` was created correctly
2. Verify plugin files exist in `claude-code-plugin/`
3. Claude Code may need to be restarted to load the plugin
4. Check that Claude actually used the `Edit` or `Write` tool

### Claude Code Authentication Failed

If Claude can't authenticate:

```bash
claude logout
claude  # Re-authenticate interactively
```

You need an active Claude Pro or Max subscription.

## Running in CI/CD

This test is **not suitable for CI/CD** because it requires:
- Claude Code CLI installed
- Active Claude subscription
- Authentication credentials

For CI/CD, use the simulated tests instead:
```bash
cargo test  # Runs all tests except Claude integration
```

## Use Cases

This test is valuable for:

1. **Manual verification** - Confirm the plugin works with real Claude Code
2. **Pre-release testing** - Validate before publishing
3. **Debugging hook issues** - See actual Claude Code behavior
4. **Demo purposes** - Show the system working end-to-end

## Cost Considerations

Each test run makes an actual API call to Claude, which:
- Costs ~$0.01-0.05 depending on the edit complexity
- Counts toward your API rate limits
- Requires active subscription

Run sparingly and intentionally!

## Next Steps

After this test passes, you know:
- ✅ The plugin loads correctly in Claude Code
- ✅ PostToolUse hooks trigger on Edit/Write
- ✅ `aiki record-change` handles real hook payloads
- ✅ Provenance tracking is 100% accurate
- ✅ The entire system works end-to-end

**You're ready for real-world use!** 🎉
