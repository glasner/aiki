# Superpowers — Competitive Intelligence Brief

**Date:** 2026-03-05
**Target:** [obra/superpowers](https://github.com/obra/superpowers) — "An agentic skills framework & software development methodology that works."
**Author:** Jesse Vincent ([@obra](https://github.com/obra)) — creator of Request Tracker, K-9 Mail, Keyboardio; former Perl 5/6 release manager ([Wikipedia](https://en.wikipedia.org/wiki/Jesse_Vincent))

---

## 1. What This Project Is

Superpowers is a **reusable skills framework** for AI coding agents. It ships 14 markdown-based skill files (SKILL.md) that shape agent behavior across the full development lifecycle: brainstorming, planning, execution, testing, code review, and merge. The bootstrap prompt is ~2,000 tokens — agents self-select relevant skills at runtime.

**Key stats:** 71.7K stars, 5.5K forks, 18 contributors, MIT license. Supports Claude Code, Cursor, Codex, and OpenCode, with community requests for Kiro and Trae IDE support. Two ecosystem repos: [superpowers-marketplace](https://github.com/obra/superpowers-marketplace) (583 stars) and [superpowers-skills](https://github.com/obra/superpowers-skills) (537 stars). Stars roughly tripled from ~27K in a few months ([byteiota](https://byteiota.com/superpowers-agentic-framework-27k-github-stars/)).

**Core workflow:** Brainstorming → git worktree isolation → plan writing (2–5 min tasks) → subagent-driven execution with two-stage review → TDD enforcement → code review → branch finish ([README](https://github.com/obra/superpowers#the-basic-workflow)).

**Notable endorsement:** Simon Willison called Jesse "one of the most creative users of coding agents" and described Superpowers as "a really significant piece" ([simonwillison.net](https://simonwillison.net/2025/Oct/10/superpowers/)). The HN thread drew 435 points and 231 comments ([HN](https://news.ycombinator.com/item?id=45547344)).

---

## 2. What Matters for Aiki

**The fundamental distinction:** Superpowers is a **behavioral layer** — ephemeral skill prompts that shape how agents act within a single session. Aiki is an **infrastructure layer** — persistent task state, cross-session tracking, automated review, and provenance that survive sessions, agents, and context windows.

**Message: "Skills tell agents what to do. Aiki makes sure it got done."**

### Direct overlaps with Aiki's wedge

| Capability | Superpowers Approach | Aiki Approach | Decision |
|---|---|---|---|
| **Code review** | Behavioral checklists (`requesting-code-review`, `receiving-code-review` skills) | Structured automated review with issue tracking, severity, file locations, followup generation via `aiki fix` | **Counter** — Aiki's review is infrastructure; Superpowers' is a reminder. Recent issues [#613](https://github.com/obra/superpowers/issues/613) and [#614](https://github.com/obra/superpowers/issues/614) (hard gates for review skipping, tool verification) confirm Superpowers users need what Aiki already provides. |
| **Task decomposition & planning** | Static markdown plans with file paths and code snippets (`writing-plans` skill) | Tracked task hierarchies with lifecycle (start/stop/close/comment), provenance (`--source`), subtask state | **Counter** — Superpowers plans are static documents. Aiki plans are live state that tracks execution, drift, and completion. |
| **Plan execution** | Ephemeral batch execution with human checkpoints (`executing-plans` skill) | Persistent execution tracking via subtask lifecycle; survives session crashes | **Counter** — If a session ends mid-plan, Superpowers loses state. Aiki knows exactly what's done and what remains. |
| **Subagent orchestration with review** | Two-stage review at the subagent level: spec compliance, then code quality (`subagent-driven-development` skill) | `aiki run` with full task context; `aiki review` for post-completion review | **Copy** — Adopt the two-stage review pattern (spec compliance + code quality) into `aiki review`. Aiki's task→diff provenance makes automated spec-compliance checking uniquely powerful. |
| **Workspace isolation** | Teaches agents to manually create git worktrees (`using-git-worktrees` skill) | Automatic JJ-backed isolated workspaces per agent session, transparent to the agent | **Counter** — Aiki handles isolation automatically. Superpowers' manual approach doesn't scale to concurrent agents. |
| **Verification before completion** | Behavioral skill reminding agents to verify fixes work | No explicit verification gate on task close | **Copy** — Add verification gates to `aiki task close` (require evidence tests pass before closing). |

### Capabilities to ignore

| Capability | Why Ignore |
|---|---|
| Brainstorming / Socratic design | Pre-coding creative process; doesn't compete with task tracking or review |
| TDD enforcement | Coding methodology orthogonal to Aiki's wedge; developers can use alongside Aiki |
| Systematic debugging | Single-session problem-solving; not a cross-session workflow concern |
| Multi-platform support | Strategic decision for later; Aiki should nail Claude Code first |
| Community skills marketplace | Aiki's value is infrastructure, not a skill marketplace; integrate with Superpowers' ecosystem, don't compete |

---

## 3. Top 3 Recommendations This Week

### Recommendation 1: Adopt two-stage review (Copy) — Scored 4.60

Split `aiki review` into two explicit phases: (1) **spec-compliance** — does the diff match the task description? and (2) **code quality** — is the code well-structured and tested? This directly responds to Superpowers' most reported pain point (review skipping, per [#613](https://github.com/obra/superpowers/issues/613) and [#614](https://github.com/obra/superpowers/issues/614)). Aiki's task→diff provenance model enables automated spec-compliance checking that Superpowers' behavioral approach cannot match.

**Next step:** Create a design doc for two-stage review architecture in `aiki review`. See [followups.md: feature/two-stage-review](followups.md) for scope and success metrics.

### Recommendation 2: Ship complementary positioning (Counter) — Scored 4.30

Position Aiki explicitly as complementary to Superpowers: **"Use Superpowers to teach agents how to work. Use Aiki to track, verify, and review the work they produce."** This turns a 71.7K-star competitor into a distribution channel. Publish an integration guide showing both tools used together, and create a comparison table (behavioral layer vs. infrastructure layer).

**Next step:** Write an "Aiki + Superpowers" positioning page and integration guide. Consider a lightweight Superpowers skill that introduces agents to `aiki task` commands. See [followups.md: positioning/complementary-with-superpowers](followups.md) for scope.

### Recommendation 3: Add verification gates on task close (Copy) — Scored 4.15

Require evidence of verification (tests pass, build succeeds) before `aiki task close` succeeds. This enforces at the infrastructure level what Superpowers attempts with a behavioral skill. A closed task should mean "verified," not "agent said so." Addresses the same pain that drove Superpowers [#614](https://github.com/obra/superpowers/issues/614).

**Next step:** Prototype a `--verified` flag or pre-close hook in `aiki task close`. See [followups.md: feature/verification-gates](followups.md) for scope and success metrics (note: listed as #5 in opportunities, cross-reference with the verification-before-completion copy opportunity in [aiki-map.md](aiki-map.md)).

---

## 4. Risks If We Do Nothing

1. **Category definition risk.** Superpowers is defining what "structured agentic workflows" means at 71.7K stars and growing. If Aiki doesn't clearly differentiate as infrastructure vs. behavioral prompts, it risks being perceived as "another skills framework" — a category where Superpowers has an insurmountable lead. The window for positioning is now, while the category is still forming. ([research.md: Growth trajectory](research.md), [aiki-map.md: Summary Assessment](aiki-map.md))

2. **Review wedge erosion.** Superpowers' two-stage subagent review and recent issues around review quality ([#613](https://github.com/obra/superpowers/issues/613), [#614](https://github.com/obra/superpowers/issues/614)) signal that the project is actively evolving toward infrastructure-level review enforcement. If Superpowers adds persistent issue tracking, severity classification, or automated followup generation, it collapses the distinction between behavioral and infrastructure approaches — directly threatening Aiki's core wedge. ([aiki-map.md: Code Review Workflow](aiki-map.md))

3. **Multi-platform distribution gap.** Superpowers runs on Claude Code, Cursor, Codex, and OpenCode, with active requests for Kiro ([#618](https://github.com/obra/superpowers/issues/618)) and Trae ([#617](https://github.com/obra/superpowers/issues/617)). It reaches developers regardless of their agent platform. Aiki is currently Claude Code-only. While multi-platform expansion isn't urgent, every month that Superpowers deepens its cross-platform adoption increases the switching cost for developers who might otherwise adopt Aiki. ([research.md: Multi-Platform Support](research.md))

4. **Community flywheel.** Superpowers' marketplace (583 stars) and community skills repo (537 stars) create a contribution flywheel — more skills attract more users who contribute more skills. This organic growth compounds over time. If Aiki doesn't position as complementary infrastructure (rather than a competing layer), it misses the opportunity to ride Superpowers' distribution and instead fights against it. ([aiki-map.md: Community Skills Marketplace](aiki-map.md))
