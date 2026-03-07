# TasteMatter — Execution Tasks

Derived from the top-scored opportunities in `opportunities.md`.

---

## feature/attention-drift-detection

**Source opportunity:** #2 Attention Drift Detection (score: 4.50)

Build plan-vs-reality comparison into `aiki review`. Compare task descriptions and source links against actual file changes (via `aiki task diff`) to surface when agent work diverges from the stated plan.

- **Hypothesis:** Surfacing plan-vs-reality divergence during review will catch drift that currently goes unnoticed, reducing rework caused by agents silently wandering off-task.
- **Success metric:** In internal dogfooding, drift detection flags at least 1 meaningful divergence per 10 reviewed tasks, and flagged divergences are confirmed as genuine (>50% true-positive rate).
- **Scope guardrails:**
  - In scope: Comparing task description + source doc intent against files touched in `aiki task diff`; surfacing a "drift summary" section in review output.
  - Out of scope: NLP-based semantic analysis of code content; auto-correcting drift; integration with external planning tools.
- **Estimated effort:** L — Requires heuristics for matching intent signals to file changes, iteration on false-positive rates, and integration into the review workflow.

---

## feature/cross-session-context-enrichment

**Source opportunity:** #3 Cross-Session Context Enrichment (score: 3.80)

Improve task summaries and source chain display so that "what happened across sessions?" is answered by structured context rather than by re-reading code or logs.

- **Hypothesis:** Richer cross-session context (better summary prompts, visible source chains, improved `aiki task list` output) will reduce the time agents and humans spend re-orienting when resuming work across sessions.
- **Success metric:** Users report faster ramp-up when picking up tasks started in prior sessions. Measurable proxy: average number of file reads before first meaningful edit decreases by 25% on cross-session task resumptions.
- **Scope guardrails:**
  - In scope: Improving summary generation prompts; displaying source chains in `aiki task show`; enriching `aiki task list` with last-activity timestamps and summary previews.
  - Out of scope: Building a separate session history UI; storing full conversation transcripts; integrating with external knowledge bases.
- **Estimated effort:** M — Extends existing features (summary quality, source chain display) without new architecture. Primarily prompt engineering and CLI output formatting.

---

## experiment/stale-task-detection

**Source opportunity:** #4 Stale Task Detection (score: 3.70)

Test whether auto-surfacing tasks that have been `in_progress` too long without comments or activity meaningfully reduces zombie task accumulation in multi-agent workflows.

- **Hypothesis:** A simple time-based staleness check (e.g., tasks in_progress for >2 hours with no comments) will surface genuinely abandoned work at least 70% of the time, and nudging agents/users to close or update stale tasks will keep the task list clean without creating noise.
- **Success metric:** Over a 2-week trial: (1) stale alerts have >70% true-positive rate (task was genuinely stuck or abandoned), and (2) average number of zombie tasks at any point in time drops by 50% compared to the pre-experiment baseline.
- **Scope guardrails:**
  - In scope: Time-based staleness query against task metadata; a `--stale` flag on `aiki task list`; optional warning on `aiki task` when stale tasks exist.
  - Out of scope: Auto-closing stale tasks; ML-based activity prediction; notifications to external systems (Slack, email).
- **Estimated effort:** S — Simple time-based check against existing task metadata. Can be shipped and tested within a day.

---

## positioning/provenance-first

**Source opportunity:** #1 Provenance-First Counter-Positioning (score: 4.65)

Position Aiki's explicit intent tracking against TasteMatter's passive file observation. Craft messaging around "structured provenance > inferred trails" to establish a clear competitive narrative.

- **Hypothesis:** Framing the comparison as "intent tracking vs. activity logging" will resonate with developers who have experienced the limits of passive observation in agentic workflows, making Aiki the obvious choice for teams that need to understand *why* changes happened, not just *what* was touched.
- **Success metric:** After publishing positioning content (landing page copy, comparison page, social posts): (1) inbound mentions of "provenance" or "intent tracking" in Aiki-related discussions increase, and (2) conversion rate from TasteMatter-aware visitors improves by 15% within 4 weeks of launch.
- **Scope guardrails:**
  - In scope: Landing page copy updates; a dedicated comparison page (Aiki vs. TasteMatter); 3-5 social/community posts with concrete examples ("TasteMatter shows you touched auth.rs 12 times; Aiki tells you why and whether the work is done").
  - Out of scope: New product features; paid advertising campaigns; direct outreach to TasteMatter users; pricing changes.
- **Estimated effort:** S — Marketing and messaging work, no engineering required. Can be executed immediately with existing product capabilities.
