# Aiki — Investor Pitch Deck (Pre-Seed)

> Draft outline for discussion. Each section maps to 1-2 slides.

---

## 1. Team

**Jordan Glasner** — Solo founder, technical

- Built aiki's entire codebase (~20k+ lines of Rust) using Claude Code as primary pair-programming partner
- 6+ months of production use pushing AI coding agents to their limits
- Direct experience with the pain: solo-rewrote 300k+ LOC using Claude Code, lived the orchestration problem daily
- Deep domain expertise in version control (built on Jujutsu/jj internals), developer tooling, and AI agent workflows

*Key narrative: Built by someone who has spent more time working alongside AI agents than almost anyone — and discovered the tooling doesn't exist yet.*

---

## 2. Big Problem — The SDLC Is Being Reinvented

**The software development lifecycle is being rewritten around AI agents.**

- AI coding agents (Claude Code, Cursor, Codex, Gemini CLI) are becoming the primary authors of code
- Every stage of the SDLC — writing, reviewing, testing, deploying — is being touched by AI
- But the infrastructure layer hasn't caught up. We're in 2008 for AI development: pre-GitHub, pre-CI/CD, pre-DevOps
- Teams have no visibility into what AI wrote, no way to orchestrate multiple agents, no quality guarantees, and no audit trail

**Market context:**
- GitHub Copilot has 1.8M+ paid subscribers (Feb 2025)
- Claude Code, Cursor, Windsurf, Codex — proliferating fast
- Enterprise AI code adoption growing 150%+ YoY
- Regulatory pressure building (EU AI Act, SOX, PCI-DSS require knowing who/what wrote code)

**The gap:** Agents write code. Nothing manages the agents.

---

## 3. First Problem — Agent Orchestration / Harness

**When you actually use AI agents to build real software, the wheels fall off.**

Concrete pain points (from daily production use):

1. **No provenance** — You can't tell which agent wrote which line of code. When a bug surfaces, you can't trace it. Compliance teams can't audit it.

2. **No orchestration** — Running multiple agents (reviewer, coder, tester) requires manual coordination. Agents don't talk to each other. Context is lost between sessions.

3. **No quality control** — Agents make changes and move on. Errors compound. One TypeScript error becomes ten. Nobody runs the build until it's too late.

4. **No memory** — Context compaction means agents forget what they were doing. Multi-session features lose the plot. Architecture is re-discovered every time.

5. **No task tracking** — Built-in todo tools don't persist. When an agent session ends, the work context vanishes.

6. **Agent lock-in** — Each agent (Claude Code, Cursor, Codex) has its own hook format, its own conventions, its own silo. Teams using multiple agents have no unified view.

*These are not hypothetical problems — they are daily realities for anyone shipping with AI agents today.*

---

## 4. First Solution — Aiki: The Open-Source AI Agent Framework

**Aiki is the orchestration layer for AI coding agents.**

A Rust CLI (single binary, zero dependencies) that sits between your codebase and your AI agents, providing:

### What's built today (working product):

| Capability | What it does |
|---|---|
| **Provenance tracking** | Records which AI agent wrote every line of code. Line-level attribution via `aiki blame`. |
| **Multi-agent support** | Works with Claude Code, Cursor, Codex, and any ACP-compatible agent (Zed, Neovim). Single unified interface. |
| **Cryptographic signing** | GPG/SSH signatures on AI-attributed changes. Tamper-proof audit trail for compliance. |
| **Event-driven flow engine** | 17 unified event types. Declarative YAML workflows that react to agent actions (file changes, shell commands, web fetches, commits). |
| **Task system** | Event-sourced, persists across sessions and context compaction. Hierarchical tasks with priorities, assignments, and source tracking. |
| **Code review pipeline** | `aiki review \| aiki fix` — agents review each other's work, create followup tasks, execute fixes. Fully autonomous. |
| **Session isolation** | Concurrent agent sessions get isolated JJ workspaces. No conflicts. Automatic absorption when done. |
| **ACP proxy** | Bidirectional proxy for IDE-agent communication. Intercepts, observes, and enhances agent messages. |

### Architecture:

Built on **Jujutsu (jj)** — next-generation version control by Google. Key advantages over Git:
- Mutable changes with stable IDs (perfect for AI workflows)
- First-class concurrent workspaces (agent isolation)
- Native commit signing
- Revset query language for provenance queries

### Technical proof points:

- ~20k+ lines of Rust
- Hook handler completes in <8ms (agents don't notice)
- 10-agent concurrent stress test passing
- Event-sourced storage (no database, no SQLite — JJ is the source of truth)
- Supports Agent Skills specification (Anthropic's open standard)

---

## 4.5. The Golden Era of Tinkerers — Community & Registry

**We are living through the greatest expansion of software-building capability in history.**

AI coding agents didn't just make professional developers faster — they created an entirely new class of builder. People who could never write production software before are now shipping real applications: designers building their own tools, PMs prototyping products, researchers automating their workflows, hobbyists turning weekend ideas into working products.

**This is the golden era of tinkerers.** And it rhymes with a pattern investors have seen before.

### The pattern: every platform shift creates a tinkerer explosion

| Era | Platform shift | Tinkerer explosion | What captured them |
|---|---|---|---|
| 2005–2010 | Web 2.0 (Rails, Django, PHP) | Millions of "non-programmers" building web apps | **WordPress plugins, jQuery plugins** |
| 2010–2015 | Mobile (iOS, Android) | Indie developers, solo app makers | **App Store, Google Play** |
| 2012–2018 | Node.js / npm | JavaScript everywhere, full-stack hobbyists | **npm registry (2M+ packages)** |
| 2015–2020 | No-code (Zapier, Notion, Airtable) | Business users building automations | **Template marketplaces, Zapier integrations** |
| **2024–now** | **AI coding agents** | **Everyone who can describe what they want** | **???** |

Every one of these eras produced a registry/marketplace that became the center of gravity for its ecosystem. The current era doesn't have one yet — not for the workflows, automations, review criteria, and agent configurations that power AI-assisted development.

### What tinkerers are building right now (and can't share)

Today's AI-assisted builders are creating valuable workflow knowledge that's trapped on their laptops:

- **Custom review rules** — "Always check for SQL injection in any file touching the database." A solo developer figures this out after getting burned. No way to share it.
- **Agent orchestration recipes** — "Run the security reviewer after every commit, but only on files in `/api`." Hard-won knowledge, locked in a local config file.
- **Domain-specific templates** — "When reviewing React components, check for accessibility violations and missing error boundaries." Useful for thousands of React developers, available to zero.
- **CI/CD agent flows** — "Before pushing, have the agent run the linter, fix issues automatically, and only push if tests pass." Everyone reinvents this from scratch.

This is exactly the state npm was in before the registry: everyone solving the same problems independently, no sharing infrastructure, no discovery, no network effects.

### Aiki's plugin registry is the capture mechanism

```
What the tinkerer builds:          How they share it:             What the community gets:
─────────────────────────          ──────────────────             ──────────────────────

Custom review criteria       →     Push to GitHub            →    aiki plugin search "security"
Agent workflow recipes       →     Add plugin.yaml           →    aiki plugin install acme/security
Domain-specific skills       →     Auto-indexed by registry  →    One command. Done.
```

**The mechanics:**

1. A tinkerer builds a workflow that solves their problem (review criteria, agent flow, domain skill)
2. They push it to a GitHub repo with a `hooks.yaml` or `templates/` directory
3. Aiki's registry scraper discovers it, indexes the metadata
4. Any developer in the world can find it with `aiki plugin search` and install it with one command
5. The plugin runs locally — no cloud dependency, no lock-in, no trust issues

**This is npm's playbook:** make publishing trivially easy, make discovery instant, let the ecosystem compound.

### Why this matters for the business

The plugin registry is the **network effects engine** of the entire strategy:

```
More tinkerers → More plugins → More reasons to use Aiki → More tinkerers
         ↑                                                        │
         └────────────────────────────────────────────────────────┘
```

**Three things happen as the registry grows:**

1. **Switching cost increases** — Your team's review criteria, security policies, and agent workflows live in the Aiki plugin ecosystem. Switching means rebuilding all of it.

2. **Discovery becomes a moat** — When a developer searches "PCI-DSS compliance review" and finds a community-built plugin that works in one command, that's a moment Cursor and Copilot can't replicate. They build features; Aiki enables a community.

3. **Enterprise upsell appears naturally** — Companies don't just want community plugins; they want private registries, approved plugin lists, and org-scoped workflows. This is the same playbook as npm (free public registry → npm Enterprise), Docker Hub (free images → Docker Business), and GitHub (free repos → GitHub Enterprise).

### The tinkerer-to-enterprise pipeline

```
Individual tinkerer                    Team lead                         Enterprise buyer
─────────────────                    ─────────                         ────────────────
Discovers Aiki for              →    Brings plugins into           →    Needs private registry,
personal project                     team workflows                     compliance dashboards,
                                                                        agent governance

Free                                 Free                               Paid
```

**This is the conversion funnel.** The tinkerers are the top of funnel. The registry is what makes the product sticky. The enterprise features are the revenue model.

### Why now: the window is open

- **AI coding tools have no ecosystem layer.** Cursor has settings. Claude Code has CLAUDE.md. Copilot has nothing. No one has a plugin registry, a skills marketplace, or community-shared workflows.
- **Tinkerers are already building.** Developers are writing custom hooks, review templates, and agent flows. They just can't share them.
- **The Agent Skills specification just launched.** Anthropic's open standard for agent capabilities creates a common format. Aiki implements it — plugins written for Aiki work in Claude Code and vice versa. This is the interop layer that makes a registry viable.
- **Registries are winner-take-most.** npm didn't have a serious competitor for a decade. Docker Hub is still dominant. The first registry that captures the AI dev workflow ecosystem will be very hard to displace.

---

## 5. Big Solution — GitHub for AI Development

**Aiki's long-term vision: the platform where AI-augmented software is built, reviewed, and trusted.**

Just as GitHub became the collaboration layer for human developers, Aiki becomes the collaboration layer for human + AI development teams.

### The platform play (roadmap):

```
Today (Built)                Near-term                     Platform
─────────────              ──────────────                ────────────
Provenance tracking    →   Plugin registry            →   Aiki Cloud
Multi-agent support    →   Auto architecture docs     →   Team dashboards
Flow engine            →   Skills marketplace         →   Enterprise compliance
Task system            →   Metrics & analytics        →   Agent performance analytics
Code review pipeline   →   Process management         →   Cross-org agent governance
Session isolation      →   TUI / web interface        →   Hosted agent fleet management
```

### Wedge strategy:

1. **Open-source CLI** (today) — Free, installs in 30 seconds, works with any agent. Land in dev machines.
2. **Plugin ecosystem** (near-term) — Registry for community flows, templates, skills. Network effects.
3. **Aiki Cloud** (future) — Team dashboards, enterprise compliance, agent fleet management. This is where the business model lives.

### Why now:

- AI agent adoption is at an inflection point (Copilot 1.8M+, Claude Code growing fast)
- The orchestration layer doesn't exist yet — first mover advantage
- Enterprise compliance requirements are crystallizing (EU AI Act 2025)
- Agent Skills specification just launched — Aiki implements it, establishing standards alignment
- Building on Jujutsu gives a 2-3 year technical moat (Git-based tools can't do what we do)

### Comparable companies:

| Company | What they did | Aiki parallel |
|---|---|---|
| **GitHub** (2008) | Collaboration layer for human developers | Collaboration layer for human + AI development |
| **CircleCI / GitHub Actions** | Automated the build/test/deploy pipeline | Automates the AI agent pipeline (write → review → fix → deploy) |
| **Datadog** | Observability for infrastructure | Observability for AI agent behavior in codebases |
| **Snyk** | Security scanning for code | Provenance and compliance scanning for AI-generated code |

---

## 6. The Ask

**Raising a pre-seed round to:**

1. **Ship the plugin ecosystem** — Registry, marketplace, community flows. Create network effects.
2. **Build the team** — Hire 1-2 engineers (Rust, developer tools background).
3. **Launch Aiki Cloud** — Team dashboards, enterprise compliance features, hosted analytics.
4. **Community growth** — Documentation, tutorials, conference talks, OSS community building.

---

## Appendix: Key Metrics / Proof Points

- **Product:** Working CLI with 10+ major features shipped across 19 development phases
- **Code:** ~20k+ lines of production Rust
- **Test coverage:** 90+ tests passing, zero compiler warnings
- **Architecture:** 17 unified event types, 5 editor integrations, declarative flow engine
- **Standards:** Implements Agent Skills specification (Anthropic), ACP (Agent Client Protocol)
- **Performance:** <8ms hook latency, 10-agent concurrent isolation passing

---

## Appendix: Competitive Landscape

| | Aiki | GitHub Copilot | Cursor | Raw Claude Code |
|---|---|---|---|---|
| Multi-agent orchestration | Yes | No | No | No |
| Line-level AI blame | Yes | No | No | No |
| Cryptographic provenance | Yes | No | No | No |
| Cross-agent task system | Yes | No | No | No |
| Autonomous review pipeline | Yes | No | No | No |
| Plugin/flow ecosystem | Yes (building) | No | No | No |
| Open source | Yes | No | No | No |
