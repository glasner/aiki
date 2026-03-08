# Aiki vs. Beads: Comprehensive Comparison

**Date**: 2026-02-07
**Purpose**: Side-by-side technical comparison of the aiki task system and beads issue tracker

---

## Executive Summary

Both systems solve the same core problem: **giving AI coding agents persistent, structured memory across sessions.** They diverge significantly in architecture, philosophy, and scope.

| | Beads (`bd`) | Aiki (`aiki task`) |
|---|---|---|
| **Author** | Steve Yegge | glasner |
| **Language** | Go (originally TypeScript) | Rust |
| **Storage** | JSONL + SQLite + git | Event-sourced JJ branch |
| **Philosophy** | Standalone tool, any VCS | Deeply integrated with JJ |
| **Output** | JSON-first | XML-first |
| **Scope** | Full issue tracker | Task system within larger framework |

---

## 1. Data Model

### Issue/Task Schema

**Beads Issue** (~50+ fields, massive struct):

| Category | Fields |
|----------|--------|
| Core | `id`, `title`, `description`, `design`, `acceptance_criteria`, `notes`, `spec_id` |
| Status | `status` (7 values), `priority` (0-4), `issue_type` (5 enum), `assignee`, `owner` |
| Timestamps | `created_at`, `updated_at`, `closed_at`, `due_at`, `defer_until` |
| Metadata | `labels[]`, `dependencies[]`, `comments[]`, `metadata` (arbitrary JSON) |
| Compaction | `compaction_level`, `compacted_at`, `original_size` |
| Deletion | `deleted_at`, `deleted_by`, `delete_reason`, `original_type` |
| External | `external_ref`, `source_system` |
| Messaging | `sender`, `ephemeral`, `wisp_type` |
| Agent | `hook_bead`, `role_bead`, `agent_state`, `last_activity`, `rig` |
| Molecule | `mol_type`, `work_type`, `bonded_from[]` |
| HOP | `creator`, `validations[]`, `quality_score`, `crystallizes` |
| Gate | `await_type`, `await_id`, `timeout`, `waiters[]` |
| Event | `event_kind`, `actor`, `target`, `payload` |

**Aiki Task** (18 fields, focused struct):

| Category | Fields |
|----------|--------|
| Core | `id`, `name`, `task_type`, `instructions` |
| Status | `status` (4 values), `priority` (P0-P3), `assignee` |
| Timestamps | `created_at`, `started_at` |
| Session | `claimed_by_session`, `last_session_id`, `stopped_reason` |
| Provenance | `sources: Vec<String>`, `template`, `working_copy` |
| Outcome | `closed_outcome` (Done/WontDo) |
| Metadata | `data: HashMap<String, String>`, `comments: Vec<TaskComment>` |

**Key differences:**
- Beads has grown organically to support molecules, wisps, gates, HOP entities, and multi-agent orchestration. The Issue struct is a God object serving many roles.
- Aiki keeps the task struct minimal and purpose-focused. Extra concerns (reviews, templates, flows) live in separate systems rather than on the task itself.

### Status Values

| Beads | Aiki |
|-------|------|
| `open` | `open` |
| `in_progress` | `in_progress` |
| `blocked` | *(no dedicated status; uses `stopped` + `blocked_reason`)* |
| `deferred` | *(not modeled)* |
| `closed` | `closed` |
| `tombstone` | *(not modeled -- JJ handles history)* |
| `pinned` | *(not modeled)* |
| `hooked` | *(not modeled)* |
| *(no equivalent)* | `stopped` (was in_progress, now paused with reason) |

Aiki's `stopped` status is unique -- it captures the concept of "was working on this, hit a wall." Beads conflates this with `blocked` (external dependency) and `deferred` (intentional postponement).

### Priority

| Beads | Aiki |
|-------|------|
| 0 = Critical | P0 = Critical |
| 1 = High | P1 = High |
| 2 = Normal (default) | P2 = Normal (default) |
| 3 = Low | P3 = Low |
| 4 = Backlog | *(no equivalent)* |

Nearly identical. Beads has an extra "backlog" tier.

### Issue Types

| Beads | Aiki |
|-------|------|
| `bug`, `feature`, `task`, `epic`, `chore` (enum) | Free-form `Option<String>` with inference |
| Plus: `gate`, `molecule`, `message`, `agent`, `role`, `rig` (Gas Town) | Inferred from name: "review" / "fix" / "bug" / default "feature" |
| Lint rules per type (e.g., epics require `## Success Criteria`) | No lint rules per type |
| No inference | Auto-inference: `infer_task_type()` checks name and sources |

Beads is explicit and enforced. Aiki is implicit and flexible. Neither approach is strictly better -- beads catches structural issues early, aiki avoids ceremony.

---

## 2. Storage Architecture

### Beads: JSONL + SQLite + Git

```
.beads/
  beads.db          # SQLite (local cache, queryable)
  issues.jsonl      # JSONL (git-tracked, source of truth)
  sync_base.jsonl   # Per-machine snapshot for 3-way merge
  bd.sock           # Unix socket for daemon
  config.yaml       # Project config
```

- **Write path**: Mutation → SQLite → mark dirty → periodic export to JSONL → git commit + push
- **Read path**: SQLite queries (fast) with daemon-managed sync from JSONL on changes
- **Sync model**: Pull-first, 3-way merge. Scalars use Last-Write-Wins; labels/deps use union; comments use append.
- **Background daemon**: LSP-style per-workspace process. File watchers (inotify/FSEvents) with 500ms debounce. Remote sync every 30s.

### Aiki: Event-Sourced JJ Branch

```
aiki/tasks branch (no files on disk):
  change1: [aiki-task] event=created task_id=... [/aiki-task]
  change2: [aiki-task] event=started task_ids=... [/aiki-task]
  change3: [aiki-task] event=closed task_ids=... [/aiki-task]
```

- **Write path**: Create new JJ change as child of `aiki/tasks` bookmark → metadata in change description → advance bookmark
- **Read path**: `jj log -r "root()..aiki/tasks" --reversed` → parse all events → materialize in-memory
- **Sync model**: JJ handles it (changes are content-addressed, no merge conflicts on descriptions)
- **No daemon**: Reads are cheap enough to replay on every invocation

### Tradeoffs

| Aspect | Beads | Aiki |
|--------|-------|------|
| Query speed | O(1) via SQLite indexes | O(n) event replay every read |
| Write durability | SQLite + periodic JSONL export | Immediate (JJ change is atomic) |
| Merge conflicts | Custom 3-way merge needed | Content-addressed changes avoid conflicts |
| State recovery | Rebuild SQLite from JSONL | Replay events from branch |
| Dependencies | Git required | JJ required |
| Daemon overhead | Background process per workspace | None |
| Scalability | Tested at 10K+ issues | Designed for <1000 tasks per project |

---

## 3. Hierarchy and Epics

### ID Format

| Beads | Aiki |
|-------|------|
| `bd-` + hash (4-8 chars, adaptive) | 32-char reverse-hex (k-z alphabet) |
| Child: `bd-a3f8.1` | Child: `mvslrspmo...tkls.1` |
| Grandchild: `bd-a3f8.1.1` | Grandchild: `mvslrspmo...tkls.1.2` |
| Adaptive length (grows with DB size) | Fixed 32-char root |

Beads IDs are human-friendly (4-8 chars). Aiki IDs are collision-resistant (32 chars) but less memorable.

### Parent-Child Implementation

| Aspect | Beads | Aiki |
|--------|-------|------|
| How parent-child is stored | Dependency edge (`parent-child` type) in dependencies table | Encoded in the ID itself (via dot notation) |
| Parent lookup | Query dependencies table | `rsplit_once('.')` on the ID string |
| Child lookup | Query dependencies table | Scan all IDs for prefix match |
| Creating children | `bd create --parent <id>` | `aiki task add --parent <id>` |
| Moving children | Update dependency edge | Not supported (ID encodes parent) |

Beads' approach is more flexible (can re-parent issues). Aiki's is simpler and faster (no join needed).

### Epic Behavior

| Aspect | Beads | Aiki |
|--------|-------|------|
| Epic is a type? | Yes (`TypeEpic`) with lint rules | No; any parent task is implicitly epic-like |
| Auto-close | `GetEpicsEligibleForClosure()` when all children closed | `all_subtasks_closed()` auto-closes parent |
| Epic status tracking | `EpicStatus` struct: `total_children`, `closed_children`, `eligible_for_close` | Computed on the fly, no dedicated struct |
| Planning subtask | Not modeled | `.0` subtask auto-created: "Review all subtasks and start first batch" |
| Scope isolation | No (flat list, filtered) | Yes (`ScopeSet` hides root tasks when working on subtasks) |

Aiki's scoping model is a major UX innovation: when you `aiki task start <parent>`, the ready queue narrows to only that parent's subtasks. This prevents agents from wandering to unrelated work. Beads has no equivalent.

### Blocking Propagation

| Aspect | Beads | Aiki |
|--------|-------|------|
| Direction | Upward (child blocked → parent blocked) | Not implemented |
| Mechanism | Recursive CTE in `blocked_issues_cache`, depth limit 50 | N/A |
| Cache | Materialized table, rebuilt on any blocking change | N/A |
| Performance | <50ms rebuild on 10K issues | N/A |

This is beads' most sophisticated feature. The cached blocking model with transactional rebuilds is production-grade. Aiki has no dependency graph and no blocking propagation -- `blocked_reason` is just a text field on the Stopped event.

---

## 4. Dependency System

### Beads: 19 Dependency Types

**Workflow types (4, affect `bd ready`):**

| Type | Semantics |
|------|-----------|
| `blocks` | A must close before B is ready |
| `parent-child` | Hierarchy; children block parent readiness |
| `conditional-blocks` | B runs only if A fails (specific close keywords) |
| `waits-for` | Gate coordination; wait for dynamic children |

**Association types (2):**

| Type | Semantics |
|------|-----------|
| `related` | Informational link |
| `discovered-from` | Found this issue while working on that one |

**Graph link types (4, added v0.30):**

| Type | Semantics |
|------|-----------|
| `relates-to` | Knowledge graph edge |
| `replies-to` | Conversation threading |
| `duplicates` | Deduplication |
| `supersedes` | Version chain |

**Entity types (4, added v0.47 for HOP):**

| Type | Semantics |
|------|-----------|
| `authored-by` | Creator relationship |
| `assigned-to` | Assignment |
| `approved-by` | Approval |
| `attests` | Skill attestation |

**Other (5):**

| Type | Semantics |
|------|-----------|
| `tracks` | Cross-project convoy reference |
| `until` | Active until target closes |
| `caused-by` | Audit trail |
| `validates` | Approval/validation |
| `delegated-from` | Delegation chain |

### Aiki: No Dependency Graph

Aiki has no `blocks`/`depends-on` edges. The only blocking concept is:
- `aiki task stop --blocked "reason text"` sets a `blocked_reason` string on the Stopped event
- No automatic blocking detection or ready queue filtering based on dependencies

### Comparison

| Aspect | Beads | Aiki |
|--------|-------|------|
| Dependency types | 19 well-known + custom | None |
| Blocking detection | Graph-based, cached | Manual text field |
| Cycle detection | Yes (excludes non-blocking types) | N/A |
| Ready queue impact | 4 types filter `bd ready` | No dependency-based filtering |
| Cross-project deps | `tracks`, external refs | Not supported |

---

## 5. Provenance

### Beads: `discovered-from` Dependency

- Single dependency type linking child → parent
- Stored as a row in the dependencies table
- Non-blocking (doesn't affect `bd ready`)
- One direction only: "I found this issue while working on that one"
- No typed provenance -- all `discovered-from` edges look the same

### Aiki: `--source` Flag with Typed Prefixes

- Field on the `Created` event: `sources: Vec<String>`
- Multiple sources per task
- Typed prefixes distinguish origin:

| Prefix | Meaning | Example |
|--------|---------|---------|
| `file:` | Design doc / plan file | `file:ops/now/design.md` |
| `task:` | Follow-up from another task | `task:abc123` |
| `comment:` | Specific comment triggered this | `comment:c1a2b3c4` |
| `issue:` | External issue tracker | `issue:GH-123` |
| `prompt:` | User prompt that triggered work | `prompt:nzwtoqqr` |

- `--source prompt` auto-resolves to the current JJ change_id
- Queryable: `aiki task list --source file:ops/now/design.md` (partial match works)
- Template variable expansion: `{{source.task_id}}`, `{{source.type}}`, `{{source.path}}`

### Comparison

| Aspect | Beads `discovered-from` | Aiki `--source` |
|--------|------------------------|-----------------|
| Mechanism | Dependency edge | Event field |
| Multiplicity | One edge per pair | Multiple sources per task |
| Typed | No (single type) | Yes (6 prefix types) |
| Auto-resolution | No | `--source prompt` → JJ change_id |
| Template integration | No | Yes (variables: `source.*`) |
| Queryable | Graph traversal | `--source` filter with partial match |
| Bidirectional | Yes (edge goes both ways) | No (source → task only) |

Aiki's source system is richer in provenance classification. Beads' edge-based approach enables bidirectional graph traversal.

---

## 6. Ready Queue

### Beads `bd ready`

Algorithm:
1. Maintain `blocked_issues_cache` (materialized table)
2. An issue is ready if: `status == open`, not in blocked cache, not excluded by type
3. Excluded types: `gate`, `molecule`, `message`, `agent`, `role`, `rig`
4. Respect `defer_until` timestamps
5. Sort by configurable policy: `hybrid` (default), `priority`, `oldest`

Cache rebuild triggers: add/remove dependency, status change, close issue. Rebuild is <50ms on 10K issues.

### Aiki `get_ready_queue`

Algorithm:
1. Filter tasks where `status == Open`
2. Sort by priority (P0 first), then `created_at` (oldest first)
3. Apply scope filtering via `ScopeSet`:
   - If no in-progress tasks: show root-level tasks
   - If working on root task: show root-level tasks
   - If working on subtask of parent X: show other subtasks of X
4. Apply assignee filtering:
   - Agent sees: unassigned + assigned-to-this-agent
   - Human sees: unassigned + assigned-to-human

### Comparison

| Aspect | Beads | Aiki |
|--------|-------|------|
| Dependency-aware | Yes (graph-based blocking) | No |
| Scoped | No | Yes (ScopeSet narrows to active parent) |
| Assignee-filtered | Via `--assignee` flag | Built-in agent/human visibility |
| Deferred items | `defer_until` timestamp | Not supported |
| Sort policies | 3 (hybrid/priority/oldest) | 1 (priority then oldest) |
| Performance | Cached, <50ms on 10K | Replay + filter, O(n) |

---

## 7. Events and Audit Trail

### Beads: Event Table

11 event types: `Created`, `Updated`, `StatusChanged`, `Commented`, `Closed`, `Reopened`, `DependencyAdded`, `DependencyRemoved`, `LabelAdded`, `LabelRemoved`, `Compacted`.

Events are rows in the `events` table. The audit trail is a complete record of every mutation. Visible via `bd show <id>`.

### Aiki: Event-Sourced Changes

7 event types: `Created`, `Started`, `Stopped`, `Closed`, `Reopened`, `CommentAdded`, `Updated`.

Events ARE the storage. Each event is a JJ change with metadata in the description. Current state is materialized by replaying all events.

### Comparison

| Aspect | Beads | Aiki |
|--------|-------|------|
| Events are | Supplementary audit trail | The primary storage |
| State | Stored in SQLite (mutable) | Computed from events (immutable) |
| Batch operations | Individual events | `Started`, `Stopped`, `Closed`, `CommentAdded` support `Vec<task_id>` |
| Auto-stop on start | Not modeled | `Started.stopped` field records pre-empted tasks |
| Compaction | Events can be compacted (summarized) | Events are immutable (JJ handles) |

Aiki's batch events are efficient -- starting a new task auto-stops the current one in a single event. Beads would need separate status change events.

---

## 8. Comments

| Aspect | Beads | Aiki |
|--------|-------|------|
| Schema | `issue_id`, `author`, `content`, `timestamp` | `id` (JJ change_id), `text`, `timestamp`, `data: HashMap` |
| Storage | SQLite table | `CommentAdded` event on `aiki/tasks` branch |
| Author tracking | `author` field | Implicit (JJ change author) |
| Custom data | No | Yes (`data` field with key:value pairs) |
| Used as data source | No | Yes (templates can iterate over comments) |
| Batch | No | Yes (one event for multiple task IDs) |
| Referenceable | Not directly | Yes (`comment:<change_id>` as source) |

Aiki's comment system doubles as a data source for templates -- a review task's comments can automatically generate fix subtasks, each sourced from a specific comment. This is a unique capability.

---

## 9. Template System

### Beads: No Templates

Beads has no built-in template system. The closest equivalents:
- `bd create -f file.md` for batch creation from markdown
- `bd mol pour <proto>` for molecule templates (Gas Town)
- External tooling for patterns

### Aiki: Full Template Engine

Templates are Markdown files with YAML frontmatter in `.aiki/tasks/`:

```markdown
---
version: "1.0.0"
type: review
assignee: claude-code
subtasks: source.comments
---
# Review {{source.task_id}}

Review the changes for task {{source.task_id}}.

# Subtasks
## Fix: {{item.text}}
---
sources:
  - comment:{{item.id}}
  - task:{{parent.id}}
---
Investigate and fix: {{item.text}}
```

Features:
- **YAML frontmatter**: defaults for type, assignee, priority, data
- **Variable substitution**: `{{var}}` with namespaces (`data.*`, `source.*`, `parent.*`, `item.*`)
- **Conditionals**: `{% if %}`, `{% elif %}`, `{% else %}`, `{% endif %}` with full operator support
- **Loops**: `{% for item in collection %}` with loop metadata (`item.index`, `item.first`, `item.last`)
- **Data sources**: `subtasks: source.comments` iterates over a source task's comments
- **Auto-resolution**: `{{source.task_id}}` extracted from `--source task:abc`
- **Template IDs**: `name@version` format for versioning

This is one of aiki's strongest differentiators. The review workflow (`aiki review <task-id> --start`) uses templates to automatically create a parent review task with subtask-per-finding, each linked back to a comment via `source: comment:<id>`.

---

## 10. Agent Integration

### Beads

- **JSON output**: `--json` flag on every command
- **MCP plugin**: Tools for init, create, list, ready, show, update, close, dep, blocked, stats
- **`bd prime`**: Injects ~1-2K tokens of workflow context into agent session
- **`bd setup claude`**: Auto-installs hooks in Claude Code config
- **Robot flags** (beads_viewer): `--robot-insights`, `--robot-plan`, `--robot-priority`
- **Daemon**: Background process handles sync without agent involvement

### Aiki

- **XML output**: All commands output structured XML for agent parsing
- **Session tracking**: `claimed_by_session` ties tasks to agent sessions
- **Agent type detection**: Auto-detects Claude Code, Gemini CLI, Codex, etc.
- **Assignee-based visibility**: `get_ready_queue_for_agent()` filters by agent type
- **Task runner**: `aiki task run <id>` spawns an agent session for a task
- **Status monitor**: Real-time terminal display of task tree with symbols
- **CLAUDE.md integration**: Task system instructions injected into agent context
- **Flow events**: `TaskStarted` dispatches to event bus for hook integration

### Comparison

| Aspect | Beads | Aiki |
|--------|-------|------|
| Output format | JSON | XML |
| Agent setup | `bd setup claude` + `bd prime` | CLAUDE.md with instructions |
| Session binding | Via `--claim` flag | Automatic on `task start` |
| Multi-agent | Assignee field | Agent type detection + visibility filtering |
| Task execution | No built-in runner | `aiki task run` with status monitor |
| Background work | Daemon for sync | `aiki task run --async` |

---

## 11. Sync and Collaboration

### Beads

- Full 3-way merge sync via git
- Daemon handles periodic push/pull (30s intervals)
- Scalar conflicts: Last-Write-Wins
- Collection conflicts: Union merge (labels, deps)
- Comment conflicts: Append
- Supports multiple backends: git (default), Dolt, stealth (local-only)
- Federation support for cross-project references

### Aiki

- JJ-native sync (content-addressed changes avoid most conflicts)
- No daemon -- sync is manual or via JJ operations
- No custom merge logic needed (JJ change descriptions don't conflict)
- Single backend: JJ only

---

## 12. Features Unique to Each System

### Beads Only

| Feature | Description |
|---------|-------------|
| **Dependency graph** | 19 dependency types, 4 blocking, cached resolution |
| **Blocking propagation** | Recursive upward propagation through hierarchy |
| **Labels** | Tagging system with AND/OR filtering |
| **Compaction** | Semantic summarization of old closed issues |
| **Tombstones** | Soft-delete with TTL |
| **Duplicate detection** | Content-hash-based dedup + merge |
| **Stale detection** | `bd stale --days N` |
| **Daemon** | Background sync process |
| **Adaptive IDs** | 4-8 char IDs that grow with DB size |
| **Editor integration** | `bd edit` opens in `$EDITOR` |
| **Molecular chemistry** | Proto/Mol/Wisp phase states for Gas Town |
| **Gate coordination** | `waits-for` for fanout patterns |
| **Key-value store** | `bd kv get/set` for arbitrary data |
| **Migration system** | Schema migrations with inspect/dry-run |
| **External refs** | Link to GitHub issues, Jira tickets |
| **Multiple backends** | Git, Dolt, stealth mode |
| **Config system** | 3-layer config with many namespaces |

### Aiki Only

| Feature | Description |
|---------|-------------|
| **Template engine** | Full conditionals, loops, variable substitution |
| **Scope isolation** | `ScopeSet` narrows ready queue to active parent's subtasks |
| **`.0` planning subtask** | Auto-created when parent with subtasks is started |
| **Typed provenance** | `--source` with 6 typed prefixes and auto-resolution |
| **Event sourcing** | Events ARE the storage (immutable history) |
| **Batch events** | Start/stop/close/comment multiple tasks in one event |
| **Stopped status** | Distinct from "blocked" -- captures "was working, now paused" |
| **Review workflow** | `aiki review` with template-generated subtasks per finding |
| **Fix workflow** | `aiki fix <review-id>` creates follow-up tasks from review comments |
| **Task runner** | `aiki task run` spawns agent sessions |
| **Status monitor** | Real-time terminal tree display |
| **Agent visibility** | Ready queue filtered by agent type |
| **Flow integration** | Task events trigger hooks via event bus |
| **Comment-as-data-source** | Templates iterate over comments to generate subtasks |
| **JJ integration** | Working copy tracking, change_id linking |

---

## 13. Scale and Maturity

| Aspect | Beads | Aiki |
|--------|-------|------|
| Age | ~6 months (launched Jan 2026) | Younger |
| Stars | 1000+ | Private |
| Contributors | Steve Yegge + Claude + community | glasner |
| Ecosystem | 30+ community tools | Self-contained |
| Lines of code | ~20K Go (core) | ~12K Rust (task system) |
| Issue count tested | 10K+ | <1000 |
| MCP integration | Yes (official plugin) | No (CLAUDE.md-based) |
| Documentation | Extensive (FAQ, quickstart, architecture, troubleshooting) | CLAUDE.md + design docs |

---

## 14. Key Takeaways

### Where Beads is Stronger

1. **Dependency graph**: The 19-type dependency system with cached blocking propagation is beads' crown jewel. Aiki has nothing comparable.
2. **Scale**: SQLite-backed queries, adaptive IDs, and compaction handle large projects. Aiki's event replay doesn't scale past ~1000 tasks.
3. **Ecosystem**: 30+ community tools, MCP plugin, multiple backends. Beads is a platform.
4. **Sync**: Production-grade 3-way merge with daemon-managed sync. Aiki relies on JJ.

### Where Aiki is Stronger

1. **Scope isolation**: `ScopeSet` prevents agents from wandering. Beads' flat list requires manual filtering.
2. **Provenance**: Typed `--source` prefixes with auto-resolution are richer than `discovered-from`.
3. **Templates**: Full template engine with conditionals, loops, and data sources. Beads has nothing equivalent.
4. **Review workflow**: Template-driven review → fix pipeline with comment-to-subtask generation.
5. **Event sourcing**: Immutable history with batch operations. No state corruption possible.
6. **Agent UX**: Status monitor, task runner, agent visibility filtering, auto-stop on start.

### Where They're Comparable

1. **Hierarchy**: Both use dot-notation IDs, both support parent auto-close.
2. **Priority**: Nearly identical (P0-P3 vs 0-3).
3. **Ready queue**: Both compute actionable work, different filtering strategies.
4. **Agent-first**: Both designed for AI agents, not adapted from human tools.

### Architectural Contrast

Beads is a **database** -- it grew to encompass molecules, gates, wisps, HOP entities, and a full orchestration substrate. The Issue struct has 50+ fields because everything is an issue.

Aiki is a **focused tool** -- tasks are simple, templates handle complexity, and flows provide the orchestration layer. The Task struct has 18 fields because concerns are separated.

Both approaches are valid. Beads optimizes for querying a rich graph. Aiki optimizes for agent ergonomics within a larger system.

---

## Sources

- [steveyegge/beads (GitHub)](https://github.com/steveyegge/beads)
- [Beads Go Package](https://pkg.go.dev/github.com/steveyegge/beads)
- [Beads Quickstart](https://github.com/steveyegge/beads/blob/main/docs/QUICKSTART.md)
- [Beads FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)
- [Beads Community Tools](https://github.com/steveyegge/beads/blob/main/docs/COMMUNITY_TOOLS.md)
- [Dicklesworthstone/beads_viewer](https://github.com/Dicklesworthstone/beads_viewer)
- Aiki source: `cli/src/tasks/{types,id,storage,manager,mod,xml,runner,status_monitor,templates/*}.rs`
- Aiki source: `cli/src/commands/{task,review}.rs`
- [Aiki Task System Design](../done/task-system.md)
