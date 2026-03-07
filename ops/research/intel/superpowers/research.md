# Superpowers — Deep Research

**Repo:** https://github.com/obra/superpowers
**Author:** Jesse Vincent ([@obra](https://github.com/obra))
**Tagline:** "An agentic skills framework & software development methodology that works."
**License:** MIT
**Language:** Shell (primary)
**Created:** 2025-10-09

---

## 1. GitHub Repository

### Metrics (as of 2026-03-05)

| Metric | Value |
|--------|-------|
| Stars | 71,679 |
| Forks | 5,527 |
| Open issues | 195 (paginated; 30+ visible) |
| Subscribers/watchers | 359 |
| Contributors | 18 (obra dominates with 245 commits) |
| Latest push | 2026-02-21 |
| Releases | 2 (v4.1.0, v4.1.1 — both 2026-01-23) |

Source: https://github.com/obra/superpowers

### Repo Structure

```
.claude-plugin/     — Claude Code plugin config (plugin.json, marketplace.json)
.codex/             — Codex integration
.cursor-plugin/     — Cursor integration
.opencode/          — OpenCode integration
agents/             — Agent definitions
commands/           — CLI commands
docs/               — Documentation (Codex README, OpenCode README, plans/, testing.md)
hooks/              — Lifecycle hooks
lib/                — Core library (skills-core.js)
skills/             — 14 skill definitions (see below)
tests/              — Test infrastructure
```

Source: https://github.com/obra/superpowers (repo tree)

### Skills Library (14 skills)

| Skill | Category | Purpose |
|-------|----------|---------|
| brainstorming | Collaboration | Socratic design refinement before coding |
| writing-plans | Collaboration | Break work into 2–5 min tasks with exact file paths |
| executing-plans | Collaboration | Batch execution with human checkpoints |
| subagent-driven-development | Collaboration | Fresh subagent per task with two-stage review |
| dispatching-parallel-agents | Collaboration | Concurrent subagent workflows |
| requesting-code-review | Collaboration | Pre-review checklist |
| receiving-code-review | Collaboration | Responding to feedback |
| using-git-worktrees | Collaboration | Isolated development branches |
| finishing-a-development-branch | Collaboration | Merge/PR decision workflow |
| test-driven-development | Testing | RED-GREEN-REFACTOR cycle enforcement |
| systematic-debugging | Debugging | 4-phase root cause process |
| verification-before-completion | Debugging | Verify fix actually works |
| writing-skills | Meta | Create new skills following best practices |
| using-superpowers | Meta | Introduction to the skills system |

Source: https://github.com/obra/superpowers/tree/main/skills

### Recent Issues (2026-03-05)

- #622 — `feat: add refining-plan skill for iterative plan pressure-testing`
- #621 — `subagent-driven-development skill references non-existent Agent type "Implementer"`
- #618 — `feat(kiro): add Kiro IDE support`
- #617 — `add support for Trae-IDE`
- #614 — `fix(code-reviewer): require tool verification before asserting facts`
- #613 — `fix: add hard gates to prevent review skipping`

Source: https://github.com/obra/superpowers/issues

### Multi-Platform Support

Superpowers supports four coding agent platforms:
- **Claude Code** — via plugin marketplace (`/plugin install superpowers@superpowers-marketplace`)
- **Cursor** — via plugin marketplace (`/plugin-add superpowers`)
- **Codex** — manual setup via INSTALL.md
- **OpenCode** — manual setup via INSTALL.md, uses native skills system

Source: https://github.com/obra/superpowers#installation

### Related Repositories

| Repo | Stars | Forks | Description |
|------|-------|-------|-------------|
| [obra/superpowers-marketplace](https://github.com/obra/superpowers-marketplace) | 583 | 104 | Curated Claude Code plugin marketplace |
| [obra/superpowers-skills](https://github.com/obra/superpowers-skills) | 537 | 127 | Community-editable skills repository |

---

## 2. Blog / Author Writing

### "Superpowers: How I'm using coding agents in October 2025"
**URL:** https://blog.fsck.com/2025/10/09/superpowers/

Key claims and methodology:
- Skills function as reusable markdown instruction sets (SKILL.md files) that dramatically improve agent compliance
- Bootstrap process tells Claude "You have skills. They give you Superpowers" and agents must check for relevant skills before any task
- Core system is "VERY token light" — pulls in one doc of fewer than 2,000 tokens
- Uses "pressure testing" with adversarial scenarios (e.g., time-pressure production outage simulations) to harden skill compliance
- Incorporated Cialdini's persuasion principles (authority, commitment, social proof, etc.) into skill design, citing research that "LLMs respond to persuasion principles"
- Subagent-driven development: dispatches fresh subagents per task with two-stage review (spec compliance, then code quality)
- Claims agents can work autonomously "for a couple hours at a time without deviating from the plan"

Source: https://blog.fsck.com/2025/10/09/superpowers/

### Simon Willison's endorsement
**URL:** https://simonwillison.net/2025/Oct/10/superpowers/

Key quotes:
- "Jesse is one of the most creative users of coding agents (Claude Code in particular) that I know."
- Called the approach "wildly more ambitious than most other people" working with AI agents
- Described Superpowers as "a really significant piece in its own right"
- "There is _so much_ to learn" from exploring the repository

Source: https://simonwillison.net/2025/Oct/10/superpowers/

---

## 3. Social & Community Discussion

### Hacker News
**URL:** https://news.ycombinator.com/item?id=45547344

- 435 points, 231 comments
- Strong endorsement from Simon Willison in the thread
- **Praise:** Token-efficient context management; reusable skill architecture; structured methodology
- **Criticism:**
  - Lack of empirical benchmarks / A/B testing to prove skills improve results vs standard prompting
  - "Voodoo engineering" concerns — persuasion psychology applied to LLMs may be cargo-cult
  - Unclear performance on complex real-world codebases vs trivial tasks
  - Token economics debated — subagents may create overhead via duplicated context
- **Emergent consensus:** LLMs work best as supervised tools for narrow tasks with human oversight; "superpower" framing may overstate autonomy

Source: https://news.ycombinator.com/item?id=45547344

### Other HN Mentions
- Referenced in Claude Code Swarms discussion: https://news.ycombinator.com/item?id=46743908
- Referenced in metaswarm "127 PRs" thread: https://news.ycombinator.com/item?id=46864977
- Standalone skills post: https://news.ycombinator.com/item?id=45580766

### Third-Party Coverage

| Source | URL | Notes |
|--------|-----|-------|
| byteiota | https://byteiota.com/superpowers-agentic-framework-27k-github-stars/ | "27K GitHub Stars" profile (stars have since tripled) |
| DecisionCrafters | https://www.decisioncrafters.com/superpowers-agentic-skills-framework/ | "Revolutionary Agentic Skills Framework" |
| DEV Community | https://dev.to/chand1012/the-best-way-to-do-agentic-development-in-2026-14mn | "The best way to do agentic development in 2026" |
| YUV.AI | https://yuv.ai/blog/superpowers | "Stop AI Agents from Writing Spaghetti: Enforcing TDD" |
| Colin McNamara | https://colinmcnamara.com/blog/stop-babysitting-your-ai-agents-superpowers-breakthrough | "Stop Babysitting Your AI Agents" |
| Pasquale Pillitteri | https://www.pasqualepillitteri.it/en/news/215/superpowers-claude-code-complete-guide | "Complete Guide 2026" |
| Agent Skills CC | https://agent-skills.cc/skills/obra-superpowers | Skills directory listing |
| Claude Marketplaces | https://claudemarketplaces.com/plugins/obra-superpowers | Plugin listing |

### LinkedIn
- Jesse posted announcement: https://www.linkedin.com/posts/jessevincent_superpowers-how-im-using-coding-agents-activity-7382239471783604224-pwmT

### Social Profiles
- Bluesky: https://bsky.app/profile/s.ly
- Mastodon: https://metasocial.com/@jesse
- Threads: https://www.threads.com/@obrajesse

---

## 4. Author Context

### Jesse Vincent (obra)

**Wikipedia:** https://en.wikipedia.org/wiki/Jesse_Vincent
**Website:** https://fsck.com/
**Blog:** https://blog.fsck.com/

**Background:**
- BA in Russian Studies from Wesleyan University
- Created **Request Tracker (RT)** in 1994 — one of the most widely deployed open-source ticketing systems
- Founded **Best Practical Solutions** (2001) to develop and support RT
- **Perl 6 project manager** (2005–2008)
- **Perl 5 release manager** (pumpking) for versions 5.12 and 5.14; changed Perl 5 to a regular timeboxed release cycle
- Co-founded **Keyboardio** (2014) — ergonomic keyboard company
- Created **K-9 Mail** — open-source Android email app (later acquired by Mozilla, rebranded as Thunderbird for Android)
- Has a Wikipedia page; well-known figure in open-source community

**Relevance to Superpowers:** Decades of experience building developer tools and managing large open-source projects. Strong track record of creating tools that achieve widespread adoption (RT, K-9 Mail). Not a newcomer; credibility in the open-source community is well established.

Sources:
- https://en.wikipedia.org/wiki/Jesse_Vincent
- https://shop.keyboard.io/pages/about
- https://hackaday.io/obra

---

## 5. Core Workflow (from README)

1. **Brainstorming** — Activates before writing code. Socratic questioning to refine ideas. Design saved as document.
2. **Using git worktrees** — Creates isolated workspace on new branch after design approval.
3. **Writing plans** — Breaks work into 2–5 min tasks with exact file paths, complete code, verification steps.
4. **Subagent-driven development / Executing plans** — Dispatches fresh subagent per task with two-stage review (spec compliance → code quality), or batch execution with human checkpoints.
5. **Test-driven development** — Enforces RED-GREEN-REFACTOR. Deletes code written before tests.
6. **Requesting code review** — Reviews against plan, reports issues by severity. Critical issues block.
7. **Finishing a development branch** — Verify tests, present options (merge/PR/keep/discard), clean up worktree.

Source: https://github.com/obra/superpowers#the-basic-workflow

---

## 6. Philosophy

- **Test-Driven Development** — Write tests first, always
- **Systematic over ad-hoc** — Process over guessing
- **Complexity reduction** — Simplicity as primary goal
- **Evidence over claims** — Verify before declaring success

Source: https://github.com/obra/superpowers#philosophy

---

## 7. Activity & Traction Summary

- **71.7K stars** as of 2026-03-05 — one of the most starred agentic coding repos
- **Growth trajectory:** From ~27K (byteiota coverage) to 71.7K; roughly tripled
- **Active development:** Last push 2026-02-21; issues being filed daily (6+ issues on 2026-03-05 alone)
- **Multi-platform:** Claude Code, Cursor, Codex, OpenCode — broadening beyond Anthropic ecosystem
- **Ecosystem:** Dedicated marketplace repo (583 stars), community skills repo (537 stars)
- **Community interest:** Requests for Kiro IDE (#618) and Trae IDE (#617) support show expanding demand
- **Contributor concentration:** obra (245 commits) dominates; 17 other contributors with 1–12 commits each
- **Release cadence:** Only 2 formal releases (v4.1.0, v4.1.1) suggesting rapid iteration on main branch rather than versioned releases
