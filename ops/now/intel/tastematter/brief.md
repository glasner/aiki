# TasteMatter — Intel Brief

**Date:** 2026-03-05
**Sources:** [research.md](research.md), [aiki-map.md](aiki-map.md), [opportunities.md](opportunities.md), [followups.md](followups.md)

---

## 1. What TasteMatter Is

TasteMatter is an alpha-stage local CLI tool (v0.1.0-alpha.29, [GitHub](https://github.com/jacob-dietle/tastematter) — 10 stars, 6 commits) that passively indexes Claude Code session files to surface "context trails" — file hotness, attention drift, and session history — so developers can see what happened across sessions without relying on memory. Built by Jacob Dietle / [Taste Systems](https://www.taste.systems/) (Digital Leverage LLC, New York), it positions itself as "Context Trails for Claude Code" ([tastematter.dev](https://tastematter.dev/)). It runs entirely locally, collects anonymous telemetry via PostHog (opt-out available), and has no pricing — free for personal and commercial use ([Terms](https://tastematter.dev/terms.html), [Privacy](https://tastematter.dev/privacy.html)). No public demos or meaningful community traction exist yet.

---

## 2. What Matters for Aiki

### Capability Decisions

| TasteMatter Capability | Decision | Rationale |
|---|---|---|
| **Attention drift detection** (plan vs. reality) | **Copy** | Most strategically urgent. Their strongest differentiated feature and most likely expansion vector into workflow territory. Aiki can build this natively by comparing task descriptions/sources against `aiki task diff` output — fits directly into the review workflow. ([aiki-map.md](aiki-map.md)) |
| **File hotness tracking** (HOT/WARM/COLD) | **Copy** | Low-effort enhancement to `aiki review`. Use file change frequency across recent tasks to prioritize which files get human review. "These files changed 8 times across 3 tasks — review them first." ([aiki-map.md](aiki-map.md)) |
| **Stale task detection** (abandoned work timestamps) | **Copy** | Trivial to implement — time-based check against existing task metadata. Add `--stale` flag to `aiki task list`. Prevents zombie task accumulation in multi-agent workflows. ([aiki-map.md](aiki-map.md)) |
| **Session tracking** (cross-session file history) | **Counter** | Aiki's explicit task tracking captures *intent* and *context* (descriptions, sources, summaries), not just file access. Position as: "TasteMatter shows you touched auth.rs 12 times; Aiki tells you *why* and whether the work is done." ([aiki-map.md](aiki-map.md)) |
| **Background session indexing** | **Counter** | Passive indexing complements but doesn't replace active task orchestration. Structured provenance > inferred trails. ([aiki-map.md](aiki-map.md)) |
| **File relationship mapping** | **Ignore** | Tangential to Aiki's review/task wedge. Code intelligence tools (LSPs, IDEs) already serve this need. ([aiki-map.md](aiki-map.md)) |
| **Context trails / CLI query interface** | **Ignore** | Different paradigm. Aiki's `aiki task list` and `aiki task show` answer "what happened?" with richer, intent-aware context. ([aiki-map.md](aiki-map.md)) |
| **Local-only data architecture** | **Ignore** | Shared design principle, not a competitive axis. Both tools do this already. ([aiki-map.md](aiki-map.md)) |

### Key Overlaps and Threats

**Overall threat: Low–Medium.** TasteMatter is a *visibility* tool (passive observation); Aiki is a *workflow* tool (active orchestration + autonomous review). They occupy adjacent but distinct spaces and could coexist ([aiki-map.md](aiki-map.md)).

**The real risk is expansion, not the current product.** TasteMatter's "attention drift" feature is closest to Aiki's territory and the most likely vector for expanding from visibility into workflow — potentially adding task management, review triggers, or build loops ([aiki-map.md](aiki-map.md)).

**Market validation signal:** TasteMatter validates that the "what happened across Claude Code sessions?" problem is real and worth solving. Their existence confirms Aiki is targeting the right pain point, but Aiki solves it with structured provenance rather than inferred trails ([tastematter.dev](https://tastematter.dev/)).

---

## 3. Top 3 Recommendations This Week

### 1. Ship provenance-first counter-positioning (effort: S, score: 4.65)

Craft and publish messaging around "structured provenance > inferred trails." No engineering needed — this leverages existing Aiki capabilities. Deliverables: updated landing page copy, a comparison page, and 3–5 community posts with concrete examples. Key message: *"Context trails show you what happened. Aiki tells you what should have happened and whether it did."* ([followups.md — positioning/provenance-first](followups.md))

### 2. Design attention drift detection for `aiki review` (effort: L, score: 4.50)

Start a design spike for plan-vs-reality comparison. Compare task descriptions + source links against files touched in `aiki task diff` to surface a "drift summary" in review output. This directly counters TasteMatter's strongest feature and blocks their most likely expansion vector. Target: >50% true-positive rate on flagged divergences. Scope guard: no NLP-based semantic analysis of code content in v1. ([followups.md — feature/attention-drift-detection](followups.md))

### 3. Build stale task detection (effort: S, score: 3.70)

Implement a `--stale` flag on `aiki task list` that surfaces tasks in_progress for >2 hours with no comments. Target: >70% true-positive rate on genuinely stuck/abandoned tasks. This is a day's work, addresses a real multi-agent pain point, and TasteMatter's "abandoned work timestamps" feature validates the need. ([followups.md — experiment/stale-task-detection](followups.md))

---

## 4. Risks If We Do Nothing

1. **Drift detection becomes their wedge into workflow.** TasteMatter's attention drift feature is the nearest bridge from visibility into orchestration. If they add "suggested tasks" or "auto-review triggers" on top of drift signals, they move directly into Aiki's territory — and they'll have a frictionless, zero-config onboarding story (background daemon vs. explicit task discipline). Acting now on drift detection denies them this expansion path. ([aiki-map.md](aiki-map.md))

2. **"Just works" beats "requires discipline" for early adopters.** TasteMatter requires zero developer behavior change — install a daemon and query it. Aiki requires learning task commands. If TasteMatter captures the early Claude Code power-user audience with a frictionless onboarding, Aiki will face a harder adoption curve. The counter-positioning message needs to be in market before TasteMatter gets traction. ([research.md](research.md), [aiki-map.md](aiki-map.md))

3. **Zombie tasks erode trust in Aiki's model.** Without stale task detection, multi-agent workflows accumulate in_progress tasks that were silently abandoned. This makes `aiki task list` noisy and unreliable, undermining Aiki's core value proposition of structured provenance. TasteMatter's passive approach doesn't have this problem because it doesn't create persistent state. ([aiki-map.md](aiki-map.md), [followups.md](followups.md))

4. **Market narrative defaults to TasteMatter's framing.** If the competitive conversation is "context trails" (TasteMatter's language), Aiki is positioned as a heavier alternative. If the conversation is "structured provenance" (Aiki's language), TasteMatter is positioned as a shallow approximation. Whoever sets the narrative first wins the framing. ([opportunities.md — #1](opportunities.md))
