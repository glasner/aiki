# Execution Tasks — UseGitAI Competitive Response

**Date**: 2026-03-05
**Source**: [opportunities.md](opportunities.md)

---

## feature/preprompt-context-injection

**From opportunity**: #1 PrePrompt Context Injection (Score: 4.30)

**Hypothesis**: Automatically injecting architectural context, prior review feedback, and task history into the PrePrompt event will make agents produce higher-quality code on the first pass — reducing review churn and countering Git AI's `/ask` with a richer, zero-effort alternative. Where `/ask` requires the agent to explicitly query past transcripts, PrePrompt delivers relevant context before the agent even starts.

**Success metric**: Agents operating with PrePrompt context injection produce code that passes autonomous review on the first attempt at least 30% more often than agents without it, measured across 20+ real tasks.

**Scope guardrails**:
- **In scope**: PrePrompt event implementation (Phase 8); architecture cache that summarizes repo structure, conventions, and recent review feedback; integration with Aiki's existing task provenance data; token budget management to avoid bloating context.
- **Out of scope**: Full Phase 10 architecture caching; transcript retrieval (Git AI's approach — we're not copying `/ask`); IDE plugin work; support for non-Claude agents (handled separately).

**Estimated effort**: L

---

## feature/ai-authored-review-weighting

**From opportunity**: #3 AI-Authored Code Review Weighting (Score: 4.05)

**Hypothesis**: AI-generated code has distinct failure modes (hallucinated APIs, missing error handling, insufficient edge-case coverage) that human-written code doesn't. Applying stricter, AI-specific review checks when authorship metadata indicates AI origin will catch more defects than uniform review — deepening Aiki's autonomous review wedge.

**Success metric**: AI-weighted reviews surface at least 2x more actionable issues on AI-authored code compared to the current uniform review pipeline, measured on a sample of 50+ AI-authored changesets.

**Scope guardrails**:
- **In scope**: Detecting AI authorship signals (Git AI's Git Notes, Aiki's own task provenance, commit message heuristics); additional review checks for AI code (hallucinated API detection, error-handling completeness, test coverage gaps); configurable strictness levels per repo.
- **Out of scope**: Building our own attribution/blame system (that's Git AI's domain); reviewing human-written code differently; any changes to the review UX — this is backend review logic only.

**Estimated effort**: M

---

## experiment/multi-agent-integration-demand

**From opportunity**: #2 Multi-Agent Integration Breadth (Score: 4.10)

**Hypothesis**: Teams using multiple AI coding agents (Cursor + Copilot + Claude Code) need review tooling that works across all of them, but we don't yet know which agents beyond Claude Code have enough Aiki-compatible user overlap to justify integration effort. A lightweight probe of the top 3 candidates (Cursor, Copilot, Codex) will reveal actual demand before we invest in full integration.

**Success metric**: Identify which of the 3 candidate agents has the highest demand signal — measured by: (a) number of inbound requests or mentions in community channels, (b) feasibility of hook integration assessed via proof-of-concept, and (c) at least one working PoC that runs Aiki's review pipeline on a non-Claude agent's output. Proceed to full integration only for agents scoring above threshold on all three signals.

**Scope guardrails**:
- **In scope**: Lightweight hook PoCs for Cursor, Copilot, and Codex; community demand survey or signal analysis; documenting each agent's hook/integration surface; decision framework for which agents to prioritize.
- **Out of scope**: Production-quality integrations; supporting all 12+ agents Git AI covers; IDE plugin development; any changes to Aiki's core review pipeline to accommodate agent-specific quirks.

**Estimated effort**: S

---

## positioning/complementary-not-competitive

**From opportunity**: #4 Git AI Metadata Interop (Score: 3.65)

**Hypothesis**: Positioning Aiki as "works with Git AI" rather than competing against it will tap into Git AI's 1,200+ star community as an acquisition channel. Teams already using Git AI for attribution want smarter review on top of that metadata — Aiki fills a gap Git AI doesn't cover. Framing the relationship as complementary (attribution + review = complete AI code quality) resonates better than positioning as a replacement.

**Success metric**: Publish a "Better together: Aiki + Git AI" integration guide and announce it in Git AI's Discord and relevant developer channels. Target: 50+ guide views in the first week and at least 5 new Aiki trials sourced from Git AI community members within 30 days.

**Scope guardrails**:
- **In scope**: Writing the integration guide showing how Git AI's authorship metadata feeds into Aiki's review intelligence; building a minimal Git Notes reader so Aiki can consume Git AI's attribution data; crafting messaging that emphasizes complementarity ("Git AI tells you who wrote it, Aiki tells you if it's good"); posting in Git AI Discord and relevant HN/Reddit threads.
- **Out of scope**: Formal partnership or co-marketing agreement with Git AI; building features that depend on Git AI being present (it should enrich, not require); changing Aiki's core positioning away from autonomous review; paid advertising.

**Estimated effort**: S
