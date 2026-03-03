# Aiki — Investor Pitch Deck (Pre-Seed)

> Structure follows the [Unusual Ventures Field Guide](https://www.unusual.vc/field-guide/building-the-pitch-deck/) outline.

---

## 1. Opening Gambit

*[Hook story — meant to be told, not read off a slide]*

Last year I used Claude Code to rewrite 300,000 lines of code. Not a weekend project — months of daily, production software development with an AI agent as my primary collaborator.

Within the first week, I hit the wall that every AI-assisted developer hits:

- The agent wrote a bug in file A, but I didn't find it until file M. No way to trace it back. No blame. No provenance.
- I needed two agents working simultaneously — one writing code, one reviewing it. They stomped on each other's files. No isolation. No coordination.
- The agent's context window filled up and it forgot what it was building. The architecture it chose yesterday? Gone. No memory. No continuity.
- At the end of every session, all the task context vanished. I was re-explaining the project from scratch. Every. Single. Time.

I didn't stop coding. I started building the infrastructure layer that should have existed. That became Aiki.

The punchline: I'd spent 6 years at HashiCorp watching Terraform go from "a CLI that a few DevOps engineers love" to a $6.4B platform. I recognized the pattern immediately — an open-source CLI becomes the standard interface for a new infrastructure category, a plugin ecosystem creates network effects, and enterprise governance becomes the business model. The same thing is about to happen for AI agents. And the orchestration layer doesn't exist yet.

---

## 2. Team

**Jordan Glasner** — Solo founder, technical

**The short version:** Head of Product at Tailscale. 6 years at HashiCorp building the Terraform ecosystem. Now running the same playbook for AI agents.

**Career arc:**

- **Tailscale (2024–present)** — Director of Product at the zero-config VPN company. Leading product strategy for infrastructure networking — another developer-first, bottom-up adoption tool. Deepened expertise in the exact go-to-market motion Aiki is running: land with individual developers, expand into teams, sell to enterprise.

- **HashiCorp (2018–2024)** — Rose from Sr. Product Manager to Principal PM. Owned Terraform Enterprise, launched HCP Packer, managed the PM team. Spent 6 years inside the company that wrote the playbook for "open-source CLI → provider registry → enterprise platform." Saw firsthand how a plugin ecosystem (Terraform providers) creates compounding network effects and an enterprise moat.

- **Groove (2013–2018)** — CTO & Head of Product at a seed-stage SaaS startup. Full-stack engineer → CTO trajectory. Built the product, led the engineering team, pushed the company toward API-first architecture and React.

- **Self-taught builder (2000–2013)** — Started as an eCommerce developer at 18, teaching himself Yahoo's proprietary RTML by reading source code because the docs didn't exist. Freelance full-stack developer, founded MonkeyWords, built and sold online businesses. The original tinkerer.

**Why this founder for this company:**

- **Lived the registry playbook** — Terraform's provider registry is the direct blueprint for Aiki's plugin registry. Same pattern: OSS CLI lands on dev machines → community builds plugins → registry creates discovery and network effects → enterprise pays for governance. Jordan built inside that machine for 6 years.

- **Built aiki end-to-end** — ~20k+ lines of production Rust, built using Claude Code as primary pair-programming partner. Solo-rewrote 300k+ LOC using AI agents, living the orchestration problem daily.

- **Deep technical range** — Version control internals (Jujutsu/jj-lib), Rust systems programming, infrastructure automation, developer tooling, full-stack web development. Self-taught across every layer of the stack.

*Key narrative: The person who helped build Terraform's ecosystem — one of the most successful open-source-to-enterprise conversions in developer tools — saw the same pattern emerging for AI agents, and started building the infrastructure layer before anyone else.*

---

## 3. Problem — The SDLC Is Being Reinvented (and the Infrastructure Hasn't Caught Up)

**The software development lifecycle is being rewritten around AI agents.**

AI coding agents (Claude Code, Cursor, Codex, Gemini CLI) are becoming the primary authors of code. Every stage — writing, reviewing, testing, deploying — is being touched by AI. But the infrastructure layer hasn't caught up. We're in 2008 for AI development: pre-GitHub, pre-CI/CD, pre-DevOps.

**Concrete pain points** (from daily production use):

| Pain | What happens | Why it matters |
|---|---|---|
| **No provenance** | Can't tell which agent wrote which line of code | Bugs are untraceable. Compliance teams can't audit. |
| **No orchestration** | Multiple agents (reviewer, coder, tester) require manual coordination | Context lost between sessions. Agents don't talk to each other. |
| **No quality control** | Agents make changes and move on. Errors compound. | One TypeScript error becomes ten. Nobody runs the build until it's too late. |
| **No memory** | Context compaction means agents forget what they were doing | Architecture is re-discovered every session. Multi-day features lose the plot. |
| **No task tracking** | Built-in todo tools don't persist across sessions | Work context vanishes when agent sessions end. |
| **Agent lock-in** | Each agent has its own hook format, conventions, silos | Teams using multiple agents have no unified view. |

*These are not hypothetical problems — they are daily realities for anyone shipping with AI agents today.*

**The gap:** Agents write code. Nothing manages the agents.

---

## 4. Market Opportunity

**AI-assisted development is at an inflection point.**

- GitHub Copilot: 1.8M+ paid subscribers (Feb 2025)
- Claude Code, Cursor, Windsurf, Codex — proliferating fast
- Enterprise AI code adoption growing 150%+ YoY
- Regulatory pressure building (EU AI Act, SOX, PCI-DSS require knowing who/what wrote code)

### TAM — Bottoms-up sizing

```
Layer 1: AI Agent Governance & Orchestration (Direct TAM)
──────────────────────────────────────────────────────────
~30M software developers worldwide (GitHub estimate, 2025)
× ~40% using AI coding tools by 2027 (Gartner projection) = 12M developers
× ~10% on teams needing orchestration/governance        = 1.2M seats
× $200/seat/year (enterprise SaaS)                      = $240M ARR

Layer 2: Plugin Registry & Marketplace (Platform TAM)
─────────────────────────────────────────────────────
Registry monetization (premium plugins, verified publishers)
Enterprise private registries ($5k–$50k/org/year)
Comparable: npm was acquired for $1.3B by GitHub (2020)

Layer 3: AI Development Platform (Long-term TAM)
────────────────────────────────────────────────
Full AI SDLC platform (provenance, compliance, fleet management)
$2B+ market by 2030 (intersection of DevOps tooling + AI governance)
Comparable: HashiCorp reached $583M ARR before $6.4B acquisition
```

### The disruption: a new infrastructure category is forming

Every major shift in how software is built has created a new infrastructure category worth billions:

| Shift | Infrastructure category created | Outcome |
|---|---|---|
| Open source (2000s) | Code hosting & collaboration | GitHub → $7.5B acquisition |
| Cloud (2010s) | Infrastructure as Code | HashiCorp → $6.4B acquisition |
| DevOps (2010s) | CI/CD pipelines | CircleCI, GitHub Actions, etc. |
| Containers (2015s) | Container orchestration | Docker, Kubernetes ecosystem |
| **AI agents (2024+)** | **Agent orchestration & governance** | **??? ← We're here** |

---

## 5. Product — Aiki: The AI Agent Orchestration Layer

**Aiki is the orchestration layer for AI coding agents.**

*In 25 words:* A Rust CLI that sits between your codebase and your AI agents — tracking provenance, orchestrating workflows, enforcing quality, and enabling collaboration.

### What's built today (working product):

| Capability | What it does | Why it's hard to build |
|---|---|---|
| **Provenance tracking** | Records which AI agent wrote every line of code. `aiki blame`. | Requires deep VCS integration — built on Jujutsu internals, not Git wrappers |
| **Multi-agent support** | Works with Claude Code, Cursor, Codex, Zed, Neovim. Single interface. | Each agent has different protocols (ACP, hooks, LSP). Unified abstraction is non-trivial. |
| **Cryptographic signing** | GPG/SSH signatures on AI-attributed changes. Tamper-proof audit trail. | Signing must happen at the VCS layer, not application layer. Requires commit-level control. |
| **Flow engine** | 17 unified event types. Declarative YAML workflows reacting to agent actions. | Event-driven architecture across multiple concurrent agent sessions. Race conditions everywhere. |
| **Task system** | Event-sourced, persists across sessions. Hierarchical tasks with priorities. | Survives context compaction — uses JJ as source of truth, not agent memory. |
| **Review pipeline** | `aiki review \| aiki fix` — agents review each other's work autonomously. | Requires task decomposition, multi-agent coordination, and conflict resolution. |
| **Session isolation** | Concurrent agents get isolated JJ workspaces. No conflicts. Auto-merge. | Jujutsu's mutable change model makes this possible. Git can't do this. |
| **ACP proxy** | Bidirectional proxy for IDE-agent communication. | Intercepts and enhances messages without breaking the agent protocol. |

### Technical proof points:

- ~20k+ lines of Rust
- Hook handler completes in <8ms (agents don't notice)
- 10-agent concurrent stress test passing
- Event-sourced storage (JJ is the source of truth — no database)
- Supports Agent Skills specification (Anthropic's open standard)

### Architecture advantage: Jujutsu

Built on **Jujutsu (jj)** — next-generation version control by Google. Key advantages over Git:
- Mutable changes with stable IDs (perfect for AI workflows)
- First-class concurrent workspaces (agent isolation)
- Native commit signing
- Revset query language for provenance queries

This gives Aiki a 2-3 year technical moat. Git-based tools fundamentally can't replicate session isolation or mutable provenance tracking.

---

## 6. Vision & Competitive Differentiation

### The big idea

**Aiki becomes the platform where AI-augmented software is built, reviewed, and trusted.** Just as GitHub became the collaboration layer for human developers, Aiki becomes the collaboration layer for human + AI development teams.

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

### We've seen this movie before — we were in the room

The closest analogy isn't npm or Docker Hub. **It's Terraform.**

| | Terraform | Aiki |
|---|---|---|
| **Core** | Open-source CLI managing infrastructure | Open-source CLI managing AI agents |
| **Tinkerers** | DevOps engineers writing custom providers | Developers writing review rules, workflows, skills |
| **Sharing problem** | Provider code trapped in internal repos | Workflow knowledge trapped in local configs |
| **Registry** | Terraform Registry: 4,000+ providers, 14,000+ modules | Aiki plugin registry |
| **Network effects** | More providers → more infra coverage → more adoption | More plugins → more workflow coverage → more adoption |
| **Enterprise** | Terraform Cloud/Enterprise: private registries, Sentinel, governance | Aiki Cloud: private registries, compliance, agent governance |
| **Outcome** | IPO'd $5.2B. Acquired $6.4B. | *Building.* |

### Competitive landscape

| | Aiki | GitHub Copilot | Cursor | Raw Claude Code |
|---|---|---|---|---|
| Multi-agent orchestration | **Yes** | No | No | No |
| Line-level AI blame | **Yes** | No | No | No |
| Cryptographic provenance | **Yes** | No | No | No |
| Cross-agent task system | **Yes** | No | No | No |
| Autonomous review pipeline | **Yes** | No | No | No |
| Plugin/flow ecosystem | **Yes** (building) | No | No | No |
| Open source | **Yes** | No | No | No |

**The key insight:** Copilot, Cursor, and Claude Code are agents. Aiki is the infrastructure layer that manages agents. We are not competing with them — we make them better. Every new agent that launches increases Aiki's value proposition.

### Comparable companies:

| Company | What they did | Aiki parallel |
|---|---|---|
| **Terraform / HashiCorp** | OSS CLI → provider registry → enterprise platform. IPO'd $5.2B, acquired $6.4B. | OSS CLI → plugin registry → enterprise platform. Same playbook, new category. *Our founder built inside this for 6 years.* |
| **GitHub** (2008) | Collaboration layer for human developers | Collaboration layer for human + AI development |
| **CircleCI / GitHub Actions** | Automated the build/test/deploy pipeline | Automates the AI agent pipeline (write → review → fix → deploy) |
| **Datadog** | Observability for infrastructure | Observability for AI agent behavior in codebases |
| **Snyk** | Security scanning for code | Provenance and compliance scanning for AI-generated code |

---

## 7. Go-to-Market — The Beachhead

*"In Solution, you told them you are going to take Paris. Now tell them where your Normandy is."*

### Beachhead: Individual developers using AI agents for real projects

Not enterprise. Not teams. **Individual developers who are already deep in AI-assisted coding and hitting the orchestration wall.** These are the tinkerers.

**Why this beachhead:**

- They experience the pain daily (no provenance, no orchestration, no memory)
- They adopt tools bottom-up (CLI install, no procurement process)
- They build plugins that extend the ecosystem (network effects engine)
- They bring Aiki into their teams (land-and-expand)

### The golden era of tinkerers

AI agents didn't just make developers faster — they created a new class of builder. People who could never write production software are now shipping: designers building tools, PMs prototyping, researchers automating workflows.

Every platform shift creates a tinkerer explosion — and every tinkerer explosion needs a registry:

| Era | Platform shift | Tinkerer explosion | What captured them |
|---|---|---|---|
| 2005–2010 | Web 2.0 (Rails, Django, PHP) | Millions building web apps | **WordPress plugins, jQuery plugins** |
| 2010–2015 | Mobile (iOS, Android) | Indie developers, solo apps | **App Store, Google Play** |
| 2012–2018 | Node.js / npm | Full-stack hobbyists | **npm registry (2M+ packages)** |
| 2015–2020 | No-code (Zapier, Notion) | Business users automating | **Template marketplaces** |
| **2024–now** | **AI coding agents** | **Everyone who can describe what they want** | **??? ← Aiki's plugin registry** |

### What tinkerers are building right now (and can't share)

- **Custom review rules** — "Always check for SQL injection in any file touching the database." No way to share it.
- **Agent orchestration recipes** — "Run the security reviewer after every commit, but only on files in `/api`." Locked in a local config.
- **Domain-specific templates** — "When reviewing React components, check for accessibility violations." Useful for thousands, available to zero.
- **CI/CD agent flows** — "Run the linter, fix issues automatically, only push if tests pass." Everyone reinvents from scratch.

### The plugin registry is the capture mechanism

```
What the tinkerer builds:          How they share it:             What the community gets:
─────────────────────────          ──────────────────             ──────────────────────

Custom review criteria       →     Push to GitHub            →    aiki plugin search "security"
Agent workflow recipes       →     Add plugin.yaml           →    aiki plugin install acme/security
Domain-specific skills       →     Auto-indexed by registry  →    One command. Done.
```

**This is npm's playbook:** make publishing trivially easy, make discovery instant, let the ecosystem compound.

### GTM motion: open-source → community → enterprise

```
Phase 1 (Now)              Phase 2 (Next)                  Phase 3 (Revenue)
─────────────              ──────────────                  ─────────────────
Open-source CLI        →   Plugin registry             →   Aiki Cloud
Free, 30-sec install       Community plugins, skills       Team dashboards
Land on dev machines       Network effects compound        Enterprise compliance
                                                           Private registries
                                                           Agent governance

Individual devs            Individual devs + teams         Teams + enterprise
Free                       Free                            Paid ($200/seat/yr)
```

### The conversion funnel

```
Individual tinkerer                    Team lead                         Enterprise buyer
─────────────────                    ─────────                         ────────────────
Discovers Aiki for              →    Brings plugins into           →    Needs private registry,
personal project                     team workflows                     compliance dashboards,
                                                                        agent governance

Free                                 Free                               Paid
```

---

## 8. Traction

**Pre-seed stage: building in public, validating with real usage.**

### What we have today

- **Working product** — Full CLI with 10+ major features shipped across 19 development phases
- **Daily production use** — Built by using Aiki to build Aiki. 6+ months of dogfooding with real multi-agent workflows.
- **Code maturity** — ~20k+ lines of production Rust. 90+ tests. Zero compiler warnings. <8ms hook latency.
- **Standards alignment** — Implements Agent Skills specification (Anthropic) and Agent Client Protocol (ACP). Positioned as the infrastructure layer for emerging standards.
- **Architecture advantage** — Built on Jujutsu (by Google). No other tool in this space has a VCS-native foundation.

### Early signals

<!-- TODO: Fill in with actual data as available -->
- GitHub repo activity (stars, forks, issues)
- Community conversations / inbound interest
- Design partner conversations
- Conference/meetup presentations
- Blog posts / content traction

### What we're hearing

<!-- TODO: Fill in with quotes from developer conversations -->
*"[Quote from developer about the pain point]"*
*"[Quote from team lead about compliance needs]"*
*"[Quote from early user about the product]"*

### Validation approach (next 90 days)

1. **Public launch** — Open-source the CLI, measure adoption
2. **Developer community** — Target AI-assisted development communities (Claude Code users, Cursor power users)
3. **Design partners** — 3-5 teams using AI agents in production for feedback on team/enterprise features
4. **Content marketing** — Blog posts on AI provenance, agent orchestration, the tinkerer ecosystem

---

## 9. Operating Plan & Financials

### 24-month execution plan

```
                    Months 1–6              Months 7–12             Months 13–18            Months 19–24
                    ──────────              ───────────             ────────────            ────────────

Product             Open-source launch      Plugin registry v1      Aiki Cloud beta         Aiki Cloud GA
                    CLI polish & docs       Skills marketplace      Team dashboards          Enterprise features
                    Community plugins       Metrics & analytics     Private registries       Compliance suite

Team                Founder + 1 eng         +1 eng, +1 DevRel      +1 eng, +1 sales        6-8 people total

Community           Launch, seed users      100+ plugins            500+ plugins             1000+ plugins
                    First design partners   Active contributors     Growing ecosystem        Standard tool in
                                                                                             AI-assisted dev

Revenue             $0                      $0                      Early enterprise         $XXk MRR
                                                                    design partners          first paying teams

Cumulative burn     $XXXk                   $XXXk                   $XXXk                   $XXXk
```

<!-- TODO: Fill in actual financial projections -->

### Key milestones for next fundraise (Series Seed)

1. **Product-market fit signal** — Measurable organic adoption of the CLI
2. **Ecosystem traction** — Meaningful plugin count, active contributors
3. **Enterprise validation** — 3-5 design partners using team features, at least 1 paying
4. **Team** — 3-4 people, shipping consistently

### Use of funds

| Category | Allocation | Purpose |
|---|---|---|
| **Engineering** | ~60% | Hire 1-2 engineers (Rust, developer tools). Ship registry + cloud. |
| **Community & DevRel** | ~20% | Documentation, tutorials, conference talks, OSS community building. |
| **Infrastructure** | ~10% | Hosting, CI/CD, registry infrastructure. |
| **Operations** | ~10% | Legal, accounting, misc. |

---

## 10. The Ask

**Raising a pre-seed round to:**

1. **Ship the plugin ecosystem** — Registry, marketplace, community flows. Create network effects.
2. **Build the team** — Hire 1-2 engineers (Rust, developer tools background).
3. **Launch Aiki Cloud** — Team dashboards, enterprise compliance features, hosted analytics.
4. **Community growth** — Documentation, tutorials, conference talks, OSS community building.

<!-- TODO: Fill in raise amount, target valuation, timeline -->

---

## Appendix A: Technical Architecture Deep Dive

*Available on request — covers Jujutsu integration, event-sourced storage model, ACP proxy architecture, and session isolation mechanics.*

## Appendix B: The Terraform Playbook in Detail

The pattern is identical at every stage:

**Stage 1 — CLI lands on dev machines**
- Terraform: `terraform init/plan/apply` becomes muscle memory for DevOps
- Aiki: `aiki task/review/fix` becomes muscle memory for AI-assisted devs

**Stage 2 — Tinkerers build plugins**
- Terraform: Community writes providers for every cloud, SaaS, and internal API
- Aiki: Community writes review rules, workflow recipes, domain skills

**Stage 3 — Registry creates network effects**
- Terraform: Terraform Registry (4,000+ providers, 14,000+ modules). `terraform init` auto-downloads.
- Aiki: Plugin registry. `aiki plugin install` auto-configures.

**Stage 4 — Enterprise pays for governance**
- Terraform: Terraform Cloud/Enterprise — private registries, Sentinel policy-as-code, run history, team management
- Aiki: Aiki Cloud — private registries, compliance dashboards, agent governance, provenance audit trails

**Stage 5 — Platform becomes the standard**
- Terraform: Every Fortune 500 runs Terraform. HashiCorp IPO'd at $5.2B. IBM acquired for $6.4B.
- Aiki: *Building.*

**Our founder spent 6 years inside that machine at HashiCorp.** He knows exactly how the flywheel works — and exactly when to build the registry, when to add governance, and when to charge for it.
