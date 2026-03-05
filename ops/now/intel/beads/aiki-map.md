# Aiki Relevance Map: steveyegge/beads

**Date:** 2026-03-05
**Source:** `ops/now/intel/beads/research.md`
**Aiki wedge:** Autonomous code review — AI-driven review loops, task decomposition, build/fix workflows, session isolation, and developer workflow orchestration.

---

## Summary Table

| # | Capability | Overlap | Threat | Opportunity | Why Now |
|---|-----------|---------|--------|-------------|---------|
| 1 | Dependency graph (4 relationship types) | medium | med | counter | Aiki has parent-child + source lineage but lacks blocks/relates_to/duplicates |
| 2 | Hierarchical tasks (epics → tasks → subtasks) | high | low | ignore | Aiki already has parent + subtask (.1, .2) hierarchy |
| 3 | Memory decay / semantic summarization | low | med | copy | Context rot is a universal agent problem; Aiki has no compaction story |
| 4 | Persistent key-value memory (remember/recall/forget) | low | med | copy | Agents need cross-session memory; Aiki relies on JJ history + CLAUDE.md |
| 5 | Atomic claiming (--claim for concurrent agents) | medium | high | counter | Aiki uses JJ workspace isolation instead of claim-based locking |
| 6 | JSON-first output for agents | medium | low | ignore | Aiki CLI already outputs structured text; JSON is a surface concern |
| 7 | Ready-work detection (bd ready) | high | med | ignore | Aiki has `aiki task` with ready queue and priority filtering |
| 8 | Messaging/threading between agents | none | med | copy | Aiki has no inter-agent messaging; Gas Town shows demand |
| 9 | Graph links (relates_to, duplicates, supersedes) | low | low | ignore | Nice-to-have; not core to autonomous review wedge |
| 10 | Stealth mode (local-only operation) | none | low | ignore | Niche use case; Aiki's JJ-based approach is inherently branch-isolated |
| 11 | Role detection (contributor vs. maintainer) | none | low | ignore | Not relevant to Aiki's single-developer-with-agents model |
| 12 | MCP server with context reduction | none | med | counter | Aiki integrates via CLAUDE.md hooks, not MCP; different integration model |
| 13 | SQL queries against task database | none | low | ignore | Aiki uses JJ as storage; SQL adds complexity without clear value |
| 14 | Offline-first / git-native sync | high | low | ignore | Aiki is already git-native via JJ; both solve the same problem |
| 15 | Gas Town (multi-agent orchestrator) | medium | high | counter | Aiki has `aiki task run` for delegation; Gas Town is more ambitious (20-30 agents) |
| 16 | Plugin framework (GitLab, Linear adapters) | none | low | ignore | Aiki is opinionated; plugin extensibility is premature |
| 17 | Dolt database backend | none | low | ignore | Aiki uses JJ; Dolt is an implementation detail, not a feature |
| 18 | Community ecosystem (TUIs, web UIs, editor plugins) | none | med | counter | Aiki has no community layer yet; Beads' ecosystem signals product-market fit |

---

## Detailed Analysis

### 1. Dependency Graph (4 relationship types)

Beads supports `blocks`, `parent-child`, `related`, and `discovered-from` relationships between tasks. This enables the `bd ready` command to compute unblocked work deterministically. Aiki currently has parent-child relationships (subtasks) and source lineage (`--source` flag), but lacks explicit blocking dependencies or lateral relationships.

The blocking relationship is the most tactically relevant — it enables automatic ready-queue computation. Aiki's ready queue is manually managed (tasks are "ready" by default unless they have a parent). However, Aiki's autonomous review wedge doesn't require complex dependency graphs; review loops are sequential by nature (review → fix → re-review). The overhead of maintaining a full dependency graph may not justify the benefit for Aiki's primary use case.

**Opportunity: counter.** Rather than copying Beads' graph model, Aiki can lean into its review-loop workflow where dependencies are implicit in the review cycle. If blocking dependencies become needed, they should be minimal (a `--blocked-by` flag) rather than a full graph.

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md), [README](https://github.com/steveyegge/beads)

### 2. Hierarchical Tasks (Epics → Tasks → Subtasks)

Beads supports 3 levels of nesting with auto-numbered children. Aiki already has parent tasks with subtasks using the `<parent-id>.1` notation, making this a direct overlap. Both systems use hash-based IDs with progressive length scaling.

This is a table-stakes feature that both products handle adequately. Beads offers "epics" as a third level, but Aiki's two-level hierarchy (parent + subtasks) is sufficient for the autonomous review use case where task decomposition rarely exceeds two levels.

**Opportunity: ignore.** Aiki's existing hierarchy is sufficient. Adding a third level would add complexity without serving the core wedge.

— Sources: [README](https://github.com/steveyegge/beads), [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)

### 3. Memory Decay / Semantic Summarization

Beads' `bd admin compact` performs semantic summarization of completed tasks to preserve context windows. This addresses "context rot" — the degradation of agent effectiveness as conversation history grows. Aiki has no equivalent; closed tasks remain in JJ history but there's no summarization or compaction mechanism.

This is a genuine gap. As Aiki sessions grow longer and span more tasks, context window pressure will increase. Aiki's current mitigation is session isolation (each agent gets a fresh context), but cross-session knowledge transfer depends on task summaries in `--summary` flags and CLAUDE.md, which are manually curated.

**Opportunity: copy.** Aiki should consider automatic summarization of completed task trees. This could be implemented as a post-close hook that generates a condensed summary of all subtask work, stored in JJ for future agents to reference. This directly supports the autonomous review wedge by keeping review context manageable across long sessions.

— Sources: [README](https://github.com/steveyegge/beads), [Ian Bull blog](https://ianbull.com/posts/beads/)

### 4. Persistent Key-Value Memory (remember/recall/forget)

Beads provides `bd remember`, `bd memories`, `bd recall`, and `bd forget` — a key-value store that's auto-injected into agent context at session start. This solves the "50 First Dates" problem where agents lose learned preferences and decisions between sessions.

Aiki currently relies on CLAUDE.md for persistent configuration and JJ history for task records. The auto-memory directory (`~/.claude/projects/.../memory/`) provides some persistence but is agent-specific (Claude Code only) and not structured. Beads' approach is more general and agent-agnostic.

**Opportunity: copy.** A structured memory layer (`aiki remember "always run tests before closing"`) would complement Aiki's task system. This is especially relevant for the review wedge — agents could remember project-specific review criteria, common failure patterns, and preferred fix approaches across sessions. Implementation could piggyback on JJ's key-value metadata or a dedicated memories branch.

— Sources: [Releases](https://github.com/steveyegge/beads/releases), [Steve Yegge Medium](https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a)

### 5. Atomic Claiming (--claim for concurrent agents)

Beads' `--claim` flag provides database-level locking to prevent race conditions when multiple agents attempt to start the same task. This is critical for multi-agent workflows where 20-30 agents may be working in parallel (as in Gas Town).

Aiki solves this differently: JJ workspace isolation gives each agent its own copy of the codebase, and `aiki task start` transitions tasks to in-progress. However, Aiki doesn't have atomic claiming — two agents could theoretically start the same task simultaneously. In practice, Aiki's orchestration model (`aiki task run`) assigns tasks to specific agents, reducing contention.

**Opportunity: counter.** Aiki's workspace isolation is a stronger primitive than claim-based locking. Where Beads prevents conflicts at the task level, Aiki prevents them at the file level. The gap is in task-level contention for ready-queue scenarios. If Aiki scales to more parallel agents, atomic claiming or an equivalent should be added to `aiki task start`.

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)

### 6. JSON-First Output for Agents

Every Beads command supports a `--json` flag for machine-readable output. This enables agents to parse task data programmatically rather than scraping CLI text.

Aiki's CLI outputs structured text that agents parse from tool results. Since Aiki is tightly integrated with specific agent platforms (Claude Code, Codex, Cursor) via CLAUDE.md hooks, the output format is already optimized for those consumers. JSON output is a nice-to-have but not a differentiator.

**Opportunity: ignore.** The current text output works well within Aiki's integration model. JSON could be added later if third-party integrations demand it.

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)

### 7. Ready-Work Detection (bd ready)

Beads' `bd ready` returns unblocked, prioritized tasks in ~10ms. It considers dependency graphs, priorities, and claiming status to compute what an agent should work on next. Aiki has `aiki task` which shows the ready queue with priority filtering.

Both systems solve the same problem. Beads' version is more sophisticated due to dependency graph awareness, while Aiki's is simpler but sufficient for its current use cases. Aiki's ready queue is priority-ordered and scope-filtered (when working on a parent task, only subtasks are shown).

**Opportunity: ignore.** Aiki's ready queue is adequate. If blocking dependencies are added (see #1), ready-work detection should be updated accordingly.

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)

### 8. Messaging/Threading Between Agents

Beads supports an "issue" type with `--thread` flag for ephemeral inter-agent communication and delegation patterns. Gas Town extends this with a "Mayor" coordinator that manages communication across 20-30 parallel agents.

Aiki has no inter-agent messaging. Agents communicate indirectly through task comments (`aiki task comment add`) and task state transitions, but there's no direct messaging channel. The community has also built `mcp-beads-village` for inter-agent coordination with file locking.

**Opportunity: copy.** As Aiki scales to more parallel agents (via `aiki task run --async`), inter-agent communication will become necessary. A lightweight messaging primitive (`aiki msg send <agent-id> "..."`) could enable coordination patterns like "agent A found a blocker that agent B needs to know about." This supports the autonomous review wedge: a review agent could message a fix agent directly rather than going through task state transitions.

— Sources: [README](https://github.com/steveyegge/beads), [Gas Town blog](https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04)

### 9. Graph Links (relates_to, duplicates, supersedes)

Beads supports lateral relationships between tasks beyond parent-child. These enable deduplication and task evolution tracking. Aiki has `--source` for lineage but no lateral links.

These are useful for large projects with many tasks but add cognitive overhead. Aiki's simpler model (parent-child + source lineage) covers most autonomous review scenarios.

**Opportunity: ignore.** Lateral links are a backlog tracker feature, not an autonomous review feature. Aiki should stay focused.

— Sources: [README](https://github.com/steveyegge/beads)

### 10. Stealth Mode (local-only operation)

Beads' `bd init --stealth` operates locally without committing to the repository. Useful for experimentation or projects where you don't want Beads artifacts in version control.

Aiki's JJ-based approach stores tasks on a separate `aiki/tasks` branch, which is already somewhat stealth (doesn't pollute the main branch). This is a niche feature.

**Opportunity: ignore.** Not relevant to Aiki's wedge.

— Sources: [README](https://github.com/steveyegge/beads)

### 11. Role Detection (Contributor vs. Maintainer)

Beads detects whether the user is a contributor or maintainer and provides isolated planning databases accordingly. This supports open-source workflows where contributors shouldn't see internal project planning.

Aiki targets a single developer or small team working with AI agents. Role-based access control is premature for this use case.

**Opportunity: ignore.** Not relevant to current Aiki users.

— Sources: [README](https://github.com/steveyegge/beads)

### 12. MCP Server with Context Reduction

Beads provides a Python-based MCP server (`beads-mcp`) with aggressive context reduction for AI agents. This enables integration with any MCP-compatible agent without requiring CLAUDE.md-style file injection.

Aiki integrates via CLAUDE.md hooks and shell commands, which is tightly coupled to specific agent platforms. MCP is a more portable integration surface but requires agents to support MCP. The trade-off: Aiki's CLAUDE.md approach is simpler and works immediately; MCP is more standard but more complex.

**Opportunity: counter.** Aiki should maintain its CLAUDE.md-first integration model (it's simpler and more reliable) while monitoring MCP adoption. If MCP becomes the dominant agent integration standard, an `aiki-mcp` server would be straightforward to build since the CLI already exists.

— Sources: [Plugin docs](https://github.com/steveyegge/beads/blob/main/docs/PLUGIN.md), [MCP docs](https://steveyegge.github.io/beads/integrations/mcp-server)

### 13. SQL Queries Against Task Database

Beads stores tasks in a Dolt database (version-controlled SQL) and exposes full SQL query access. This enables ad-hoc analytics on task data.

Aiki stores tasks in JJ history, which is not SQL-queryable. However, the task CLI provides all the query capability needed for the autonomous review use case. SQL access is a power-user feature that adds complexity.

**Opportunity: ignore.** SQL access serves backlog analytics, not autonomous review workflows.

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)

### 14. Offline-First / Git-Native Sync

Beads is fully offline-first, syncing via git push/pull. All queries run locally. Aiki is similarly git-native via JJ, with tasks stored on a separate branch and synced through normal JJ/git operations.

Both products share the same architectural philosophy here. This is table stakes for developer tools.

**Opportunity: ignore.** Already covered by Aiki's JJ foundation.

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md)

### 15. Gas Town (Multi-Agent Orchestrator)

Gas Town is a separate project that manages "colonies" of 20-30 parallel Claude Code agents through a "Mayor" coordinator pattern. It builds on Beads for task storage and adds agent lifecycle management, work distribution, and result aggregation.

Aiki has `aiki task run` for delegating work to subagents and `--async` + `aiki task wait` for parallel execution. However, Aiki's orchestration is simpler — it delegates individual tasks rather than managing a fleet of agents. Gas Town represents a more ambitious vision for multi-agent workflows.

**Opportunity: counter.** Aiki's orchestration model is already differentiated by being review-loop-aware (review → fix → re-review cycles). Rather than copying Gas Town's Mayor pattern, Aiki should deepen its review-specific orchestration: automatic re-review after fixes, cascading fix propagation, and review quality scoring. The key differentiator is that Aiki's orchestration understands code review semantics, while Gas Town is a generic task dispatcher.

— Sources: [Gas Town GitHub](https://github.com/steveyegge/gastown), [Gas Town blog](https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04), [SE Daily](https://softwareengineeringdaily.com/2026/02/12/gas-town-beads-and-the-rise-of-agentic-development-with-steve-yegge/)

### 16. Plugin Framework (GitLab, Linear Adapters)

Beads v0.50+ introduced a plugin-based architecture for external tracker adapters (GitLab, Linear). This enables bidirectional sync with existing project management tools.

Aiki is opinionated and self-contained. Plugin extensibility would dilute focus at this stage.

**Opportunity: ignore.** Premature for Aiki's current stage.

— Sources: [Releases](https://github.com/steveyegge/beads/releases)

### 17. Dolt Database Backend

Beads uses Dolt (version-controlled SQL) for storage, enabling cell-level merging, native branching, and remote sync. This replaced the earlier SQLite+JSONL dual storage.

This is an implementation detail. Aiki uses JJ, which provides similar version-control primitives. The choice of storage backend doesn't directly affect user-facing features.

**Opportunity: ignore.** Implementation detail, not a competitive concern.

— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md), [DoltHub blog](https://www.dolthub.com/blog/)

### 18. Community Ecosystem (TUIs, Web UIs, Editor Plugins)

Beads has spawned a significant community ecosystem: terminal UIs (beads_viewer, lazybeads, bdui), web interfaces (beads-ui, kanban views), editor integrations (VS Code, Neovim, Emacs), and multi-agent coordination tools (mcp-beads-village). This signals strong product-market fit and extensibility.

Aiki currently has no community ecosystem. The CLAUDE.md integration model makes third-party tooling harder since the protocol is file-based rather than API-based.

**Opportunity: counter.** Aiki's community story should emerge from its core wedge. Rather than building generic TUIs or dashboards, Aiki should focus on review-specific visualization: diff views of review iterations, fix-rate dashboards, and review quality trends. These would be harder for Beads to replicate because they require deep understanding of code review semantics.

— Sources: [README](https://github.com/steveyegge/beads), [Ian Bull blog](https://ianbull.com/posts/beads/), [Community Tools](https://github.com/steveyegge/beads/blob/main/docs/COMMUNITY_TOOLS.md)

---

## Overall Threat Assessment

**Threat level: MEDIUM**

Beads is a formidable project with strong community traction (18.1k stars, active ecosystem), rapid iteration (80 releases, 7,420 commits), and a credible author (Steve Yegge). However, Beads and Aiki are competing on **different axes**:

- **Beads** is a **general-purpose agent memory/task system** — it wants to be the universal issue tracker for AI agents, regardless of what those agents are doing. Its strength is breadth: any agent, any workflow, any project.
- **Aiki** is a **vertical workflow tool for autonomous code review** — it combines task tracking with review loops, build/fix workflows, and session isolation into an opinionated development workflow. Its strength is depth: it understands code review semantics.

**Where Beads threatens Aiki:**
1. **Mindshare capture.** Beads' community growth could establish it as "the" agent task system, making it harder for Aiki to gain adoption even in the review-specific niche.
2. **Feature gravity.** Gas Town's multi-agent orchestration could evolve to include review-specific features, closing the gap from the general-purpose side.
3. **Memory and context management.** Beads' memory decay and persistent memory features address real pain points that Aiki hasn't solved yet.

**Where Beads does NOT threaten Aiki:**
1. **Code review semantics.** Beads has no concept of reviews, diffs, issues with severity, or fix verification. Aiki's `aiki review`, `aiki fix`, and review-aware orchestration are genuine differentiators.
2. **Session isolation.** Aiki's JJ workspace isolation is a stronger concurrency primitive than Beads' claim-based locking. It prevents file-level conflicts, not just task-level conflicts.
3. **Opinionated workflow.** Aiki's CLAUDE.md integration enforces a specific workflow (start → comment → close → review → fix). Beads is unopinionated, which means agents need more prompting to use it correctly (a known weakness per the FAQ).
4. **Build/fix loops.** Aiki's integration of build verification, test running, and fix-then-verify cycles has no equivalent in Beads.

---

## Key Differentiators

### What Beads has that Aiki doesn't
| Capability | Impact on Aiki |
|-----------|---------------|
| Dependency graph with blocking relationships | Medium — would improve ready-queue accuracy |
| Memory decay / semantic summarization | Medium — would help with long-session context management |
| Persistent key-value memory (remember/recall) | Medium — would enable cross-session learning |
| Inter-agent messaging/threading | Low-Medium — relevant as parallel agent count grows |
| MCP server integration | Low — different integration model, not necessarily better |
| Community ecosystem (TUIs, web UIs) | Low — Aiki needs its own ecosystem, not generic tools |
| SQL query access to task data | Low — analytics feature, not workflow feature |

### What Aiki has that Beads doesn't
| Capability | Competitive Moat |
|-----------|-----------------|
| Autonomous code review (`aiki review`, `aiki fix`) | High — core wedge, no Beads equivalent |
| Review issue tracking with severity levels | High — structured review output |
| JJ workspace isolation (file-level conflict prevention) | High — stronger than claim-based locking |
| Build/fix verification loops | High — integrated build-test-fix cycles |
| Review-aware task orchestration | Medium — review → fix → re-review automation |
| Source lineage tracking (`--source prompt/file/task`) | Medium — Beads has `discovered-from` but less structured |
| Opinionated workflow enforcement via CLAUDE.md hooks | Medium — agents use Aiki correctly without extra prompting |
| Won't-do task closure | Low — niche but useful for review triage |

---

## Strategic Recommendations

1. **Protect the wedge.** Aiki's autonomous review loop is the primary differentiator. Double down on review-specific features (review quality metrics, auto-re-review, fix verification) rather than chasing Beads' general-purpose breadth.

2. **Close the memory gap.** Memory decay and persistent memory are Beads' strongest features that Aiki lacks. Implement task summarization on close and a structured memory layer. These directly support longer autonomous review sessions.

3. **Don't compete on ecosystem.** Beads' community tooling (TUIs, dashboards) is impressive but generic. Aiki's ecosystem should be review-specific: diff viewers, review iteration trackers, fix-rate dashboards.

4. **Monitor Gas Town.** If Gas Town adds review-specific features, the threat level increases to HIGH. Currently it's a generic orchestrator, but Yegge's velocity is concerning (7,420 commits, weekly releases).
