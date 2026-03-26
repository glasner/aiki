---
status: draft
---

# Claude Code Agent Teams: Integration Research

> Research into Claude Code's new "agent teams" feature and how it maps onto aiki's architecture.

## What Are Agent Teams?

Agent teams (released Feb 2026 with Opus 4.6, experimental/research preview) let a single Claude Code session spawn multiple independent Claude Code instances ("teammates") that work in parallel. Unlike subagents (Task tool), teammates get their own full context window, can message each other directly, and coordinate through a shared task list.

**Architecture:**
```
Team Lead (main session)
├── Teammate A (own context, own session)
├── Teammate B (own context, own session)
└── Teammate C (own context, own session)
```

**Key primitives:**
| Primitive | Description |
|-----------|-------------|
| `spawnTeam` | Create a team with name + description |
| `message` | Send message to a specific teammate |
| `broadcast` | Send message to all teammates |
| Task list | Shared task list with dependency tracking |
| Self-claiming | Teammates auto-pick next unblocked task |
| Shutdown | Gracefully end a teammate session |

**Storage locations:**
- Team config: `~/.claude/tasks/{team-name}/`
- Team config: `~/.claude/teams/{team-name}/config.json`

**Enable with:**
```json
{ "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" } }
```

---

## How It Maps to Aiki

### What Aiki Already Has

| Capability | Aiki Status | Notes |
|------------|-------------|-------|
| Session tracking | ✅ Complete | Each session gets UUID, events persisted on JJ branch |
| Task system | ✅ Complete | Event-sourced, persistent, with priorities/assignees |
| Agent detection | ✅ Complete | ClaudeCode, Codex, Cursor, Gemini |
| Assignee routing | ✅ Complete | Tasks can be assigned to specific agent types |
| Task spawning | ✅ Complete | `aiki run` spawns background agents |
| Provenance tracking | ✅ Complete | Line-level attribution via PostToolUse hooks |
| Flow engine | ✅ Complete | YAML-based event-driven automation |
| Review workflows | ✅ Complete | `aiki review` creates review tasks |
| Session claiming | ✅ Complete | Atomic task claiming per session |

### What Teams Add (That Aiki Doesn't Have)

| Capability | Teams Feature | Aiki Gap |
|------------|---------------|----------|
| Peer-to-peer messaging | Teammates message each other | Aiki tasks are broadcast, no direct messaging |
| Team-level grouping | Team is a named unit with members | Aiki tracks sessions individually |
| Shared task list | Built-in, with dependencies | Aiki has tasks but no dependency graph |
| Self-claiming | Auto-pickup of next task | Aiki requires explicit `task start` |
| Real-time coordination | Live communication between agents | Aiki is async/event-sourced |
| Plan approval | Lead reviews teammate plans | No approval gate in aiki task flow |

---

## Integration Opportunities

### 1. Team-Aware Provenance (High Priority)

**Problem:** When a team spawns, each teammate fires `SessionStart` and `PostToolUse` hooks independently. Aiki currently tracks these as unrelated sessions. There's no way to see "this was a coordinated team effort."

**Integration:**
- Detect team membership from environment or config
- Add `team_id` and `team_role` (lead/teammate) to session metadata
- Group related sessions in `aiki blame` and `aiki history` output
- New provenance fields in `[aiki]` metadata blocks:
  ```
  team: my-team-name
  team_role: teammate
  team_lead_session: <session-uuid>
  ```

**Value:** "These 4 sessions were part of the same coordinated effort" — essential for attribution and review.

**Implementation approach:**
- On `session.started`, check for team environment variables or `~/.claude/teams/` config
- Store team metadata alongside existing session data
- Extend `AikiSessionStartPayload` with optional team fields
- `aiki blame` shows team name when multiple team members touched a file

### 2. Bridging Task Systems (High Priority)

**Problem:** Teams have their own shared task list at `~/.claude/tasks/{team-name}/`. Aiki has its own event-sourced task system on the `aiki/tasks` branch. Two competing task systems = confusion and lost provenance.

**Options:**

**Option A: Aiki as the canonical task system (preferred)**
- Teams feature stores tasks in flat files; aiki's event-sourced system is more capable
- Aiki tasks already support: priorities, assignees, sources, templates, parent/subtasks, comments
- Could inject aiki task context into teammates via `session.started` flow hooks
- Team lead creates tasks in aiki; teammates see them via context injection
- Challenge: Claude Code's TeammateTool reads from its own task store, not aiki

**Option B: Sync between systems**
- Watch `~/.claude/tasks/{team-name}/` for changes, mirror to aiki tasks
- Watch aiki task events, mirror to Claude Code's task store
- Pro: Both systems work; Con: Sync complexity, eventual consistency issues

**Option C: Observe-only (MVP)**
- Don't try to replace or sync the task systems
- Just observe team task activity and record it as aiki events
- Add team task events to session history for provenance
- Build richer integration later once the teams API stabilizes

**Recommendation:** Start with **Option C** (observe-only), plan for **Option A** long-term. The teams feature is experimental; building tight coupling now is premature.

### 3. Flow Engine Extensions (Medium Priority)

**Problem:** Aiki's flow engine reacts to session/turn/change events but has no concept of team events. When a team spawns, coordinates, or completes, there's no flow hook.

**New events to consider:**
```yaml
# Proposed new event types
team.started:      # Team was created, lead session detected
team.member_joined:  # New teammate session detected
team.member_left:    # Teammate session ended
team.completed:    # All teammate sessions ended
```

**Flow integration example:**
```yaml
team.started:
  - log: "Team '$team.name' started with $team.member_count members"
  - context: "Team coordination active. Use aiki task for persistent tracking."

team.completed:
  - review:
      task_id: "$team.lead_task_id"
      agent: codex
```

**Value:** Automated review of team output, team-level provenance collection, context injection for teammates.

### 4. Team-Level Review (Medium Priority)

**Problem:** `aiki review` currently reviews a single task's changes. When a team completes work, you want to review the combined output of all teammates holistically.

**Integration:**
- `aiki review --team <team-name>` reviews all changes from all team sessions
- Collects diffs across all teammate sessions
- Identifies potential conflicts (overlapping file edits)
- Generates a unified review covering the full team's work

**Implementation:**
- Query sessions by `team_id` metadata
- Aggregate `change.completed` events across sessions
- Run existing review template against combined diff
- Flag any files touched by multiple teammates

### 5. Conflict Detection (Medium Priority, ties to Phase 18)

**Problem:** Multiple teammates editing the same files is a known limitation. Claude Code teams docs explicitly warn about same-file edits. Aiki's Phase 18 (Local Multi-Agent Coordination) already plans conflict detection — teams accelerate the need.

**Integration:**
- Use existing `change.completed` events to detect overlapping edits in real-time
- During team execution, warn the lead if teammates touch the same files
- Could use `block:` action on `change.permission_asked` to prevent conflicting writes

**Example flow:**
```yaml
change.permission_asked:
  - if: "$team.active && self.file_locked_by_teammate($event.file_path)"
    then:
      - block: "File $event.file_path is being edited by teammate $locked_by. Choose a different approach."
```

### 6. Context Injection for Teammates (Low Priority)

**Problem:** Teammates don't inherit the lead's conversation history. They load CLAUDE.md and standard context, but miss team-specific context like "what the lead discovered" or "the current codebase state."

**Integration:**
- On `session.started` for teammate sessions, inject team context via flow
- Include: team objective, assigned tasks, files already modified by other teammates, known constraints
- Aiki's `context:` action in flows already supports this pattern

**Example:**
```yaml
session.started:
  - if: "$session.team_role == 'teammate'"
    then:
      - context:
          prepend:
            - "## Team Context"
            - "Team: $session.team_name"
            - "Your role: $session.team_role"
            - "Lead assigned you: $session.task_name"
            - "Files modified by others: $team.modified_files"
```

---

## Integration Architecture

```
┌─────────────────────────────────────────────────┐
│ Claude Code Team Session                         │
│                                                   │
│  Lead ──spawns──> Teammate A                      │
│    │               │                              │
│    │               ├── PostToolUse ──> aiki hook   │
│    │               └── SessionStart ──> aiki hook  │
│    │                                              │
│    ├──spawns──> Teammate B                        │
│    │               │                              │
│    │               ├── PostToolUse ──> aiki hook   │
│    │               └── SessionStart ──> aiki hook  │
│    │                                              │
│    ├── PostToolUse ──> aiki hook                  │
│    └── SessionStart ──> aiki hook                 │
│                                                   │
└─────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────┐
│ Aiki                                             │
│                                                   │
│  Session Store (JJ branch: aiki/conversations)    │
│  ├── Lead session (team_id: X, role: lead)        │
│  ├── Teammate A session (team_id: X, role: mate)  │
│  └── Teammate B session (team_id: X, role: mate)  │
│                                                   │
│  Task Store (JJ branch: aiki/tasks)               │
│  ├── Parent task: "Team X objective"              │
│  ├── Subtask 1: assigned to Teammate A            │
│  └── Subtask 2: assigned to Teammate B            │
│                                                   │
│  Provenance (JJ change descriptions)              │
│  └── [aiki] blocks with team metadata             │
│                                                   │
│  Flow Engine                                      │
│  └── team.started / team.completed triggers       │
└─────────────────────────────────────────────────┘
```

---

## Phased Implementation Plan

### Phase A: Detection & Observation (MVP)

**Goal:** Aiki correctly identifies team sessions and groups them.

1. Detect team membership on `session.started`
   - Check environment variables set by Claude Code teams
   - Check `~/.claude/teams/*/config.json` for active teams
   - Determine role (lead vs teammate)
2. Store team metadata in session events
   - Extend `AikiSessionStartPayload` with `team_id`, `team_role`, `team_name`
   - Record in JJ change descriptions
3. `aiki history` shows team grouping
   - Group sessions by `team_id`
   - Show team timeline (lead started, teammates joined, work done, team completed)

**Depends on:** Understanding what environment variables/files Claude Code sets for team members (needs investigation once feature is GA).

### Phase B: Provenance Enhancement

**Goal:** `aiki blame` shows team attribution.

1. Provenance metadata includes team fields
2. `aiki blame` output shows team name alongside agent name
3. `aiki authors` aggregates by team

### Phase C: Flow Integration

**Goal:** Flows can react to team lifecycle events.

1. New event types: `team.started`, `team.member_joined`, `team.completed`
2. Flow engine dispatches team events
3. Example flows for team context injection and auto-review

### Phase D: Task Bridge

**Goal:** Aiki tasks work seamlessly with team coordination.

1. Team lead can pre-create aiki tasks that map to team work
2. Context injection tells teammates about their aiki tasks
3. Team completion triggers aiki task closure

### Phase E: Conflict Prevention

**Goal:** Prevent teammates from overwriting each other's work.

1. File-level locking via `change.permission_asked` hooks
2. Real-time conflict warnings to team lead
3. Integration with Phase 18 (Local Multi-Agent Coordination)

---

## Open Questions

1. **What environment does Claude Code set for teammates?** Need to investigate what env vars or config files identify a session as part of a team. This determines how aiki detects team membership.

2. **Is the teams task format documented?** The file format in `~/.claude/tasks/{team-name}/` matters for Option B (sync) integration.

3. **Can hooks influence team behavior?** If `session.started` hooks can inject context into teammate sessions, that's the key integration point. Need to verify teammates trigger hooks.

4. **Will teams support custom task backends?** If Claude Code eventually allows pluggable task stores, aiki could become the canonical task system without sync hacks.

5. **What happens to team data on session resume?** Claude Code docs note in-process teammates aren't restored on `/resume`. How does this affect aiki's session tracking?

6. **Split-pane vs in-process:** Does the display mode affect hook behavior? Teammates in tmux panes are separate processes — do they each trigger their own hooks?

---

## Relationship to Existing Roadmap

| Roadmap Phase | Relationship to Teams |
|---------------|----------------------|
| Phase 5 (Flow Engine) ✅ | Foundation for team event handling |
| Phase 8 (Core Extensions) | PrePrompt context injection enables teammate context |
| Phase 13 (Autonomous Review) | Team output is a natural review target |
| Phase 14 (Event System) | Team events extend the event taxonomy |
| Phase 18 (Multi-Agent Coordination) | Teams are the primary local multi-agent scenario now |
| Phase 21 (Shared JJ Brain) | Team coordination is the stepping stone to cross-developer coordination |

**Key insight:** Claude Code teams accelerate the need for Phase 18. The "multiple local agents overwriting each other" scenario from Phase 18 is exactly what happens when teammates edit the same files. Teams make this problem concrete and immediate rather than theoretical.

---

## Recommendation

**Start with Phase A (Detection & Observation).** The teams feature is experimental and its API surface will change. Building deep integration now risks coupling to unstable internals. But passively observing and recording team activity into aiki's provenance system is low-risk, high-value, and positions aiki to be the provenance layer for team workflows.

**Immediate next steps:**
1. Enable `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` in a test repo
2. Run a team and observe what hooks fire (SessionStart, PostToolUse per teammate?)
3. Inspect `~/.claude/teams/` and `~/.claude/tasks/` file formats
4. Document actual environment variables and identifiers available to hooks
5. Prototype team detection in `session.started` handler
