# Agent Loading UX

## Problem

When subtasks spawn agents (build, review, fix workflows), there's a brief moment between:
1. Task transitioning to InProgress
2. Agent process spawning and reporting first status

During this window (typically 0.5-2s), the UI shows the subtask as `▸` (in-progress) but without an agent badge or elapsed time. This creates ambiguity:
- Is the agent running?
- Did spawn fail silently?
- Is the system stuck?

For slow machines or cold starts, this pause can be noticeable and feel broken.

## Current Behavior

```
Before: Review just started
 ✓ build  3/3  2m28s
 ▸ review                                       ← parent shows in-progress
    ○ explore                                   ← subtask still shows pending
    ○ criteria
    ○ record-issues
```

```
After: Agent running (after ~1s delay)
 ✓ build  3/3  2m28s
 ▸ review  0:08
    ▸ explore                          cc 0:08  ← suddenly appears with agent badge
    ○ criteria
    ○ record-issues
```

**The gap**: User sees "Before" state for 0.5-2 seconds before "After" appears. During this time, it's unclear what's happening.

## Proposed Solution

Add an explicit **Starting** state that displays while the agent is spawning.

### Visual Design

```
Agent starting (transition state, 0.5-2s)
 ✓ build  3/3  2m28s
 ▸ review
    ⧗ explore                                   ← hourglass symbol, yellow text
    ○ criteria
    ○ record-issues
```

**Symbol options:**
- `⧗` (hourglass, U+29D7) - preferred, clearly indicates "waiting"
- `◐` (circle with left half black, U+25D0) - loading spinner aesthetic
- `⟳` (anticlockwise gapped circle arrow, U+27F3) - refresh/loading

**Color**: Yellow (same as `▸` in-progress) to indicate activity

### State Transitions

```
○ pending → ⧗ starting → ▸ running → ✓ completed
```

**Timing rules:**
1. Show `⧗` immediately when `task_run()` is called
2. Transition to `▸ cc 0:00` when agent reports first status
3. If spawn fails before first status, show error state

**Optimization**: If agent reports status within 200ms, skip `⧗` and go straight to `▸`. This avoids flicker for fast spawns.

## Implementation

### Phase 1: Track Agent Spawn State

**File**: `cli/src/agents/mod.rs` (or wherever agent runtime lives)

Add spawn state tracking:

```rust
pub enum AgentSessionState {
    Starting,      // spawn() called, process not ready
    Running,       // agent reported first status
    Stopped,
    Failed,
}
```

When `spawn_monitored()` is called:
1. Emit `AgentSessionState::Starting`
2. Wait for first status update from agent
3. Transition to `AgentSessionState::Running`

### Phase 2: Update TUI Rendering

**File**: `cli/src/tui/widgets/stage_track.rs` (or wherever subtask children render)

Update `StageChild` rendering logic:

```rust
fn render_subtask_status(subtask: &Task, agent_state: Option<AgentSessionState>) -> Symbol {
    match (subtask.status, agent_state) {
        (TaskStatus::Open, _) => SYM_PENDING,
        (TaskStatus::InProgress, Some(AgentSessionState::Starting)) => SYM_STARTING,
        (TaskStatus::InProgress, _) => SYM_ACTIVE,
        (TaskStatus::Closed, _) => SYM_DONE,
        _ => SYM_PENDING,
    }
}
```

**New constant**:
```rust
const SYM_STARTING: &str = "⧗";  // hourglass
```

### Phase 3: Apply to All Workflows

Apply this pattern to:
- **Build subtasks** (decompose, implement)
- **Review subtasks** (explore, criteria, record-issues)
- **Fix subtasks** (individual fix tasks)

All use `task_run()` under the hood, so the spawn state tracking will apply uniformly.

## Edge Cases

### Spawn Failure Before First Status

If agent spawn fails before reporting first status:
```
 ✗ review  failed
    ✗ explore                          spawn failed
```

Show `✗` (failure symbol, red) with error message.

### Parallel Subtasks (Future)

If multiple subtasks run concurrently:
```
 ▸ fix  2/5
    ⧗ Fix null check                            ← starting
    ▸ Fix error format             cc 0:03       ← running
    ○ Fix whitespace                             ← pending
```

The starting state makes parallel execution more comprehensible.

## Benefits

1. **Clearer feedback** - User knows agent spawn is in progress
2. **Better diagnostics** - Distinguish "waiting to start" from "spawn failed"
3. **Smoother UX** - Visual continuity between task start and agent running
4. **Future-proof** - Scales to parallel subtask execution

## Alternatives Considered

### Alt 1: Skip starting state, just show pending longer

Keep `○ explore` until agent reports first status, then jump to `▸ explore cc 0:00`.

**Pros**: Simpler implementation
**Cons**: Breaks the mental model that InProgress tasks show `▸`

### Alt 2: Show `▸ explore` without agent badge during spawn

Show in-progress symbol immediately, but delay agent badge until first status.

**Pros**: No new symbol needed
**Cons**: Confusing why there's no agent badge or elapsed time

### Alt 3: Show a generic "spawning..." message at parent level

```
 ▸ review  spawning agent...
    ○ explore
```

**Pros**: Doesn't require per-subtask state tracking
**Cons**: Less granular, doesn't help with parallel subtasks

## Decision

**Go with the proposed solution**: Add `⧗` starting state with 200ms debounce to avoid flicker.

This provides the best UX for both sequential and future parallel workflows.
