# Aiki Relevance Map — UseGitAI

**Date**: 2026-03-05
**Source**: [ops/now/intel/usegitai/research.md](research.md)

## Capability Classification

| # | Capability | Aiki Wedge Overlap | Threat Level | Opportunity | Why Now |
|---|---|---|---|---|---|
| 1 | AI Code Attribution (Git Notes) | Low | Low | Ignore | Git AI has shipped this as stable (v1.0+); Aiki's wedge is review quality, not line-level blame. Attribution is complementary infrastructure, not competitive. |
| 2 | AI Blame (`git ai blame`) | Low | Low | Ignore | Extends `git blame` with agent/model metadata. Useful for visibility but orthogonal to autonomous review. No timing pressure — this is table-stakes traceability, not review intelligence. |
| 3 | `/ask` Skill (Session Context Retrieval) | **Med** | **Med** | Counter | `/ask` lets agents answer questions about code they wrote by retrieving original transcripts. This overlaps with Aiki's architecture caching vision (Phase 10) — both aim to reduce re-discovery cost. Git AI's `/ask` blog (Feb 2026) claims agents become "noticeably smarter" in plan mode. Aiki should counter with richer contextual injection via PrePrompt (Phase 8) rather than copying transcript retrieval. |
| 4 | Prompt Storage & Preservation | Low | Low | Ignore | Stores every prompt associated with code lines. Aiki already tracks task provenance and session context. Prompt-level storage is a different design point (individual prompts vs. task-level tracking). Low overlap, no urgency. |
| 5 | Personal Dashboards & Stats | Low | Low | Ignore | % AI per commit, accepted rates by agent/model. Analytics-oriented, not review-oriented. Aiki's value is in quality assurance, not usage measurement. |
| 6 | IDE Plugins (VS Code, Cursor, Windsurf) | Low | Low | Ignore | Color-coded gutter decorations for AI vs human lines. IDE integration is a distribution channel, not a wedge overlap. Aiki operates at the workflow/agent layer, not the IDE gutter layer. |
| 7 | Agent Integration Breadth (12+ agents) | **Med** | **Med** | Copy | Git AI supports 12+ agents including Claude Code, Cursor, Copilot, Codex, Gemini CLI. Aiki currently integrates primarily with Claude Code. Breadth matters for adoption — as teams use multiple agents, single-agent tools lose leverage. The weekly release cadence adding new agents (Amp CLI added Mar 2026) signals accelerating ecosystem coverage. |
| 8 | Teams Tier (Repo-level AI %, Durability, ROI) | Low | Low | Ignore | Team analytics: AI code durability, readiness scoring, ROI tracking. These are enterprise analytics features. Aiki's wedge is autonomous review, not reporting dashboards. Different buyer persona (engineering managers vs. IC developers). |
| 9 | Enterprise Tier (Self-hosted, Multi-SCM, Data Lake) | Low | Low | Ignore | Enterprise deployment options. Premature for Aiki to compete here. Different stage, different market segment. |
| 10 | Incident Linking (Prod incidents → AI code) | **Med** | Low | Counter | Links production incidents to AI-generated code. Interesting signal — connects quality concerns to authorship. Aiki could counter by catching quality issues *before* they reach production via autonomous review, making incident-level attribution less necessary. Not urgent — this is a Teams-tier feature with unclear adoption. |
| 11 | Post-commit Hooks (Zero-disruption Workflow) | **Med** | **Med** | Copy | Git AI uses post-commit hooks with zero workflow disruption. Aiki uses similar hook-based architecture (PreToolUse, PostToolUse). The pattern is validated — hooks as the integration surface. Aiki should ensure its hook system is equally frictionless. Git AI's "100% offline, no login" positioning pressures Aiki to keep its local-first story strong. |
| 12 | Open Source Core (Apache 2.0, Rust) | Low | **Med** | Counter | 1,200+ stars, Apache 2.0, Rust implementation. The open-source positioning creates community trust and contribution velocity (2,155 commits, weekly releases). Aiki should counter by emphasizing its differentiated value (autonomous review intelligence) rather than competing on openness directly. The threat is mid because OSS creates switching-cost moats through community adoption. |

## Summary: Overall Competitive Positioning

**UseGitAI and Aiki are largely complementary, not directly competitive.** Git AI's core is *attribution and traceability* — tracking which agent wrote which line. Aiki's wedge is *autonomous review and quality assurance* — ensuring AI-generated code meets quality standards before merge.

**Key overlaps exist in two areas:**
1. **Contextual intelligence** — Git AI's `/ask` and Aiki's architecture caching (Phase 10) both solve the "agent re-discovery" problem, but from different angles (transcript retrieval vs. structural caching).
2. **Agent integration surface** — Both use hooks to integrate with AI coding agents. Git AI's broader agent support (12+ vs. Aiki's Claude Code focus) is a distribution advantage.

**Threat assessment: Low-Medium overall.**
- Git AI does not compete on review quality or autonomous code review.
- The `/ask` feature is the closest competitive overlap, but it solves a different sub-problem (authorship context vs. architectural context).
- Git AI's open-source momentum and agent breadth could create a de facto standard for AI code metadata, which Aiki should interoperate with rather than fight.

**Strategic posture:**
- **Ignore** attribution/blame/dashboards — different problem space.
- **Counter** `/ask` with richer PrePrompt context injection and architecture caching.
- **Copy** the multi-agent integration breadth and frictionless hook patterns.
- **Potential integration**: Git AI's attribution metadata could *feed into* Aiki's autonomous reviews (e.g., review AI-authored code more carefully), making them natural complements rather than competitors.
