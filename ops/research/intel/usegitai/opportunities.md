# Opportunity Scoring — UseGitAI Intel

**Date**: 2026-03-05
**Source**: [aiki-map.md](aiki-map.md), [research.md](research.md)
**Formula**: `score = 0.35*pain + 0.35*fit + 0.20*gtm + 0.10*(6-complexity)`

## Summary Table

| Rank | Opportunity | Pain | Fit | GTM | Cmplx | Score |
|------|-------------|------|-----|-----|-------|-------|
| 1 | PrePrompt context injection (counter `/ask`) | 5 | 5 | 3 | 4 | **4.30** |
| 2 | Multi-agent integration breadth | 4 | 4 | 5 | 3 | **4.10** |
| 3 | AI-authored code review weighting | 4 | 5 | 3 | 3 | **4.05** |
| 4 | Git AI metadata interop | 3 | 4 | 4 | 2 | **3.65** |
| 5 | Frictionless hook installation | 3 | 3 | 5 | 2 | **3.50** |
| 6 | Offline-first local review | 3 | 3 | 3 | 3 | **3.00** |
| 7 | AI code durability signal in reviews | 2 | 3 | 3 | 3 | **2.65** |

## Detailed Rationale

### 1. PrePrompt Context Injection (Counter `/ask`) — Score: 4.30

`0.35(5) + 0.35(5) + 0.20(3) + 0.10(6-4) = 1.75 + 1.75 + 0.60 + 0.20 = 4.30`

Build Aiki's PrePrompt event (Phase 8) to inject architectural context, prior review feedback, and relevant task history before the agent sees a prompt. This counters Git AI's `/ask` with a richer, automatic alternative.

- **Pain (5)**: Agents constantly re-discover architecture, wasting tokens and producing inconsistent code. Git AI's blog confirms agents are "noticeably smarter" with session context.
- **Fit (5)**: Core to Aiki's Phase 8 roadmap (PrePrompt event). Already planned — this intel validates the direction.
- **GTM (3)**: Hard to demo in a screenshot but very sticky once experienced. Requires hands-on trial.
- **Complexity (4)**: Requires session state tracking, architecture caching, and PrePrompt event infrastructure.

### 2. Multi-Agent Integration Breadth — Score: 4.10

`0.35(4) + 0.35(4) + 0.20(5) + 0.10(6-3) = 1.40 + 1.40 + 1.00 + 0.30 = 4.10`

Extend Aiki beyond Claude Code to support Cursor, Copilot, Codex, Gemini CLI, and other agents that Git AI already supports.

- **Pain (4)**: Teams increasingly use multiple AI coding agents. Single-agent tooling limits adoption.
- **Fit (4)**: Autonomous review should be agent-agnostic. Architecture supports it but work needed.
- **GTM (5)**: Every new agent integration opens a new acquisition channel. Git AI's 12+ agent support is a distribution moat.
- **Complexity (3)**: Each agent has different hook/integration patterns, but the pattern is well-understood.

### 3. AI-Authored Code Review Weighting — Score: 4.05

`0.35(4) + 0.35(5) + 0.20(3) + 0.10(6-3) = 1.40 + 1.75 + 0.60 + 0.30 = 4.05`

When AI authorship metadata is available (from Git AI or similar), apply stricter review scrutiny to AI-generated code — deeper checks for hallucinated APIs, missing error handling, and test coverage gaps.

- **Pain (4)**: AI code passes superficial review but harbors subtle bugs. Teams lack systematic rigor for AI-authored code.
- **Fit (5)**: This is exactly Aiki's autonomous review wedge — differentiated review intelligence.
- **GTM (3)**: Requires educating buyers on why AI code needs different review standards.
- **Complexity (3)**: Needs authorship signal integration, but review logic is Aiki's core competency.

### 4. Git AI Metadata Interop — Score: 3.65

`0.35(3) + 0.35(4) + 0.20(4) + 0.10(6-2) = 1.05 + 1.40 + 0.80 + 0.40 = 3.65`

Read Git AI's Git Notes metadata to enrich Aiki reviews with authorship context — which agent, model, and session produced each code block.

- **Pain (3)**: Reviewers want this context but can function without it.
- **Fit (4)**: Complementary to review workflows. Authorship data makes reviews smarter.
- **GTM (4)**: Positions Aiki as "works with Git AI" rather than competing, tapping into their 1,200+ star community.
- **Complexity (2)**: Git Notes are standard Git — straightforward to read and parse.

### 5. Frictionless Hook Installation — Score: 3.50

`0.35(3) + 0.35(3) + 0.20(5) + 0.10(6-2) = 1.05 + 1.05 + 1.00 + 0.40 = 3.50`

Match Git AI's "zero workflow disruption" installation — one command, auto-configured hooks, works offline, no login required.

- **Pain (3)**: Installation friction exists but motivated users push through.
- **Fit (3)**: Table stakes, not differentiating.
- **GTM (5)**: Frictionless onboarding is the single biggest driver of tool adoption.
- **Complexity (2)**: Mostly packaging, installer, and auto-detection work.

### 6. Offline-First Local Review — Score: 3.00

`0.35(3) + 0.35(3) + 0.20(3) + 0.10(6-3) = 1.05 + 1.05 + 0.60 + 0.30 = 3.00`

Ensure Aiki's core review capabilities work fully offline, matching Git AI's "100% offline" positioning.

- **Pain (3)**: Some users care deeply about offline/air-gapped environments; most are connected.
- **Fit (3)**: Good to have but not Aiki's differentiator.
- **GTM (3)**: Resonates with privacy-conscious teams but smaller segment.
- **Complexity (3)**: Depends on how much of Aiki's review logic requires external services.

### 7. AI Code Durability Signal in Reviews — Score: 2.65

`0.35(2) + 0.35(3) + 0.20(3) + 0.10(6-3) = 0.70 + 1.05 + 0.60 + 0.30 = 2.65`

Track how often AI-generated code survives vs. gets rewritten. Surface this as a review signal ("this agent's code has 40% rewrite rate in this repo").

- **Pain (2)**: Nice-to-know but not blocking anyone today.
- **Fit (3)**: Adds review intelligence but is a secondary signal.
- **GTM (3)**: Interesting data story for content marketing.
- **Complexity (3)**: Requires historical tracking and statistical analysis.
