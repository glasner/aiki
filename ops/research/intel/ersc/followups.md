# ERSC — Follow-up Execution Tasks

## feature/1: Pre-Human Review Layer
- **Source opportunity:** #1 Pre-Human Review Layer (Score: 4.90)
- **Hypothesis:** If Aiki runs autonomous code review on every new change before human reviewers are notified, it will catch 40%+ of reviewable issues (style, bugs, missing tests) and reduce median human review cycle time by at least 30%. Teams will adopt this because reviewer overload is a universal, daily pain.
- **Success metric:** In a pilot with 3+ teams: (a) ≥40% of review comments that would have been left by humans are caught by Aiki first, (b) median time-to-first-human-approval drops by ≥30%, (c) reviewer satisfaction score improves (survey).
- **Scope guardrails:**
  - **In scope:** Autonomous review triggered on new changes, posting findings as review comments, creating fix tasks for issues found, integration with ERSC's review data model (interdiff format, review state machine).
  - **Out of scope:** Replacing human review entirely, auto-merging changes, building our own review UI, CI integration (separate task), supporting non-ERSC forges in v1.
- **Estimated effort:** L

## feature/2: Agent-Initiated Reviews on ERSC
- **Source opportunity:** #3 Agent-Initiated Reviews on ERSC (Score: 4.45)
- **Hypothesis:** If Aiki agents can be full participants in ERSC's review workflow — opening reviews, responding to human feedback, and pushing updated changes — teams will treat agent-authored code with the same confidence as human-authored code, increasing agent adoption by ≥50% among teams currently blocked by the "agent code = unreviewed code" problem.
- **Success metric:** (a) Agents can create, update, and respond to reviews end-to-end without human intervention on the tooling side, (b) ≥80% of agent-opened reviews reach approval without manual re-submission, (c) team adoption of agent-driven workflows increases measurably in pilot cohort.
- **Scope guardrails:**
  - **In scope:** Creating reviews in ERSC from Aiki, posting review comments, reacting to human reviewer feedback (parsing comments, generating fixes, pushing updates), managing review lifecycle (draft → ready → approved).
  - **Out of scope:** Agent-to-agent review (v1 is agent↔human only), building review UI, handling merge conflicts automatically, supporting GitHub/GitLab review APIs.
- **Estimated effort:** L

## experiment/1: Autonomous Task Decomposition Validation
- **Source opportunity:** #2 Autonomous Workflow Intelligence (Score: 4.60)
- **Hypothesis:** Agents that autonomously decompose ambiguous tasks into subtask sequences and prioritize work by codebase impact will complete complex multi-file changes with ≥30% fewer human interventions (corrections, re-prompts, manual fixes) compared to agents operating on flat, human-written instructions.
- **Success metric:** Run 20 comparable multi-file tasks (10 with autonomous decomposition, 10 with standard flat prompting). Measure: (a) human interventions per task, (b) task completion rate without re-prompting, (c) code quality of output (review scores). Target: ≥30% reduction in interventions, ≥20% improvement in first-attempt completion.
- **Scope guardrails:**
  - **In scope:** Building a prototype autonomous decomposition layer (task analysis → subtask generation → priority ordering → execution), running the A/B comparison on real-world tasks, measuring the three metrics above.
  - **Out of scope:** Full production-grade autonomous intelligence, self-healing CI loops, learning from outcomes across sessions, multi-agent coordination, integration with ERSC (this is a standalone validation).
- **Estimated effort:** M

## positioning/1: "The Agent Layer" — Complementary Positioning vs. ERSC
- **Source opportunity:** #1 Pre-Human Review Layer (Score: 4.90), #3 Agent-Initiated Reviews (Score: 4.45), #5 Agent Identity & Authorship (Score: 4.25)
- **Hypothesis:** Positioning Aiki as "the autonomous agent layer that makes ERSC better" (complement, not competitor) will resonate more strongly with the jj community than positioning as an independent tool. The message "ERSC is where code lives; Aiki is how agents work on it" will drive higher intent-to-adopt among ERSC-aware developers than generic "AI dev tool" messaging.
- **Success metric:** (a) Create positioning doc + landing page copy + 3 community-facing artifacts (blog post, demo script, README section), (b) A/B test two message variants ("complement to ERSC" vs. "standalone AI workflow tool") in community channels — measure click-through and sign-up intent, (c) target ≥2x engagement on complement positioning vs. standalone.
- **Scope guardrails:**
  - **In scope:** Core positioning narrative, key messaging pillars (pre-human review, agent-initiated reviews, agent identity), landing page copy, blog post draft, demo script showing Aiki + ERSC together, competitive differentiation talking points.
  - **Out of scope:** Full brand redesign, paid marketing campaigns, pricing strategy, partnership agreements with ERSC team, messaging for non-ERSC forges.
- **Estimated effort:** S
