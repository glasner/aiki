# Context Hooks: Re-inject State After Compaction, Resume, and Clear

## Problem

When Claude Code compacts context mid-turn, the agent loses:
1. **Workspace isolation path** — forgets it's in `~/.aiki/workspaces/<repo>/<session>/`
2. **Active task awareness** — forgets which aiki task it was working on
3. **Workflow reminders** — forgets to use `aiki task` at all

CLAUDE.md survives compaction (always in system prompt), but hook-injected context does not.

## Current State

Claude Code fires two relevant events around compaction:

**Pre-compaction:** `PreCompact` hook (fires before compaction)
```json
{ "hook_event_name": "PreCompact", "trigger": "auto", "session_id": "...", "cwd": "..." }
```
- Matcher values: `manual` (user ran `/compact`), `auto` (context window full)
- Input includes `trigger` and `custom_instructions` fields
- Cannot block compaction (exit code 2 just shows stderr to user)
- Aiki does **not** install a PreCompact hook today

**Post-compaction:** `SessionStart` with `source: "compact"`
```json
{ "hook_event_name": "SessionStart", "source": "compact", "session_id": "...", "cwd": "..." }
```
- Can inject context via stdout (same as startup)
- Aiki's installed hook only matches `"startup"`, so this is **silently dropped**

```json
// cli/src/config.rs:122-129
"SessionStart": [{ "matcher": "startup", "hooks": [{ ... }] }]
```

### Event mapping (current)

| Claude Code event | Matcher hit? | Aiki event fired |
|-------------------|-------------|------------------|
| `SessionStart(source="startup")` | Yes | `session.started` |
| `SessionStart(source="resume")` | No | (dropped) |
| `SessionStart(source="clear")` | No | (dropped) |
| `SessionStart(source="compact")` | No | (dropped) |
| `PreCompact` | — | (no hook installed) |

Note: `"resume"` is also dropped. The code in `events.rs` has logic to map it to `session.resumed`, but the matcher prevents it from ever reaching that code.

## Design

### New aiki events — `session.*` namespace

These events use the `session.*` namespace since they're part of session lifecycle management:

| Event | When | Source | Purpose |
|-------|------|--------|---------|
| `session.will_compact` | Before compaction | Claude Code `PreCompact` | Persist state that must survive (future use) |
| `session.compacted` | After compaction | `SessionStart(source="compact")` | Re-inject workspace path, active tasks, workflow reminders |
| `session.cleared` | After `/clear` | `SessionStart(source="clear")` | Re-inject workspace and task state after conversation reset |

### Mapping from Claude Code

```
PreCompact                        → session.will_compact
SessionStart(source="compact")    → session.compacted
SessionStart(source="clear")      → session.cleared
SessionStart(source="resume")     → session.resumed       (existing, currently dropped)
SessionStart(source="startup")    → session.started        (existing, works today)
```

### Hook actions for `session.compacted`

```yaml
session.compacted:
    # Re-inject workspace isolation (same logic as turn.started)
    - let: ws_path = self.workspace_create_if_concurrent
    - if: ws_path
      then:
          - context: |
                WORKSPACE ISOLATION: An isolated JJ workspace has been created for your session.
                You MUST `cd {{ws_path}}` before any file operations and work from that directory.
                All file reads, writes, and edits must use paths relative to {{ws_path}}.
                This ensures your changes don't conflict with other concurrent sessions.

    # Re-inject active task reminder
    - let: active_tasks = self.task_in_progress
    - if: active_tasks
      then:
          - context:
                append: |
                    ---
                    ACTIVE TASKS (context was compacted — here's what you were working on):
                    {{active_tasks}}
                    Continue these tasks or close them with: aiki task close <id> --summary "..."

    # Re-inject task count
    - let: task_count = self.task_list_size
    - if: task_count
      then:
          - context:
                append: |
                    ---
                    Tasks ({{task_count}} ready)
                    Run `aiki task` to view - OR - `aiki task start` to begin work.

session.cleared:
    # Same as context.compacted — /clear resets conversation but keeps session
    - let: ws_path = self.workspace_create_if_concurrent
    - if: ws_path
      then:
          - context: |
                WORKSPACE ISOLATION: An isolated JJ workspace has been created for your session.
                You MUST `cd {{ws_path}}` before any file operations and work from that directory.
                All file reads, writes, and edits must use paths relative to {{ws_path}}.
                This ensures your changes don't conflict with other concurrent sessions.

    - let: task_count = self.task_list_size
    - if: task_count
      then:
          - context:
                append: |
                    ---
                    Tasks ({{task_count}} ready)
                    Run `aiki task` to view - OR - `aiki task start` to begin work.

session.will_compact:
    # Reserved for future use
    # PreCompact fires before compaction — opportunity to persist state
    # Could write workspace path, active task IDs to a recovery file
    # so session.compacted can recover even if session state is lost
```

## Implementation

### Step 1: Fix SessionStart matcher + add PreCompact hook

**File:** `cli/src/config.rs`

Change the SessionStart matcher to accept all sources, and add a new PreCompact hook:

```rust
// Before:
settings["hooks"]["SessionStart"] = json!([{
    "matcher": "startup",
    ...
}]);

// After:
settings["hooks"]["SessionStart"] = json!([{
    "matcher": "",   // match all SessionStart sources (startup, compact, resume, clear)
    ...
}]);

// NEW: Add PreCompact hook
settings["hooks"]["PreCompact"] = json!([{
    "matcher": "",   // match both manual and auto triggers
    "hooks": [{
        "type": "command",
        "command": format!("{} hooks stdin --agent claude-code --event PreCompact", aiki_path),
        "timeout": 10
    }]
}]);
```

Users must re-run `aiki init` after this change to update their `~/.claude/settings.json`.

### Step 2: Add session event types

**File:** `cli/src/events/mod.rs`

Add three new variants to `AikiEvent`:

```rust
// Session lifecycle events
/// Session compaction is about to happen (pre-compaction)
#[serde(rename = "session.will_compact")]
SessionWillCompact(AikiSessionWillCompactPayload),
/// Session was compacted — re-inject critical state
#[serde(rename = "session.compacted")]
SessionCompacted(AikiSessionCompactedPayload),
/// Session was cleared via /clear — re-inject critical state
#[serde(rename = "session.cleared")]
SessionCleared(AikiSessionClearedPayload),
```

**New file:** `cli/src/events/session_compacted.rs`

```rust
pub struct AikiSessionCompactedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
}

pub fn handle_session_compacted(payload: AikiSessionCompactedPayload) -> Result<HookResult> {
    let core_hook = crate::flows::load_core_hook();
    let mut state = AikiState::new(payload);
    let flow_result = execute_hook(
        EventType::SessionCompacted,
        &mut state,
        &core_hook.handlers.session_compacted,
    )?;
    // ... return HookResult with context
}
```

**New file:** `cli/src/events/session_cleared.rs` — same pattern

**New file:** `cli/src/events/session_will_compact.rs` — same pattern

### Step 3: Add PreCompact to Claude Code event mapping

**File:** `cli/src/editors/claude_code/events.rs`

Add `PreCompact` variant to `ClaudeEvent` enum:

```rust
#[serde(rename = "PreCompact")]
PreCompact(PreCompactPayload),
```

Add payload:

```rust
#[derive(Debug, Deserialize)]
pub struct PreCompactPayload {
    pub session_id: String,
    pub cwd: String,
    pub trigger: String,           // "manual" or "auto"
    pub custom_instructions: Option<String>,
}
```

Add mapping function:

```rust
fn build_session_will_compact_event(payload: PreCompactPayload) -> AikiEvent {
    let session = create_session(&payload.session_id, &payload.cwd);
    AikiEvent::SessionWillCompact(AikiSessionWillCompactPayload {
        session,
        cwd: PathBuf::from(&payload.cwd),
        timestamp: chrono::Utc::now(),
    })
}
```

### Step 4: Route compaction in SessionStart mapping

**File:** `cli/src/editors/claude_code/events.rs`

Update `build_session_started_event` to route `compact` and `clear` sources:

```rust
fn build_session_started_event(payload: SessionStartPayload) -> AikiEvent {
    let session = create_session(&payload.session_id, &payload.cwd);
    let cwd = PathBuf::from(&payload.cwd);
    let timestamp = chrono::Utc::now();

    match payload.source.as_str() {
        "resume" => AikiEvent::SessionResumed(AikiSessionResumedPayload {
            session, cwd, timestamp,
        }),
        "compact" => AikiEvent::SessionCompacted(AikiSessionCompactedPayload {
            session, cwd, timestamp,
        }),
        "clear" => AikiEvent::SessionCleared(AikiSessionClearedPayload {
            session, cwd, timestamp,
        }),
        _ => AikiEvent::SessionStarted(AikiSessionStartPayload {
            session, cwd, timestamp,
        }),
    }
}
```

### Step 5: Add dispatch routing

**File:** `cli/src/event_bus.rs`

```rust
// Session lifecycle events
AikiEvent::SessionWillCompact(e) => events::handle_session_will_compact(e),
AikiEvent::SessionCompacted(e) => events::handle_session_compacted(e),
AikiEvent::SessionCleared(e) => events::handle_session_cleared(e),
```

### Step 6: Add to flow system

**File:** `cli/src/flows/types.rs` — add to `EventHandlers`:

```rust
#[serde(rename = "session.will_compact", ...)]
pub session_will_compact: Vec<HookStatement>,
#[serde(rename = "session.compacted", ...)]
pub session_compacted: Vec<HookStatement>,
#[serde(rename = "session.cleared", ...)]
pub session_cleared: Vec<HookStatement>,
```

**File:** `cli/src/flows/composer.rs` — add to `EventType`:

```rust
SessionWillCompact,
SessionCompacted,
SessionCleared,
```

### Step 7: Add hook actions

**File:** `cli/src/flows/core/hooks.yaml`

Add the `session.compacted`, `session.cleared`, and `session.will_compact` handlers (as shown in the Design section above).

### Step 8: Fix session.resumed handling

While we're fixing the matcher, `session.resumed` will now actually fire. Update its hook actions:

```yaml
session.resumed:
    # Re-inject workspace isolation on resume
    - let: ws_path = self.workspace_create_if_concurrent
    - if: ws_path
      then:
          - context: |
                WORKSPACE ISOLATION: ...

    - let: task_count = self.task_list_size
    - if: task_count
      then:
          - context:
                append: |
                    ---
                    Tasks ({{task_count}} ready)
                    Run `aiki task` to view - OR - `aiki task start` to begin work.
```

## Files to Modify

| File | Change |
|------|--------|
| `cli/src/config.rs` | Change SessionStart matcher to `""`, add PreCompact hook |
| `cli/src/events/mod.rs` | Add `SessionWillCompact`, `SessionCompacted`, `SessionCleared` variants |
| `cli/src/events/session_will_compact.rs` | New file: payload + handler |
| `cli/src/events/session_compacted.rs` | New file: payload + handler |
| `cli/src/events/session_cleared.rs` | New file: payload + handler |
| `cli/src/editors/claude_code/events.rs` | Add `PreCompact` to `ClaudeEvent`, route `compact`/`clear` in SessionStart |
| `cli/src/event_bus.rs` | Add dispatch for session events |
| `cli/src/flows/types.rs` | Add `session_will_compact`, `session_compacted`, `session_cleared` to `EventHandlers` |
| `cli/src/flows/composer.rs` | Add `SessionWillCompact`, `SessionCompacted`, `SessionCleared` to `EventType` |
| `cli/src/flows/core/hooks.yaml` | Add `session.*` handlers, update `session.resumed` |
| `AGENTS.md` | Already done — workspace isolation section added |

## Testing

1. **Unit test**: Verify `SessionStart(source="compact")` maps to `SessionCompacted`
2. **Unit test**: Verify `SessionStart(source="clear")` maps to `SessionCleared`
3. **Unit test**: Verify `SessionStart(source="resume")` maps to `SessionResumed`
4. **Unit test**: Verify `SessionStart(source="startup")` still maps to `SessionStarted`
5. **Unit test**: Verify `PreCompact` maps to `SessionWillCompact`
6. **Integration test**: Trigger compaction in a long conversation and verify workspace path is re-injected
7. **Integration test**: Verify active task IDs are re-injected after compaction
8. **Integration test**: Run `/clear` and verify workspace + task context is re-injected

## Event mapping (proposed)

| Claude Code event | Aiki event | Context injected? |
|-------------------|------------|-------------------|
| `SessionStart(source="startup")` | `session.started` | Yes (task count) |
| `SessionStart(source="resume")` | `session.resumed` | Yes (workspace + tasks) |
| `SessionStart(source="compact")` | `session.compacted` | Yes (workspace + active tasks + task count) |
| `SessionStart(source="clear")` | `session.cleared` | Yes (workspace + task count) |
| `PreCompact(trigger=*)` | `session.will_compact` | No (reserved for future state persistence) |

## Open Questions

1. Should `session.compacted` run `jj new`? Probably not — the session is continuing, not starting fresh. We just need context re-injection.
2. Should we persist workspace path to a file (e.g., `$AIKI_HOME/sessions/<uuid>/workspace-path`) for redundancy? The `workspace_create_if_concurrent` function is idempotent and returns the existing path, so this may not be needed.
3. Should `session.will_compact` do any state persistence today, or keep it as a no-op for future use? Given PreCompact can't block, it's purely informational.
