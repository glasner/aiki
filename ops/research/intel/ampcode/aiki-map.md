# Aiki Relevance Map: Ampcode (Amp)

**Date:** 2026-03-05
**Source:** `ops/now/intel/ampcode/research.md`

---

## Capability Classifications

| # | Capability | Overlap | Threat | Opportunity | Why Now |
|---|-----------|---------|--------|-------------|---------|
| 1 | Multi-model orchestration | Partial | Medium | Counter | Amp assigns best-in-class models per role (review, search, reasoning). Aiki is model-agnostic and orchestrates agents, not models — different layer. But Amp's review-specific model (Gemini 3 Pro) shows they treat review as a first-class concern. |
| 2 | Oracle system (second-opinion) | Partial | Medium | Copy | Oracle provides architectural guidance mid-conversation. Aiki's review system serves a similar validation role but post-hoc. An inline "advisor" during task execution could strengthen Aiki's iterative loop. |
| 3 | Skills framework | None | Low | Ignore | Markdown-based extensibility packages. Aiki uses CLAUDE.md instructions and task-based workflows instead. Different extension philosophy — Aiki extends via workflow, not plugins. |
| 4 | Checks system (composable review) | Direct | High | Counter | User-defined review criteria in `.agents/checks/`. This is squarely in Aiki's wedge — autonomous review with structured, composable criteria. Amp shipped this Feb 2026, signaling they see review as a competitive differentiator. Aiki must ensure its review system is at least as composable. |
| 5 | MCP support | None | Low | Ignore | Standard protocol support. Table stakes for any agent tool. Not relevant to Aiki's wedge. |
| 6 | Subagent spawning | Direct | High | Counter | Parallel isolated execution contexts for agent work. Directly overlaps with `aiki task run` and multi-agent orchestration. Amp's approach is model-native; Aiki's is workflow-native (task tracking, provenance, hooks). Aiki's advantage: persistence and observability across sessions. |
| 7 | Thread collaboration | Partial | Medium | Counter | Shareable, searchable conversation threads across teams. Aiki tasks are persistent and visible across agents/humans but lack the polished collaboration UX. Thread sharing is enterprise-attractive. Worth monitoring but not core to the autonomous review wedge. |
| 8 | Cross-repo search (Librarian) | None | Low | Ignore | Subagent for external code research. Useful but not in Aiki's autonomous review/iteration wedge. Aiki operates within repo boundaries. |
| 9 | Handoff system | Partial | Medium | Copy | Context transfer between focused threads. Aiki's task system provides continuity across sessions but lacks explicit "handoff" semantics for splitting complex tasks into focused sub-conversations. Could inform how Aiki handles task delegation context. |
| 10 | Custom toolboxes | None | Low | Ignore | User-defined executable tools. Aiki hooks and task system serve a different purpose. Not competitive threat. |
| 11 | AGENTS.md | None | Low | Ignore | Codebase-level agent configuration — equivalent to CLAUDE.md. Standard pattern, not differentiating. |
| 12 | Ad-supported free tier | None | Low | Ignore | Novel business model. Aiki competes on workflow value, not pricing model. Irrelevant to technical positioning. |

---

## Detailed Rationale

### High-Threat Capabilities

**Checks system (composable review):** Amp's `.agents/checks/` system lets users define structured review criteria that compose with each other. This is the most direct competitive overlap — Aiki's wedge is autonomous review, and Amp is building composable review primitives. The Feb 2026 launch timing suggests this is a strategic priority for Amp. Aiki needs to ensure its `aiki review` workflow supports user-defined review criteria with at least equivalent composability.

**Subagent spawning:** Both Aiki and Amp support parallel agent execution. Amp's is tightly integrated with its multi-model system (different models for different subagent roles). Aiki's advantage is that `aiki task run` provides full workflow context — task tracking, provenance, hooks, persistence. Amp's subagents are ephemeral within a thread; Aiki's are durable across sessions. This durability is a real differentiator for autonomous iteration loops.

### Medium-Threat Capabilities

**Multi-model orchestration:** Amp's assignment of specialized models to specific roles (review gets Gemini 3 Pro, search gets Gemini 3 Flash) shows intentional optimization. Aiki doesn't prescribe models — it's agent-runtime agnostic. This is a strength (flexibility) but also means Aiki can't optimize the review experience at the model level. The threat is medium because model selection is a thin moat — any tool can adopt this.

**Oracle system:** The "second opinion" pattern during active development is interesting. Aiki's review is post-hoc; Oracle is inline. If developers start expecting inline architectural validation, Aiki's review-at-the-end model could feel late. Medium threat because the use cases are complementary, not substitutive.

**Thread collaboration:** Enterprise teams sharing agent conversations is valuable for knowledge transfer. Aiki's task history is persistent but not designed for team browsing. Medium threat because collaboration is adjacent to, not within, the autonomous review wedge.

**Handoff system:** Context-preserving task splitting. Aiki does this via `aiki task run` with task descriptions, but lacks Amp's automatic context analysis for optimal handoff. Worth studying but not an immediate threat.

### Low-Threat Capabilities

Skills, MCP, cross-repo search, custom toolboxes, AGENTS.md, and the ad-supported tier are either table stakes or orthogonal to Aiki's autonomous review wedge.

---

## Overall Competitive Posture

**Posture: Competitive overlap is narrow but intensifying in the review space.**

Amp is a broad agentic coding tool that competes with Claude Code and Cursor. Most of its capabilities (multi-model, skills, MCP, threads) are about general-purpose agent quality and developer experience — not directly threatening to Aiki's wedge.

**However, two areas demand attention:**

1. **Composable review (Checks system):** This is the sharpest competitive signal. Amp is investing in structured, user-defined review criteria — exactly what makes Aiki's autonomous review valuable. Aiki must ensure its review system matches or exceeds this composability. The timing (Feb 2026) means this is fresh and likely to improve rapidly.

2. **Subagent orchestration:** Both tools support parallel agent work, but with different philosophies. Amp's is model-optimized and ephemeral; Aiki's is workflow-tracked and persistent. Aiki's approach is better for iterative development loops (build → review → fix → review), which is the core wedge. Leaning into this persistence advantage is the right counter-strategy.

**Aiki's defensible advantages vs Amp:**
- **Workflow persistence:** Tasks, reviews, and provenance survive across sessions. Amp threads are server-stored but not workflow-aware.
- **Iterative loops:** Aiki's build/review/fix cycle is a structured workflow. Amp's review is a one-shot check, not an iterative loop.
- **Agent-runtime agnosticism:** Aiki works with Claude Code, Codex, Cursor, etc. Amp is a single proprietary runtime.
- **Local-first:** Aiki stores everything in JJ/git. Amp stores threads on Sourcegraph servers (privacy concern for some teams).

**Key risk:** If Amp deepens its Checks system into full iterative review loops (check → fix → re-check), it would directly encroach on Aiki's primary wedge. Monitor their changelog closely.
