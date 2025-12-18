# Aiki Flow Examples

This directory contains example flows demonstrating the Aiki flow system.

## Flow Model Overview

Flows are YAML files that define automated reactions to AI coding events. They use:
- **Terraform-style addressing**: `aiki/quick-lint`, `company/slack-notify`, `my/backup`
- **Simplified syntax**: Action types as keys (`flow:`, `shell:`, `http:`)
- **Sequential by default**: Actions run in order
- **Explicit parallelism**: Use `parallel:` when needed
- **Multi-event**: Single flow can handle multiple events (PostChange, PreCommit, etc.)

## Directory Structure

```
.aiki/flows/                    # Repository-level flows
├── aiki/                       # Built-in Aiki flows
│   ├── quick-lint.yaml         # Addressable as: aiki/quick-lint
│   ├── security-scan.yaml      # Addressable as: aiki/security-scan
│   └── complexity-check.yaml   # Addressable as: aiki/complexity-check
│
└── company/                    # Organization flows
    └── slack-notify.yaml       # Addressable as: company/slack-notify

~/.aiki/flows/                  # User-level flows
└── my/                         # Personal flows
    ├── desktop-alert.yaml      # Addressable as: my/desktop-alert
    └── backup.yaml             # Addressable as: my/backup
```

## Example Files

### [flow.yaml](./flow.yaml) - Complete Pipeline
Multi-event flow showing PostChange, PreCommit, Start, and Stop events with DAG execution.

### Built-in Aiki Flows

#### [aiki/quick-lint.yaml](./aiki/quick-lint.yaml)
Fast linting for immediate feedback after AI edits. Supports Python, JavaScript, TypeScript, Rust, and Go.

**Usage:**
```yaml
PostChange:
  - flow: aiki/quick-lint
```

#### [aiki/security-scan.yaml](./aiki/security-scan.yaml)
Comprehensive security scanning with Semgrep. Blocks commits with security errors.

**Usage:**
```yaml
PreCommit:
  - flow: aiki/security-scan
```

#### [aiki/complexity-check.yaml](./aiki/complexity-check.yaml)
Code complexity analysis to detect overly complex functions (common AI anti-pattern).

**Usage:**
```yaml
PreCommit:
  - flow: aiki/complexity-check
```

### Organization Flows

#### [company/slack-notify.yaml](./company/slack-notify.yaml)
Send Slack notifications about AI code activity on PostChange and PreCommit.

**Setup:**
```bash
export SLACK_WEBHOOK_URL="https://hooks.slack.com/services/YOUR/WEBHOOK"
```

**Usage:**
```yaml
PostChange:
  - flow: company/slack-notify
```

### Personal Flows

#### [my/desktop-alert.yaml](./my/desktop-alert.yaml)
Personal desktop notifications (macOS/Linux) for AI edits.

**Usage:**
```yaml
PostChange:
  - flow: my/desktop-alert
```

#### [my/backup.yaml](./my/backup.yaml)
Backup AI-edited files to `~/aiki-backups/` after each edit and create session archives.

**Usage:**
```yaml
Stop:
  - flow: my/backup
```

## Flow Syntax

### Basic Structure

```yaml
name: "Flow Name"
description: "What this flow does"
version: 1

# Event types as top-level keys
PostChange:
  - flow: aiki/quick-lint
  - shell: echo 'Done'

PreCommit:
  - flow: aiki/security-scan
  - shell: pytest
    blocking: true
```

### Action Types

```yaml
# Reference another flow
- flow: aiki/quick-lint

# Shell command
- shell: ruff check .
  timeout: 5s
  continue_on_error: true

# HTTP request
- http:
    url: https://api.example.com/webhook
    method: POST
    body:
      event: PostChange
  timeout: 10s

# JJ command
- jj: describe -m "reviewed"

# JJ command with author
- with_author: "Name <email>"
  jj: describe -m "User changes"

# JJ command with metadata (sets both author and message)
- with_author_and_message: self.build_human_metadata
  jj: describe -m "$message"

# Inline conditional (when: guard)
- when: $security.failed
  flow: company/slack-alert

- when: $event.file_path ends_with '.py'
  shell: pytest

# Block conditional (if/then/else: branching)
- if: $agent == 'claude-code'
  then:
    - shell: echo 'Claude edit'
    - flow: company/notify
  else:
    - shell: echo 'Other agent'

# Parallel execution
- parallel:
    - flow: aiki/security-scan
    - flow: aiki/complexity-check
    - shell: npm audit

# Sleep/wait
- sleep: 2s

# Log message
- log: Flow completed
```

### Execution Model

**Sequential by default:**
```yaml
PostChange:
  - flow: aiki/quick-lint     # Step 1
  - shell: echo 'Done'        # Step 2 (waits for step 1)
```

**Parallel when explicit:**
```yaml
PreCommit:
  - flow: aiki/quick-lint     # Step 1
  - parallel:                  # Step 2 (waits for step 1)
      - flow: aiki/security-scan
      - flow: aiki/complexity-check
  - shell: pytest             # Step 3 (waits for all parallel)
    blocking: true
```

## Variable Interpolation

### Flow-Level Variables

Define reusable variables at the flow level using the `variables:` section:

```yaml
name: "Slack Notification"
version: 1

variables:
  webhook: "${SLACK_WEBHOOK_URL}"
  channel: "#ai-activity"
  timeout: "5s"

PostChange:
  - http:
      url: $webhook
      body:
        text: "File edited: $event.file_path"
        channel: $channel
    timeout: $timeout
```

**Benefits:**
- **DRY**: Define once, use everywhere
- **Environment variables**: Can reference `${ENV_VAR}` in variable definitions
- **Event interpolation**: Can interpolate event data like `"$agent-session"`

### Event Variables

Access event data using `$variable` syntax:

```yaml
PostChange:
  - shell: echo 'Agent: $agent'
  - shell: echo 'File: $event.file_path'
  - shell: echo 'Session: $session_id'
  - shell: echo 'Time: $timestamp'
```

**Available variables:**
- `$event_type` - PostChange, PreCommit, Start, Stop
- `$agent` - claude-code, cursor, windsurf
- `$session_id` - Agent session ID
- `$cwd` - Working directory
- `$timestamp` - Event timestamp
- `$event.file_path` - Edited file path
- `$event.tool_name` - Edit, Write, etc.
- `$event.change_id` - JJ change ID
- `$ENV_VAR` - Environment variables (all caps, direct access)

## Installing Flows

### 1. Copy to Repository

```bash
# Copy all example flows
cp -r ops/examples/.aiki .

# Or individual flows
mkdir -p .aiki/flows/aiki
cp ops/examples/aiki/quick-lint.yaml .aiki/flows/aiki/
```

### 2. Copy to User Directory

```bash
# Personal flows
mkdir -p ~/.aiki/flows/my
cp ops/examples/my/desktop-alert.yaml ~/.aiki/flows/my/
cp ops/examples/my/backup.yaml ~/.aiki/flows/my/
```

### 3. Verify Installation

```bash
aiki flows list
# Should show:
# aiki/quick-lint
# aiki/security-scan
# aiki/complexity-check
# company/slack-notify
# my/desktop-alert
# my/backup
```

## Creating Custom Flows

### Interactive Creation

```bash
aiki flows create company/my-flow
# Creates: .aiki/flows/company/my-flow.yaml
```

### Manual Creation

```bash
# Create directory
mkdir -p .aiki/flows/company

# Create flow file
cat > .aiki/flows/company/my-flow.yaml << 'EOF'
name: My Custom Flow
description: Does something useful
version: 1

PostChange:
  - shell: echo 'Hello from $agent'
EOF
```

### Step References

**Default: Full Flow Identifier**

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
aiki/security-scan    → $aiki_security_scan
company/slack-notify  → $company_slack_notify
my/backup             → $my_backup
```

**Optional: Aliases for Shorter References**

Add optional `alias:` for shorter variable names:

```yaml
PreCommit:
  - flow: aiki/security-scan
    alias: security
  
  - flow: aiki/complexity-check
    alias: complexity
  
  # Reference using aliases
  - if: "$security.failed OR $complexity.failed"
    then:
      - flow: company/slack-alert
```

**Available step variables:**
```yaml
$identifier.result        # "success" | "failed"
$identifier.exit_code     # 0 | 1 | 2...
$identifier.output        # stdout
$identifier.duration_ms   # execution time
$identifier.failed        # boolean: true if exit_code != 0

$previous_step.result     # Always available for previous step
$previous_step.exit_code
```

## Common Patterns

### Two-Stage Review

Fast feedback on edits, comprehensive review on commit:

```yaml
name: Two-Stage Review

PostChange:
  - flow: aiki/quick-lint
  - flow: my/desktop-alert

PreCommit:
  - flow: aiki/quick-lint
  - parallel:
      - flow: aiki/security-scan
      - flow: aiki/complexity-check
  - shell: pytest
    blocking: true
  - flow: company/slack-notify
```

### Conditional Execution

Run different actions based on event data:

```yaml
PostChange:
  # Inline conditional (simple guard)
  - when: $event.file_path ends_with '.py'
    shell: ruff check $event.file_path
  
  # Block conditional (branching logic)
  - if: $event.file_path ends_with '.py'
    then:
      - shell: ruff check $event.file_path
      - shell: mypy $event.file_path
    else:
      - log: Not a Python file, skipping lint
```

### Chaining Flows

Flows can call other flows and reference results:

```yaml
name: Security Pipeline

PreCommit:
  - flow: aiki/security-scan
    alias: security
  
  # Inline conditional
  - when: $security.exit_code == 0
    flow: company/slack-notify
  
  # Block conditional
  - if: $security.failed
    then:
      - shell: echo 'Security scan failed, blocking commit'
      - shell: exit 1
        blocking: true
    else:
      - log: Security scan passed
```

## Namespace Resolution

Flows are resolved in this order:

1. **Repository flows** (`.aiki/flows/`):
   - `aiki/quick-lint` → `.aiki/flows/aiki/quick-lint.yaml`
   - `company/jira` → `.aiki/flows/company/jira.yaml`

2. **User flows** (`~/.aiki/flows/`):
   - `my/backup` → `~/.aiki/flows/my/backup.yaml`

3. **Override precedence**: Repository flows override user flows of the same name

## Troubleshooting

### Flow Not Found

```bash
# List all available flows
aiki flows list

# Show flow details
aiki flows show aiki/quick-lint

# Validate flow syntax
aiki flows validate .aiki/flows/company/my-flow.yaml
```

### Flow Execution Failed

```bash
# View execution logs
aiki flows logs aiki/security-scan

# Test flow with mock event
aiki flows test aiki/quick-lint --event PostChange
```

### Debugging

```bash
# Enable debug logging
export AIKI_DEBUG=1

# Run flow manually
aiki flows run aiki/quick-lint --event PostChange
```

## Best Practices

1. **Start simple** - Begin with single-event flows, add complexity as needed
2. **Use namespaces** - Organize flows by `aiki/`, `company/`, `my/`
3. **Keep flows focused** - One responsibility per flow
4. **Use composition** - Reference flows from other flows
5. **Test in isolation** - Use `aiki flows test` before enabling
6. **Add timeouts** - Prevent flows from hanging
7. **Handle errors** - Use `continue_on_error: true` for non-critical actions
8. **Document clearly** - Add descriptions and comments

## Related Documentation

- [Phase 5: Flows Implementation Plan](../phase-5.md)
- [Two-Stage Review Example](./README-two-stage-review.md)
