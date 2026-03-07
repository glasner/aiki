# Execution Tasks: Beads Intel Follow-ups

**Date:** 2026-03-05
**Source:** `ops/now/intel/beads/opportunities.md`

---

## feature/review-quality-metrics — Track and Surface Review Effectiveness

**Source opportunity:** #1 Review Quality Metrics & Scoring (score: 4.55)

**Hypothesis:** Developers using autonomous code review cannot currently measure whether reviews are effective. If we track fix rates, issue recurrence, and severity distributions across review cycles, developers will gain confidence in autonomous review quality and increase adoption. We believe "quantified code review" is a differentiator Beads cannot replicate because it lacks structured review data.

**Success metric:**
- `aiki review stats` produces a summary showing: fix rate (% of issues resolved), issue recurrence rate, severity breakdown, and per-review quality score
- At least 3 meaningful metrics are computed from existing `aiki review` + `aiki review issue add` data with no new user input required
- A user running 5+ review cycles can see a trend (improving/stable/declining)

**Scope guardrails:**
- **In scope:** Define metric formulas, collect data from review close events, compute aggregates, `aiki review stats` CLI output (text-based, no UI)
- **In scope:** Store computed metrics on a JJ branch for persistence
- **Out of scope:** Web dashboards, visualization UIs, per-file or per-author breakdowns, ML-based predictions
- **Out of scope:** Retroactive computation on historical reviews (forward-only)

**Estimated effort:** M (1-3 days)

---

## feature/auto-re-review-cycles — Automate the Review-Fix-Verify Loop

**Source opportunity:** #2 Auto Re-Review Cycles (score: 4.35)

**Hypothesis:** The manual step of re-triggering reviews after fixes is the biggest friction point in Aiki's review workflow. If we automatically trigger a re-review after a fix task closes, developers will ship higher-quality fixes (fewer regressions) and spend less time on manual coordination. This turns Aiki from a review tool into a review engine.

**Success metric:**
- After `aiki fix <review-id>` completes and the fix task closes, a re-review is automatically triggered on the changed files
- The cycle terminates when: (a) re-review finds no new issues, or (b) a configurable max iteration count is reached (default: 3)
- End-to-end demo: review finds issue → fix agent resolves it → re-review confirms fix — with zero manual intervention after initial `aiki review`

**Scope guardrails:**
- **In scope:** Post-fix-close hook that triggers re-review, diff-scoped re-review (only files changed by the fix), loop termination logic (max iterations + convergence), iteration counter visible in task comments
- **In scope:** `--no-re-review` flag on `aiki fix` to opt out
- **Out of scope:** Cross-module cascading re-reviews (that's opportunity #3), parallel re-review of independent fix branches, custom termination policies
- **Out of scope:** Automatic fix generation (the fix agent is already a separate step)

**Estimated effort:** M (1-3 days)

---

## experiment/task-summarization-quality — Validate LLM-Generated Task Summaries

**Source opportunity:** #5 Task Summarization / Memory Compaction (score: 3.70)

**Hypothesis:** Context rot in long agent sessions degrades review quality. We believe that LLM-generated summaries of completed task trees can preserve critical context (decisions made, issues found, fixes applied) while reducing token pressure. However, we need to validate that automated summaries are accurate and useful enough to replace reading full task histories — poor summaries could be worse than none.

**Success metric:**
- Generate summaries for 10 completed parent tasks with 3+ subtasks each
- Human evaluation: rate each summary on accuracy (1-5) and completeness (1-5)
- Target: mean accuracy >= 4.0 and mean completeness >= 3.5
- Measure token reduction: summaries should be <20% of the full task tree content while retaining all key decisions and outcomes

**Scope guardrails:**
- **In scope:** Build a prototype `aiki task summarize <id>` command that generates a summary of a closed parent task and its subtasks, using the LLM to synthesize task descriptions, comments, and close summaries
- **In scope:** Run the prototype on real completed task trees and evaluate output quality
- **In scope:** Document findings: what works, what fails, and whether to proceed to a full feature
- **Out of scope:** Auto-injection into agent context, storage on JJ branches, integration with `aiki task close` workflow, production-quality implementation
- **Out of scope:** Cross-session memory (that's opportunity #6, a separate concern)

**Estimated effort:** S (< 1 day)

---

## positioning/quantified-autonomous-review — Own the "Measurable Code Review" Narrative

**Source opportunity:** #1 Review Quality Metrics & Scoring (score: 4.55) + #3 Review-Aware Orchestration Deepening (score: 4.35)

**Hypothesis:** Beads' narrative is "agent memory for context persistence." Aiki can carve a distinct position by owning "quantified autonomous code review" — the idea that autonomous reviews should be measurable, improvable, and orchestrated with domain intelligence. This positions Aiki as depth-first (deep review expertise) vs. Beads/Gas Town's breadth-first (generic task dispatch at scale). We believe developers who care about code quality will choose a tool that proves its value with metrics over one that simply remembers things.

**Success metric:**
- Publish a positioning document (blog post draft or landing page copy) that articulates the "quantified autonomous review" narrative
- The document clearly differentiates Aiki from Beads on 3+ dimensions (metrics, review loops, review-aware orchestration)
- Internal team review: at least 2 people agree the positioning is compelling and distinct from Beads' narrative
- Identify 3 concrete proof points (features, demos, or data) that support the positioning

**Scope guardrails:**
- **In scope:** Draft positioning document with key messages, competitive differentiation, and proof points; identify target audience segments (quality-focused teams, enterprise, regulated industries)
- **In scope:** Create a 1-paragraph elevator pitch and 3 supporting talking points
- **Out of scope:** Final design/layout, publishing, paid marketing campaigns, pricing strategy
- **Out of scope:** Positioning against tools other than Beads/Gas Town (e.g., Linear, GitHub Issues)

**Estimated effort:** S (< 1 day)
