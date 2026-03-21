# Plan: `aiki bug` Command

## Overview

Add an `aiki bug` command that captures a snapshot of the task system state, environment, and configuration for debugging purposes. The output is a structured report (printed to stdout or written to a file) that can be pasted into a GitHub issue.

## What it captures

### 1. Environment Info
- **aiki version** (`env!("CARGO_PKG_VERSION")`)
- **OS / platform** (`std::env::consts::{OS, ARCH}`)
- **Shell** (`$SHELL`)
- **Prerequisite versions** — reuse `check_command_version` for `git`, `jj`
- **AIKI_HOME** — whether overridden or default

### 2. Repository State
- **Project root** — detected via `RepoDetector`
- **JJ initialized** — `.jj/` exists
- **Git initialized** — `.git/` exists
- **`.aiki/` present** — config dir exists
- **Current JJ change ID** — `jj log -r @ --no-graph -T change_id` (short)
- **JJ status summary** — `jj status` (conflict state, working copy)

### 3. Task System Snapshot
- **In-progress tasks** — task ID, name, assignee, time started
- **Recent task events** — last N events from the task branch (e.g., last 10)
- **Task graph stats** — total tasks, by status (open/in-progress/closed), link count
- **Active sessions** — list from `~/.aiki/sessions/`

### 4. Configuration State
- **Claude Code hooks installed** — check `~/.claude/settings.json` for aiki hooks
- **Git hooks installed** — check `.aiki/githooks/`
- **Plugins** — list installed plugins from `.aiki/tasks/` manifest
- **Instructions file** — which file is in use (AGENTS.md / CLAUDE.md), exists?

### 5. Optional: User Description
- `aiki bug "description of the problem"` — free-text description included at top of report
- Without args: just the snapshot, no description

## Output Format

Markdown-formatted report to stdout, fenced in a code block for easy copy-paste:

```
## Aiki Bug Report

### Description
<user-provided text, if any>

### Environment
- aiki: 0.1.0
- OS: linux (x86_64)
- Shell: /bin/zsh
- git: git version 2.43.0
- jj: jj 0.35.0

### Repository
- Project root: /home/user/myproject
- JJ: initialized
- Git: initialized
- .aiki/: present
- Working copy change: abc123def
- JJ status: No conflicts. 2 modified files.

### Tasks
- Total: 15 (3 open, 2 in-progress, 10 closed)
- In-progress:
  - [mvslrsp...] "Add auth endpoint" (claude-code, started 2h ago)
- Recent events (last 5):
  - Started "Add auth endpoint" (2h ago)
  - Closed "Setup DB schema" (3h ago)
  ...

### Sessions
- Active: 1
  - abc-123 (claude-code, interactive, pid 12345)

### Configuration
- Claude Code hooks: installed
- Git hooks: installed
- Plugins: 2 (review, fix)
- Instructions: AGENTS.md (exists)
```

## Implementation

### Files to create/modify

1. **`src/commands/bug.rs`** (new) — the command implementation
   - `pub fn run(description: Option<String>) -> Result<()>`
   - Helper functions to collect each section
   - Reuse existing infrastructure: `RepoDetector`, `check_command_version`, `TaskGraph`, session file listing

2. **`src/commands/mod.rs`** — add `pub mod bug;`

3. **`src/main.rs`** — add `Bug` variant to `Commands` enum + dispatch:
   ```rust
   /// Generate a bug report with system and task state
   Bug {
       /// Description of the issue
       description: Option<String>,
       /// Write report to a file instead of stdout
       #[arg(long, short)]
       output: Option<PathBuf>,
   },
   ```

### Implementation approach

- Keep it simple — mostly string formatting and shelling out to `jj`
- Reuse `check_command_version` from `prerequisites.rs` for tool versions
- Reuse `RepoDetector` for repo state detection
- Read task graph via existing `tasks::storage` + `tasks::graph` for task stats
- List session files from `global::global_sessions_dir()`
- Check config state similar to how `doctor.rs` does it
- No new dependencies needed
- Wrap each section in a `collect_*` helper that returns `String` and catches its own errors (a broken section shouldn't prevent the rest of the report)

### Error handling

Each section collector should be resilient — if we can't read the task graph (e.g., not in a repo), print `<not available>` for that section rather than failing the whole command. The bug report command should work even in a broken state (that's when you need it most).

### Not in scope (for now)
- Uploading to GitHub automatically (`gh issue create`)
- Clipboard copy
- Log file attachment
- Anonymization/redaction of paths
