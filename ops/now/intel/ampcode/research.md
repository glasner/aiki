# Ampcode Deep Research

**Date:** 2026-03-05
**Subject:** Amp (formerly Ampcode) — AI coding agent by Sourcegraph (now spinning out as Amp, Inc.)

---

## 1. Product / News Pages

### Core Product
Amp is a frontier agentic coding tool built by Sourcegraph, engineered for autonomous reasoning, code editing, and complex task execution. It runs primarily as a CLI tool (`npm install -g @sourcegraph/amp`), with a JetBrains integration. VS Code/Cursor extensions were sunset in Feb 2026.

**Source:** [https://ampcode.com](https://ampcode.com)

### Key Claims
- "Always uses the best models" — multi-model architecture, no arbitrary token limits
- Pay-as-you-go with no markup on model costs for individual users
- 200K token context window (vs Cursor's 128K)
- Thread-based collaboration with sharing across teams

**Source:** [https://ampcode.com/manual](https://ampcode.com/manual)

### "Towards a New CLI" (Jul 2025)
Launch announcement positioning terminal agents as the future. Brief philosophical post, minimal technical detail. Key quote: "Running agents in your terminal was considered a curiosity [months ago]."

**Source:** [https://ampcode.com/news/towards-a-new-cli](https://ampcode.com/news/towards-a-new-cli)

---

## 2. Architecture & Technical Capabilities

### Multi-Model System
Amp uses different models for different roles rather than a single model for everything:

| Role | Model | Purpose |
|------|-------|---------|
| Smart (primary agent) | Claude Opus 4.6 | Unconstrained state-of-the-art coding |
| Rush (fast mode) | Claude Haiku 4.5 | Quick, well-defined tasks |
| Deep (reasoning) | GPT-5.3 Codex | Extended thinking for complex problems |
| Oracle (advisor) | GPT-5.4 | Complex reasoning & planning, second opinion |
| Review | Gemini 3 Pro | Bug identification & code review |
| Search (subagent) | Gemini 3 Flash | Fast codebase retrieval |
| Librarian (subagent) | Claude Sonnet 4.6 | Cross-repo research |
| Look At | Gemini 3 Flash | Image/PDF analysis |
| Painter | Gemini 3 Pro Image | Image generation/editing |
| Handoff | Gemini 3 Flash | Context analysis for thread transitions |
| Titling | Claude Haiku 4.5 | Thread title generation |

**Source:** [https://ampcode.com/models](https://ampcode.com/models)

### Agent Capabilities
- File reading, editing, creation with undo support
- Bash command execution with permission-based controls
- Git operations and blame tracking
- Web URL inspection via screenshots
- Cross-repository code search (Librarian subagent)
- Subagent spawning for parallel work (isolated execution contexts)
- MCP (Model Context Protocol) server support — local and remote
- Skills system (markdown-based packages with YAML frontmatter)
- Checks system (user-defined review criteria in `.agents/checks/`)
- Toolbox system (custom executable tools)

**Source:** [https://ampcode.com/manual](https://ampcode.com/manual)

### Configuration
- AGENTS.md files for codebase-level agent guidance (analogous to CLAUDE.md)
- Permission rules system (allow/reject/ask/delegate) for tool invocation
- Command allowlisting for security
- Global config at `~/.config/amp/settings.json`

**Source:** [https://ampcode.com/manual](https://ampcode.com/manual)

### Threading Model
- Public, unlisted, workspace-shared, group-shared (enterprise), private threads
- `handoff` command transfers context to new focused threads
- Thread search by keyword, file path, author, date
- All threads stored server-side on Sourcegraph servers (privacy concern)

**Source:** [https://ampcode.com/manual](https://ampcode.com/manual)

---

## 3. Product Evolution (Chronicle)

### Key Milestones

**Mar 2026:**
- GPT-5.4 becomes oracle; deep mode gains oracle access

**Feb 2026:**
- GPT-5.3-Codex powers deep mode
- Editor extensions sunset — CLI becomes primary interface
- Amp Free closes new signups
- Code review becomes composable with checks system

**Jan 2026:**
- Deep mode launches (extended thinking)
- Amp Tab discontinued ("post-agentic era")
- Custom commands deprecated → skills system
- Agents Panel for multi-thread management
- Daily free credits ($10/day) with ad support
- MCP tools load lazily via skills
- Painter tool (image gen/edit)
- TODO feature removed ("agents self-manage better")
- Fork command removed → handoff replaces it

**Dec 2025:**
- Agentic review agent with specialized toolset
- Agent skills framework launches
- Python SDK released
- Mermaid diagrams link to source code
- Look_at tool for PDFs/images
- Thread labels and thread map visualization

**Source:** [https://ampcode.com/chronicle](https://ampcode.com/chronicle)

---

## 4. Public Repositories

### GitHub Presence
- **Organization:** [github.com/ampcode-com](https://github.com/ampcode-com) — minimal; only a `.github` profile repo (9 stars)
- **Sourcegraph repos:**
  - [sourcegraph/amp-examples-and-guides](https://github.com/sourcegraph/amp-examples-and-guides) — supplemental usage guides
  - [sourcegraph/cra-github](https://github.com/sourcegraph/cra-github) — Code Review GitHub App
- **Community:**
  - [jdorfman/awesome-amp-code](https://github.com/jdorfman/awesome-amp-code) — curated resource list
  - [ben-vargas/ai-amp-cli](https://github.com/ben-vargas/ai-amp-cli) — CLI internals documentation (prompts, tools, endpoints)
- **npm package:** `@sourcegraph/amp`

**Source:** [https://github.com/ampcode-com](https://github.com/ampcode-com), GitHub search

### Notable: Closed Source
Amp's core agent is not open source. The product is a proprietary SaaS wrapper around multiple LLM APIs. Competitive moat described by HN commenters as "context assembly and prompting" rather than proprietary models.

**Source:** [https://news.ycombinator.com/item?id=46124649](https://news.ycombinator.com/item?id=46124649)

---

## 5. Pricing & Business Model

### Tiers

| Tier | Price | Details |
|------|-------|---------|
| Amp Free | $0 (ad-supported) | $10/day credit grant, training mode required, currently closed to new signups |
| Paid | Usage-based | Direct pass-through LLM costs via Stripe, no markup claimed |
| Enterprise | $59/user/month | SSO, zero data retention, audit controls, IP allowlisting |

### Ad-Supported Model (Amp Free)
- Toggle to "Free" mode via `/mode free`
- Ads from companies like Axiom, Chainguard, Vanta, WorkOS at bottom of editor/CLI
- Requires opt-in to training mode (data shared with Sourcegraph + model providers)
- Uses open-source and pre-release models with narrower context windows
- Rate-limited; no automated/API usage allowed
- "Kind of fascinating" — some see it as democratizing access; others see data-sharing as a dealbreaker

**Source:** [https://tessl.io/blog/amp-s-new-business-model-ad-supported-ai-coding/](https://tessl.io/blog/amp-s-new-business-model-ad-supported-ai-coding/), [https://ampcode.com/news/amp-free](https://ampcode.com/news/amp-free)

### Spin-Out
Amp is spinning out of Sourcegraph as Amp, Inc. — independent company with separate fundraising.

**Source:** [https://news.ycombinator.com/item?id=46124649](https://news.ycombinator.com/item?id=46124649)

---

## 6. Community Reception & Sentiment

### Hacker News

**"I don't know why amp isn't talked about more. It's better than Claude code"** (Aug 2025)
- OP claims lower Amp bills than Claude Code with superior output
- Counter: users report $5-20/day token spend; prefer fixed-cost alternatives
- Key tension: performance vs. cost predictability

**Source:** [https://news.ycombinator.com/item?id=44773896](https://news.ycombinator.com/item?id=44773896)

**"Sourcegraph Amp is now free"** (Oct 2025)
- Discussion around data training requirements for free tier
- Privacy-conscious users uncomfortable with data sharing

**Source:** [https://news.ycombinator.com/item?id=45616908](https://news.ycombinator.com/item?id=45616908)

**"Amp, Inc. – Amp is spinning out of Sourcegraph"** (Dec 2025)
- Users praise quality: "I love AMP, it delivers great results"
- Independent benchmarks show "highest success rate for small and large tasks"
- Concern that moat is thin — just "context assembly and prompting"
- Some devs find simpler tools (raw Claude CLI) "good enough and significantly cheaper/faster"
- Steve Yegge departure noted

**Source:** [https://news.ycombinator.com/item?id=46124649](https://news.ycombinator.com/item?id=46124649)

### Blog Reviews

**"Sourcegraph Amp in 5 minutes — Good, Bad, Ugly"** (Substack)
- Good: seamless integration, 200K context, intelligent retrieval, MCP support, command allowlisting
- Bad: model lock-in (at time of review), server-side thread storage, no project-level MCP config
- Ugly: leaderboards/vanity metrics misaligned with professional engineering; initial data training concerns

**Source:** [https://zoltanbourne.substack.com/p/early-preview-of-amp-the-new-ai-coding](https://zoltanbourne.substack.com/p/early-preview-of-amp-the-new-ai-coding)

**"1 Month with Amp vs 1 Year with Cursor"** (Medium)
- Author finds Amp delivers consistently better results than Cursor
- Praises agentic reliability and "finishing the job"

**Source:** [https://medium.com/@jonathanaraney/1-month-with-amp-vs-1-year-with-cursor-15572fca36ee](https://medium.com/@jonathanaraney/1-month-with-amp-vs-1-year-with-cursor-15572fca36ee)

**"7 Truths About Amp as an Enterprise AI Coding Agent"** (Medium)
- Positive on enterprise capabilities but notes data privacy concerns

**Source:** [https://medium.com/@jonathanaraney/7-truths-about-amp-as-an-enterprise-ai-coding-agent-aa8ecbf61087](https://medium.com/@jonathanaraney/7-truths-about-amp-as-an-enterprise-ai-coding-agent-aa8ecbf61087)

### Security Incident
- Prompt injection vulnerability discovered and fixed by Sourcegraph
- Invisible instructions could hijack the agent

**Source:** [https://embracethered.com/blog/posts/2025/amp-code-fixed-invisible-prompt-injection/](https://embracethered.com/blog/posts/2025/amp-code-fixed-invisible-prompt-injection/)

---

## 7. Competitive Positioning

### vs Claude Code
- Both use Claude models; Amp adds multi-model orchestration (Oracle, search subagents)
- Amp has thread sharing/collaboration; Claude Code is single-user focused
- Amp usage-based pricing can be higher; Claude Code offers fixed monthly plans
- Server-side thread storage vs local-first

### vs Cursor
- Amp offers 200K context (vs 128K)
- Amp sunset editor extensions; Cursor is editor-native
- Amp multi-model by design; Cursor allows model switching via OpenRouter
- Amp usage-based; Cursor subscription-based

### vs GitHub Copilot
- Amp positioned as "agentic" (autonomous multi-step); Copilot historically inline suggestions
- Amp targets power users; Copilot targets broad developer market

---

## 8. Summary of Key Technical Capabilities

1. **Multi-model orchestration** — different best-in-class models for different roles
2. **Oracle system** — second-opinion model for architectural guidance within conversation
3. **Skills framework** — extensible agent capabilities via markdown packages
4. **Checks system** — composable, user-defined code review criteria
5. **MCP support** — local and remote Model Context Protocol servers
6. **Subagent spawning** — parallel isolated execution contexts
7. **Thread collaboration** — shareable, searchable conversation threads
8. **Cross-repo search** — Librarian subagent for external code research
9. **Handoff system** — context transfer between focused threads
10. **Custom toolboxes** — user-defined executable tools
11. **AGENTS.md** — codebase-level agent configuration files
12. **Ad-supported free tier** — novel business model for AI coding tools
