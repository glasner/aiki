# SuperAGI Intel — Follow-up Tasks

**Source:** `ops/now/intel/superagi/opportunities.md`
**Created:** 2026-03-05

---

## feature/safe-concurrent-agent-hardening

**From:** Opportunity #2 — Safe Concurrent Agent Execution (Score: 4.50)

**Hypothesis:** Hardening JJ-based workspace isolation — better error messages, edge-case handling, and conflict resolution UX — will reduce agent-caused merge conflicts to near-zero and become Aiki's most cited differentiator in user feedback.

**Success metric:** Zero unrecoverable workspace conflicts in a 5-agent parallel session (tested across 10 runs). At least 3 user testimonials or community mentions citing safe concurrency as a reason for adopting Aiki.

**Scope guardrails:**
- **In scope:** Edge-case handling for concurrent file edits, improved conflict marker resolution UX, better error messages when isolation fails, automated test suite for parallel agent scenarios.
- **Out of scope:** New isolation backends (stay on JJ), GUI for conflict resolution, support for non-JJ VCS backends.

**Estimated effort:** M

---

## feature/agent-observability-metrics

**From:** Opportunity #4 — Agent Observability & Performance Metrics (Score: 4.25)

**Hypothesis:** Surfacing agent behavior data (task duration, success/failure rates, cost per task, where agents get stuck) from existing task provenance history will make teams confident enough to deploy more agents — increasing Aiki usage and retention.

**Success metric:** Ship `aiki metrics` CLI command that reports per-agent and per-task-type stats (duration, success rate, review pass rate). At least 60% of active users run it within 30 days of release.

**Scope guardrails:**
- **In scope:** Aggregation logic over existing task/review/comment data, CLI table output (`aiki metrics`), per-agent and per-task-type breakdowns, cost tracking if token counts are available.
- **Out of scope:** Web dashboard, real-time streaming metrics, integration with external APM tools (Datadog, Grafana), custom metric definitions.

**Estimated effort:** M

---

## experiment/code-review-depth

**From:** Opportunity #5 — Deepened Code Review Workflow (Score: 4.05)

**Hypothesis:** Adding multi-reviewer support and severity-weighted issue scoring to `aiki review` will increase the percentage of AI-generated code that gets meaningfully reviewed (not just rubber-stamped), improving code quality and making review workflows sticky enough to drive retention.

**Success metric:** Run a 2-week trial with 3–5 internal or design-partner teams. Measure: (a) review completion rate increases by >20% vs. baseline, (b) at least 50% of reviews use severity tagging, (c) qualitative feedback confirms reviews feel "useful, not ceremonial."

**Scope guardrails:**
- **In scope:** Multi-reviewer assignment (round-robin or manual), severity-weighted issue rollup scores, experiment tracking (A/B between current and enhanced review), user feedback collection.
- **Out of scope:** PR platform integration (GitHub/GitLab review sync), automated review assignment based on code ownership, review SLA enforcement.

**Estimated effort:** M

---

## positioning/orchestration-vacuum

**From:** Opportunity #1 — Capture Agent Orchestration Vacuum (Score: 4.90)

**Hypothesis:** SuperAGI's 14+ month abandonment has left a vacuum in SDLC-focused multi-agent orchestration. By explicitly positioning Aiki as the answer — with targeted content, comparisons, and community engagement — Aiki can capture mindshare before a new competitor fills the gap.

**Success metric:** Within 60 days: (a) publish 2 comparison pieces (SuperAGI vs. Aiki, generic vs. SDLC-specific orchestration), (b) achieve top-3 search ranking for "SuperAGI alternative" and "multi-agent orchestration for developers", (c) 30% increase in organic inbound traffic from orchestration-related queries.

**Scope guardrails:**
- **In scope:** Comparison content (blog posts, landing page), SEO-targeted pages, community engagement (HN, Reddit, Discord) with authentic positioning, demo video showing Aiki's orchestration capabilities vs. the status quo.
- **Out of scope:** Paid advertising, generic "AI agent framework" positioning (stay SDLC-vertical), building features solely for marketing purposes, disparaging SuperAGI's team (focus on the gap, not the failure).

**Estimated effort:** S
