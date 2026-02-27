# Aiki Summary

Aiki is an AI code provenance tracking tool built on Jujutsu (jj). It automatically records which AI agents contributed to a codebase, providing transparent attribution for AI-generated code.

## Core Capabilities

- **Provenance tracking** - Records AI agent metadata in jj change descriptions
- **Line-level blame** - `aiki blame` shows which AI agent wrote each line
- **Git co-authorship** - Automatically adds `Co-authored-by:` lines to Git commits
- **Task management** - Event-sourced task system for AI agent workflows
- **Code review** - Pipeable review system (`aiki review | aiki fix`)
- **Session history** - Conversation tracking across AI sessions
- **Flow engine** - Declarative YAML-based automation for editor events

## Supported Editors

| Editor | Integration |
|--------|------------|
| Claude Code | Lifecycle hooks via `~/.claude/settings.json` |
| Cursor | `afterFileEdit` hooks via `~/.cursor/hooks.json` |
| Codex | OpenTelemetry trace parsing |
| Zed | ACP (Agent Client Protocol) bidirectional proxy |

## Key Commands

| Command | Purpose |
|---------|---------|
| `aiki init` | Initialize Aiki in a project |
| `aiki doctor` | Check configuration health |
| `aiki blame <file>` | View AI attribution per line |
| `aiki authors` | List AI authors for changes |
| `aiki task` | Manage tasks (add, start, close) |
| `aiki review` | Create and run code reviews |
| `aiki fix` | Create followup tasks from reviews |
| `aiki session list/show` | View session history |
| `aiki hooks install` | Install global editor hooks |
| `aiki acp <agent>` | Run ACP proxy server |

## Architecture

- Built on **Jujutsu (jj)**, not Git directly
- Provenance stored as `[aiki]...[/aiki]` blocks in jj change descriptions
- Tasks event-sourced on `aiki/tasks` branch
- Sessions event-sourced on `aiki/conversations` branch (global `~/.aiki/`)
- 17 unified event types spanning session, turn, file, shell, web, MCP, and git lifecycle
- Flow engine routes events through declarative YAML workflows
