# SuperAGI Intel — Opportunity Scoring

**Source:** `ops/now/intel/superagi/aiki-map.md`
**Scoring formula:** `score = 0.35×pain + 0.35×fit + 0.20×gtm + 0.10×(6−complexity)`

| Dimension | Scale |
|-----------|-------|
| User Pain Severity | 1 (mild) → 5 (severe) |
| Strategic Fit to Aiki | 1 (poor) → 5 (perfect) |
| Build Complexity | 1 (very complex) → 5 (trivial) |
| GTM Leverage | 1 (weak) → 5 (strong) |

---

## Ranked Opportunities

| Rank | Opportunity | Pain | Fit | Complexity | GTM | Score |
|------|-------------|------|-----|------------|-----|-------|
| 1 | Capture Agent Orchestration Vacuum | 5 | 5 | 2 | 5 | **4.90** |
| 2 | Safe Concurrent Agent Execution | 5 | 5 | 4 | 4 | **4.50** |
| 3 | SDLC Vertical Depth Strategy | 4 | 5 | 2 | 4 | **4.35** |
| 4 | Agent Observability & Performance Metrics | 4 | 5 | 3 | 4 | **4.25** |
| 5 | Deepened Code Review Workflow | 4 | 5 | 3 | 3 | **4.05** |
| 6 | Task Provenance as Competitive Moat | 4 | 5 | 4 | 3 | **3.95** |
| 7 | CLI-native Multi-Agent Orchestration | 3 | 5 | 4 | 3 | **3.60** |
| 8 | SDLC-focused Event Hooks | 3 | 4 | 3 | 2 | **3.15** |

---

## Rationales

### 1. Capture Agent Orchestration Vacuum — 4.90

SuperAGI's abandonment (14+ months, no releases) leaves a vacuum in multi-agent orchestration. No credible OSS alternative exists for SDLC-focused agent coordination. Aiki is purpose-built for exactly this space.

- **Pain 5:** Teams running multiple AI coding agents have no orchestration layer — conflicts, lost work, no audit trail.
- **Fit 5:** This IS Aiki's core thesis. SDLC-specific orchestration with JJ isolation, task provenance, and review workflows.
- **Complexity 2:** Requires significant market positioning, documentation, and feature polish to claim the space. Not a single feature — it's a positioning play.
- **GTM 5:** First-mover in a vacated space. Strong narrative: "SuperAGI tried this generically and failed; Aiki solves it for the vertical that matters."

### 2. Safe Concurrent Agent Execution — 4.50

JJ-based workspace isolation for parallel agents is a strict upgrade over SuperAGI's approach (which had no conflict prevention). This is Aiki's most defensible technical differentiator.

- **Pain 5:** Running multiple agents that clobber each other's work is a top pain point. No other tool solves this cleanly.
- **Fit 5:** Core capability already built into Aiki's architecture.
- **Complexity 4:** Already implemented. Needs hardening, edge-case handling, and better error messages — not a ground-up build.
- **GTM 4:** Highly demonstrable. Side-by-side demo of "agents conflicting" vs "Aiki isolating" is compelling.

### 3. SDLC Vertical Depth Strategy — 4.35

SuperAGI's failure mode was going broad (20+ generic toolkits, marketplace, GUI). Aiki's counter-strategy is depth in code review, task provenance, and workspace isolation.

- **Pain 4:** Developers are drowning in shallow, generic AI tools that don't understand SDLC workflows. Deep integration beats breadth.
- **Fit 5:** This is Aiki's strategic thesis — depth over breadth.
- **Complexity 2:** "Go deep" means sustained investment across code review, task tracking, and workspace isolation. Not a single deliverable.
- **GTM 4:** Strong narrative against the "generic AI agent framework" category. Resonates with developers burned by shallow tools.

### 4. Agent Observability & Performance Metrics — 4.25

The one capability explicitly worth copying from SuperAGI. Build dashboards/metrics on top of existing task provenance data to answer "what did agents do, how well, where did they get stuck?"

- **Pain 4:** Teams deploying AI agents lack visibility into agent behavior, cost, and quality. Debugging agent failures is painful.
- **Fit 5:** Direct extension of existing task/review/comment history. The data already exists — it needs surfacing.
- **Complexity 3:** Moderate effort. Needs aggregation logic, metrics definitions, and a presentation layer (CLI tables, optional dashboard).
- **GTM 4:** "Agent APM" is a buzzworthy category. Demos well. Differentiates from tools that treat agents as black boxes.

### 5. Deepened Code Review Workflow — 4.05

Aiki's structured review workflow (autonomous review + structured approval) is more sophisticated than SuperAGI's binary allow/deny. Deepening this creates distance from competitors.

- **Pain 4:** AI-generated code still needs human review, but current workflows are ad-hoc. Structured review with issue tracking and severity is a real gap.
- **Fit 5:** Code review is core to Aiki's wedge. `aiki review` and `aiki fix` already exist.
- **Complexity 3:** Needs work on review quality, multi-reviewer support, and integration with PR workflows. Moderate effort.
- **GTM 3:** Harder to demo quickly — value compounds over time. Less flashy than "safe concurrency" but stickier.

### 6. Task Provenance as Competitive Moat — 3.95

Aiki's structured task/review history is a superior alternative to generic agent memory (STM/LTS). It answers "why does this code exist?" not just "what happened recently?"

- **Pain 4:** Understanding the reasoning behind code changes is a persistent problem, especially when AI agents make changes autonomously.
- **Fit 5:** Task provenance with sources, comments, and review lineage is a core Aiki differentiator.
- **Complexity 4:** The system already exists. Needs better querying, cross-referencing, and surfacing in agent context.
- **GTM 3:** Hard to market as a standalone feature. Value is in the compound effect — useful once you have enough history.

### 7. CLI-native Multi-Agent Orchestration — 3.60

SuperAGI's GUI-first approach attracted GitHub stars but didn't retain production users. CLI-first + editor integration is the right modality for developers.

- **Pain 3:** Moderate. Some teams want GUIs, but the power-user developer audience prefers CLI workflows. Not an acute pain.
- **Fit 5:** CLI-native is Aiki's DNA. This is already the approach — it's about continuing to polish it.
- **Complexity 4:** Already built. Incremental improvements to CLI UX, completions, and editor integrations.
- **GTM 3:** CLI is polarizing. Strong with the right audience but limits total addressable market. Needs strong editor integrations to broaden appeal.

### 8. SDLC-focused Event Hooks — 3.15

Aiki's hooks system is more tightly integrated with agent workflows than SuperAGI's webhook pattern. Continue investing in hooks rather than adopting generic webhooks.

- **Pain 3:** Moderate. Hooks are useful for automation but not a top-of-mind pain point for most teams.
- **Fit 4:** Aligns well with Aiki's architecture but is more of an enabler than a primary feature.
- **Complexity 3:** Medium effort. Needs more hook points, better error handling, and documentation.
- **GTM 2:** Hard to market hooks as a feature. They're infrastructure — valuable but not demo-friendly.

---

## Key Insights

1. **Top cluster (4.25–4.90):** The top 4 opportunities are all positioning/narrative plays backed by existing technical capabilities. The biggest wins come from claiming the vacuum SuperAGI left and making Aiki's existing strengths more visible.

2. **The "Copy" opportunity (Agent Observability) ranks #4.** It's the only net-new feature category worth building from SuperAGI's playbook. Everything else is "Counter" or "Ignore."

3. **Build effort is low for the top opportunities.** Rankings 2, 6, and 7 all leverage capabilities that already exist — the work is polish, positioning, and surfacing, not ground-up development.

4. **The bottom 2 opportunities are enablers, not features.** CLI orchestration and hooks are important infrastructure but don't drive acquisition on their own.
