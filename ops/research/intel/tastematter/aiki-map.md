# TasteMatter — Aiki Relevance Map

Aiki's core wedge: **autonomous review** + structured task tracking for agentic development workflows.

TasteMatter's core wedge: **passive visibility** into Claude Code session history via file access pattern indexing.

## Capability Classifications

| Capability | Overlap with Aiki Wedge | Threat | Opportunity | Why Now |
|---|---|---|---|---|
| Background session indexing | Low | Low | Counter | See below |
| File hotness tracking (HOT/WARM/COLD) | Low | Low | Copy | See below |
| Session tracking (cross-session file history) | Medium | Medium | Counter | See below |
| Attention drift detection (plan vs. reality) | Medium | Medium | Copy | See below |
| File relationship mapping | Low | Low | Ignore | See below |
| Abandoned work timestamps | Low–Medium | Low | Copy | See below |
| Context trails / CLI query interface | Low | Low | Ignore | See below |
| Local-only data architecture | High (shared) | Low | Ignore | See below |

## Detailed Reasoning

### 1. Background Session Indexing
- **Overlap: Low** — Aiki tracks work explicitly via task start/close/comments; TasteMatter infers passively from Claude Code session JSONL files. Fundamentally different paradigms (active structured tracking vs. passive observation).
- **Threat: Low** — Passive indexing complements but doesn't replace active task orchestration. A developer using TasteMatter still needs a system to *manage* work, not just *see* it.
- **Opportunity: Counter** — Aiki's explicit task tracking produces richer context (why something was done, not just what files were touched). Position this as "structured provenance > inferred trails."
- **Why now:** Claude Code adoption is growing. As session count per project increases, the "what happened?" problem becomes real. TasteMatter validates this pain point.

### 2. File Hotness Tracking (HOT/WARM/COLD)
- **Overlap: Low** — Aiki doesn't currently track file access frequency or recency. Aiki's task diffs show *what changed*, but not *how actively* a file is being worked on.
- **Threat: Low** — File hotness is a visibility signal, not a workflow tool. It informs decisions but doesn't make them.
- **Opportunity: Copy** — File hotness could enhance Aiki's review prioritization. Hot files = higher review urgency. Could integrate as a lightweight signal in `aiki review` to surface files that are changing rapidly and may need more scrutiny.
- **Why now:** In autonomous agent workflows, many files get touched in quick succession. Hotness tracking helps developers know where to focus human attention — aligns with Aiki's review wedge.

### 3. Session Tracking (Cross-Session File History)
- **Overlap: Medium** — Aiki's task system tracks what was done across sessions via task summaries and JJ history. TasteMatter achieves similar visibility through automatic session file parsing. Both solve "what happened across sessions?" but differently.
- **Threat: Medium** — If developers prefer zero-effort passive tracking over explicit task management, TasteMatter's approach is more frictionless. The "just works" appeal could pull users who find Aiki's task discipline too effortful.
- **Opportunity: Counter** — Aiki's explicit approach captures *intent* and *context* (task descriptions, sources, summaries), not just file access. Position as: "TasteMatter shows you touched auth.rs 12 times; Aiki tells you *why* and whether the work is done."
- **Why now:** Claude Code sessions are ephemeral by design. Both tools recognize this gap. TasteMatter's alpha launch validates market timing.

### 4. Attention Drift Detection (Plan vs. Reality)
- **Overlap: Medium** — Aiki's task→subtask→source chain creates a plan-to-execution trail, but doesn't explicitly measure or visualize drift. TasteMatter's "attention drift" feature directly compares planned work to actual file access patterns.
- **Threat: Medium** — This is TasteMatter's most compelling differentiated feature. Drift detection addresses a real pain in autonomous workflows where agents frequently go off-script. If developers value this signal, it could drive adoption.
- **Opportunity: Copy** — Drift detection could enhance Aiki's task tracking. Compare task descriptions/sources against actual file changes (from `aiki task diff`) to surface when work diverges from plan. This fits naturally into the review workflow.
- **Why now:** Autonomous agents frequently drift from instructions. As agent autonomy increases, the gap between "what was planned" and "what was built" grows. This is the most time-sensitive capability to address.

### 5. File Relationship Mapping
- **Overlap: Low** — Aiki doesn't map file relationships. This is a code intelligence feature, not a workflow/review feature.
- **Threat: Low** — Tangential to Aiki's core value prop. Code intelligence tools (LSPs, IDE features) already serve this need.
- **Opportunity: Ignore** — Not aligned with Aiki's review/task orchestration wedge. Building this would dilute focus.
- **Why now:** Marginally useful in large codebases, but well-served by existing tools. No urgent timing signal.

### 6. Abandoned Work Timestamps
- **Overlap: Low–Medium** — Aiki tasks can be stopped with reasons (`aiki task stop --reason`), but stale/abandoned tasks aren't automatically surfaced or highlighted. TasteMatter passively detects abandoned work via time-since-last-access.
- **Threat: Low** — Nice-to-have visibility, not a workflow driver.
- **Opportunity: Copy** — Aiki could add a lightweight "stale task" detector — tasks that have been `in_progress` for too long without comments or activity. This fits naturally into the existing task model and would be trivial to implement.
- **Why now:** Multi-agent workflows increase the rate of abandoned work. Agents start tasks, get interrupted, and context is lost. Surfacing stale work prevents accumulation of zombie tasks.

### 7. Context Trails / CLI Query Interface
- **Overlap: Low** — Aiki's `aiki task list` and `aiki task show` provide structured task history. TasteMatter's query interface (`tastematter query flex --time 7d`) is oriented around file access patterns rather than task state.
- **Threat: Low** — Different paradigms. Developers who want to query "which files were hot last week?" and developers who want to query "what tasks were completed?" have overlapping but distinct needs.
- **Opportunity: Ignore** — Aiki's value is in orchestrating work, not in providing a general-purpose query layer over file access history. The task system already serves the "what happened?" question with richer context.
- **Why now:** No urgent timing signal. Aiki's existing query capabilities (`aiki task list --source`) are sufficient for its use case.

### 8. Local-Only Data Architecture
- **Overlap: High (shared philosophy)** — Both Aiki and TasteMatter store all data locally. Aiki uses JJ (local version control); TasteMatter indexes session files on disk.
- **Threat: Low** — This is a shared design principle, not a competitive axis. Both tools appeal to privacy-conscious developers.
- **Opportunity: Ignore** — Aiki already does this. No action needed.
- **Why now:** Developer privacy concerns are increasing, but both tools already address this. No competitive advantage to be gained.

## Summary Assessment

**Overall threat level: Low–Medium.** TasteMatter occupies an adjacent but distinct space. It's a *visibility* tool (passive observation); Aiki is a *workflow* tool (active orchestration + autonomous review). The two could coexist or even complement each other.

**Key risk:** If TasteMatter expands from visibility into workflow (adding task management, review triggers, or build loops), it becomes a direct competitor. The "attention drift" feature is the closest to Aiki's territory and the most likely expansion vector.

**Top 3 opportunities to absorb:**
1. **Attention drift detection** (Copy) — Most strategically urgent. Integrate plan-vs-reality comparison into Aiki's review workflow.
2. **File hotness for review prioritization** (Copy) — Low-effort enhancement to Aiki's review. Use access frequency to prioritize which files get reviewed.
3. **Stale task detection** (Copy) — Trivial to implement. Surface tasks that have gone stale to prevent zombie work accumulation.

**Counter-positioning:** Aiki's explicit tracking captures *intent* (why work was done, what was planned). TasteMatter's passive tracking captures *behavior* (what files were accessed). Aiki's approach is richer for autonomous workflows where provenance and accountability matter. Message: "Context trails show you what happened. Aiki tells you what *should* have happened and whether it did."
