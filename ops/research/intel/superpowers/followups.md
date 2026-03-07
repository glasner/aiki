# Superpowers — Follow-up Execution Tasks

Derived from the top-scored opportunities in [opportunities.md](opportunities.md).
Evidence and context linked to [research.md](research.md) and [aiki-map.md](aiki-map.md).

---

## feature/two-stage-review

**Source opportunity:** #1 — Two-stage review (spec + quality) — Score: 4.60

**Hypothesis:** Splitting `aiki review` into two explicit phases — (1) spec-compliance (does the diff match the task description?) and (2) code quality (is the code well-structured and tested?) — will catch more issues than a single-pass review, directly addressing the review-skipping problem that Superpowers users report as their top pain point ([research.md: issues #613, #614](research.md)).

**Success metric:** In a before/after comparison across 20+ review runs, two-stage review surfaces at least 30% more spec-compliance issues (task description vs. actual diff mismatches) than single-pass review, with no increase in false positives.

**Scope guardrails:**
- **In scope:** Review phase architecture (spec-compliance pass + code-quality pass), structured output per phase, phase-specific review criteria, integration with existing `aiki review` CLI
- **Out of scope:** Custom per-project review criteria configuration, LLM model selection for review phases, review caching/memoization, UI/dashboard for review results

**Estimated effort:** L — Requires review phase architecture, structured output format for each phase, potentially different prompting strategies per phase. Builds on existing `aiki review` infrastructure but is a meaningful extension touching review orchestration, output formatting, and issue classification.

---

## feature/cross-session-execution-tracking

**Source opportunity:** #2 — Cross-session execution tracking — Score: 4.35

**Hypothesis:** Making Aiki's cross-session task persistence a first-class recovery feature — so that when a session crashes mid-plan, a new agent session can immediately see what was completed, what's in-progress, and what remains — will reduce wasted re-work on interrupted multi-step tasks. Superpowers' batch execution is ephemeral; Aiki's JJ-backed state already persists but the recovery UX is underdeveloped ([aiki-map.md: Plan Execution](aiki-map.md), [research.md: "couple hours at a time" autonomy](research.md)).

**Success metric:** A new agent session picking up an interrupted task can resume work within 60 seconds (reading task state + understanding what's done) vs. the current baseline of needing manual investigation. Measured by time-to-first-useful-action in recovery scenarios.

**Scope guardrails:**
- **In scope:** Recovery UX for incoming agents (clear display of completed/in-progress/remaining subtasks), session crash detection, "resume from here" affordance in `aiki task show`, progress summary generation for handoff
- **Out of scope:** Automatic session restart/scheduling, cross-machine state sync, notification system for crashed sessions, changes to JJ persistence layer

**Estimated effort:** M — Core infrastructure (JJ-backed task persistence) already exists. Main work is building a clean recovery presentation: showing an incoming agent the execution state in a way that enables immediate productive work. Mostly a UX/presentation problem with some state detection logic.

---

## experiment/spec-compliance-checking

**Source opportunity:** #4 — Spec-compliance checking (task→diff) — Score: 4.25

**Hypothesis:** Using LLM-assisted semantic comparison between a task's description and its actual code diff can automatically detect when an agent's changes drift from the assigned task — catching omissions, tangential changes, and scope creep. This leverages Aiki's unique task→diff provenance chain that no other tool has at the infrastructure level ([aiki-map.md: Subagent-Driven Development](aiki-map.md), [opportunities.md: #4 rationale](opportunities.md)).

**Success metric:** On a labeled test set of 30 task-diff pairs (15 compliant, 15 non-compliant with known drift types), the spec-compliance checker achieves >80% precision and >75% recall in detecting drift. Run as a time-boxed experiment (2 weeks) to validate feasibility before committing to production integration.

**Scope guardrails:**
- **In scope:** Prototype spec-compliance checker (task description + diff as inputs, compliance verdict + explanation as output), test corpus of labeled task-diff pairs, evaluation metrics (precision, recall, false positive rate), prompt engineering for compliance assessment
- **Out of scope:** Production integration into `aiki review`, custom compliance rules per project, fine-tuning or training models, real-time compliance checking during development (only post-hoc)

**Estimated effort:** M — Well-scoped experiment. Requires building a prompt pipeline for semantic comparison, creating a labeled test corpus, and running evaluation. No infrastructure changes needed — this is a standalone prototype that validates whether automated spec-compliance checking is feasible and accurate enough to integrate into the review pipeline.

---

## positioning/complementary-with-superpowers

**Source opportunity:** #3 — Complementary positioning with Superpowers — Score: 4.30

**Hypothesis:** Positioning Aiki explicitly as complementary infrastructure to Superpowers — "Use Superpowers to teach agents how to work. Use Aiki to track, verify, and review the work they produce." — will convert Superpowers' 71.7K-star user base into a distribution channel rather than a competitive threat. The behavioral-layer vs. infrastructure-layer distinction is genuine and messaging it clearly will reduce adoption confusion ([research.md: Multi-Platform Support, Community Skills Marketplace](research.md), [aiki-map.md: Summary Assessment](aiki-map.md)).

**Success metric:** Within 4 weeks of publishing positioning materials: (1) at least one integration guide or "Aiki + Superpowers" tutorial published, (2) measurable inbound traffic or mentions from Superpowers community channels, (3) zero user reports of confusion about whether Aiki competes with or complements Superpowers.

**Scope guardrails:**
- **In scope:** Positioning page/doc ("Aiki + Superpowers"), integration guide showing both tools used together, comparison table (behavioral layer vs. infrastructure layer), lightweight Superpowers skill that introduces agents to `aiki task` commands
- **Out of scope:** Deep technical integration (API-level hooks between tools), Superpowers marketplace plugin submission, changes to Aiki's core CLI or task system, paid marketing or advertising

**Estimated effort:** S — Primarily messaging, documentation, and one integration guide. May include a lightweight Superpowers-compatible skill file. No significant code changes required. Highest leverage-to-effort ratio of all follow-up tasks.
