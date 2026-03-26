# Opportunity Scoring: steveyegge/beads

**Date:** 2026-03-05
**Source:** `ops/now/intel/beads/aiki-map.md`, `ops/now/intel/beads/research.md`
**Scoring formula:** `score = 0.35*pain + 0.35*fit + 0.20*gtm + 0.10*(6-complexity)`

---

## Ranked Opportunity Table

| Rank | Opportunity | Pain | Fit | GTM | Complexity | Score |
|------|-----------|------|-----|-----|------------|-------|
| 1 | Review Quality Metrics & Scoring | 4 | 5 | 5 | 2 | **4.55** |
| 2 | Auto Re-Review Cycles | 4 | 5 | 4 | 2 | **4.35** |
| 3 | Review-Aware Orchestration Deepening | 4 | 5 | 4 | 2 | **4.35** |
| 4 | Review-Specific Dashboards | 3 | 4 | 4 | 1 | **3.75** |
| 5 | Task Summarization / Memory Compaction | 4 | 4 | 3 | 3 | **3.70** |
| 6 | Structured Cross-Session Memory | 4 | 3 | 4 | 4 | **3.45** |
| 7 | Inter-Agent Messaging | 3 | 3 | 3 | 2 | **3.10** |
| 8 | Atomic Task Claiming | 3 | 3 | 2 | 4 | **2.70** |
| 9 | MCP Server Integration | 2 | 2 | 3 | 3 | **2.30** |
| 10 | Blocking Dependencies | 2 | 2 | 2 | 4 | **2.00** |

---

## Top 3 Summary

**1. Review Quality Metrics & Scoring (4.55)** — The highest-scoring opportunity and Aiki's strongest potential differentiator. Beads has no concept of review quality; Aiki can own "quantified code review" by tracking fix rates, issue severity distributions, and regression patterns. This is hard for Beads to replicate because it requires deep review semantics that Beads' generic task model doesn't capture.

**2. Auto Re-Review Cycles (4.35)** — Automating the review → fix → verify loop is the heart of Aiki's wedge. Today, agents must be manually re-triggered to re-review after fixes. Closing this gap turns Aiki from a review tool into a review engine. Combined with quality metrics (#1), this creates a self-improving review pipeline.

**3. Review-Aware Orchestration Deepening (4.35)** — Gas Town is a generic orchestrator; Aiki's orchestration should understand code review semantics. This means cascading fix propagation (a fix in module A triggers re-review of dependent modules), review prioritization by severity, and intelligent work distribution based on review context. This counters Gas Town's scale advantage with domain depth.

---

## Detailed Write-Ups

### 1. Review Quality Metrics & Scoring

**Score: 4.55** | Pain: 4 | Fit: 5 | GTM: 5 | Complexity: 2

**Description:** Track and surface review effectiveness metrics: fix rates (what percentage of review issues get resolved), issue recurrence (do the same patterns reappear), severity distributions, and per-review quality scores. Enable developers to measure whether autonomous reviews are actually improving code quality over time.

**Rationale:**
- **Pain (4):** Developers using autonomous review have no feedback loop — they can't tell if reviews are effective, trending better, or catching real issues vs. generating noise. This is a universal problem with code review at scale.
- **Fit (5):** This is uniquely aligned with Aiki's autonomous review wedge. Beads has no review concept at all; quality metrics are impossible without structured review data, which Aiki already collects via `aiki review` and issue tracking with severity levels.
- **GTM (5):** "Quantified code review" is a compelling narrative. Metrics dashboards create shareable content (blog posts, screenshots, conference talks). Enterprise buyers specifically ask for quality metrics to justify tooling adoption.
- **Complexity (2):** Requires defining meaningful metrics, building data collection across review cycles, computing aggregate scores, and presenting results. Needs careful design to avoid vanity metrics.

**Evidence:**
- Beads has no review concept — [README](https://github.com/steveyegge/beads), [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)
- Aiki already has structured review data: `aiki review`, `aiki review issue add` with severity levels — `ops/now/intel/beads/aiki-map.md` (Key Differentiators table)
- Community criticism of Beads: "requires explicit prompting; agents don't proactively use it" — agents need measurable outcomes to self-correct — [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)

---

### 2. Auto Re-Review Cycles

**Score: 4.35** | Pain: 4 | Fit: 5 | GTM: 4 | Complexity: 2

**Description:** After a fix agent closes its fix task, automatically trigger a re-review of the changed files to verify the fix is correct and hasn't introduced regressions. Continue the cycle until the reviewer reports no remaining issues or a maximum iteration count is reached.

**Rationale:**
- **Pain (4):** Manual re-triggering of reviews after fixes is tedious and error-prone. Developers must remember to re-review, figure out which files changed, and verify fixes didn't introduce new issues. This friction causes many fixes to ship without verification.
- **Fit (5):** This is the core loop of autonomous code review — review → fix → verify. Automating it is the single most impactful thing Aiki can do for its wedge. The review-fix cycle is already implicit in `aiki review` → `aiki fix`; making it automatic is a natural extension.
- **GTM (4):** "Fix and verify automatically" is a compelling demo — show a review finding an issue, an agent fixing it, and the system automatically confirming the fix. This is the kind of workflow automation that sells tools.
- **Complexity (2):** Requires review diffing (what changed since last review), trigger logic (when to re-review), loop termination (max iterations, convergence detection), and coordination between review and fix agents.

**Evidence:**
- Beads/Gas Town is a generic task dispatcher with no review loop concept — [Gas Town blog](https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04)
- Aiki's `aiki review` → `aiki fix` pipeline exists but requires manual re-triggering — `ops/now/intel/beads/aiki-map.md` (Strategic Recommendations #1)
- Context rot in long sessions (Beads' known limitation) is partially caused by unbounded review cycles — [Ian Bull blog](https://ianbull.com/posts/beads/)

---

### 3. Review-Aware Orchestration Deepening

**Score: 4.35** | Pain: 4 | Fit: 5 | GTM: 4 | Complexity: 2

**Description:** Enhance Aiki's task orchestration to understand code review semantics: cascading re-reviews when fixes affect dependent modules, priority routing based on issue severity, intelligent work distribution that considers review context, and fix propagation tracking across the codebase.

**Rationale:**
- **Pain (4):** When a fix in one module affects others, developers must manually identify and re-review downstream dependencies. This is especially painful in large codebases where a single fix can cascade through multiple files. Currently agents don't track these relationships.
- **Fit (5):** This directly counters Gas Town's generic orchestration with domain-specific intelligence. Beads/Gas Town dispatches tasks; Aiki would dispatch reviews with semantic understanding of what needs re-reviewing and why. This is the deepest expression of the autonomous review wedge.
- **GTM (4):** "Orchestration that understands code review" is a strong positioning statement against Gas Town's "dispatch anything to 30 agents." Depth vs. breadth is a compelling story for developers who care about review quality.
- **Complexity (2):** Requires dependency analysis (which modules are affected by a fix), priority computation from severity scores, cascading logic, and integration with the existing `aiki run` orchestration.

**Evidence:**
- Gas Town manages 20-30 agents but with generic task dispatch, no review semantics — [Gas Town GitHub](https://github.com/steveyegge/gastown), [SE Daily](https://softwareengineeringdaily.com/2026/02/12/gas-town-beads-and-the-rise-of-agentic-development-with-steve-yegge/)
- Aiki already has `aiki run --async` + `aiki task wait` for parallel execution — `ops/now/intel/beads/aiki-map.md` (#15)
- Strategic recommendation: "deepen review-specific orchestration rather than copying Gas Town" — `ops/now/intel/beads/aiki-map.md` (Strategic Recommendations #1)

---

### 4. Review-Specific Dashboards

**Score: 3.75** | Pain: 3 | Fit: 4 | GTM: 4 | Complexity: 1

**Description:** Build review-specific visualization tools: diff views showing review iterations (what changed between review rounds), fix-rate dashboards tracking resolution over time, and review quality trend charts. These are harder for Beads to replicate because they require deep understanding of code review semantics.

**Rationale:**
- **Pain (3):** Developers manage without visualization but waste time manually tracking review progress. The pain is moderate — workarounds exist (reading task comments, checking diffs manually) but they're inefficient.
- **Fit (4):** Directly serves the review wedge with visual tooling. Generic TUIs (like Beads' community tools) don't understand review semantics. Review-specific views are a natural extension of `aiki review` data.
- **GTM (4):** Visual demos sell tools. Screenshots and screen recordings of review dashboards create shareable marketing content. Beads' community has built generic TUIs; Aiki can differentiate with review-specific visualization.
- **Complexity (1):** Building a web UI or TUI with data pipelines and visualization is a significant engineering effort. Needs frontend development, data aggregation, and ongoing maintenance.

**Evidence:**
- Beads community has generic TUIs (beads_viewer, lazybeads, bdui) and web UIs but none review-specific — [COMMUNITY_TOOLS.md](https://github.com/steveyegge/beads/blob/main/docs/COMMUNITY_TOOLS.md), [Ian Bull blog](https://ianbull.com/posts/beads/)
- Strategic recommendation: "Aiki's ecosystem should be review-specific: diff viewers, review iteration trackers, fix-rate dashboards" — `ops/now/intel/beads/aiki-map.md` (Strategic Recommendations #3)

---

### 5. Task Summarization / Memory Compaction

**Score: 3.70** | Pain: 4 | Fit: 4 | GTM: 3 | Complexity: 3

**Description:** Automatically generate condensed summaries of completed task trees. When a parent task with subtasks is closed, produce a semantic summary of all work done, decisions made, and outcomes — stored in JJ for future agents to reference. This addresses "context rot" in long sessions and enables efficient cross-session knowledge transfer.

**Rationale:**
- **Pain (4):** Context rot is a universal agent problem. As sessions grow longer and span more tasks, context window pressure increases. Beads identified this as a core pain point and built `bd admin compact` to address it. Aiki currently relies on manual `--summary` flags and CLAUDE.md, which are inconsistent.
- **Fit (4):** Directly supports longer autonomous review sessions. Review workflows are inherently multi-step (review → fix → re-review), and context accumulates across iterations. Automatic summarization keeps review context manageable without losing critical information.
- **GTM (3):** Addresses a known Beads strength ("memory decay") but isn't a headline feature. More of a "keep up" move than a differentiator.
- **Complexity (3):** Moderate — requires LLM-based summarization, storage in JJ, and integration with task close workflow. The summarization quality is critical; poor summaries are worse than none.

**Evidence:**
- Beads `bd admin compact` performs semantic summarization — [README](https://github.com/steveyegge/beads)
- Context rot still occurs in long Beads sessions (known limitation) — [Ian Bull blog](https://ianbull.com/posts/beads/), [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)
- Aiki has no compaction story — `ops/now/intel/beads/aiki-map.md` (#3)

---

### 6. Structured Cross-Session Memory

**Score: 3.45** | Pain: 4 | Fit: 3 | GTM: 4 | Complexity: 4

**Description:** Implement `aiki remember` / `aiki recall` / `aiki forget` for structured, persistent agent memory. Store project-specific preferences, patterns, and decisions in a dedicated JJ branch. Auto-inject relevant memories into agent context at session start. This solves the "50 First Dates" problem where agents forget learned preferences between sessions.

**Rationale:**
- **Pain (4):** Agents losing learned preferences between sessions is a widely recognized pain point. Beads' memory feature is one of its most discussed capabilities. Currently Aiki agents rely on CLAUDE.md (static) and auto-memory directories (agent-specific, unstructured).
- **Fit (3):** Useful but not specifically tied to the review wedge. Memory benefits all agent workflows, not just code review. However, review-specific memories (e.g., "this project uses strict null checks", "always check for SQL injection in this module") would be valuable.
- **GTM (4):** Direct comparison to Beads' most talked-about feature. "Our agents remember" is a simple, compelling message. Beads' launch was framed around the memory metaphor; matching this feature removes a competitive disadvantage.
- **Complexity (4):** Relatively straightforward — key-value store on a JJ branch, CLI commands, context injection hook. The core implementation is simple; the challenge is curating useful memories vs. noise.

**Evidence:**
- Beads `bd remember`/`bd recall`/`bd forget` — [Releases](https://github.com/steveyegge/beads/releases), [Steve Yegge Medium](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a)
- "50 First Dates" framing in Beads launch post — [Steve Yegge Medium](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a)
- Aiki relies on CLAUDE.md + auto-memory (Claude Code only, unstructured) — `ops/now/intel/beads/aiki-map.md` (#4)

---

### 7. Inter-Agent Messaging

**Score: 3.10** | Pain: 3 | Fit: 3 | GTM: 3 | Complexity: 2

**Description:** Add a lightweight messaging channel for multi-agent coordination. Enable agents to send direct messages (`aiki msg send <agent-id> "found a blocker"`) rather than communicating indirectly through task state transitions and comments. This supports scenarios where a review agent needs to alert a fix agent about a critical finding.

**Rationale:**
- **Pain (3):** Currently agents communicate indirectly through task comments and state transitions. This works for sequential workflows but becomes limiting with parallel agents. The pain is moderate — workarounds exist but add latency to coordination.
- **Fit (3):** Supports review workflows where reviewer and fixer need direct coordination, but task comments already serve this purpose for most cases. More relevant as the number of parallel agents grows.
- **GTM (3):** Signals multi-agent maturity. Gas Town's inter-agent coordination via Beads messaging is a visible feature; having an Aiki equivalent prevents the "can't do multi-agent" perception.
- **Complexity (2):** Moderate-to-complex — requires message routing, delivery guarantees, lifecycle management, and integration with agent sessions. Must handle agent sessions that start and stop.

**Evidence:**
- Beads issue type with `--thread` flag for inter-agent communication — [README](https://github.com/steveyegge/beads)
- Gas Town uses Beads messaging for 20-30 agent coordination — [Gas Town blog](https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04)
- `mcp-beads-village` adds file locking to inter-agent coordination — [COMMUNITY_TOOLS.md](https://github.com/steveyegge/beads/blob/main/docs/COMMUNITY_TOOLS.md)
- Aiki has no inter-agent messaging — `ops/now/intel/beads/aiki-map.md` (#8)

---

### 8. Atomic Task Claiming

**Score: 2.70** | Pain: 3 | Fit: 3 | GTM: 2 | Complexity: 4

**Description:** Add atomic claiming to `aiki task start` to prevent race conditions when multiple agents attempt to start the same task concurrently. While Aiki's JJ workspace isolation prevents file-level conflicts, two agents could currently start the same task simultaneously. An atomic check-and-claim operation would close this gap.

**Rationale:**
- **Pain (3):** Not painful today with Aiki's current scale (typically 1-5 parallel agents via `aiki run`). Will become more painful as parallel agent count grows. The risk is wasted work, not data corruption (JJ isolation prevents that).
- **Fit (3):** Supports multi-agent review scenarios where multiple agents pull from the ready queue. Relevant but not core to the review wedge.
- **GTM (2):** Infrastructure feature with no marketing appeal. Users don't buy tools for race condition prevention; they buy for workflow outcomes.
- **Complexity (4):** Relatively simple — add an atomic check-and-transition to `aiki task start` using JJ's atomic operations. The core change is small.

**Evidence:**
- Beads `--claim` flag for atomic claiming — [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)
- Aiki's JJ workspace isolation is a stronger primitive (file-level vs. task-level) but lacks task-level atomicity — `ops/now/intel/beads/aiki-map.md` (#5)

---

### 9. MCP Server Integration

**Score: 2.30** | Pain: 2 | Fit: 2 | GTM: 3 | Complexity: 3

**Description:** Build an `aiki-mcp` server that exposes Aiki's task and review capabilities as MCP tools. This would allow any MCP-compatible agent to use Aiki without requiring CLAUDE.md-style file injection, broadening the potential user base beyond Claude Code, Codex, and Cursor.

**Rationale:**
- **Pain (2):** Current CLAUDE.md integration works well for supported agents. MCP would be a "nice to have" for agents not currently supported, but the target user base for those agents is small.
- **Fit (2):** Integration surface, not review-specific. MCP doesn't improve review workflows; it just makes Aiki accessible to more agents. Worth monitoring but not strategically urgent.
- **GTM (3):** MCP is a trending standard in the agent ecosystem. Having MCP support signals modernity and broadens compatibility. However, Beads already has MCP; adding it to Aiki is parity, not differentiation.
- **Complexity (3):** Moderate — requires building a Python or TypeScript MCP server, mapping CLI commands to MCP tools, and maintaining the integration. The CLI already exists, so the mapping is mechanical.

**Evidence:**
- Beads `beads-mcp` on PyPI with aggressive context reduction — [Plugin docs](https://github.com/steveyegge/beads/blob/main/docs/PLUGIN.md), [MCP docs](https://steveyegge.github.io/beads/integrations/mcp-server)
- Aiki's CLAUDE.md approach is simpler but less portable — `ops/now/intel/beads/aiki-map.md` (#12)

---

### 10. Blocking Dependencies

**Score: 2.00** | Pain: 2 | Fit: 2 | GTM: 2 | Complexity: 4

**Description:** Add a minimal `--blocked-by <task-id>` flag to `aiki task add` and `aiki task start`. Blocked tasks would be excluded from the ready queue until their blockers are resolved. This improves ready-queue accuracy without requiring Beads' full 4-type dependency graph.

**Rationale:**
- **Pain (2):** Aiki's review loops are inherently sequential (review → fix → re-review), so blocking dependencies are rarely needed. The ready queue works well with priority-based ordering for most use cases.
- **Fit (2):** Review loops don't need complex dependency graphs. The review-fix cycle has implicit ordering that doesn't require explicit blocking relationships. Adding dependencies would add cognitive overhead without clear benefit for the primary use case.
- **GTM (2):** Feature parity with Beads' dependency graph isn't a compelling story. Aiki's narrative should be "we understand reviews" not "we have the same graph model." Copying Beads' graph model signals following, not leading.
- **Complexity (4):** Relatively simple — add a flag, store the relationship, and filter the ready queue. The implementation is straightforward; the real challenge is deciding where to stop (blocks only? or also relates_to, duplicates, supersedes?).

**Evidence:**
- Beads supports 4 relationship types: blocks, parent-child, related, discovered-from — [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)
- Aiki has parent-child + source lineage but no blocking relationships — `ops/now/intel/beads/aiki-map.md` (#1)
- Strategic recommendation: "If blocking dependencies become needed, they should be minimal (a `--blocked-by` flag) rather than a full graph" — `ops/now/intel/beads/aiki-map.md` (#1)
