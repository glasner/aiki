# Two-Stage Code Review: Lint on Change + Review on Commit

This example demonstrates a **production-ready two-stage review workflow** for AI-generated code using Aiki flows.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    AI Agent (Claude Code, Cursor)           │
│                  Makes edits to source files                │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ↓ PostToolUse/afterFileEdit
┌─────────────────────────────────────────────────────────────┐
│              STAGE 1: Fast Linting (PostChange)            │
│              Flow: flow-lint-on-change.yaml                │
│                                                             │
│  • Triggers IMMEDIATELY after each AI edit                 │
│  • Runs lightweight linters (< 2 seconds)                  │
│  • NON-BLOCKING (errors logged, doesn't stop AI)           │
│  • Stores results in change metadata                       │
│                                                             │
│  Tools: ruff, eslint, clippy, golangci-lint                │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ↓ AI continues working...
┌─────────────────────────────────────────────────────────────┐
│                   Developer attempts commit                 │
│                   (git commit or jj commit)                 │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ↓ prepare-commit-msg hook
┌─────────────────────────────────────────────────────────────┐
│        STAGE 2: Comprehensive Review (PreCommit)           │
│           Flow: flow-review-on-precommit.yaml              │
│                                                             │
│  • Triggers BEFORE commit is finalized                     │
│  • Runs comprehensive checks (10-30 seconds)               │
│  • BLOCKING (failures prevent commit)                      │
│  • Reads Stage 1 lint results from metadata                │
│  • Adds multiple analysis layers                           │
│                                                             │
│  Checks:                                                    │
│    1. Read quick-lint results (from Stage 1)               │
│    2. Security analysis (Semgrep)                          │
│    3. Code complexity (radon, complexity metrics)          │
│    4. AI anti-patterns (hardcoded secrets, TODOs, etc.)    │
│    5. Test suite execution                                 │
│    6. Aggregate decision (block if critical issues)        │
│                                                             │
│  Blocking conditions:                                       │
│    • Security errors > 0                                    │
│    • Tests failed                                           │
│    • Total issues > 10                                      │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ↓ If review passes
┌─────────────────────────────────────────────────────────────┐
│                  Commit succeeds                            │
│                                                             │
│  Change description contains:                               │
│    [aiki] - AI attribution                                 │
│    [quick-lint] - Stage 1 results                          │
│    [comprehensive-review] - Stage 2 results                │
│    [security-scan] - Semgrep findings                      │
└─────────────────────────────────────────────────────────────┘
```

## Why Two Stages?

### Stage 1: Fast Feedback Loop (PostChange)
**Goal:** Give AI immediate feedback without blocking its workflow

- **Speed:** < 2 seconds per file
- **Scope:** Single file that was just edited
- **Tools:** Lightweight linters (ruff, eslint, clippy)
- **Behavior:** Non-blocking (warnings only)
- **Value:** AI can see issues immediately and potentially fix them in the next iteration

**Example:**
```bash
# AI edits auth.py
🐍 Running Ruff on src/auth.py...
⚠️  Ruff: Found 2 issue(s)
  [F401] Line 12: Module imported but unused
  [E501] Line 45: Line too long (88 > 80 characters)
  
# AI continues working, sees the warning, might fix it next edit
```

### Stage 2: Comprehensive Gate (PreCommit)
**Goal:** Ensure nothing broken/insecure makes it into Git history

- **Speed:** 10-30 seconds (full analysis)
- **Scope:** All changed files in the commit
- **Tools:** Semgrep, test suite, complexity analysis, AI pattern detection
- **Behavior:** BLOCKING (commit fails if critical issues)
- **Value:** Guaranteed quality before code enters permanent history

**Example:**
```bash
# Developer runs: git commit -m "Add authentication"

📊 COMPREHENSIVE REVIEW SUMMARY
=========================================

Quick Lint Issues:        2
Security Errors:          1
Security Warnings:        3
Complex Functions:        0
AI Anti-patterns:         1
Test Result:              PASS

🚫 BLOCKING: Security errors must be fixed
❌ COMMIT BLOCKED - Fix issues above before committing

# Commit is rejected, developer must fix security issue
```

## Installation

### 1. Install Dependencies

```bash
# Python linting
pip install ruff radon

# JavaScript linting (if applicable)
npm install -g eslint

# Rust linting (if applicable)
rustup component add clippy

# Security scanning
pip install semgrep

# Go linting (if applicable)
go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest
```

### 2. Copy Flows to Your Repository

```bash
# Create flows directory
mkdir -p .aiki/flows

# Copy the two flows
cp flow-lint-on-change.yaml .aiki/flows/
cp flow-review-on-precommit.yaml .aiki/flows/
```

### 3. Enable the Flows

```bash
# Both flows are enabled by default in their YAML
# Verify they're loaded:
aiki flows list

# Should show:
#   ✓ Quick Lint on AI Edit
#   ✓ Comprehensive Pre-Commit Review
```

### 4. Configure Environment Variables (Optional)

```bash
# For Slack notifications (optional)
export SLACK_WEBHOOK_URL="https://hooks.slack.com/services/YOUR/WEBHOOK"

# For security team alerts (optional)
export SECURITY_WEBHOOK_URL="https://your-security-api.com/webhook"
```

## Usage Example

### Scenario: AI Adds Authentication Feature

```bash
# 1. AI (Claude Code) edits auth.py
# → Stage 1 runs immediately:
🐍 Running Ruff on src/auth.py...
✅ Ruff: No issues found
✅ Quick lint complete: No issues found

# 2. AI edits config.py
# → Stage 1 runs again:
🐍 Running Ruff on src/config.py...
⚠️  Ruff: Found 1 issue(s)
  [F401] Line 5: Module imported but unused
⚠️  Quick lint complete: 1 issue(s) found (will be reviewed at commit)

# 3. Developer attempts commit
$ git commit -m "Add JWT authentication"

# → Stage 2 runs (comprehensive review):
📂 Collecting changed files...
Found 2 changed file(s)

📋 Checking quick lint results from PostChange...
Quick lint found 1 issue(s)

🔒 Running Semgrep security analysis...
Semgrep: 1 findings (1 errors, 0 warnings)

❌ CRITICAL SECURITY ISSUES FOUND:
  • src/auth.py:23 - Hardcoded JWT secret key

📊 Analyzing code complexity...
Found 0 overly complex function(s)

🤖 Checking for common AI coding mistakes...
⚠️  Found 1 TODO/FIXME comments (AI may have left incomplete work)

🧪 Running tests...
✅ Tests passed

========================================
📊 COMPREHENSIVE REVIEW SUMMARY
========================================

Quick Lint Issues:        1
Security Errors:          1
Security Warnings:        0
Complex Functions:        0
AI Anti-patterns:         1
Test Result:              PASS

🚫 BLOCKING: Security errors must be fixed
❌ COMMIT BLOCKED - Fix issues above before committing

# 4. Developer fixes the hardcoded secret
$ vim src/auth.py  # Move secret to environment variable

# 5. AI or developer removes the TODO
$ vim src/config.py  # Remove TODO comment

# 6. Try commit again
$ git commit -m "Add JWT authentication"

# → Stage 2 runs again:
========================================
📊 COMPREHENSIVE REVIEW SUMMARY
========================================

Quick Lint Issues:        0
Security Errors:          0
Security Warnings:        0
Complex Functions:        0
AI Anti-patterns:         0
Test Result:              PASS

✅ REVIEW PASSED - Commit approved
✅ Added comprehensive review metadata

[main abc123] Add JWT authentication
 2 files changed, 45 insertions(+), 2 deletions(-)
```

## Metadata Tracking

Both stages store results in the JJ change description:

```bash
$ jj log -r @ -v

Change abc123def456
Author: Developer <dev@example.com>
Date: 2025-01-15 14:32:10 UTC

Add JWT authentication

[aiki]
agent=claude-code
session=session-abc123
tool=Edit
confidence=High
method=Hook
[/aiki]

[quick-lint]
timestamp=2025-01-15T14:30:05Z
file=src/auth.py
agent=claude-code
issues=0
[/quick-lint]

[comprehensive-review]
timestamp=2025-01-15T14:32:10Z
lint_issues=0
security_errors=0
security_total=0
complexity_issues=0
ai_antipatterns=0
tests_passed=true
status=approved
[/comprehensive-review]
```

## Customization

### Adjust Blocking Thresholds

Edit `flow-review-on-precommit.yaml`:

```yaml
# Change from:
if [ "$TOTAL_ISSUES" -gt 10 ]; then
  echo "🚫 BLOCKING: Too many issues ($TOTAL_ISSUES > 10 threshold)"

# To your preferred threshold:
if [ "$TOTAL_ISSUES" -gt 5 ]; then
  echo "🚫 BLOCKING: Too many issues ($TOTAL_ISSUES > 5 threshold)"
```

### Add More Linters (Stage 1)

Add new conditionals in `flow-lint-on-change.yaml`:

```yaml
# Add PHP linting:
- name: "Lint PHP files"
  type: condition
  if: "{{edited_file}} ends_with '.php'"
  then:
    - name: "Run PHP_CodeSniffer"
      type: shell
      command: "phpcs {{edited_file}}"
      timeout: 3s
      continue_on_error: true
```

### Add Custom Security Rules (Stage 2)

Create custom Semgrep rules:

```bash
# Create custom rules directory
mkdir -p .semgrep/rules

# Add your custom rules
cat > .semgrep/rules/ai-security.yaml << EOF
rules:
  - id: ai-hardcoded-secret
    pattern: |
      $SECRET = "..."
    message: AI may have hardcoded a secret
    severity: ERROR
EOF

# Semgrep will auto-detect and use these rules
```

### Disable Stages Temporarily

```bash
# Disable quick linting (Stage 1)
aiki flows disable "Quick Lint on AI Edit"

# Disable comprehensive review (Stage 2)
aiki flows disable "Comprehensive Pre-Commit Review"

# Re-enable later
aiki flows enable "Quick Lint on AI Edit"
aiki flows enable "Comprehensive Pre-Commit Review"
```

## Performance Considerations

### Stage 1 (PostChange)
- **Target:** < 2 seconds
- **Optimization:** Only lint the single changed file
- **Caching:** Linters cache results between runs
- **Parallelism:** Each file linted independently

### Stage 2 (PreCommit)
- **Target:** < 30 seconds (acceptable for commit-time)
- **Optimization:** Only scan changed files, not entire codebase
- **Parallelism:** Run multiple tools concurrently (future improvement)
- **Skipping:** Skip if no code files changed

## Troubleshooting

### Stage 1 Linting Fails
```bash
# Check if linters are installed
which ruff
which eslint
which clippy

# Test linter manually
ruff check src/file.py

# View flow logs
aiki flows logs "Quick Lint on AI Edit"
```

### Stage 2 Blocks Valid Commits
```bash
# Check what blocked the commit
cat /tmp/aiki-semgrep-*.json | jq '.results'

# Temporarily disable the flow
aiki flows disable "Comprehensive Pre-Commit Review"

# Commit
git commit -m "..."

# Re-enable
aiki flows enable "Comprehensive Pre-Commit Review"
```

### Performance Too Slow
```bash
# Stage 1: Disable slow linters
# Edit flow-lint-on-change.yaml and comment out slow sections

# Stage 2: Skip tests if too slow
# Edit flow-review-on-precommit.yaml:
# Comment out the "Run test suite" action
```

## Best Practices

1. **Start permissive, tighten gradually**
   - Begin with warnings only (set `continue_on_error: true`)
   - Monitor false positive rate
   - Gradually add blocking conditions

2. **Customize for your codebase**
   - Different languages need different linters
   - Adjust complexity thresholds for your team's standards
   - Add project-specific security rules

3. **Provide escape hatches**
   - Allow `git commit --no-verify` to skip pre-commit hooks
   - Document when it's acceptable to skip review
   - Log all skipped reviews for audit

4. **Monitor and iterate**
   - Track how often commits are blocked
   - Gather developer feedback
   - Tune thresholds based on real usage

5. **Keep Stage 1 fast**
   - Never block AI workflow in Stage 1
   - Use fastest linters only
   - Single file only, never full codebase

## Related Documentation

- [Phase 5: Flows Implementation Plan](../phase-5.md)
- [Flow Definition Schema](../phase-5.md#flow-definition-schema)
- [Action Types Reference](../phase-5.md#action-types)
