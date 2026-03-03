# Aiki - AI Code Provenance Tracking

Aiki automatically tracks which AI agents contributed to your codebase, providing transparent attribution for AI-generated code changes.

## Features

- **Automatic Provenance Tracking**: Records AI agent changes in Jujutsu (jj) change descriptions
- **Line-Level Attribution**: See which AI agent wrote each line of code with `aiki blame`
- **Git Co-Author Attribution**: Automatically adds `Co-authored-by:` lines to Git commits for AI contributors
- **Multi-Editor Support**: Claude Code, Cursor, Codex, and Zed (via ACP proxy)
- **Task Management**: Event-sourced task system designed for AI agent workflows
- **Session History**: Conversation tracking with prompt/response timeline
- **Flow Engine**: Declarative YAML-based automation for all editor events
## Quick Start

### Prerequisites

- Git (for co-author attribution feature)
- Rust toolchain (for building from source)

**Note:** Jujutsu (jj) is bundled directly into Aiki via `jj-lib`, so you don't need to install it separately.

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/aiki.git
cd aiki

# Build and install
cd cli
cargo install --path .
```

### Initialize Aiki in Your Project

```bash
cd your-project
aiki init
```

This will:
- Initialize Jujutsu if not already present
- Create `.aiki/` directory structure
- Install Git hooks for automatic co-author attribution
- Configure Claude Code hooks (global user hooks in `~/.claude/settings.json`)
- Configure Cursor hooks (global user hooks in `~/.cursor/hooks.json`)
- Offer to automatically restart editors if they're running

### Check Configuration Health

```bash
aiki doctor
```

This checks:
- Repository setup (JJ, Git, Aiki directory)
- Global hooks installation (Git, Claude Code, Cursor)
- Local configuration (Git core.hooksPath)

Add `--fix` to automatically repair issues:
```bash
aiki doctor --fix
```

## Usage

### View AI Attribution for a File

```bash
aiki blame src/main.rs
```

Output shows which AI agent contributed each line:

```
abc12345 (Claude Code   session-123  High  )    1| fn main() {
abc12345 (Claude Code   session-123  High  )    2|     println!("Hello, world!");
def67890 (Cursor        session-456  High  )    3|     // Added by Cursor
abc12345 (Claude Code   session-123  High  )    4| }
```

Filter by specific editor:

```bash
# Show only Claude Code contributions
aiki blame src/main.rs --agent claude-code

# Show only Cursor contributions
aiki blame src/main.rs --agent cursor
```

### View AI Authors

```bash
# Show all AI authors for working copy changes (default)
aiki authors

# Show authors for Git staged changes
aiki authors --changes=staged

# Git trailer format (for commit messages)
aiki authors --format=git --changes=staged
Co-authored-by: Claude Code <claude-code@anthropic.ai>
Co-authored-by: Cursor <cursor@cursor.sh>

# JSON format (for tooling)
aiki authors --format=json
```

### Automatic Git Co-Author Attribution

When you commit changes that include AI-contributed code, Aiki automatically adds co-author lines:

```bash
git add src/main.rs
git commit -m "Add main function"

# Aiki automatically adds:
# Co-authored-by: Claude Code <claude-code@anthropic.ai>
```

These co-author lines appear in `git log`, GitHub commit history, and Git blame annotations.

### Task Management

Aiki includes an event-sourced task system designed for AI agent workflows. Tasks persist across context compaction and provide structured work tracking.

```bash
# View ready tasks
aiki task

# Create and start a task
aiki task start "Implement login validation" --source prompt

# Add progress notes
aiki task comment <id> "Added email format check"

# Close when done
aiki task close <id> --summary "Validation complete"
```

**Task features:**
- **Priorities**: P0 (urgent) through P3 (low)
- **Parent/subtask relationships**: Hierarchical task decomposition with `.0`, `.1`, `.2` suffixes
- **Source tracking**: Lineage via `file:`, `task:`, `comment:`, `issue:`, `prompt:` sources
- **Assignees**: Assign to agents (`claude-code`, `codex`, `cursor`, `gemini`) or `human`
- **Won't-do outcomes**: Mark declined tasks with `--wont-do`
- **Agent execution**: `aiki task run <id>` spawns an agent to work on a task

Task data is event-sourced and stored on the `aiki/tasks` branch in JJ.

### Code Review

Aiki includes a review system for AI agents to review each other's work with pipeable commands.

```bash
# Create and run a code review (waits for completion)
aiki review

# Review specific task
aiki review <task-id>

# Review asynchronously (returns immediately)
aiki review --async

# Agent takes over review in current session
aiki review --start

# Create followup tasks from review findings and run them
aiki fix <review-task-id>

# Pipeline: autonomous review + fix
aiki review | aiki fix

# Pipeline with async review
aiki review --async | aiki wait | aiki fix
```

**Review workflow:**
1. `aiki review` creates a review task with subtasks (digest changes, review code)
2. An agent (default: codex) executes the review, recording issues via `aiki review issue add`
3. `aiki fix` reads issues from the completed review and creates followup tasks
4. Followup tasks are run to address the findings

**Command flags:**

| Flag | Effect |
|------|--------|
| (default) | Create + run to completion |
| `--async` | Create + run async, return immediately |
| `--start` | Create + start, agent takes over |
| `--template <name>` | Use custom template |
| `--agent <name>` | Override agent assignment |

**Query commands:**
```bash
# List review tasks
aiki review list

# Show review details with comments and followups
aiki review show <id>
```

### Session History

Aiki records conversation history across AI agent sessions:

```bash
# List all sessions with conversation summaries
aiki session list

# Show detailed session timeline with prompts and responses
aiki session show <id>
```

Session data is event-sourced and stored on the `aiki/conversations` branch in the global JJ repo at `~/.aiki/`.

### Install Global Hooks

Install Aiki hooks globally for all editors and repositories:

```bash
aiki hooks install
```

This is an alternative to `aiki init` that configures all editors globally. Individual repositories will be automatically initialized on first use.

### ACP Proxy Server

Run Aiki as an ACP (Agent Client Protocol) bidirectional proxy:

```bash
# Proxy between an IDE and Claude Code
aiki acp claude-code

# Proxy with a custom agent binary
aiki acp claude-code --bin /path/to/custom-agent

# Proxy with agent arguments
aiki acp cursor -- --verbose --debug
```

The ACP proxy intercepts communication between IDEs (Zed, Neovim) and AI agents, tracking tool calls and file changes in real-time.

**Supported agents:** `claude-code`, `cursor`, `codex`

### Benchmark Performance

```bash
# Run the core workflow benchmark
aiki benchmark aiki/core
```

Benchmarks the complete Aiki workflow: repository init, SessionStart, PreFileChange, and PostFileChange events. Compares results to previous runs with per-event timing breakdowns. Results persist in `.aiki/benchmarks/`.

## How It Works

### Unified Event System

Aiki's core is a unified event system with 17 event types spanning the full lifecycle of AI agent interactions:

| Category | Events |
|----------|--------|
| **Session** | `session.started`, `session.resumed`, `session.ended` |
| **Turn** | `turn.started`, `turn.completed` |
| **File Changes** | `change.permission_asked`, `change.completed` |
| **Reads** | `read.permission_asked`, `read.completed` |
| **Shell** | `shell.permission_asked`, `shell.completed` |
| **Web** | `web.permission_asked`, `web.completed` |
| **MCP** | `mcp.permission_asked`, `mcp.completed` |
| **Git** | `commit.message_started` |

Each editor integration translates its native hook format into these unified events. Events are then routed through the flow engine for processing.

### Flow Engine

Flows are declarative YAML workflows that react to events. A bundled core flow (`aiki/core`) handles all provenance recording, and users can extend behavior with custom flows.

**Flow capabilities:**
- **Actions**: `shell`, `jj`, `context` (inject into agent prompts), `autoreply`, `commit_message`, `log`, `task.run`, `review`
- **Control flow**: `if`/`else`, `switch`/`case`
- **Variables**: Event variables (`$event.*`), let bindings, environment variables, JSON field access
- **Composition**: `before`/`after` chaining with cycle detection
- **Failure handling**: Per-action `on_failure` with `continue`, `stop`, or `block` (reject the editor operation)
- **Built-in functions**: Native Rust functions for complex operations (edit classification, metadata generation, co-author extraction)

**Flow locations:**
- Bundled core flow (embedded in binary, always runs)
- `.aiki/flows/` - Project-specific flows
- `~/.aiki/flows/` - User global flows

### Provenance Tracking

Metadata is stored in Jujutsu change descriptions using `[aiki]...[/aiki]` blocks:

```
[aiki]
author=claude
author_type=agent
session=claude-session-abc123
tool=Edit
confidence=High
method=Hook
coauthor=User Name <user@email.com>
[/aiki]
```

When users modify files during AI sessions, Aiki detects the human edits and separates them into distinct changes with `author_type=human`.

### Git Integration

The `prepare-commit-msg` Git hook analyzes staged changes, attributes modified lines to AI agents via blame, and appends `Co-authored-by:` lines to the commit message. It chains to any previously configured hooks.

## Editor Support

### Zed (via Agent Client Protocol)
- **Integration type**: ACP bidirectional proxy
- **Supported agents**: Claude Code, Codex, Gemini
- **Requirements**: Zed installed, agent enabled in Zed, Node.js 18+ (for Node.js-based agents)
- **Installation**: Automatic during `aiki init`

### Claude Code (Standalone)
- **Hook type**: SessionStart, PostToolUse, and other lifecycle hooks
- **Configuration**: Global user-level in `~/.claude/settings.json`
- **Installation**: Automatic during `aiki init`

### Cursor
- **Hook type**: `afterFileEdit` hooks
- **Configuration**: Global user-level in `~/.cursor/hooks.json`
- **Installation**: Automatic during `aiki init`

### Codex
- **Integration type**: OpenTelemetry trace parsing
- **Hook type**: OTel receiver (`aiki otel-receive`)
- **Installation**: Automatic during `aiki init`

### Hook Preservation

Aiki preserves existing hooks for all editors. Since hooks are global, you only need to restart editors once after your first `aiki init`.

## Architecture

Aiki is built on **Jujutsu (jj)**, not Git. Key concepts:

- **Change**: The atomic unit in jj (mutable, with stable change IDs)
- **Change ID**: Stable identifier that persists across rewrites
- **Change Description**: Where Aiki stores provenance metadata

While Aiki tracks changes in Jujutsu, it provides Git integration for broader compatibility.

### Storage

- **Provenance**: `[aiki]` blocks in JJ change descriptions (per-repository)
- **Tasks**: Event-sourced on `aiki/tasks` branch (per-repository)
- **Sessions**: Event-sourced on `aiki/conversations` branch (global `~/.aiki/`)
- **Config**: `.aiki/` directory (per-repository) and `~/.aiki/` (global)

## Project Structure

```
aiki/
├── cli/                          # Main Rust CLI application
│   ├── src/
│   │   ├── commands/            # CLI command implementations
│   │   │   ├── init.rs          # Repository initialization
│   │   │   ├── doctor.rs        # Health checks and diagnostics
│   │   │   ├── blame.rs         # Line-level attribution
│   │   │   ├── authors.rs       # AI author extraction
│   │   │   ├── session.rs       # Session history commands
│   │   │   ├── task.rs          # Task management commands
│   │   │   ├── review.rs        # Code review commands
│   │   │   ├── fix.rs           # Followup task commands
│   │   │   ├── wait.rs          # Task wait command
│   │   │   ├── benchmark.rs     # Performance testing
│   │   │   ├── hooks.rs         # Hook management
│   │   │   └── acp.rs           # ACP proxy server
│   │   ├── editors/             # Editor integrations
│   │   │   ├── claude_code/     # Claude Code hooks
│   │   │   ├── cursor/          # Cursor hooks
│   │   │   ├── codex/           # Codex OTel integration
│   │   │   ├── acp/             # ACP proxy protocol
│   │   │   └── zed.rs           # Zed editor detection
│   │   ├── events/              # Unified event system (17 event types)
│   │   ├── flows/               # Flow engine
│   │   │   ├── core/            # Bundled core flow + native functions
│   │   │   ├── engine.rs        # Flow execution
│   │   │   ├── types.rs         # Flow, Statement, Action types
│   │   │   ├── composer.rs      # Flow composition
│   │   │   └── variables.rs     # Variable resolution
│   │   ├── tasks/               # Task system
│   │   ├── history/             # Session/conversation recording
│   │   ├── session/             # Session lifecycle management
│   │   ├── agents/              # Agent type detection
│   │   ├── jj/                  # Jujutsu integration
│   │   ├── provenance.rs        # Metadata parsing
│   │   ├── blame.rs             # Blame logic
│   │   ├── authors.rs           # Authors logic
│   │   ├── event_bus.rs         # Event routing/dispatch
│   │   ├── error.rs             # Error types
│   │   └── main.rs              # CLI entry point
│   └── templates/
│       └── prepare-commit-msg.sh  # Git hook template
└── ops/                          # Planning and architecture docs
    ├── done/                    # Completed phases
    ├── now/                     # Active work
    ├── next/                    # Upcoming work
    └── ROADMAP.md               # Long-term vision
```

## Contributing

Contributions are welcome! See the [ops/](ops/) directory for architecture documentation and [CLAUDE.md](CLAUDE.md) for terminology guidelines.

## License

[License information to be added]

## Acknowledgments

Built with:
- [Jujutsu](https://martinvonz.github.io/jj/) - Next-generation version control
- [Claude Code](https://claude.ai/code) - AI-powered code editor
