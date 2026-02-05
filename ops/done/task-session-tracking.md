# Session Tracking for Background Task Execution

## Summary

Track and manage background vs interactive sessions to support `aiki task run --async` and provide visibility into autonomous task execution.

## Background vs Interactive Sessions

Sessions can run in two modes:

**Background sessions:**
- Created when running `aiki task run` (with or without `--async`)
- Run autonomously without user interaction
- Can be monitored and stopped independently
- Automatically work on assigned tasks
- `--async` flag controls whether caller waits for completion, not session mode

**Interactive sessions:**
- User working directly in an agent (Claude Code, Cursor, etc.)
- Foreground execution
- User controls the session directly

## Session Mode Tracking

**One background session per task:** Only one background agent can be assigned to a task at a time. When you run `aiki task run <id>`, it either:
- Creates a new background session if none exists
- Fails if a background session is already working on that task

**Task events track session assignment:** The task event log records which session claimed the task.

## Session List Command

View and filter sessions by mode:

```bash
# List all sessions (shows both background and interactive)
aiki session list

# Example output:
# Session ID    Agent        Type          Task                    Status      Started
# abc123        claude-code  background    Fix auth bug            active      2m ago
# def456        cursor       background    Implement feature X     active      5m ago
# ghi789        claude-code  interactive   -                       active      1h ago

# Filter to only background sessions
aiki session list --background

# Example output:
# Session ID    Agent        Task                           Status      Started
# abc123        claude-code  Fix auth bug                   active      2m ago
# def456        cursor       Implement feature X            active      5m ago

# Filter to only interactive sessions
aiki session list --interactive

# Example output:
# Session ID    Agent        Task                           Status      Started
# ghi789        claude-code  -                              active      1h ago
```

## Implementation

### Session Mode Metadata

Add `mode: "background" | "interactive"` field to session metadata.

**How to detect session mode:**

1. **Background sessions:** Explicitly created via `aiki task run`
   - Store `mode: "background"` in session metadata at creation
   - Set when Aiki programmatically creates the session to work on a task

2. **Interactive sessions:** User working directly in agent
   - Store `mode: "interactive"` when session is initialized
   - Detected via SessionStart hook when user manually opens the agent
   - Can also be inferred: any session NOT created by `aiki task run` is interactive

**Where to store:**
- Session files are stored globally at `$AIKI_HOME/sessions/{uuid}` (not per-repo)
- Format: Plain text metadata in `[aiki]...[/aiki]` blocks
- Add `mode=background` or `mode=interactive` field to existing format
- Set once at session creation, immutable for session lifetime
- Already persisted fields include: `agent`, `external_session_id`, `session_id`, `started_at`, `parent_pid`, `runner_task`

**Filtering and display:**
- Use for filtering in `aiki session list --background` and `aiki session list --interactive`
- Display in default `aiki session list` output as "Type" column

### Session-Task Relationship

Track which task (if any) a session is working on:

**For background sessions:**
- Always associated with a specific task
- Task ID stored in session metadata
- Displayed in session list output

**For interactive sessions:**
- May or may not be working on a task
- Task association tracked via task start/stop events
- Shows "-" in session list if no task claimed

## Use Cases

### Starting Background Work

```bash
# Run a task in the background
aiki task run <task-id> --agent claude-code --async

# Returns immediately with task ID
# <started task_id="xyz789" async="true">
#   Task started asynchronously.
# </started>
```

### Monitoring Background Sessions

```bash
# See all background work
aiki session list --background
```

### Stopping Background Work

```bash
# Stop a background session
aiki session stop <session-id>
```

## Future Enhancements

- **Task-to-session lookup:** `aiki session show --task <task-id>` to find session for a task
- **Session logs:** `aiki session logs <session-id>` to view output
- **Session attach:** `aiki session attach <session-id>` to take over interactively
- **Session status:** `aiki session status <session-id>` for detailed state
- **Task filtering:** `aiki session list --task <task-id>` to find sessions by task

## Related Work

- `ops/now/workflow-commands.md` - Workflow commands use background sessions
- Task management and lifecycle
- Agent coordination
