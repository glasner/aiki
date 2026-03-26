# Ampcode: Scored Opportunities

**Date:** 2026-03-05
**Source:** `ops/now/intel/ampcode/aiki-map.md`
**Formula:** `score = 0.35×pain + 0.35×fit + 0.20×gtm + 0.10×(6−complexity)`

---

## Ranked Opportunities

| Rank | Opportunity | Pain | Fit | Complexity | GTM | Score |
|------|------------|------|-----|------------|-----|-------|
| 1 | Iterative review loops | 5 | 5 | 3 | 5 | **4.80** |
| 2 | Composable review criteria | 4 | 5 | 3 | 4 | **4.25** |
| 3 | Persistent subagent orchestration | 4 | 5 | 4 | 4 | **4.15** |
| 4 | Cross-session review memory | 4 | 4 | 3 | 4 | **3.90** |
| 5 | Review-aware task provenance | 3 | 5 | 4 | 3 | **3.60** |
| 6 | Handoff context optimization | 3 | 4 | 2 | 3 | **3.45** |
| 7 | Local-first privacy positioning | 3 | 4 | 5 | 4 | **3.35** |
| 8 | Inline advisor (Oracle-like) | 3 | 3 | 2 | 2 | **2.90** |
| 9 | Team task browsing | 3 | 2 | 2 | 3 | **2.75** |
| 10 | Model-aware task routing | 2 | 3 | 3 | 2 | **2.45** |

*Complexity: 1=very hard, 5=trivial. Higher complexity score → lower contribution to total (formula inverts it).*

---

## Rationale

### 1. Iterative review loops — 4.80
Formalize the build→review→fix→review cycle as a first-class workflow primitive. Amp's Checks system is one-shot; Aiki can own the *iterative* review loop where findings automatically spawn fix tasks, which get re-reviewed. This is the single strongest differentiator vs all competitors.
- **Pain (5):** The manual cycle of review→fix→hope-you-didn't-break-something is the core developer pain in agent-assisted coding.
- **Fit (5):** This IS Aiki's wedge — autonomous review with iterative correction.
- **Complexity (3):** Needs structured loop orchestration, convergence detection, and review-diff awareness. Moderate effort.
- **GTM (5):** Primary differentiator. "Your agent doesn't just review — it fixes and re-reviews until it's right."

### 2. Composable review criteria — 4.25
Add user-defined, composable review criteria to match/exceed Amp's Checks system. Users define review standards (security, style, architecture) that compose and persist across sessions.
- **Pain (4):** Generic reviews miss project-specific standards. Teams want reviews that enforce *their* rules.
- **Fit (5):** Dead center of the autonomous review wedge.
- **Complexity (3):** Needs criteria DSL design, integration with `aiki review`, and composability semantics.
- **GTM (4):** Direct competitive counter to Amp's Feb 2026 Checks launch. Strong messaging: "reviews that understand YOUR standards."

### 3. Persistent subagent orchestration — 4.15
Lean into Aiki's durability and observability advantage for multi-agent work. Amp's subagents are ephemeral within a thread; Aiki's persist across sessions with full task tracking and provenance.
- **Pain (4):** Losing context between agent sessions is a real and recurring frustration.
- **Fit (5):** Core to Aiki's task orchestration wedge.
- **Complexity (4):** Already partly built via `aiki run`. Incremental improvements needed.
- **GTM (4):** Clear differentiator. "Your agent work survives across sessions — nothing gets lost."

### 4. Cross-session review memory — 3.90
Reviews that remember previous findings across sessions. If a reviewer flagged a pattern issue last week, it shouldn't re-discover it from scratch — it should build on prior knowledge.
- **Pain (4):** Repetitive review findings waste developer time and erode trust in automation.
- **Fit (4):** Leverages Aiki's persistence advantage directly.
- **Complexity (3):** Needs review history indexing and pattern matching across sessions.
- **GTM (4):** Unique capability. No competitor offers review memory. "Reviews that learn from your project's history."

### 5. Review-aware task provenance — 3.60
Strengthen traceability from review findings to fix tasks to re-review results. Every review issue should trace forward to the task that fixed it and the re-review that confirmed it.
- **Pain (3):** Traceability is valuable but developers have workarounds (PR comments, commit messages).
- **Fit (5):** Directly strengthens the review→fix→re-review loop.
- **Complexity (4):** Relatively straightforward — extend existing task source system.
- **GTM (3):** Compelling story for quality-focused teams but niche appeal.

### 6. Handoff context optimization — 3.45
Add automatic context analysis when delegating tasks to subagents. Instead of manually writing task descriptions, Aiki analyzes what context the receiving agent needs and transfers it optimally.
- **Pain (3):** Task delegation works but context loss between agents is frustrating.
- **Fit (4):** Strengthens task orchestration, a core Aiki capability.
- **Complexity (2):** Hard — requires automatic context analysis, relevance scoring, and context summarization.
- **GTM (3):** Meaningful improvement but not a headline feature.

### 7. Local-first privacy positioning — 3.35
Market Aiki's JJ/git-based local storage as a privacy advantage over Amp's server-stored threads. Everything stays in your repo — no data leaves your machine.
- **Pain (3):** Privacy matters to enterprise teams and regulated industries, less to individual devs.
- **Fit (4):** Aiki's architecture is inherently local-first; this is a positioning play.
- **Complexity (5):** Already built. This is messaging and documentation, not engineering.
- **GTM (4):** Strong enterprise differentiator vs Amp's Sourcegraph server dependency.

### 8. Inline advisor (Oracle-like) — 2.90
Add real-time architectural guidance during task execution, similar to Amp's Oracle system. An advisor that validates decisions as they're being made, not just in post-hoc review.
- **Pain (3):** Nice to have but most devs don't get real-time arch guidance today.
- **Fit (3):** Adjacent to review wedge but not core — Aiki's review is post-hoc by design.
- **Complexity (2):** Hard — requires real-time context analysis, interruption UX, and knowing when to intervene.
- **GTM (2):** Amp already has this. Copying is defensive, not differentiating.

### 9. Team task browsing — 2.75
Add collaboration UX for sharing and browsing task histories across team members. Make Aiki tasks discoverable and reviewable by the whole team.
- **Pain (3):** Teams want visibility into agent work, but PRs and Slack partially solve this.
- **Fit (2):** Adjacent to wedge. More enterprise UX than autonomous review.
- **Complexity (2):** Hard — requires sharing infrastructure, access control, and browsing UX.
- **GTM (3):** Enterprise-attractive but not core to individual developer wedge.

### 10. Model-aware task routing — 2.45
Allow Aiki to suggest or configure optimal models for different task types (e.g., use a reasoning model for complex reviews, a fast model for simple searches).
- **Pain (2):** Most developers don't optimize model selection today.
- **Fit (3):** Aiki is agent-runtime agnostic — model routing is possible but not core.
- **Complexity (3):** Moderate — needs benchmarking data and model metadata.
- **GTM (2):** Amp already does this natively with tight model integration. Hard to compete on their turf.
