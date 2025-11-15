# Phase 5: Internal Flow Engine

## Status: Design Complete

This document defines the flow system for Phase 5. The primary goal is to build enough flow infrastructure to:
1. **Refactor existing Aiki functionality** into system flows (provenance, session tracking, JJ integration)
2. **Ship built-in flows** like autonomous review as defaults users can customize
3. **Enable Phase 6** (Autonomous Review Flow)

**Phase 5 scope:** Internal flows only (system + default). User-defined flows (Phase 7) and external flow ecosystem (Phase 8) come later.

**What's NOT in Phase 5:**
- ❌ User-defined flows in `.aiki/flows/` (Phase 7)
- ❌ Flow composition with `includes:` (Phase 7)
- ❌ Bundled binaries and `bin/<platform>/` (Phase 8)
- ❌ Lazy loading and flow caching (Phase 8)
- ❌ `aiki flows install/cleanup` commands (Phase 8)
- ❌ External flow ecosystem (Phase 8)

---

## Flow Types

### 1. System Flows (Mandatory)

Internal flows that power Aiki's core functionality. These **always run** and cannot be disabled.

**Examples:**
- `aiki/system/provenance` - Embed `[aiki]` metadata in JJ change descriptions
- `aiki/system/session-tracking` - Track agent sessions (Start/Stop events)
- `aiki/system/jj-integration` - JJ commands executed during events

**Characteristics:**
- Built into Aiki binary (not YAML files)
- Run before any user flows
- Cannot be disabled or customized
- Power core Aiki functionality

**Example (internal representation):**
```yaml
# aiki/system/provenance (internal, not user-facing)
PostChange:
  - jj: ["describe", "--no-edit", "-m", "[aiki]\\nagent=$agent\\nsession=$session_id\\n[/aiki]"]
    on_failure: block
```

### 2. Default Flows (Optional)

Built-in flows shipped with Aiki that users can include, customize, or skip entirely.

**Examples:**
- `aiki/autonomous-review` - Pre-commit quality gates
- `aiki/quick-lint` - Fast linting on PostChange
- `aiki/security-scan` - Security scanning

**Characteristics:**
- Shipped with Aiki but optional
- Users opt-in via `.aiki/flow.yaml`
- Can be overridden or disabled
- Demonstrate flow patterns

**Example (user-facing):**
```yaml
# .aiki/flow.yaml
name: My Workflow
version: 1

PreCommit:
  - flow: aiki/autonomous-review   # Include default flow
  - shell: pytest --fast             # Add custom steps
    on_failure: block
```

---

## Refactoring Existing Functionality

A key goal of Phase 5 is to refactor existing Aiki functionality from hardcoded Rust into declarative system flows.

### Current Rust Implementation → System Flows

| Current Functionality | Becomes System Flow | Event |
|----------------------|---------------------|-------|
| Provenance embedding (hooks.rs) | `aiki/system/provenance` | PostChange |
| Session tracking | `aiki/system/session-tracking` | Start, Stop |
| JJ change description updates | `aiki/system/jj-integration` | PostChange |
| Agent detection | `aiki/system/agent-detection` | PostChange |

### Example: Provenance Embedding

**Before (Rust code in hooks.rs):**
```rust
// Hardcoded in Rust
fn post_change_hook(event: &AikiEvent) -> Result<()> {
    let metadata = format!(
        "[aiki]\nagent={}\nsession={}\ntool={}\n[/aiki]",
        event.agent, event.session_id, event.tool_name
    );
    
    let mut cmd = Command::new("jj");
    cmd.args(&["describe", "--no-edit", "-m", &metadata]);
    cmd.status()?;
    Ok(())
}
```

**After (System flow in flow engine):**
```yaml
# aiki/system/provenance (built into Aiki)
PostChange:
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

### Benefits of Refactoring

1. **Declarative core** - Aiki's behavior defined in YAML, not scattered across Rust
2. **Easier testing** - Test flows, not Rust code
3. **Dog-fooding** - Aiki uses its own flow system
4. **Cleaner architecture** - Rust handles execution, flows define behavior
5. **Future extensibility** - Adding new system behaviors is just YAML

### What Stays in Rust

- Flow execution engine
- Event detection and triggering
- File system operations
- Platform-specific logic
- Performance-critical paths

---

## Core Principles

1. **System flows first** - Refactor existing Rust code into declarative flows
2. **Dog-food our own system** - Aiki uses flows internally
3. **Built-in only** - Phase 5 has no external flows or registries
4. **Single flow file** - Users edit `.aiki/flow.yaml` only
5. **Sequential by default** - Actions run in order
6. **Explicit parallelism** - Use `parallel:` when needed
7. **Simple conditionals** - `if/then/else` for all conditional logic

---

## Flow Structure

```yaml
# .aiki/flows/company/ai-review.yaml
name: "AI Code Review Pipeline"
description: "Human-readable description"
version: 1

# Optional: Flow-level variables (for DRY)
variables:
  webhook: "${SLACK_WEBHOOK_URL}"
  channel: "#ai-activity"

# Event types as top-level keys
PostChange:
  - flow: aiki/quick-lint
  - shell: echo 'Linting complete'

PreCommit:
  - flow: aiki/quick-lint
  - parallel:
      - flow: aiki/security-scan
      - flow: aiki/complexity-check
  - shell: pytest
    on_failure: block

Start:
  - shell: aiki init --quiet

Stop:
  - log: Session ended
```

**Key points:**
- Each event type (PostChange, PreCommit, Start, Stop) is a top-level array
- Optional `variables:` for reusable values across the flow
- Actions are directly in the event arrays

---

## Namespace Convention

### Directory Structure

```
.aiki/flows/                    # Repository-level
├── aiki/                       # Built-in Aiki flows
│   ├── quick-lint.yaml         # aiki/quick-lint
│   ├── security-scan.yaml      # aiki/security-scan
│   └── linters/
│       └── python.yaml         # aiki/linters/python
├── company/                    # Organization flows
│   ├── slack-notify.yaml       # company/slack-notify
│   └── jira-update.yaml        # company/jira-update

~/.aiki/flows/                  # User-level
└── my/                         # Personal flows
    ├── desktop-alert.yaml      # my/desktop-alert
    └── backup.yaml             # my/backup
```

### Resolution Rules

1. Repository flows (`.aiki/flows/`) checked first
2. User flows (`~/.aiki/flows/`) checked second
3. Repository can override built-in `aiki/*` flows

### Addressing Examples

```yaml
PostChange:
  - flow: aiki/quick-lint           # Built-in
  - flow: company/slack-notify      # Organization
  - flow: my/desktop-alert          # Personal
  - flow: aiki/linters/python       # Hierarchical
```

---

## Flow Composition

Flows can reference other flows, enabling powerful composition patterns. There are two levels of composition:

### Event-Level Composition (Default)

When you reference a flow, you invoke **only the event handler for the current event**:

```yaml
# company/ai-review.yaml
PostChange:
  - flow: aiki/quick-lint        # Calls ONLY aiki/quick-lint's PostChange handler
  - flow: my/desktop-alert        # Calls ONLY my/desktop-alert's PostChange handler

PreCommit:
  - flow: aiki/quick-lint        # Calls ONLY aiki/quick-lint's PreCommit handler
  - flow: aiki/security-scan      # Calls ONLY aiki/security-scan's PreCommit handler
```

**How it works:**
- In a `PostChange` handler, `flow: aiki/quick-lint` executes `aiki/quick-lint.yaml`'s `PostChange` section
- In a `PreCommit` handler, `flow: aiki/quick-lint` executes `aiki/quick-lint.yaml`'s `PreCommit` section
- If the referenced flow doesn't define that event, it's skipped (no error)

**Example:**

```yaml
# aiki/quick-lint.yaml
PostChange:
  - shell: ruff check $event.file_path

PreCommit:
  - shell: ruff check . --strict
  - shell: mypy .

# company/review.yaml
PostChange:
  - flow: aiki/quick-lint        # Runs: ruff check $event.file_path

PreCommit:
  - flow: aiki/quick-lint        # Runs: ruff check . --strict; mypy .
```

### Flow-Level Composition (Includes)

To include **all event handlers** from other flows, use the `includes:` keyword:

```yaml
# company/ai-review.yaml
name: "Extended Review"
includes:
  - aiki/quick-lint               # Import ALL event handlers from aiki/quick-lint
  - company/slack-notify          # Import ALL event handlers from company/slack-notify

# Add additional handlers
PostChange:
  - flow: my/desktop-alert        # Runs AFTER included PostChange handlers

PreCommit:
  - flow: aiki/security-scan      # Runs AFTER included PreCommit handlers
  - shell: pytest
```

**Execution order with `includes:`**
1. Run all included flows' event handlers (in order)
2. Then run this flow's event handlers

**Single include example:**

```yaml
# aiki/quick-lint.yaml
PostChange:
  - shell: ruff check $event.file_path

PreCommit:
  - shell: ruff check .

# company/review.yaml
includes:
  - aiki/quick-lint

PostChange:
  - flow: company/slack-notify

# Effective PostChange handler:
# 1. shell: ruff check $event.file_path    (from aiki/quick-lint)
# 2. flow: company/slack-notify                     (from company/review)
```

**Multiple includes example:**

```yaml
# company/full-review.yaml
includes:
  - aiki/quick-lint               # Brings in linting for PostChange and PreCommit
  - aiki/security-scan            # Brings in security scanning for PreCommit
  - company/slack-notify          # Brings in notifications for PostChange and PreCommit

# Effective PostChange handler:
# 1. aiki/quick-lint's PostChange steps
# 2. company/slack-notify's PostChange steps
# 3. (this flow's PostChange steps, if any)

# Effective PreCommit handler:
# 1. aiki/quick-lint's PreCommit steps
# 2. aiki/security-scan's PreCommit steps
# 3. company/slack-notify's PreCommit steps
# 4. (this flow's PreCommit steps, if any)
```

### Positioning Steps Within Includes

Use `before:` and `after:` to insert custom steps between included flows:

```yaml
includes:
  - aiki/quick-lint
  - company/slack-notify

PostChange:
  - before: aiki/quick-lint
    shell: runs before lint
  
  - after: aiki/quick-lint
    shell: runs between lint and slack
  
  - shell: runs at end (after all includes)
```

**Execution order:**
1. `shell: runs before lint` (before: aiki/quick-lint)
2. aiki/quick-lint's PostChange
3. `shell: runs between lint and slack` (after: aiki/quick-lint)
4. company/slack-notify's PostChange
5. `shell: runs at end` (no positioning)

**Key points:**
- `before: flow-name` - Insert step before the specified included flow
- `after: flow-name` - Insert step after the specified included flow
- No positioning - Runs after all included flows (default append behavior)
- Works with any action type (shell, flow, http, etc.)

**Complex example:**

```yaml
includes:
  - aiki/quick-lint
  - aiki/security-scan
  - company/slack-notify

PreCommit:
  # Setup before any checks
  - before: aiki/quick-lint
    shell: echo 'Starting pre-commit checks'
  
  # Custom check between lint and security
  - after: aiki/quick-lint
    shell: custom-validator --strict
  
  # Conditional flow between security and slack
  - after: aiki/security-scan
    when: $aiki_security_scan.failed
    flow: my/security-alert
  
  # Final steps after all includes
  - shell: pytest
    on_failure: block
  - flow: my/desktop-alert

# Execution order:
# 1. shell: echo 'Starting...'
# 2. aiki/quick-lint's PreCommit
# 3. shell: custom-validator
# 4. aiki/security-scan's PreCommit
# 5. flow: my/security-alert (conditional)
# 6. company/slack-notify's PreCommit
# 7. shell: pytest
# 8. flow: my/desktop-alert
```

### Why Two Levels?

**Event-level composition** (default):
- ✅ Explicit: You see exactly what runs in each event
- ✅ Flexible: Mix and match different flows per event
- ✅ No surprises: Clear execution order

**Flow-level composition** (`includes:`):
- ✅ DRY: Build on existing flows without repeating
- ✅ Multiple inheritance: Combine multiple flows together
- ✅ Organization standards: Enforce base checks, add custom steps

**When to use each:**

Use **event-level** (default):
```yaml
PostChange:
  - flow: aiki/quick-lint         # Just the linter
  - flow: my/backup               # Just the backup
```

Use **flow-level** (`includes:`):
```yaml
# Import all handlers from multiple flows
includes:
  - aiki/quick-lint
  - company/slack-notify

PreCommit:
  - flow: aiki/security-scan      # Additional check after included flows
```

---

## Action Types (Simplified Syntax)

### 1. Flow Reference

```yaml
- flow: aiki/quick-lint
  timeout: 10s
  continue_on_error: true
```

**Behavior:** Executes only the referenced flow's handler for the **current event type**.

### 2. Shell Command

```yaml
- shell: ruff check .
  working_dir: $cwd
  env:
    CUSTOM_VAR: value
    AGENT: $agent
  timeout: 30s
  on_failure: block   # Options: continue (default), block
```

### 3. HTTP Request

```yaml
- http:
    url: https://api.example.com/webhook
    method: POST
    headers:
      Authorization: Bearer $API_TOKEN
    body:
      event: $event_type
  timeout: 10s
```

### 4. JJ Command

```yaml
- jj: ["describe", "-m", "reviewed"]
  working_dir: $cwd
```

### 5. Parallel Execution

```yaml
- parallel:
    - flow: aiki/security-scan
    - flow: aiki/complexity-check
    - shell: npm audit
```

### 6. Log Message

```yaml
- log: Flow completed successfully
```

### 7. Sleep/Wait

```yaml
- sleep: 2s
```

---

## Conditionals

Use `if/then` to conditionally execute steps. The `else` block is optional.

### Single Action

```yaml
PostChange:
  # Only run on Python files
  - if: $event.file_path ends_with '.py'
    then:
      - shell: ruff check $event.file_path
  
  # Only notify on failures
  - if: $previous_step.failed
    then:
      - flow: my/desktop-alert
```

### Multiple Actions (Branching)

```yaml
PreCommit:
  - flow: aiki/security-scan
    alias: security
  
  # Run different steps based on condition
  - if: $security.failed
    then:
      - flow: company/slack-alert
      - http:
          url: $SECURITY_WEBHOOK
          body:
            severity: critical
      - shell: exit 1
        on_failure: block
    else:
      - log: Security scan passed
      - flow: company/slack-notify
```

---

## Step References

### Default: Full Flow Identifier

Steps are automatically referenceable using their full flow identifier (with `/` and `-` converted to `_`):

```yaml
PreCommit:
  - flow: aiki/security-scan
    # Auto-referenceable as: $aiki_security_scan
  
  - flow: aiki/complexity-check
    # Auto-referenceable as: $aiki_complexity_check
  
  # Reference using full identifier
  - if: "$aiki_security_scan.failed OR $aiki_complexity_check.failed"
    then:
      - flow: company/slack-alert
```

**Conversion rules:**
```
Flow identifier               → Variable name
aiki/security-scan           → $aiki_security_scan
company/slack-notify         → $company_slack_notify
my/backup                    → $my_backup
aiki/linters/python          → $aiki_linters_python
```

### Optional: Aliases for Shorter References

Add optional `alias:` for shorter variable names:

```yaml
PreCommit:
  - flow: aiki/security-scan
    alias: security    # Creates $security.* instead of $aiki_security_scan.*
  
  - flow: aiki/complexity-check
    alias: complexity
  
  # Reference using aliases (shorter)
  - if: "$security.failed OR $complexity.failed"
    then:
      - flow: company/slack-alert
```

**When to use aliases:**
- Long flow names that you reference frequently
- Improved readability in complex conditionals
- Personal preference

**When NOT to use aliases:**
- Short flow names (`my/backup` → `$my_backup` is already short)
- Only referenced once
- When clarity is more important than brevity

### Available Step Variables

For any step (using full identifier or alias):
```yaml
$identifier.result        # "success" | "failed"
$identifier.exit_code     # 0 | 1 | 2...
$identifier.output        # stdout
$identifier.duration_ms   # execution time in milliseconds
$identifier.failed        # boolean: true if exit_code != 0
```

For the immediately previous step:
```yaml
$previous_step.result
$previous_step.exit_code
$previous_step.output
$previous_step.failed
```

---

## Variable Interpolation

Aiki uses `$variable` syntax for interpolation (similar to shell variables).

**Naming convention:**
- `$lowercase` or `$snake_case` = flow/event variables
- `$ALL_CAPS` or `$SCREAMING_SNAKE_CASE` = environment variables


```yaml
# Single reference - no quotes needed in simple cases
url: $webhook
channel: $channel

# Environment variables (all caps)
url: $SLACK_WEBHOOK_URL
token: $API_TOKEN

# Multiple words/operators - no quotes needed
when: $event.file_path ends_with '.py'
if: $security.failed OR $complexity.failed

# In shell commands - variables expand as expected
shell: echo $agent edited $event.file_path
```

### Flow Variables

Define reusable variables at the flow level:

```yaml
variables:
  webhook: $SLACK_WEBHOOK_URL    # Reference env var (all caps)
  channel: "#ai-activity"         # String literal
  threshold: "10"

PostChange:
  - http:
      url: $webhook               # Flow variable
      body:
        channel: $channel
```

**Benefits:**
- DRY: Define once, use everywhere
- Can reference environment variables: `$SLACK_WEBHOOK_URL`
- Can interpolate event data: `"$agent-$timestamp"`

### Event Variables

```yaml
$event_type             # PostChange, PreCommit, Start, Stop
$agent                  # claude-code, cursor, windsurf
$session_id             # Agent session ID
$cwd                    # Working directory
$timestamp              # ISO 8601 timestamp
```

### Event Context Variables

```yaml
$event.file_path     # Path to edited file
$event.tool_name     # Edit, Write, etc.
$event.change_id     # JJ change ID
$event.*             # Any custom metadata field
```

### Environment Variables

Environment variables use ALL_CAPS naming:

```yaml
$SLACK_WEBHOOK_URL      # From shell environment
$API_TOKEN
$SECURITY_WEBHOOK
$HOME                   # Standard shell variables work too
```

**No special syntax needed** - just use `$ALL_CAPS` and Aiki will look it up in the environment.

---

## Execution Model

### Sequential by Default

```yaml
PostChange:
  - flow: aiki/quick-lint     # Step 1
  - shell: echo 'Done'        # Step 2 (waits for step 1)
  - log: Complete             # Step 3 (waits for step 2)
```

Execution order: 1 → 2 → 3

### Explicit Parallelism

```yaml
PreCommit:
  - flow: aiki/quick-lint     # Step 1
  - parallel:                  # Step 2 (waits for step 1)
      - flow: aiki/security-scan
      - flow: aiki/complexity-check
      - shell: npm audit
  - shell: pytest             # Step 3 (waits for ALL parallel to complete)
    on_failure: block
```

Execution order:
1. `quick-lint`
2. `[security-scan || complexity-check || npm audit]` (parallel)
3. `pytest` (after all parallel complete)

### DAG Execution

```
     lint
    /    \
security  complexity
    \    /
     tests
```

```yaml
PreCommit:
  - flow: aiki/quick-lint
  - parallel:
      - flow: aiki/security-scan
      - flow: aiki/complexity-check
  - shell: pytest
```

The `parallel:` creates the fan-out/fan-in pattern.

---

## Complete Example

```yaml
# .aiki/flows/company/ai-review.yaml
name: "AI Code Review Pipeline"
description: "Two-stage review: fast lint on edit, full review on commit"
version: 1

# Flow variables for DRY
variables:
  slack_webhook: "${SLACK_WEBHOOK_URL}"
  security_webhook: "${SECURITY_WEBHOOK_URL}"
  backup_dir: "~/backups"

PostChange:
  - flow: aiki/quick-lint
    alias: lint
  
  # Only notify on failures
  - if: $lint.failed
    then:
      - flow: my/desktop-alert
  
  # Only backup Python files
  - if: $event.file_path ends_with '.py'
    then:
      - shell: cp $event.file_path $backup_dir/

PreCommit:
  - flow: aiki/quick-lint
  
  # Run security and complexity in parallel
  - parallel:
      - flow: aiki/security-scan
        alias: security
      - flow: aiki/complexity-check
        alias: complexity
  
  # Complex branching based on results
  - if: $security.failed OR $complexity.failed
    then:
      - flow: company/slack-alert
      - http:
          url: $security_webhook
          method: POST
          body:
            severity: high
            change_id: $event.change_id
            security_failed: $security.failed
            complexity_failed: $complexity.failed
      - shell: exit 1
        on_failure: block
    else:
      - log: Static analysis passed
  
  - shell: pytest
    alias: tests
    timeout: 60s
  
  # Notify on test failures
  - if: $tests.failed
    then:
      - flow: company/slack-alert
  
  # Block conditional for final status
  - if: $tests.exit_code == 0
    then:
      - http:
          url: $slack_webhook
          body:
            text: ✅ All checks passed for $change_id
      - log: Review complete: SUCCESS
    else:
      - http:
          url: $slack_webhook
          body:
            text: ❌ Tests failed for $change_id
      - shell: exit 1
        on_failure: block

Start:
  - shell: aiki init --quiet
  - if: $event.first_session == 'true'
    then:
      - log: Welcome to Aiki!

Stop:
  # Backup based on session length
  - if: $session.duration_minutes > 30
    then:
      - flow: my/backup
      - log: Long session - backup created
    else:
      - log: Short session - no backup needed
```

---

## CLI Commands

### System Health

```bash
# Check Aiki installation and configuration
$ aiki doctor
Aiki Installation Health Check
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Environment:
  ✓ JJ repository detected
  ✓ ~/.aiki directory exists
  ✓ ~/.aiki/bin in PATH
  ✓ Shell: zsh

Flows:
  ✓ 3 flows installed
  ✓ All flow binaries accessible
  ⚠ 1 flow has missing external dependencies:
    - vendor/container-scanner requires docker (>= 20.10)
      Install: https://docs.docker.com/get-docker/

Binaries:
  ✓ semgrep (vendor/security-scan)
  ✓ trivy (vendor/container-scanner)

Configuration:
  ✓ .aiki/flow.yaml exists
  ✓ Flow syntax valid

# Fix common issues
$ aiki doctor --fix
✓ Added ~/.aiki/bin to PATH in ~/.zshrc
✓ Recreated missing symlinks in ~/.aiki/bin
```

### Flow Management

```bash
# List installed flows
$ aiki flows list
Built-in flows:
  aiki/quick-lint
  aiki/security-scan
  aiki/complexity-check

Installed flows:
  vendor/security-scan v1.2.0 (2 days ago)
  company/custom-scanner v2.0.1 (5 hours ago)

Repository flows (.aiki/flows):
  my/desktop-alert
  my/backup

# Install/upgrade all flows from .aiki/flow.yaml
$ aiki flows install
==> Reading .aiki/flow.yaml
==> Installing 3 flows
==> aiki/quick-lint (built-in)
✓ aiki/quick-lint
==> Downloading vendor/security-scan
==> Upgrading vendor/security-scan v1.2.0 → v1.3.0
==> Creating symlinks in ~/.aiki/bin
  - semgrep
✓ vendor/security-scan v1.3.0
==> company/custom-scanner v2.0.1 already installed
✓ company/custom-scanner v2.0.1

# Remove unused flows from cache
$ aiki flows cleanup
==> This will remove:
  old/unused-flow (120 MB)
  vendor/deprecated (45 MB)
==> 165 MB will be freed

Proceed? (y/N) y
==> Removing old/unused-flow
==> Removing vendor/deprecated
✓ 165 MB freed
```

### Flow Creation

```bash
# Interactive creation
aiki flows create company/my-flow
# Creates: .aiki/flows/company/my-flow.yaml

# Manual creation
mkdir -p .aiki/flows/company
cat > .aiki/flows/company/my-flow.yaml << 'EOF'
name: "My Custom Flow"
version: 1

PostChange:
  - shell: "echo 'Hello from $agent'"
EOF
```

### Default Workflow

Aiki ships with a default workflow that users can customize:

```yaml
# .aiki/flow.yaml (created by `aiki init`)
name: "Aiki Default Workflow"
description: "Standard AI code review workflow - customize to your needs"
version: 1

includes:
  - aiki/quick-lint
  - aiki/security-scan

PostChange:
  # Quick feedback after each edit
  # (inherited from aiki/quick-lint)
  
  # Add your custom PostChange steps here:
  # - flow: my/desktop-alert
  # - when: $event.file_path ends_with '.py'
  #   shell: cp $event.file_path ~/backups/

PreCommit:
  # Full review before commit
  # (inherited from aiki/quick-lint and aiki/security-scan)
  
  # Add your custom PreCommit steps here:
  # - shell: pytest
  #   on_failure: block
  # - flow: company/slack-notify
```

**First-run experience:**

```bash
$ aiki init
✓ Created .aiki/flow.yaml with default workflow
✓ Installed aiki/quick-lint
✓ Installed aiki/security-scan
✓ Created ~/.aiki/bin directory
✓ Added ~/.aiki/bin to PATH in ~/.zshrc

Restart your shell or run: source ~/.zshrc

Edit .aiki/flow.yaml to customize your workflow.
```

**Shell configuration:**

`aiki init` automatically adds `~/.aiki/bin` to your PATH by appending to your shell config:

```bash
# ~/.zshrc (or ~/.bashrc)
export PATH="$HOME/.aiki/bin:$PATH"
```

**Benefits:**
- Works out of the box with sensible defaults
- Shows the `includes:` pattern immediately
- Has helpful comments showing where to customize
- Users learn by modifying real examples
- No magic config - just edit the flow file
- Can see exactly what's running

**User customization workflow:**

1. `aiki init` creates `.aiki/flow.yaml` with defaults
2. User edits `.aiki/flow.yaml` to add custom steps
3. User adds organization flows: `cp company-flows/*.yaml .aiki/flows/company/`
4. User adds them to includes or as steps
5. Changes are version-controlled with the repo

---

## Bundled Binaries

Flows can bundle platform-specific binaries to work out of the box without requiring users to install dependencies.

### Directory Structure

```
~/.aiki/flows/vendor/security-scan/
├── flow.yaml
└── bin/
    ├── darwin-arm64/
    │   └── semgrep          # macOS Apple Silicon
    ├── darwin-x86_64/
    │   └── semgrep          # macOS Intel
    ├── linux-x86_64/
    │   └── semgrep          # Linux
    └── windows-x86_64/
        └── semgrep.exe      # Windows
```

**Convention over configuration:** The directory structure IS the declaration. No need to list bundled tools in YAML.

### How It Works

**Single Aiki bin directory:**
```
~/.aiki/bin/              # User adds this ONCE to PATH
├── semgrep -> ../flows/vendor/security-scan/bin/darwin-arm64/semgrep
├── trivy -> ../flows/vendor/container-scan/bin/darwin-arm64/trivy
└── custom-tool -> ../flows/company/custom/bin/darwin-arm64/custom-tool
```

At install time, Aiki automatically:
1. Detects current platform (e.g., `darwin-arm64`)
2. Creates symlinks in `~/.aiki/bin/` pointing to flow binaries
3. User adds `~/.aiki/bin` to PATH once (during `aiki init`)

**Environment variables provided:**
```bash
PATH=~/.aiki/bin:$PATH               # User sets this once
FLOW_ROOT=/Users/me/.aiki/flows/vendor/security-scan
FLOW_BIN=$FLOW_ROOT/bin/darwin-arm64 # Platform-specific bin dir (if needed)
```

### Using Bundled Binaries

Flows can reference bundled binaries in three ways:

```yaml
name: Security Scan
version: 1.0.0

PostChange:
  # Option 1: Direct command (auto-discovered via PATH)
  - shell: semgrep --config=auto $event.file_path
  
  # Option 2: Explicit via FLOW_ROOT
  - shell: $FLOW_ROOT/bin/darwin-arm64/semgrep --config=auto
  
  # Option 3: Platform-agnostic via FLOW_BIN
  - shell: $FLOW_BIN/semgrep --config=auto
```

**Recommended:** Use Option 1 (direct command) - it's simplest and works across platforms.

### External Dependencies

For tools that can't be bundled (like Docker), declare them in `requires:`:

```yaml
name: Container Scanner
version: 1.0.0

# Declare external dependencies (must be on user's PATH)
requires:
  docker: ">=20.10"
  git: ">=2.0"

PostChange:
  - shell: docker build -t temp .
  - shell: trivy image temp    # trivy is bundled in bin/
```

**On install:**
```bash
$ aiki flows install
==> vendor/container-scanner
✓ Installed vendor/container-scanner v1.0.0
✓ Found bundled tools for darwin-arm64:
  - trivy
⚠ Missing external dependency: docker (>= 20.10)
  Install docker to use this flow
```

### Lazy Loading

Flows are automatically downloaded and cached on first use:

```yaml
# .aiki/flow.yaml
includes:
  - aiki/quick-lint
  - vendor/security-scan

PostChange:
  - flow: aiki/quick-lint
```

**First time a flow runs:**

```
PostChange event triggered...
Loading flow: aiki/quick-lint... ✓
Loading flow: vendor/security-scan...
  ⚠ Not found locally, downloading... ✓ v1.2.0
  Creating symlinks in ~/.aiki/bin... ✓ semgrep
Running PostChange handlers...
  → aiki/quick-lint ✓
  → vendor/security-scan ✓
```

**Subsequent runs (cached):**

```
PostChange event triggered...
Loading flows... ✓ (all cached)
Running PostChange handlers...
  → aiki/quick-lint ✓
  → vendor/security-scan ✓
```

**Download failures:**

If a flow can't be downloaded (network issues, registry down):

```
PostChange event triggered...
Loading flow: vendor/security-scan...
  ✗ Not found locally
  ✗ Failed to download: Network unreachable
  
⚠ Skipping vendor/security-scan (unavailable)
Running PostChange handlers...
  → aiki/quick-lint ✓
```

**Binary conflicts:**

If multiple flows bundle the same binary name:

```
Loading flow: company/alternative-scanner...
  ⚠ Binary conflict: semgrep
    Current: vendor/security-scan/bin/darwin-arm64/semgrep
    New:     company/alternative-scanner/bin/darwin-arm64/semgrep
  
  Keeping current symlink (vendor/security-scan)
  Flow will use $FLOW_BIN/semgrep to access its own binary
```

### Distribution

Flows are distributed as tarballs containing the complete directory structure:

```bash
# Create flow bundle
$ tar -czf vendor-security-scan-v1.0.0.tar.gz vendor/security-scan/

# Install from tarball
$ aiki flows install vendor-security-scan-v1.0.0.tar.gz
```

**Platform-specific bundles** (smaller downloads):
```bash
vendor-security-scan-v1.0.0-darwin-arm64.tar.gz   # Only macOS ARM binaries
vendor-security-scan-v1.0.0-linux-x86_64.tar.gz   # Only Linux binaries
```

**Universal bundle** (works everywhere):
```bash
vendor-security-scan-v1.0.0.tar.gz                # All platforms
```

### Pure Shell Flows

Flows that don't need binaries work without a `bin/` directory:

```
my/desktop-alert/
└── flow.yaml    # Pure shell, no dependencies
```

```yaml
name: Desktop Alert
version: 1.0.0

PostChange:
  - if: $event.platform == 'macos'
    then:
      - shell: osascript -e 'display notification "File edited"'
  - if: $event.platform == 'linux'
    then:
      - shell: notify-send "File edited"
```

These work everywhere without bundling anything.

### Introspection

```bash
# Show flow details
$ aiki flows show vendor/security-scan
Name: Security Scan
Version: 1.0.0
Location: ~/.aiki/flows/vendor/security-scan

Bundled tools:
  ✓ semgrep (darwin-arm64, darwin-x86_64, linux-x86_64, windows-x86_64)

External requirements:
  (none)

# Validate dependencies
$ aiki flows validate vendor/security-scan
✓ All bundled binaries present for darwin-arm64
✓ All external dependencies satisfied
```

### Benefits

1. **Zero configuration** - Drop binaries in `bin/<platform>/`, they just work
2. **Cross-platform** - Automatically uses correct binary for user's system
3. **Self-contained** - Flows work out of the box without manual setup
4. **Introspectable** - `ls bin/` shows what's available
5. **Flexible** - Can bundle 0, 1, or many tools
6. **Distributable** - Vendors can ship complete, working flows

---

## What We Lost (and Why It's OK)

### 1. Complex DAG Dependencies

**Can't do:**
```
     A
   /   \
  B     C
  |     |
  D     E
   \   /
     F
```
Where D depends on B (not C) and E depends on C (not B).

**Why it's OK:**
- 95% of use cases are simple: sequential or simple parallel
- Complex DAGs are rare in CI/CD workflows
- Can be added later with optional `depends_on:` if needed

**Workaround:** Break into multiple flows or use flow chaining.

### 2. Named References in Triggers

**Can't do:**
```yaml
triggers:
  - event_type: PostChange
    metadata:
      previous_step: "lint"
```

**Why it's OK:**
- Event types are now top-level keys (cleaner)
- Triggers are implicit (flow subscribes to all events it defines)

---

## Migration Path

### Old Model (Original phase-5.md)

```yaml
name: "My Flow"
triggers:
  - event_type: PostChange
actions:
  - name: "Run lint"
    type: shell
    command: "ruff check ."
```

### New Model (Simplified)

```yaml
name: "My Flow"

PostChange:
  - shell: "ruff check ."
```

**Changes:**
1. Remove `triggers:` - event types are top-level keys
2. Remove `type:` - action type IS the key
3. Remove `actions:` wrapper - directly under event
4. Optional: Remove `name:` unless needed for references

---

## Implementation Priorities

### Phase 5.1: Core Flow Engine
- Flow loading from `.aiki/flows/` and `~/.aiki/flows/`
- Terraform-style namespace resolution
- Sequential execution
- Basic action types: `flow`, `shell`, `log`
- Variable interpolation

### Phase 5.2: Advanced Features
- Parallel execution
- Named steps and references
- Inline conditionals (`when:`)
- Block conditionals (`if/then/else`)
- HTTP and JJ actions

### Phase 5.3: CLI & Tooling
- `aiki flows list/show/enable/disable`
- `aiki flows create` (interactive wizard)
- `aiki flows test` (dry-run)
- `aiki flows logs` (execution history)

### Phase 5.4: Polish
- Flow validation and error reporting
- Performance optimization
- Documentation and examples
- Community flow registry (optional)

---

## Key Design Decisions

### ✅ What We Kept

1. **Event abstraction** - Flows consume `AikiEvent` instances
2. **YAML/JSON** - No code required
3. **Variable interpolation** - `$variable` and `${ENV_VAR}`
4. **Composability** - Flows reference other flows
5. **Non-blocking** - Flows never block core handlers

### ✅ What We Simplified

1. **No pipeline concept** - Everything is a flow
2. **No trigger configuration** - Event types are top-level keys
3. **No nested stages** - Flat arrays under event types
4. **Action type as key** - `flow:` not `type: flow`
5. **Dual conditionals** - `when:` vs `if/then/else`

### ✅ What We Added

1. **Terraform-style addressing** - `aiki/quick-lint`
2. **Namespaces** - `aiki/`, `company/`, `my/`
3. **Named steps** - Optional `alias:` for references
4. **Step variables** - `$stepname.failed`, `$previous_step.exit_code`
5. **Inline conditionals** - `when:` for guards
6. **Two-level composition** - Event-level (default) and flow-level (`includes:`)

---

## Next Steps

1. ✅ Create example flows (DONE - see `ops/examples/`)
2. ✅ Document simplified model (DONE - this document)
3. ⏳ Update phase-5.md with simplified model
4. ⏳ Implement Phase 5.1 (core engine)
5. ⏳ Build CLI commands
6. ⏳ Write comprehensive tests

---

## Related Documentation

- [Flow Examples](../examples/) - Complete working examples
- [Flow README](../examples/README-flows.md) - User guide
- [Two-Stage Review Example](../examples/README-two-stage-review.md) - Production-ready workflow
