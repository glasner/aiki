# Beads Research — Deep Research Pass

**Date:** 2026-03-05
**Target:** https://github.com/steveyegge/beads
**Author:** Steve Yegge (ex-Amazon, ex-Google, ex-Head of Engineering at Sourcegraph)

---

## 1. GitHub Repository

### Metadata
- **Stars:** 18.1k | **Forks:** 1.1k | **Watchers:** 68
- **License:** MIT
- **Language:** Go (92.9%), Python (4.4%), Shell, JS, PowerShell
- **Latest release:** v0.58.0 (2026-03-03) — 80 total releases, 7,420 commits
- **Topics:** coding, agents, claude-code
- **Source:** https://github.com/steveyegge/beads

### Core Product Claim
Beads is "a distributed, git-backed graph issue tracker for AI agents" — a persistent, structured memory layer for coding agents that replaces unstructured markdown with a dependency-aware task graph.
— Source: [README](https://github.com/steveyegge/beads/blob/main/README.md)

### Architecture
- **Database:** Dolt (version-controlled SQL with cell-level merging, native branching, remote sync). Transitioned from SQLite+JSONL dual storage in v0.50.x series (Feb 2025).
- **Storage:** `.beads/` directory in project root contains the Dolt database.
- **Sync:** Git-native — no central server. Background daemon handles SQLite-JSONL sync (legacy) / Dolt sync.
- **IDs:** Hash-based (format `bd-a1b2`) with progressive length scaling (4-6 chars) — prevents merge conflicts in multi-agent/multi-branch scenarios.
- **Binary:** Reduced from 168MB to 41MB in v0.50.x through driver dependency removal.
— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md), [BetterStack Guide](https://betterstack.com/community/guides/ai/beads-issue-tracker-ai-agents/)

### Key Features
| Feature | Description | Source |
|---------|-------------|--------|
| Dependency graph | 4 relationship types: blocks, parent-child, related, discovered-from | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |
| Hierarchical tasks | Epics → tasks → subtasks (3 nesting levels). Parent gets hash ID, children auto-number (.1, .2) | [README](https://github.com/steveyegge/beads) |
| Memory decay | Semantic summarization of completed tasks to preserve context windows (`bd admin compact`) | [README](https://github.com/steveyegge/beads) |
| Persistent memory | `bd remember`, `bd memories`, `bd recall`, `bd forget` — key/value store auto-injected at prime time | [Releases](https://github.com/steveyegge/beads/releases) |
| Atomic claiming | `--claim` flag prevents race conditions when multiple agents work concurrently | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |
| JSON-first output | `--json` flag on every command for agent consumption | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |
| Ready-work detection | `bd ready` returns unblocked, prioritized tasks — deterministic, offline, ~10ms | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |
| Messaging/threading | Issue type with `--thread` flag, ephemeral lifecycle, delegation patterns | [README](https://github.com/steveyegge/beads) |
| Graph links | `relates_to`, `duplicates`, `supersedes`, `replies_to` | [README](https://github.com/steveyegge/beads) |
| Stealth mode | `bd init --stealth` — operates locally without committing to repo | [README](https://github.com/steveyegge/beads) |
| Role detection | Contributor vs. maintainer roles with isolated planning databases | [README](https://github.com/steveyegge/beads) |
| MCP server | Python-based MCP with aggressive context reduction for AI agents | [Plugin docs](https://github.com/steveyegge/beads/blob/main/docs/PLUGIN.md) |
| SQL queries | Full SQL access against local Dolt database | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |
| Offline-first | All queries run locally, sync via git push/pull | [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md) |

### Installation
- npm: `npm install -g @beads/bd`
- Homebrew: `brew install beads`
- Go: `go install github.com/steveyegge/beads/cmd/bd@latest`
- PyPI: `beads-mcp` (MCP server)
- Platforms: Linux, macOS, Windows, FreeBSD

### Release Velocity (Recent)
| Version | Date | Highlights |
|---------|------|------------|
| v0.58.0 | 2026-03-03 | Per-worktree .beads/redirect, metadata filtering, SSH push/pull fallback. 70+ features, 40+ fixes |
| v0.57.0 | 2026-03-01 | Hook migration, JSONL backup, expanded query DSL |
| v0.56.1 | 2026-02-23 | SQLite ephemeral store removed, Dolt consolidation |
| v0.51.0 | 2026-02-16 | Major Dolt-native cleanup, removed 1,400+ lines legacy code |
| v0.50.x | 2026-02-xx | Dolt as default backend, plugin framework (GitLab, Linear adapters) |

### Known Limitations (from FAQ & community)
- Alpha software — no 1.0 release yet, API may change
- Requires explicit prompting; agents don't proactively use it
- Session cleanup ("landing the plane") needs reminding
- Context rot still occurs in long sessions
- Best suited for near-term work, not distant backlogs
- Cross-project references not supported (isolated databases)
- No built-in migration tools from Jira/GitHub
— Sources: [FAQ](https://github.com/steveyegge/beads/blob/main/docs/FAQ.md), [Ian Bull blog](https://ianbull.com/posts/beads/)

---

## 2. Blog & Social

### Steve Yegge's Blog Posts (Medium)

**"Introducing Beads: A coding agent memory system"** (Oct 13, 2025)
- Origin story: vibe-coded the entire project in ~6 days with Claude
- Problem framed as "50 First Dates" — agents wake up with no memory
- Previous project "vibecoder" was scrapped in favor of Beads
- Hit 1,000 stars and ~50 forks within 6 days of launch
— Source: https://steve-yegge.medium.com/introducing-beads-a-coding-agent-memory-system-637d7d92514a

**"The Beads Revolution"** (Oct 15, 2025)
- Positions Beads as solving hierarchical TODOs and long-horizon, multi-session planning
- Claims traditional tools (Jira, GitHub Issues) aren't designed for agent workflows
— Source: https://steve-yegge.medium.com/the-beads-revolution-how-i-built-the-todo-system-that-ai-agents-actually-want-to-use-228a5f9be2a9

**"Beads Best Practices"** (Nov 26, 2025)
- Growing momentum signal; addressing real developer pain points
- Community scaling guidance
— Source: https://steve-yegge.medium.com/beads-best-practices-2db636b9760c

**"Beads Blows Up"** (date unclear)
- Growth and adoption update
— Source: https://steve-yegge.medium.com/beads-blows-up-a0a61bb889b4

**"The Future of Coding Agents"** (Jan 2026)
- Written 3 days after Gas Town launch
- Vision for multi-agent future
— Source: https://steve-yegge.medium.com/the-future-of-coding-agents-e9451a84207c

**"Welcome to Gas Town"** (Jan 2026)
- Gas Town: multi-agent workspace manager built on top of Beads
- Manages 20-30 parallel Claude Code agents through a "Mayor" coordinator
- "Desire Paths" approach to agent UX design
— Source: https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04
— GitHub: https://github.com/steveyegge/gastown

### Twitter/X
- Launch tweet: "I just released Beads, a drop-in cognitive upgrade for your coding agent of choice... a magical 4-dimensional graph-based git-backed fairy-dusted issue-tracker database"
— Source: https://x.com/Steve_Yegge/status/1977645937225822664

### LinkedIn
- "Released Beads: A cognitive upgrade for coding agents"
— Source: https://www.linkedin.com/posts/steveyegge_github-steveyeggebeads-beads-a-memory-activity-7383408928665042944-tkcj

### Conference/Podcast Appearances
- **AI Tinkerers "One-Shot"**: Discussion of 85% rule, team lead methodology, "Land the Plane" protocol, multimodal debugging
— Source: https://one-shot.aitinkerers.org/p/steve-yegge-on-agentic-coding-beads-and-the-future-of-ai-workflows

- **Software Engineering Daily** (Feb 12, 2026): "Gas Town, Beads, and the Rise of Agentic Development" — full podcast episode
— Source: https://softwareengineeringdaily.com/2026/02/12/gas-town-beads-and-the-rise-of-agentic-development-with-steve-yegge/

---

## 3. Product Ecosystem & Documentation

### Documentation
- Installation guide: https://github.com/steveyegge/beads/blob/main/docs/INSTALLING.md
- Quickstart: https://github.com/steveyegge/beads/blob/main/docs/QUICKSTART.md
- FAQ: https://github.com/steveyegge/beads/blob/main/docs/FAQ.md
- Agent workflow: documented in README
- Copilot integration: separate doc
- Plugin/MCP docs: https://github.com/steveyegge/beads/blob/main/docs/PLUGIN.md
- DeepWiki: https://deepwiki.com/steveyegge/beads
- Dedicated docs site: https://steveyegge.github.io/beads/

### Gas Town (Multi-Agent Orchestrator)
- Separate project built on top of Beads
- Manages colonies of 20-30 parallel AI coding agents
- "Mayor" coordinator pattern — single-agent interface, multi-agent backend
- "Desire Paths" approach: observe what agents try, then build the thing they tried
— Source: https://github.com/steveyegge/gastown

### MCP Integration
- Python-based MCP server (`beads-mcp` on PyPI)
- Aggressive context reduction for token efficiency
- Dedicated docs: https://steveyegge.github.io/beads/integrations/mcp-server

### Plugin Framework (v0.50+)
- Plugin-based tracker adapters: GitLab, Linear
- Extensible architecture for additional integrations
— Source: [Releases](https://github.com/steveyegge/beads/releases)

### Agent Integrations
- Claude Code (primary target, documented setup via `bd setup claude`)
- Codex
- AMP (Sourcegraph's agent)
- Any agent that can read/write files
— Source: [paddo.dev](https://paddo.dev/blog/beads-memory-for-coding-agents/)

---

## 4. Community

### Hacker News Threads

**Thread #46075616 — "Beads – A memory upgrade for your coding agent"**
- Mixed reception: praised by power users, criticized for verbose/AI-generated docs
- Key praise: "superior to spec kit or any other form of structured workflow", "the only spec tool I've found that has stuck"
- Key criticism: README confusing, anthropomorphization of agent preferences, unclear dogfooding
- Alternatives mentioned: GitHub Issues CLI, Taskwarrior, spec-kit, Jira+MCP, plain markdown
- Conceptual praise: "Agent Experience" (AX) design as emerging discipline
— Source: https://news.ycombinator.com/item?id=46075616

**Thread #45566864 — "Beads: A coding agent memory system"**
- Modest engagement (19 points, 1 comment)
- Comparison to Fossil SCM's native ticket system
— Source: https://news.ycombinator.com/item?id=45566864

**Thread #46487580 — "Show HN: I replaced Beads with a faster, simpler Markdown-based task tracker"**
- Someone built a lighter alternative, suggesting Beads may be over-engineered for some use cases
— Source: https://news.ycombinator.com/item?id=46487580

### Community-Built Tools
Extensive ecosystem documented in `docs/COMMUNITY_TOOLS.md`:
- **Terminal UIs:** beads_viewer, lazybeads, bdui
- **Web interfaces:** beads-ui, kanban views, dashboards
- **Editor integrations:** VS Code, Neovim, Emacs plugins
- **Orchestration:** MCP Agent Mail for inter-agent coordination
- **Multi-agent:** mcp-beads-village (task coordination + file locking)
— Source: [README](https://github.com/steveyegge/beads), [Ian Bull blog](https://ianbull.com/posts/beads/)

### Third-Party Blog Coverage
- **paddo.dev** — "Beads: Memory for Your Coding Agents" + "From Beads to Tasks" (Anthropic comparison)
- **ianbull.com** — "The Best Damn Issue Tracker You're Not Using"
- **betterstack.com** — Technical setup guide
- **edgartools.io** — "beads and the future of programming" (mixed: productive but invasive)
- **maggieappleton.com** — Gas Town analysis
- **DoltHub blog** — "Long-running Agentic Work with Beads" (315 files refactored in 12hr session)
- **yuv.ai** — "Git-Backed Memory for AI Agents That Actually Remembers"

### Adoption Metrics
- 18.1k GitHub stars
- "Tens of thousands" of users (per Yegge)
- 29 contributors at 4-week mark
- ~15k lines of code (core)
— Sources: [GitHub](https://github.com/steveyegge/beads), [paddo.dev](https://paddo.dev/blog/beads-memory-for-coding-agents/)

### No Dedicated Discord Found
- No Beads-specific Discord or Slack community identified
- Discussion happens on HN, GitHub Issues, Twitter/X, blog comments

---

## 5. Competitive Positioning

### How Beads Positions Itself
1. **Agent-native, not human-first:** Designed for AI agents from the ground up, not a retrofit of human tools
2. **Git-native infrastructure:** Like git itself — a protocol + CLI with community building UIs on top
3. **Replaces markdown plans:** Queryable database > flat files for agent context efficiency
4. **Zero-conflict distributed:** Hash IDs + Dolt merge = multi-agent safe

### Key Differentiators vs. Existing Tools
| vs. | Beads Advantage |
|-----|-----------------|
| GitHub Issues | 4 dependency types, offline-first, hash IDs, agent-optimized JSON output |
| Jira | No server, git-native, agent-first design, lightweight |
| Taskwarrior | Agent semantics, discovered-from links, git sync |
| Markdown/TODO files | Queryable, structured dependencies, automatic ready detection |
| Claude Code Tasks | Project-level persistence (not session-level), agent-agnostic |

### Beads → Claude Code Influence
Anthropic engineer explicitly credited Beads as inspiration for Claude Code v2.1.16 task system:
> "We took inspiration from projects like Beads by Steve Yegge."

However, the article frames them as complementary layers:
- **Tasks:** immediate session coordination
- **Beads:** longer-term project memory
— Source: https://paddo.dev/blog/from-beads-to-tasks/

### Criticisms & Weaknesses
- Documentation quality (AI-generated, verbose, repetitive)
- Over-aggressive automation (auto git pushes)
- Alpha stability — API may change
- Requires explicit agent prompting to use
- Invasive project integration (hard to disable features)
- Some view it as over-engineered vs. simpler markdown approaches
— Sources: [HN #46075616](https://news.ycombinator.com/item?id=46075616), [edgartools.io](https://www.edgartools.io/beads-and-the-future-of-programming/)
