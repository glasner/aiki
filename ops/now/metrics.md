# Aiki Metrics: The Learning Layer

> Implementing Evans' three-tier measurement architecture within aiki's event-sourced model.

## Problem

Aiki captures rich event streams — task lifecycle, session history, turn events, provenance — but provides no aggregation, analysis, or learning layer. Users can't answer:

- Why did Task A cost 90K tokens when Task B cost 12K?
- Which agents produce the highest-quality work?
- Are agents improving over time or degrading?
- What makes some tasks fast vs. slow?
- How does spec/instruction quality affect downstream outcomes?

## Design Principles

1. **Event-sourced, not database-backed** — Metrics live on `aiki/metrics` branch (same pattern as tasks and conversations)
2. **Computed from existing events** — Tier 2 (execution tracking) derives from session/turn/task events already captured
3. **Agent-reported for Tier 1** — Self-assessment is reported via `aiki task comment` with structured data
4. **Materialized on demand** — `aiki metrics` command replays events and computes signals (like `TaskGraph`)
5. **Extensible via task `data` field** — No schema changes needed for custom metrics

## Architecture: Three Tiers Mapped to Aiki

```
┌─────────────────────────────────────────────────────┐
│ TIER 1: AGENT SELF-ASSESSMENT                       │
│ Stored: task comments with data={} structured fields │
│ Source: agent reports via aiki task comment --data   │
└─────────────────────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────┐
│ TIER 2: EXECUTION TRACKING                          │
│ Stored: metrics events on aiki/metrics branch       │
│ Source: computed from turn/session/change events     │
└─────────────────────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────┐
│ TIER 3: LEARNING SIGNALS                            │
│ Computed: on-demand aggregation by aiki metrics cmd │
│ Output: JSON report, terminal summary               │
└─────────────────────────────────────────────────────┘
```

---

## Tier 1: Agent Self-Assessment

### How It Works Today

Aiki tasks already have:
- `data: HashMap<String, String>` — arbitrary key-value metadata on tasks
- `CommentAdded { data: HashMap<String, String> }` — structured data on comments
- `summary: Option<String>` — close summary

### What We Add

A convention (not new code) for agents to report assessment data when closing tasks:

```bash
# Agent reports self-assessment when closing
aiki task close <id> \
  --summary "Implemented auth with JWT tokens" \
  --data confidence=0.92 \
  --data quality_score=0.85 \
  --data input_quality=0.90 \
  --data challenges="WebSocket auth required custom middleware" \
  --data decisions="Used Turbo Streams for real-time;considered ActionCable,Polling" \
  --data deviations="none" \
  --data risks="WebSocket connection reliability"
```

Or via structured comment during work:

```bash
# Mid-task assessment
aiki task comment <id> "Architecture phase complete" \
  --data confidence=0.95 \
  --data phase=architecture \
  --data patterns="REST,Turbo Streams" \
  --data assumptions="User model has notification_preferences"
```

### Data Schema Convention

These keys are recognized by `aiki metrics` but not enforced — any key works:

| Key | Type | Description |
|-----|------|-------------|
| `confidence` | 0.0-1.0 | Agent's certainty in output |
| `quality_score` | 0.0-1.0 | Agent's assessment of input quality |
| `input_quality` | 0.0-1.0 | Quality of spec/instructions received |
| `challenges` | string | Challenges encountered (semicolon-delimited) |
| `decisions` | string | Key decisions made (semicolon-delimited) |
| `deviations` | string | Deviations from plan ("none" if faithful) |
| `risks` | string | Risks identified (semicolon-delimited) |
| `patterns` | string | Architectural patterns used (comma-delimited) |
| `assumptions` | string | Assumptions made (semicolon-delimited) |
| `skills` | string | Skills/rules applied (comma-delimited, e.g. "BR-01,BR-08,FR-02") |

### CLAUDE.md Integration

Agents learn to report metrics via instructions in CLAUDE.md:

```markdown
## Task Completion

When closing tasks, include self-assessment data:

aiki task close <id> --summary "What you did" \
  --data confidence=0.92 \
  --data quality_score=0.88

Confidence guide:
- 0.95+: Simple CRUD, well-understood patterns
- 0.85-0.95: Standard features with some complexity
- 0.70-0.85: Complex state transitions, edge cases
- <0.70: Uncertain, novel territory, needs review
```

### Implementation

**What changes:**
- `aiki task close` and `aiki task comment` already accept `--data` — no CLI changes
- `data` fields are already stored in task events — no storage changes
- Only new: `aiki metrics` reads these fields during aggregation

**Cost: Zero code changes for Tier 1.**

---

## Tier 2: Execution Tracking

### What Aiki Already Captures

| Data Point | Source | Location |
|---|---|---|
| Task duration | `Started.timestamp` → `Closed.timestamp` | `aiki/tasks` branch |
| Turn count per session | `ConversationEvent::Prompt` count | `aiki/conversations` branch |
| Files changed per turn | `turn.completed.modified_files` | Event payload |
| Agent type | `session.agent_type` | Every event |
| Session duration | `SessionStart.timestamp` → `SessionEnd.timestamp` | `aiki/conversations` |

### What's Missing: Token & Cost Tracking

Aiki does not currently capture token counts or costs. Two approaches:

#### Option A: Hook-Based Capture (Recommended)

Add token tracking to `turn.completed` event payloads. Claude Code and Cursor expose usage info in their hook payloads — aiki can extract this:

```rust
// In AikiTurnCompletedPayload (new optional fields)
pub struct AikiTurnCompletedPayload {
    // ... existing fields ...

    /// Token usage for this turn (if available from editor)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TurnUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnUsage {
    /// Input tokens consumed
    pub input_tokens: Option<u64>,
    /// Output tokens generated
    pub output_tokens: Option<u64>,
    /// Model used for this turn
    pub model: Option<String>,
    /// Estimated cost in USD (computed from model pricing)
    pub cost_usd: Option<f64>,
}
```

The flow engine can populate this from editor-specific payloads:

```yaml
# In aiki/default flow
turn.completed:
  - let:
      usage: $event.usage
    if: $event.usage
    then:
      - action:
          type: task.comment
          task: $task.current
          data:
            turn_input_tokens: $usage.input_tokens
            turn_output_tokens: $usage.output_tokens
            turn_model: $usage.model
            turn_cost_usd: $usage.cost_usd
```

#### Option B: Agent Self-Report

Agents report their own token usage when closing tasks (simpler, less accurate):

```bash
aiki task close <id> --summary "Done" \
  --data input_tokens=45000 \
  --data output_tokens=8200 \
  --data model=claude-sonnet-4-5 \
  --data cost_usd=0.18
```

#### Recommendation: Option A First, Option B as Fallback

Option A is more accurate (editor provides real numbers) and requires less agent training. Option B is a good fallback for editors that don't expose usage.

### Metrics Event Storage

New event type on `aiki/metrics` branch for per-task aggregated execution data:

```rust
/// Events stored on aiki/metrics branch
pub enum MetricsEvent {
    /// Execution metrics for a completed task
    TaskMetrics {
        task_id: String,
        /// Duration from start to close (seconds)
        duration_seconds: Option<f64>,
        /// Number of turns in the session(s) that worked on this task
        turn_count: Option<u32>,
        /// Files created or modified
        files_changed: Option<u32>,
        /// Total input tokens across all turns
        total_input_tokens: Option<u64>,
        /// Total output tokens across all turns
        total_output_tokens: Option<u64>,
        /// Total estimated cost in USD
        total_cost_usd: Option<f64>,
        /// Model(s) used
        models: Vec<String>,
        /// Agent type
        agent_type: String,
        timestamp: DateTime<Utc>,
    },
}
```

**Storage pattern:** Same as tasks — `[aiki-metrics]...[/aiki-metrics]` blocks in JJ change descriptions on `aiki/metrics` branch.

### When Metrics Events Are Written

**On task close** — the `task.closed` event handler computes and writes execution metrics:

```rust
// In handle_task_closed:
// 1. Look up task from graph (has started_at, timestamps)
// 2. Query session history for turn count
// 3. Sum token usage from task data/comments
// 4. Write MetricsEvent::TaskMetrics to aiki/metrics branch
```

This is triggered automatically by the flow engine when tasks close — no manual action needed.

---

## Tier 3: Learning Signals

### Nine Signals (Computed, Not Stored)

These are computed on-demand by `aiki metrics` from Tier 1 + Tier 2 data:

#### 1. Task Complexity

```
Input: duration, turn_count, files_changed, agent confidence
Output: simple | medium | complex | very_complex

Rules:
  simple:       duration < 10min AND turns < 5 AND confidence > 0.90
  medium:       duration < 30min AND turns < 15
  complex:      duration < 60min AND turns < 30
  very_complex: everything else
```

#### 2. Input Quality Score

```
Source: task.data["input_quality"] or task.data["quality_score"]
       (reported by agent in Tier 1)
```

#### 3. Average Agent Confidence

```
Source: average of task.data["confidence"] across all tasks
        (or per-phase if subtasks exist)
```

#### 4. Implementation Quality Score

```
Source: review task's data["quality_score"]
        (the reviewing agent assesses implementation quality)
```

#### 5. Plan-to-Implementation Fidelity

```
Source: task.data["deviations"]
Score: 1.0 if "none", decreasing with number of deviations
```

#### 6. Skills Referenced

```
Source: task.data["skills"] and task.data["patterns"]
Aggregated across all tasks for pattern detection
```

#### 7. Required Clarifications (Future)

```
Source: count of AskUserQuestion tool calls per task
        (from turn events or agent self-report)
```

#### 8. External Research (Future)

```
Source: count of WebSearch/WebFetch tool calls per task
        (from shell.completed or web.completed events)
```

#### 9. Similar Tasks (Future)

```
Source: embedding-based similarity on task names + summaries
        (requires external embedding service)
```

---

## CLI: `aiki metrics`

### Commands

```bash
# Show metrics for a specific task
aiki metrics <task-id>

# Show metrics for all closed tasks (default: last 30 days)
aiki metrics --all [--since 2026-01-01]

# Show aggregated summary
aiki metrics summary [--since 2026-01-01]

# Export as JSON (for external analysis)
aiki metrics --all --json

# Compare metrics between task groups
aiki metrics compare --source file:plan-a.md --source file:plan-b.md
```

### Output: `aiki metrics <task-id>`

```
Task: xqrmnpst — Implement user authentication
Status: closed (done)
Duration: 42m 18s
Turns: 12
Files changed: 8
Tokens: 90,200 input / 12,400 output
Cost: $0.38
Model: claude-sonnet-4-5

Self-Assessment:
  Confidence: 0.88
  Input quality: 0.92
  Challenges: WebSocket auth required custom middleware
  Deviations: none

Complexity: medium
```

### Output: `aiki metrics summary`

```
Metrics Summary (last 30 days, 20 tasks)
─────────────────────────────────────────

Cost:
  Total: $41.20
  Average: $2.06/task
  Range: $0.45 — $2.80
  Most expensive phase: architecture ($1.01 avg with Opus)

Quality:
  Avg confidence: 0.93
  Avg input quality: 0.88
  Avg implementation quality: 0.92

Performance:
  Avg duration: 42 min
  Avg turns: 14
  Revision cycles: 0.2/task

Complexity Distribution:
  simple: 6 (30%)  medium: 9 (45%)  complex: 4 (20%)  very_complex: 1 (5%)

Trends (10-task rolling average):
  Duration: 48min → 38min (↓21%)
  Cost: $2.40 → $1.85 (↓23%)
  Quality: 0.89 → 0.94 (↑6%)
```

### Output: `aiki metrics --all --json`

```json
{
  "schema_version": "1.0",
  "generated_at": "2026-02-21T10:00:00Z",
  "tasks": [
    {
      "task_id": "xqrmnpst...",
      "name": "Implement user authentication",
      "execution": {
        "duration_seconds": 2538,
        "turn_count": 12,
        "files_changed": 8,
        "total_input_tokens": 90200,
        "total_output_tokens": 12400,
        "total_cost_usd": 0.38,
        "models": ["claude-sonnet-4-5"],
        "agent_type": "claude-code"
      },
      "self_assessment": {
        "confidence": 0.88,
        "quality_score": 0.92,
        "input_quality": 0.92,
        "challenges": ["WebSocket auth required custom middleware"],
        "deviations": [],
        "risks": ["WebSocket connection reliability"],
        "skills": ["BR-01", "BR-08", "FR-02"]
      },
      "learning_signals": {
        "complexity": "medium",
        "input_quality_score": 0.92,
        "avg_confidence": 0.88,
        "plan_fidelity": 1.0,
        "skills_referenced": ["BR-01", "BR-08", "FR-02"]
      }
    }
  ],
  "summary": {
    "total_cost_usd": 41.20,
    "avg_cost_usd": 2.06,
    "avg_duration_seconds": 2520,
    "avg_confidence": 0.93,
    "complexity_distribution": {
      "simple": 6, "medium": 9, "complex": 4, "very_complex": 1
    }
  }
}
```

---

## Implementation Plan

### Phase 1: Conventions + Read Path (No Code Changes)

**Goal:** Start capturing Tier 1 data immediately using existing infrastructure.

1. Update CLAUDE.md to instruct agents to report `--data confidence=X quality_score=Y` on close
2. Write `aiki metrics` command that reads task events and extracts existing data fields
3. Compute duration from `Started.timestamp` → `Closed.timestamp`
4. Display basic per-task and summary metrics

**Files changed:**
- `cli/src/commands/metrics.rs` (new) — the `aiki metrics` command
- `cli/src/commands/mod.rs` — register new command
- `cli/src/main.rs` — add CLI subcommand
- `CLAUDE.md` / `AGENTS.md` — add self-assessment instructions

**Estimated scope:** ~500 lines of new code. No changes to existing types.

### Phase 2: Execution Tracking (Tier 2)

**Goal:** Capture token usage and compute per-task cost.

1. Add `TurnUsage` struct to `turn.completed` payload (optional fields)
2. Update flow engine to extract usage from editor payloads
3. Add `aiki/metrics` branch and `MetricsEvent` type
4. Write `TaskMetrics` events on task close
5. Incorporate token/cost data into `aiki metrics` output

**Files changed:**
- `cli/src/events/turn_completed.rs` — add `usage` field
- `cli/src/metrics/` (new module) — types, storage, computation
- `cli/src/flows/core/functions.rs` — add `extract_usage()` native function
- `cli/src/events/task_closed.rs` — trigger metrics computation

**Estimated scope:** ~800 lines. Minimal changes to existing types (one optional field).

### Phase 3: Learning Signals (Tier 3)

**Goal:** Compute aggregated signals, enable trend analysis and predictions.

1. Implement complexity classification
2. Add rolling-average trend computation
3. Add cost prediction from similar tasks
4. Add pattern detection (which skills correlate with quality)
5. Add `aiki metrics compare` for A/B analysis

**Files changed:**
- `cli/src/commands/metrics.rs` — extend with signals + trends
- `cli/src/metrics/signals.rs` (new) — signal computation
- `cli/src/metrics/trends.rs` (new) — trend analysis

**Estimated scope:** ~600 lines. No changes to existing types.

### Phase 4: Learning Loop (Future)

**Goal:** Auto-improving skills and predictive quality gates.

1. Violation pattern detection → auto-suggest Sacred Rules
2. Predictive quality gates (warn before implementation if signals are low)
3. Adaptive model selection recommendations
4. Integration with `aiki review` for quality-aware review triggers

This phase depends on having 20+ tasks with metrics data to analyze.

---

## Design Decisions

### Why Not a Separate Database?

Aiki's storage philosophy is "everything in JJ." Metrics follow this:
- **Consistency**: Same materialization pattern as tasks (events → graph)
- **Portability**: Clone the repo, get the metrics
- **No dependencies**: No SQLite, no external service
- **Audit trail**: Immutable event history

### Why Not Store Signals?

Learning signals (Tier 3) are computed, not stored, because:
- Algorithms change — recomputation gives updated results
- Adding new signals doesn't require backfilling
- Storage cost is zero (just replay events)
- Query patterns are always "all closed tasks" (full scan is fine for < 10K tasks)

### Why Convention Over Schema for Tier 1?

Agent self-assessment uses the existing `data` HashMap with key conventions rather than new typed fields because:
- Zero code changes to start capturing data today
- Different agents/teams can extend with custom keys
- No migration needed when adding new assessment dimensions
- `aiki metrics` simply ignores unknown keys

### Why `aiki/metrics` Branch (Not Task Data)?

Tier 2 execution metrics go on a separate branch because:
- They're derived/computed, not primary data
- Recomputation should be possible without affecting task history
- Different retention policies may apply
- Keeps task events focused on task lifecycle

---

## Mapping to Evans' Article

| Evans Concept | Aiki Implementation |
|---|---|
| orchestration.json | `aiki metrics <task-id> --json` output |
| Agent self-assessment | `--data confidence=X` on task close |
| Per-phase tracking | Parent task with subtasks, each with own metrics |
| Execution tracking | Turn events + TurnUsage + MetricsEvent |
| Learning signals | Computed by `aiki metrics summary` |
| Cost prediction | `aiki metrics compare` with complexity matching |
| Quality prediction | Input quality → confidence → outcome correlation |
| Pattern detection | Skills/patterns aggregation across tasks |
| Continuous improvement | Rolling-average trends in summary output |

### What Evans Has That We Don't Need

- **orchestration.json file per feature** — Aiki's event-sourced model is richer; metrics are computed from the event stream, not stored as a single document
- **Fixed phase structure** (architect → engineer → reviewer) — Aiki tasks are flexible; subtasks naturally model phases
- **Ruby/Rails-specific skills** — Aiki is language-agnostic; skills are user-defined via `data["skills"]`

### What Aiki Has That Evans Doesn't

- **Provenance tracking** — Line-level attribution to specific agent + session + turn
- **DAG-based task relationships** — Subtasks, blocking, orchestration links
- **Flow engine integration** — Metrics computation triggered automatically by events
- **Multi-editor support** — Metrics across Claude Code, Cursor, Codex in a single view
