# Aiki Task Context Survival Across Context Compaction

## Problem

When Claude Code compacts context (summarizes conversation to free tokens), the agent loses:
1. **Active aiki task awareness** - Forgets which task it was working on
2. **AGENTS.md workflow requirements** - Forgets to use `aiki task` at all
3. **Session state** - Loses connection to aiki session/turn tracking

**Result**: After compaction, the agent continues working but stops using aiki tasks, breaking provenance tracking.

## Root Cause

Context compaction in Claude Code creates a summary of the conversation. The summary:
- Preserves technical context (what files were modified, what the task was)
- Does NOT preserve aiki task IDs or workflow reminders
- Does NOT trigger any aiki event we can hook into

## Available Hooks

Currently we have these events that fire on session/turn boundaries:

| Event | When it fires | Could help? |
|-------|--------------|-------------|
| `session.started` | New session begins | No - compaction doesn't start new session |
| `session.resumed` | Existing session resumes | Maybe - if compaction triggers this |
| `turn.started` | Each user message | Yes - fires every turn |
| `turn.completed` | Agent finishes response | No - too late |

## Solution Options

### Option A: Inject task state on every turn.started (Recommended)

Add to `hooks.yaml`:
```yaml
turn.started:
  # Existing task count reminder
  - let: task_count = self.task_list_size
  - if: $task_count
    then:
      - context:
          append: |
            ---
            Tasks ($task_count ready)
            Run `aiki task` to view - OR - `aiki task start` to begin work.

  # NEW: Inject active task reminder
  - let: active_tasks = self.get_active_tasks
  - if: $active_tasks
    then:
      - context:
          append: |
            ---
            ACTIVE TASK: $active_tasks.id - $active_tasks.name
            Continue this task or close it with: aiki task close $active_tasks.id --comment "..."
```

**Pros:**
- Works on every turn, survives compaction
- Already have the hook point
- Uses existing context injection pattern

**Cons:**
- Adds tokens to every turn
- Need to implement `self.get_active_tasks` function

### Option B: Claude Code compaction hook (if available)

Check if Claude Code fires an event when context is compacted.

```yaml
# Hypothetical
context.compacted:
  - context:
      append: |
        ---
        ⚠️ Context was compacted. Check aiki task status:
        - Run `aiki task` to see active/ready tasks
        - Read AGENTS.md if unsure about workflow
```

**Pros:**
- Only fires when needed
- Explicit reminder at the right moment

**Cons:**
- May not exist as an event
- Need to verify Claude Code hook capabilities

### Option C: Enhance session.resumed

If resuming a session (which might happen after compaction):

```yaml
session.resumed:
  - let: active_tasks = self.get_active_tasks
  - if: $active_tasks
    then:
      - context:
          append: |
            ---
            RESUMED SESSION - Active task: $active_tasks.id
            $active_tasks.name
```

**Pros:**
- Fires on session resume
- Natural place for reinitialization

**Cons:**
- May not fire on compaction (same session continues)

## Implementation Plan

### Phase 1: Verify hook behavior (Research)

1. Test what happens during Claude Code context compaction:
   - Does `session.resumed` fire?
   - Does `turn.started` fire with any special indicators?
   - Is there a compaction-specific event?

2. Check Claude Code hook documentation for compaction events

### Phase 2: Implement get_active_tasks function

Add to `cli/src/flows/core/functions.rs`:

```rust
/// Get active (in_progress) tasks for the current session
pub fn get_active_tasks(context: &ExecutionContext) -> Result<Option<TaskInfo>> {
    let session_id = context.session.uuid();

    // Query aiki task list for in_progress tasks
    let output = Command::new("aiki")
        .args(["task", "list", "--status", "in_progress", "--format", "json"])
        .current_dir(&context.cwd)
        .output()?;

    // Parse and return first active task
    // ...
}
```

### Phase 3: Add turn.started injection

Update `hooks.yaml` to inject active task reminder on every turn.

### Phase 4: Test compaction behavior

1. Start a long conversation with an active task
2. Trigger context compaction
3. Verify the agent sees and continues the task

## Alternative: CLAUDE.md enhancement

Instead of hooks, enhance CLAUDE.md to instruct Claude to check aiki task at session start:

```markdown
## After Context Compaction

If you see "This session is being continued from a previous conversation":
1. Run `aiki task` to check for active tasks
2. If there's an active task, continue it
3. If not, check if you need to start one for the current work
```

**Pros:** Simpler, no code changes
**Cons:** Relies on Claude following instructions (can fail)

## Success Criteria

- [ ] After context compaction, agent sees active task reminder
- [ ] Agent continues using `aiki task` workflow
- [ ] No duplicate task creation after compaction
- [ ] Works for all supported editors (Claude Code, Cursor, Codex, ACP)

## Files to Modify

- `cli/src/flows/core/hooks.yaml` - Add active task injection
- `cli/src/flows/core/functions.rs` - Add `get_active_tasks` function
- `AGENTS.md` or `CLAUDE.md` - Add compaction recovery instructions

## Questions to Resolve

1. Does Claude Code fire any event on context compaction?
2. How do we get active tasks efficiently (avoid slow CLI calls)?
3. Should we inject task state into the summary itself?
