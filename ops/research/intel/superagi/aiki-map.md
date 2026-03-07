# SuperAGI vs Aiki — Relevance Map

**Aiki wedge:** Autonomous code review, task management for AI agents, SDLC workflow automation. Key differentiators: JJ-based workspace isolation, task provenance tracking, multi-agent orchestration.

---

## Capability Classification

| # | SuperAGI Capability | Overlap with Aiki Wedge | Threat | Decision | Why Now / Timing Signal |
|---|---------------------|------------------------|--------|----------|------------------------|
| 1 | **Agent Loop (ReAct-style)** | Low — generic reasoning loop, not SDLC-specific. Aiki orchestrates agents; it doesn't replace the agent's internal reasoning. | Low | Ignore | Framework is abandoned (no release since Jan 2024). No competitive pressure from this component. |
| 2 | **Memory System (STM + LTS)** | Low — Aiki's task provenance and JJ history serve a similar "persistent context" role but are structured around SDLC artifacts, not generic agent memory. | Low | Ignore | Generic agent memory is table stakes. Aiki's structured task/review history is more valuable for SDLC than rolling summaries. |
| 3 | **GUI-first Orchestration** | Med — Both manage agent workflows. SuperAGI's GUI manages generic agents; Aiki manages code-focused agents via CLI/hooks. Different modality, overlapping intent. | Low | Counter | GUI-first approach attracted stars but didn't retain production users. CLI-first + editor integration is the right modality for developers. Counter by staying CLI-native. |
| 4 | **Concurrent Agent Execution** | High — Direct overlap with Aiki's multi-agent orchestration (`aiki task run --async`, workspace isolation). Both solve "run multiple agents in parallel safely." | Med | Counter | SuperAGI's version lacked workspace isolation (no conflict prevention). Aiki's JJ-based isolation is a strict upgrade. Counter by emphasizing safe concurrency. |
| 5 | **Toolkit/Plugin Marketplace** | Low — Aiki doesn't have a marketplace model. SuperAGI's toolkits are generic (Google Search, Twitter, etc.); Aiki's integrations are SDLC-focused (JJ, git, code review). | Low | Ignore | Marketplace model didn't drive retention. SDLC-specific depth beats breadth of generic toolkits. |
| 6 | **Agent Performance Monitoring (APM)** | Med — Aiki's task provenance, comment history, and review lineage serve a similar "observe what agents did" role. APM is telemetry; Aiki is audit trail. | Low | Copy (partially) | Agent observability is becoming expected. Copy the concept of dashboarding agent performance, but implement it as task/review metrics rather than generic APM. |
| 7 | **Agent Scheduling (cron-style)** | Low — Aiki doesn't currently do scheduled agent runs. Aiki is event-driven (prompt → task → agent) not time-driven. | Low | Ignore | Scheduled agents are a niche need. Aiki's event-driven model (triggered by code changes, reviews, prompts) is more natural for SDLC. |
| 8 | **SuperCoder (code generation agent)** | Med — Both operate in the code domain. But SuperCoder is a coding agent; Aiki orchestrates coding agents (Claude Code, Codex, Cursor). Aiki is the layer above. | Low | Ignore | SuperCoder is abandoned and outclassed by Claude Code, Cursor, Copilot. Aiki doesn't need its own coding agent — it orchestrates best-in-class ones. |
| 9 | **Restricted Mode (human-in-the-loop)** | Med — Aiki's review workflow is a form of structured human-in-the-loop for code changes. SuperAGI's restricted mode is generic permission gating. | Low | Counter | Aiki's approach (autonomous review + structured approval) is more sophisticated than binary "allow/deny" permission. Counter by deepening the review workflow. |
| 10 | **Public APIs + SDKs** | Med — Both expose programmatic interfaces. Aiki uses CLI (`aiki task`, `aiki review`); SuperAGI used REST APIs + Python/Node SDKs. | Low | Ignore (for now) | CLI-first is right for current stage. APIs/SDKs become relevant when Aiki needs third-party integrations, but not yet. |
| 11 | **Webhooks (event-driven integration)** | Med — Aiki's hooks system serves a similar role (pre/post hooks on tool calls, session events). Different mechanism, same intent. | Low | Counter | Aiki's hook system is more tightly integrated with the agent workflow. Continue investing in hooks rather than adopting webhook patterns. |
| 12 | **Agent Templates** | Low — Pre-built agent configs (sales, recruitment). Completely different domain from SDLC. | None | Ignore | Irrelevant to Aiki's wedge. |
| 13 | **Token Usage Optimization** | Low — Cost management is orthogonal to Aiki's core value prop. Agent hosts (Claude Code, etc.) handle their own token management. | None | Ignore | Not Aiki's problem to solve. |
| 14 | **Vector DB Support (knowledge retrieval)** | Low — Aiki's "knowledge" is structured (tasks, reviews, diffs, JJ history), not embedded in vector DBs. Different retrieval paradigm. | None | Ignore | Structured provenance > vector similarity for SDLC context. |
| 15 | **Commercial CRM Pivot** | None — Completely different market (sales teams vs. dev teams). | None | Ignore | Confirms SuperAGI abandoned the developer tools space. No competitive threat from CRM product. |

---

## Summary Matrix

| Decision | Count | Capabilities |
|----------|-------|-------------|
| **Ignore** | 9 | Agent Loop, Memory, Marketplace, Scheduling, SuperCoder, APIs/SDKs, Templates, Token Optimization, Vector DB, CRM |
| **Counter** | 5 | GUI Orchestration, Concurrent Agents, Restricted Mode, Webhooks |
| **Copy** | 1 | APM (as task/review metrics) |

---

## Key Takeaways

1. **SuperAGI is not a competitive threat.** The OSS framework is abandoned (14+ months without a release, no core team commits). The company pivoted to CRM. The developer agent framework space they vacated is exactly where Aiki operates.

2. **The one thing worth copying is agent observability.** SuperAGI's APM dashboard concept — understanding what agents did, how well they performed, where they got stuck — maps to Aiki's task provenance. Building metrics/dashboards on top of task history would be valuable.

3. **Concurrent agent execution is the closest overlap** but Aiki's JJ-based workspace isolation is strictly superior to SuperAGI's approach (which had no conflict prevention for parallel agents).

4. **SuperAGI's failure mode is instructive.** They went broad (20+ generic toolkits, marketplace, GUI) instead of deep in one domain. Aiki's SDLC focus is the right counter-strategy: depth in code review, task provenance, and workspace isolation beats breadth of generic agent tooling.

5. **Timing signal:** SuperAGI's abandonment leaves a vacuum in "orchestration layer for AI agents." But the real opportunity isn't their abandoned users — it's that the problem they tried to solve generically (multi-agent orchestration) now has a concrete, high-value vertical (SDLC) that Aiki is purpose-built for.
