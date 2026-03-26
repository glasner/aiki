# Competitive Intel Brief: steveyegge/beads

**Date:** 2026-03-05
**Target:** https://github.com/steveyegge/beads
**Threat level:** MEDIUM

---

## 1. What This Project Is

Beads is a distributed, git-backed graph issue tracker built specifically for AI coding agents, created by Steve Yegge (ex-Amazon, ex-Google, ex-Sourcegraph). It replaces unstructured markdown with a dependency-aware task graph stored in a Dolt database, synced via git with zero central server. At 18.1k stars, 80 releases, and 7,420 commits since its Oct 2025 launch, it is the most-adopted agent task system in the ecosystem. Its companion project **Gas Town** extends Beads into a multi-agent orchestrator managing 20-30 parallel Claude Code agents through a "Mayor" coordinator pattern. Beads targets any coding agent doing any workflow — it is breadth-first by design, agent-agnostic, and unopinionated about what agents actually do.

— Sources: [GitHub](https://github.com/steveyegge/beads), [README](https://github.com/steveyegge/beads/blob/main/README.md), [Gas Town](https://github.com/steveyegge/gastown)

---

## 2. What Matters for Aiki

**Beads and Aiki compete on different axes.** Beads is a general-purpose agent memory/task system; Aiki is a vertical workflow tool for autonomous code review. The overlap is in task tracking primitives — both have hierarchical tasks, ready queues, priority ordering, and git-native sync. The divergence is in domain depth: Aiki has review loops, issue severity tracking, fix verification, and JJ workspace isolation that Beads doesn't attempt.

### Capabilities to COPY (Beads has it, Aiki needs it)

| Capability | Why | Source |
|-----------|-----|--------|
| **Memory decay / semantic summarization** | Context rot degrades long review sessions. Beads' `bd admin compact` summarizes completed tasks to free context window. Aiki has no compaction story — closed tasks pile up in JJ history with no synthesis. | [README](https://github.com/steveyegge/beads), [Ian Bull](https://ianbull.com/posts/beads/) |
| **Persistent cross-session memory** | Beads' `bd remember/recall/forget` solves the "50 First Dates" problem. Aiki agents rely on CLAUDE.md (static) and agent-specific auto-memory (unstructured). Review-specific memories (e.g., "this project always fails on strict null checks") would directly improve review quality. | [Releases](https://github.com/steveyegge/beads/releases), [Yegge blog](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a) |
| **Inter-agent messaging** | As Aiki scales parallel agents via `aiki run --async`, direct messaging between review and fix agents will beat indirect coordination through task state transitions. Gas Town's messaging shows real demand. | [Gas Town blog](https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04) |

### Capabilities to COUNTER (Beads has it, Aiki should respond differently)

| Capability | Aiki's counter-move | Source |
|-----------|---------------------|--------|
| **Dependency graph (4 types)** | Aiki's review loops are inherently sequential — add a minimal `--blocked-by` flag only if needed, not a full graph model. | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |
| **Atomic claiming (`--claim`)** | JJ workspace isolation is a stronger primitive (file-level, not just task-level). Add task-level atomicity to `aiki task start` as parallel agent count grows. | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |
| **Gas Town (20-30 agent orchestrator)** | Don't copy the generic Mayor pattern. Deepen review-aware orchestration: cascading re-reviews, severity-based priority routing, fix propagation tracking. Depth > breadth. | [Gas Town GitHub](https://github.com/steveyegge/gastown), [SE Daily](https://softwareengineeringdaily.com/2026/02/12/gas-town-beads-and-the-rise-of-agentic-development-with-steve-yegge/) |
| **MCP server** | Maintain CLAUDE.md-first integration (simpler, more reliable). Build `aiki-mcp` only if MCP becomes the dominant agent integration standard. | [Plugin docs](https://github.com/steveyegge/beads/blob/main/docs/PLUGIN.md) |
| **Community ecosystem (TUIs, web UIs)** | Build review-specific visualization (diff iteration views, fix-rate dashboards), not generic TUIs. Beads' community tools don't understand review semantics. | [Community Tools](https://github.com/steveyegge/beads/blob/main/docs/COMMUNITY_TOOLS.md) |

### Capabilities to IGNORE (not relevant to Aiki's wedge)

Hierarchical tasks (already have it), JSON output (surface concern), stealth mode, role detection, SQL queries, plugin framework, Dolt backend, offline sync (already git-native via JJ).

### Aiki's moat (what Beads can't easily replicate)

- **Autonomous code review** (`aiki review`, `aiki fix`, issue severity tracking) — Beads has zero review concept
- **JJ workspace isolation** — file-level conflict prevention, stronger than claim-based locking
- **Build/fix verification loops** — integrated build-test-fix cycles
- **Opinionated workflow enforcement** via CLAUDE.md hooks — agents use Aiki correctly without extra prompting (Beads' FAQ admits agents need reminding)

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md), [HN thread](https://news.ycombinator.com/item?id=46075616)

---

## 3. Top 3 Recommendations This Week

### 1. Ship review quality metrics (`aiki review stats`)

**What:** Define and compute review effectiveness metrics — fix rate, issue recurrence, severity distribution — from existing `aiki review` data. Add `aiki review stats` CLI command. No UI, forward-only collection.

**Why now:** This is Aiki's highest-scored opportunity (4.55) and the single strongest differentiator vs. Beads. Beads has no review concept, so it cannot build this. "Quantified autonomous code review" is a narrative Beads cannot counter. Enterprise buyers specifically ask for quality metrics to justify tooling.

**Expected impact:** Establishes Aiki as the measurable code review tool. Creates a feedback loop that improves review quality over time. Produces shareable proof points (metrics, trends) for positioning and marketing.

**Effort:** M (1-3 days)

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) (no review concept), [README](https://github.com/steveyegge/beads) (no review features)

### 2. Automate re-review after fixes

**What:** When a fix task closes after `aiki fix`, automatically trigger a scoped re-review on the changed files. Loop until clean or max 3 iterations. Add `--no-re-review` opt-out.

**Why now:** Manual re-triggering is the biggest friction point in the review-fix cycle. This turns Aiki from a review tool into a review engine. The demo is compelling: review finds issue → agent fixes → system auto-verifies. Gas Town dispatches generic tasks; Aiki would close the review loop end-to-end with zero manual intervention.

**Expected impact:** Higher-quality fixes (fewer regressions ship), less manual coordination, and a visible workflow advantage over Beads/Gas Town's generic dispatch model.

**Effort:** M (1-3 days)

— Sources: [Gas Town blog](https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04) (generic dispatch, no review loop)

### 3. Run a task summarization experiment

**What:** Build a prototype `aiki task summarize <id>` that generates an LLM-condensed summary of a completed parent task and its subtasks. Test on 10 real task trees. Evaluate accuracy (target: 4.0/5) and token reduction (target: <20% of original).

**Why now:** Memory decay is Beads' strongest feature that Aiki lacks. Context rot is already a problem in multi-step review sessions. This is a low-effort (< 1 day) experiment that validates whether automatic summarization works before committing to a full feature. If results are good, it directly supports longer autonomous review sessions and closes Aiki's biggest gap vs. Beads.

**Expected impact:** Validates a key feature direction at minimal cost. Success unlocks task compaction for production, reducing context pressure in long review workflows.

**Effort:** S (< 1 day)

— Sources: [README](https://github.com/steveyegge/beads) (`bd admin compact`), [Ian Bull](https://ianbull.com/posts/beads/) (context rot remains a problem even with Beads)

---

## 4. Risks If We Do Nothing

**Mindshare capture.** Beads is growing fast — 18.1k stars, weekly releases, podcast appearances ([SE Daily](https://softwareengineeringdaily.com/2026/02/12/gas-town-beads-and-the-rise-of-agentic-development-with-steve-yegge/)), HN threads ([#46075616](https://news.ycombinator.com/item?id=46075616)), and a thriving community ecosystem. If Beads becomes the default "agent task system," Aiki faces an uphill adoption battle even in its review-specific niche. Developers who adopt Beads first may not switch to Aiki later, even if Aiki is better for code review.

**Feature gravity from Gas Town.** Gas Town is currently a generic multi-agent orchestrator, but Yegge's velocity (7,420 commits, 80 releases in 5 months) means review-specific features could appear at any time. If Gas Town adds review loops or quality metrics, it closes the gap from the general-purpose side while already having the community and memory features Aiki lacks. The threat level escalates from MEDIUM to HIGH if this happens.

**Memory gap becomes a liability.** Beads' memory decay and persistent memory features address real pain points (context rot, session amnesia) that Aiki hasn't solved. As agent sessions grow longer and users run more parallel agents, the lack of compaction and cross-session memory will degrade Aiki's review quality — the exact thing Aiki is supposed to be best at. Ignoring this gap means Aiki's core wedge erodes from within.

**Narrative vacuum.** Beads owns "agent memory." Gas Town owns "multi-agent orchestration." If Aiki doesn't claim "quantified autonomous code review" now, the narrative space fills with Beads extensions and community tools. First-mover advantage in positioning matters — the tools that define the category vocabulary win long-term.

— Sources: [GitHub](https://github.com/steveyegge/beads), [Gas Town GitHub](https://github.com/steveyegge/gastown), [Yegge blog](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a), [paddo.dev](https://paddo.dev/blog/from-beads-to-tasks/) (Anthropic credited Beads as inspiration)
