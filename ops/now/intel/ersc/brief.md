# ERSC — Competitive Intelligence Brief

**Date:** 2026-03-05
**Target:** ERSC (ersc.io)
**Analyst:** aiki/intel

## 1. What This Project Is

East River Source Control (ERSC) is a stealth-mode startup building a purpose-built collaboration platform ("forge") for the Jujutsu (jj) version control system. Founded in 2025 by Benjamin Brittain (ex-Google, build systems) and David Barsky (Rust/distributed systems, ex-Meta/Amazon), the company has raised ~$5M from Vermilion Cliffs Ventures and recruited Steve Klabnik (author of "The Rust Programming Language") to work on jj full-time. The team of four brings deep Rust, distributed systems, and VCS expertise. ([source](https://pitchbook.com/profiles/company/894743-11), [source](https://ersc.io/), [source](https://fallthrough.transistor.fm/43))

ERSC aims to be for Jujutsu what GitHub is for Git — the missing hosting, code review, and collaboration layer. Their three stated pillars are "high-quality code review," "understanding codebase evolution," and "source control as infrastructure," with a tagline of "source control for humans and machines" that explicitly signals AI/agent-first design intent. The founder's side project jj-yak (a gRPC + NFS virtualized remote backend for jj) hints at a server-side architecture that goes well beyond a web UI. ([source](https://ersc.io/), [source](https://github.com/benbrittain/jj-yak), [source](https://lobste.rs/s/kflxi5/east_river_source_control))

As of March 2026, ERSC has no public product, documentation, or demos. The website is a placeholder with a newsletter signup. A team member acknowledged they are "nowhere near ready to launch." Despite this opacity, they are filling a massive market gap: jj has 26k+ GitHub stars and growing adoption, but no purpose-built forge exists — users must rely on GitHub/GitLab via Git compatibility, losing jj's best features (change-ids, stacked changes, patch-based review). ([source](https://ersc.io/), [source](https://lobste.rs/s/kflxi5/east_river_source_control), [source](https://github.com/jj-vcs/jj))

## 2. What Matters for Aiki

ERSC is building the **platform layer** for jj; Aiki builds the **agent workflow layer** on top of jj. These are complementary — until ERSC moves into agent-native features, at which point they compete directly on Aiki's core wedge.

### Copy/Counter/Ignore Decisions

| Capability | Decision | Rationale |
|-----------|----------|-----------|
| **Jujutsu-native forge** | **Counter** | Position Aiki as the best autonomous agent client for ERSC's platform. Don't build forge features; instead be the specialized agent orchestration layer that ERSC can't replicate as a side feature. ([aiki-map.md](ops/now/intel/ersc/aiki-map.md)) |
| **High-quality code review** | **Counter** | Position `aiki review` as the pre-human-review layer. Agents catch issues before human reviewers see the code, making Aiki complementary to ERSC's human review UX. If ERSC adds AI review, Aiki's advantage is deeper task management, build/fix loops, and full autonomous workflow. ([aiki-map.md](ops/now/intel/ersc/aiki-map.md)) |
| **Codebase evolution understanding** | **Ignore** | Different problem space. ERSC's history visualization/analytics targets human developers, not agent workflows. No action needed. ([aiki-map.md](ops/now/intel/ersc/aiki-map.md)) |
| **Source control as infrastructure (API-first)** | **Copy** | Monitor ERSC's API design closely. When they publish APIs for change management and review workflows, adopt compatible patterns. Aiki's task-to-change mapping should leverage ERSC's change-id APIs rather than rolling its own. ([aiki-map.md](ops/now/intel/ersc/aiki-map.md)) |
| **Stacked PRs** | **Copy** | Adopt stacked PR patterns for Aiki's task-to-change workflow. When an agent works through subtasks sequentially, each subtask's changes should map to a stacked change in ERSC's model, giving reviewers granular diffs rather than monolithic ones. ([aiki-map.md](ops/now/intel/ersc/aiki-map.md)) |
| **Virtualized remote backend (jj-yak)** | **Ignore** | Experimental infrastructure plumbing. Aiki should work transparently on any jj backend. No action needed now. ([aiki-map.md](ops/now/intel/ersc/aiki-map.md)) |
| **Agent-friendly workflows ("humans and machines")** | **Counter** | Highest-threat capability. ERSC's "machines" framing suggests agents as forge users, not just API consumers. Counter by going deeper on autonomous workflow intelligence — task decomposition, review intelligence, build/fix loops — than a forge vendor can as a side feature. ([aiki-map.md](ops/now/intel/ersc/aiki-map.md)) |

**Summary:** 3 counter, 2 copy, 2 ignore. The two high-threat areas (code review, agent workflows) are both "counter" — Aiki must be deeper at the workflow layer than ERSC can go from the platform layer.

## 3. Top 3 Recommendations This Week

1. **Position as "The Agent Layer for ERSC"** — Draft positioning that frames Aiki as the complement to ERSC, not a competitor. The message: "ERSC is where code lives; Aiki is how agents work on it." Create a positioning doc, landing page copy, and community-facing artifacts (blog post, demo script). Test "complement to ERSC" messaging against generic "AI dev tool" messaging in community channels. This is low-effort (S) with outsized strategic impact, and must be done before ERSC launches. (from: [positioning/1](ops/now/intel/ersc/followups.md), [opportunity #1](ops/now/intel/ersc/opportunities.md) score 4.90, [opportunity #3](ops/now/intel/ersc/opportunities.md) score 4.45)

2. **Build the Pre-Human Review Layer** — Develop Aiki's autonomous review as the step that runs before human reviewers are notified. Target: catch 40%+ of reviewable issues (style, bugs, missing tests) and reduce median human review cycle time by 30%+. This is Aiki's highest-scoring opportunity (4.90) and directly addresses the biggest overlap with ERSC's "high-quality code review" pillar. Start by defining the integration model now so it's ready when ERSC's review APIs land. (from: [feature/1](ops/now/intel/ersc/followups.md), [opportunity #1](ops/now/intel/ersc/opportunities.md) score 4.90)

3. **Validate Autonomous Task Decomposition** — Run a controlled experiment: 20 comparable multi-file tasks, 10 with autonomous decomposition vs. 10 with flat prompting. Measure human interventions per task, first-attempt completion rate, and code quality. Target: 30% fewer interventions. This validates Aiki's deepest moat (autonomous workflow intelligence, score 4.60) — the capability ERSC cannot replicate as a forge side-feature. (from: [experiment/1](ops/now/intel/ersc/followups.md), [opportunity #2](ops/now/intel/ersc/opportunities.md) score 4.60)

## 4. Risks If We Do Nothing

- **ERSC absorbs the agent layer:** ERSC's "humans and machines" positioning signals explicit intent to build agent-native features into the forge. If ERSC ships built-in AI review, agent identities, and automated change management before Aiki establishes itself as the standard agent client, Aiki becomes redundant — a third-party tool duplicating platform-native features. Timeline: 6-12 months from ERSC's public launch. ([source](https://ersc.io/), [source](https://lobste.rs/s/kflxi5/east_river_source_control))

- **Platform lock-out:** ERSC will define the APIs, data models, and collaboration patterns for jj. If Aiki isn't deeply integrated when ERSC launches, we'll be playing catch-up against an API surface we didn't influence. Worse, ERSC could make architectural choices (closed review API, proprietary agent protocols) that structurally disadvantage third-party agent tools. Timeline: when ERSC's APIs stabilize, likely late 2026. ([source](https://github.com/benbrittain/jj-yak), [source](https://codeberg.org/forgejo/discussions/issues/325))

- **Narrative loss in the jj community:** The jj community (26k+ stars, active ecosystem) is waiting for a forge. When ERSC launches, it will dominate jj mindshare. If Aiki hasn't established a visible presence and clear relationship to ERSC by then, we'll be invisible in the ecosystem that matters most. The community will see ERSC as "the jj solution" and Aiki as an unrelated tool. Timeline: ERSC launch window, likely H2 2026. ([source](https://github.com/jj-vcs/jj), [source](https://news.ycombinator.com/item?id=44195211))

## Sources

- https://ersc.io/ — Landing page
- https://lobste.rs/s/kflxi5/east_river_source_control — Lobsters discussion with team commentary
- https://news.ycombinator.com/item?id=44195211 — HN discussion
- https://pitchbook.com/profiles/company/894743-11 — PitchBook profile ($4.86M raised)
- https://bsky.app/profile/ersc.io — Bluesky profile (1.1k followers)
- https://github.com/jj-vcs/jj — Jujutsu VCS repo (26.3k stars)
- https://github.com/benbrittain — Benjamin Brittain GitHub
- https://github.com/benbrittain/jj-yak — Virtualized remote backend for jj
- https://github.com/steveklabnik — Steve Klabnik GitHub
- https://davidbarsky.com/ — David Barsky personal site
- https://fallthrough.transistor.fm/43 — Podcast: "JJ and How to Evolve an Open Source Ecosystem"
- https://rocketreach.co/east-river-source-control-management_b69754fec97a7073 — Team org chart
- https://www.streetinsider.com/SEC+Filings/Form+D+East+River+Source+Contro/25033178.html — SEC Form D filing
- https://www.alleywatch.com/2025/07/the-alleywatch-startup-daily-funding-report-7-9-2025/ — Funding report
- https://codeberg.org/forgejo/discussions/issues/325 — Patch-based code review discussion (jj context)
