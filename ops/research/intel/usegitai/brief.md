# Intel Brief — UseGitAI

**Date**: 2026-03-05
**Sources**: [research.md](research.md), [aiki-map.md](aiki-map.md), [opportunities.md](opportunities.md), [followups.md](followups.md)

---

## 1. What This Project Is

UseGitAI (usegitai.com) is an open-source (Apache 2.0, Rust) Git extension that tracks AI-generated code through the entire SDLC. It uses Git Notes to store line-level attributions — which agent, model, and session produced each line — surviving rebase, cherry-pick, and squash operations. It supports 12+ agents (Claude Code, Cursor, Copilot, Codex, Gemini CLI, Amp, and others), integrates via post-commit hooks with zero workflow disruption, and works 100% offline with no login required. The project has 1,200+ GitHub stars, 2,155 commits, weekly releases, and is led by Aidan Cunniffe. Its most notable recent feature is `/ask` (Feb 2026), which retrieves original session transcripts so agents can answer questions about code they wrote "as the original author." Pricing spans a free open-source tier, a Teams tier (AI code durability, ROI tracking), and an Enterprise tier (self-hosted, multi-SCM). ([Homepage](https://usegitai.com/), [GitHub](https://github.com/acunniffe/git-ai), [/ask Blog](https://usegitai.com/blog/ask-the-agent))

---

## 2. What Matters for Aiki

**Overall threat: Low-Medium.** UseGitAI and Aiki are largely complementary — Git AI solves *attribution and traceability* (who wrote what), while Aiki solves *autonomous review and quality assurance* (is it good). They do not directly compete on review intelligence. ([aiki-map.md](aiki-map.md))

**Key competitive implications:**

| Area | Decision | Rationale |
|------|----------|-----------|
| AI Code Attribution & Blame | **Ignore** | Git AI's core — line-level blame, dashboards, stats. Different problem space from autonomous review. No overlap with Aiki's wedge. ([aiki-map.md #1-2](aiki-map.md)) |
| `/ask` Session Context Retrieval | **Counter** | Closest overlap. `/ask` retrieves original transcripts so agents are "noticeably smarter" in plan mode. Aiki should counter with PrePrompt context injection (Phase 8) — richer, automatic, zero-effort — rather than copying transcript retrieval. ([/ask Blog](https://usegitai.com/blog/ask-the-agent), [aiki-map.md #3](aiki-map.md)) |
| Multi-Agent Breadth (12+ agents) | **Copy** | Git AI supports 12+ agents vs. Aiki's Claude Code focus. Breadth is a distribution moat — teams using multiple agents will prefer tools that work across all of them. Weekly additions (Amp CLI added Mar 2026) signal acceleration. ([research.md](research.md), [aiki-map.md #7](aiki-map.md)) |
| Hook-Based Integration Pattern | **Copy** | Git AI validates post-commit hooks as the right integration surface. Aiki should match the "zero workflow disruption" and "100% offline, no login" frictionlessness. ([aiki-map.md #11](aiki-map.md)) |
| Open-Source Community Momentum | **Counter** | 1,200+ stars, Apache 2.0, active community. Counter by emphasizing differentiated value (autonomous review intelligence) rather than competing on openness. Consider interoperating with their metadata rather than fighting their community. ([aiki-map.md #12](aiki-map.md)) |
| Dashboards, Enterprise, IDE Plugins | **Ignore** | Different buyer persona (engineering managers vs. IC devs), different market segment, different stage. Not worth attention. ([aiki-map.md #5-6, #8-9](aiki-map.md)) |
| Incident Linking (prod → AI code) | **Counter** | Interesting signal but Aiki's approach — catching issues *before* production via autonomous review — makes post-hoc incident attribution less necessary. ([aiki-map.md #10](aiki-map.md)) |

**Potential integration path:** Git AI's authorship metadata could feed *into* Aiki reviews — e.g., applying stricter scrutiny to AI-authored code, knowing which agent and model produced it. This makes them natural complements. ([aiki-map.md summary](aiki-map.md), [opportunities.md #3](opportunities.md))

---

## 3. Top 3 Recommendations This Week

### R1. Spike a PrePrompt context injection PoC (counter `/ask`)
**Score: 4.30** — highest-ranked opportunity. ([opportunities.md #1](opportunities.md))

Git AI's `/ask` validates the pain: agents waste tokens re-discovering architecture and produce inconsistent code. Their blog confirms agents become "noticeably smarter" with session context. ([/ask Blog](https://usegitai.com/blog/ask-the-agent))

**This week**: Design the PrePrompt event schema (Phase 8). Prototype injecting repo structure summary + last 3 review comments into the agent's context window before it sees a prompt. Measure token overhead. This counters `/ask` with a richer, automatic alternative — agents don't have to remember to query, context arrives proactively.

### R2. Survey multi-agent demand and build one hook PoC
**Score: 4.10** — second-ranked opportunity. ([opportunities.md #2](opportunities.md))

Git AI's 12-agent breadth is a distribution moat. Every agent integration is an acquisition channel. Aiki's Claude Code exclusivity limits TAM. ([research.md](research.md), [aiki-map.md #7](aiki-map.md))

**This week**: Scan community channels and inbound for mentions of Cursor, Copilot, and Codex. Pick the one with the strongest demand signal. Build a minimal hook PoC that runs Aiki's review pipeline on that agent's output. Goal: prove feasibility, not ship production quality.

### R3. Draft a "Better Together: Aiki + Git AI" integration guide
**Score: 3.65** — but highest GTM leverage of any "this week" action. ([opportunities.md #4](opportunities.md), [followups.md positioning](followups.md))

Positioning as complementary taps into Git AI's 1,200+ star community as an acquisition channel. Teams already using Git AI for attribution want smarter review on top. ([aiki-map.md summary](aiki-map.md))

**This week**: Write a short integration guide showing how Git AI's Git Notes metadata enriches Aiki's review intelligence ("Git AI tells you who wrote it, Aiki tells you if it's good"). Build a minimal Git Notes reader. Target: post in Git AI's Discord ([discord.gg/XJStYvkb5U](https://discord.gg/XJStYvkb5U)) and the existing HN thread ([HN](https://news.ycombinator.com/item?id=45878276)) by end of week.

---

## 4. Risks If We Do Nothing

1. **`/ask` becomes the default context solution.** Git AI's `/ask` is already shipping and their blog is evangelizing it for plan mode. If developers adopt `/ask` as "how agents remember," Aiki's Phase 8 PrePrompt injection launches into a market that already has a "good enough" solution. The counter must ship before `/ask` becomes habitual. ([/ask Blog](https://usegitai.com/blog/ask-the-agent))

2. **Multi-agent lock-out.** Git AI adds agents weekly — Amp CLI was added Mar 3, 2026. ([v1.1.8 release](https://github.com/acunniffe/git-ai/releases)) As teams standardize on Git AI across all their agents, Aiki's Claude-Code-only posture makes it the odd tool out. Teams won't adopt a review tool that only works with one of their five agents. The window to establish multi-agent presence is while Git AI is still building — once they're the integration standard, Aiki must interoperate on their terms.

3. **Complementary positioning window closes.** Git AI is early (1,200 stars, small Discord). Right now, reaching out with a "works with Git AI" message is collegial. If Git AI grows 5-10x and adds its own review features (their Teams tier already has "tips for engineers"), the complementary framing becomes harder to sell. Acting now while the community is small and the founder is accessible (he responds on HN) locks in the partnership narrative. ([HN thread](https://news.ycombinator.com/item?id=45878276), [Pricing](https://usegitai.com/pricing))

4. **Attribution metadata becomes a standard Aiki can't read.** If Git AI's Git Notes format becomes the de facto standard for AI code metadata (plausible given their agent breadth and open-source license), and Aiki can't read it, Aiki reviews lack context that competitors can access. Building the Git Notes reader now (low complexity, [opportunities.md #4](opportunities.md)) is cheap insurance against format lock-in.
