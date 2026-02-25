# Getting Started with Aiki

This guide walks you from zero to productive with Aiki — AI code provenance tracking and workflow orchestration.

## Prerequisites

- **Git** — for co-author attribution and version control
- **Rust toolchain** — for building from source (`rustup` recommended)

Jujutsu (jj) is bundled into Aiki via `jj-lib`, so you don't need to install it separately.

## Installation

```bash
git clone https://github.com/glasner/aiki.git
cd aiki/cli
cargo install --path .
```

Verify the installation:

```bash
aiki --version
```

## Initialize a Project

Navigate to any Git repository and run:

```bash
cd your-project
aiki init
```

This will:
- Initialize Jujutsu (non-colocated, independent from your `.git`)
- Create the `.aiki/` directory with default configuration
- Configure commit signing (auto-detects GPG/SSH keys)
- Install Git hooks for automatic co-author attribution
- Configure editor hooks globally (Claude Code, Cursor)

## Health Check

Verify everything is set up correctly:

```bash
aiki doctor
```

This checks repository setup, global hooks, and local configuration. If it finds issues:

```bash
aiki doctor --fix
```

## Editor Setup

`aiki init` configures all supported editors automatically. Here's what gets set up:

| Editor | What happens |
|--------|-------------|
| **Claude Code** | Lifecycle hooks added to `~/.claude/settings.json` |
| **Cursor** | File edit hooks added to `~/.cursor/hooks.json` |
| **Codex** | OTel receiver configured |
| **Zed** | ACP proxy available via `aiki acp claude-code` |

Since hooks are global, you only need to restart your editor once after `aiki init`. Aiki preserves any existing hooks you had.

## Your First AI Session

Once Aiki is initialized, just use your AI editor normally. Aiki works in the background:

1. **Session starts** — Aiki creates a fresh JJ change for the session
2. **AI edits files** — provenance metadata is recorded automatically in JJ change descriptions
3. **You `git commit`** — Aiki's Git hook adds `Co-authored-by:` lines for AI contributors
4. **Check attribution** — `aiki blame` shows who wrote what

### See AI Attribution

```bash
aiki blame src/main.rs
```

Output:

```
abc12345 (Claude Code   session-123  High  )    1| fn main() {
abc12345 (Claude Code   session-123  High  )    2|     println!("Hello, world!");
def67890 (Cursor        session-456  High  )    3|     // Added by Cursor
```

Filter by editor:

```bash
aiki blame src/main.rs --agent claude-code
```

Verify signatures:

```bash
aiki blame src/main.rs --verify
```

### See AI Authors

```bash
# Working copy changes
aiki authors

# Git trailer format for commit messages
aiki authors --format=git --changes=staged
```

## Task Management Basics

Aiki includes an event-sourced task system designed for AI agent workflows. Tasks persist across sessions and are visible to all agents.

### Create and Work on Tasks

```bash
# Create and start a task in one command
aiki task start "Implement login validation"

# Add progress notes as you work
aiki task comment <id> "Added email format check"

# Close when done
aiki task close <id> --summary "Validation complete"
```

### View Tasks

```bash
# See what's ready to work on
aiki task

# Show details for a specific task
aiki task show <id>
```

### Priorities and Subtasks

```bash
# Create with priority
aiki task start "Urgent fix" --p0

# Create subtasks for multi-part work
aiki task add "Fix all review issues" --source prompt
aiki task add --parent <id> "Fix null check"
aiki task add --parent <id> "Add error handling"
aiki task start <id>
```

### Delegate to Agents

```bash
# Have another agent work on a task
aiki task run <id>
```

## Code Review Pipeline

Aiki's review system lets AI agents review each other's work. Commands are pipeable for autonomous workflows.

### Basic Review

```bash
# Create and run a review (waits for completion)
aiki review

# Review a specific task's changes
aiki review <task-id>
```

### Fix Issues Found

```bash
# Create followup tasks from review findings
aiki fix <review-task-id>
```

### Pipeline Pattern

```bash
# Autonomous review + fix in one command
aiki review | aiki fix

# Review with automatic fix loop
aiki review --fix
```

### Build + Review

```bash
# Build from a spec, then review the output
aiki build ops/now/my-feature.md --review

# Build, review, and auto-fix
aiki build ops/now/my-feature.md --fix
```

## Cryptographic Signing

Aiki automatically configures signing during `aiki init`. It detects existing keys in priority order:

1. Git signing configuration (if already set up)
2. GPG keys
3. SSH keys

Verify a change's signature:

```bash
aiki verify        # Verify working copy
aiki verify @-     # Verify previous change
```

## Next Steps

- [Customizing Defaults](customizing-defaults.md) — modify Aiki's behavior with flows, events, context injection, and template overrides
- [Creating Plugins](creating-plugins.md) — build reusable, shareable hooks and templates
- [Contributing](contributing.md) — develop Aiki itself
