# Superpowers — Aiki Relevance Map

Aiki's core wedge: **autonomous review** + structured task tracking for agentic development workflows.

Superpowers' core wedge: **reusable skills framework** — markdown instruction sets that shape agent behavior across the entire development lifecycle (brainstorming → planning → execution → review → merge).

## Capability Classifications

| Capability | Overlap with Aiki Wedge | Threat | Opportunity | Why Now |
|---|---|---|---|---|
| Skills framework (reusable SKILL.md files) | Medium | High | Counter | See below |
| Brainstorming / Socratic design refinement | Low | Low | Ignore | See below |
| Plan writing (2–5 min task decomposition) | High | High | Counter | See below |
| Plan execution (batch with checkpoints) | High | High | Counter | See below |
| Subagent-driven development (two-stage review) | High | High | Copy | See below |
| Parallel agent dispatching | Medium | Medium | Counter | See below |
| Test-driven development enforcement | Low | Low | Ignore | See below |
| Systematic debugging (4-phase process) | Low | Low | Ignore | See below |
| Code review workflow (request + receive) | High | High | Counter | See below |
| Git worktree isolation | High (shared) | Medium | Counter | See below |
| Verification before completion | Medium | Medium | Copy | See below |
| Multi-platform support (Claude/Cursor/Codex/OpenCode) | Low | Medium | Ignore | See below |
| Community skills marketplace | Low | Medium | Ignore | See below |

## Detailed Reasoning

### 1. Skills Framework (Reusable SKILL.md Files)
- **Overlap: Medium** — Aiki uses CLAUDE.md instructions and hook-based lifecycle integration to shape agent behavior. Superpowers uses standalone SKILL.md files that agents self-select based on context. Both aim to make agents follow structured workflows, but through different mechanisms (injected system instructions vs. agent-discovered skill documents).
- **Threat: High** — At 71.7K stars, Superpowers is the most widely adopted agentic skills framework. Its token-light design (~2K tokens for bootstrap) and multi-platform support make it frictionless to adopt. Developers already using Superpowers may see Aiki's task system as redundant if Superpowers' skills cover enough of the workflow. The skills framework is also extensible — community can contribute new skills that erode Aiki's differentiation over time.
- **Opportunity: Counter** — Aiki's approach is fundamentally different: it provides *persistent infrastructure* (JJ-backed task state, cross-session provenance, structured review) rather than *behavioral prompts*. Skills tell agents *how* to work; Aiki provides *where work is tracked and verified*. Position as: "Skills shape behavior in one session; Aiki provides the infrastructure that persists across sessions, agents, and humans." The two are complementary, not substitutes.
- **Why now:** Superpowers' rapid growth (27K → 71.7K stars in months) signals strong market demand for structured agentic workflows. The category is being defined now. If Aiki doesn't clearly differentiate, it risks being perceived as "another skills framework" rather than infrastructure.

### 2. Brainstorming / Socratic Design Refinement
- **Overlap: Low** — Aiki doesn't include a brainstorming workflow. This is a pre-coding creative process that sits upstream of Aiki's task tracking and review.
- **Threat: Low** — Brainstorming is a prompt engineering technique, not a workflow tool. It doesn't compete with task management or code review.
- **Opportunity: Ignore** — Not aligned with Aiki's wedge. Developers can use Superpowers' brainstorming skill alongside Aiki without friction.
- **Why now:** No urgent timing signal. Brainstorming is valuable but doesn't threaten Aiki's positioning.

### 3. Plan Writing (2–5 Minute Task Decomposition)
- **Overlap: High** — Aiki's task system (`aiki task add`, subtasks, `--source` for plan lineage) directly addresses work decomposition. Superpowers' `writing-plans` skill produces plans with exact file paths, complete code snippets, and verification steps — similar in intent but output as markdown documents rather than tracked task state.
- **Threat: High** — Superpowers' plan writing produces *immediately executable* plans with code and file paths baked in. This is more opinionated and detailed than Aiki's task descriptions. Developers who want "just tell the agent what to do" may prefer Superpowers' approach to Aiki's more structured task tracking.
- **Opportunity: Counter** — Aiki's plans are *tracked state* with lifecycle management (start, stop, close, comment). Superpowers' plans are *static documents* that agents follow but don't update. Aiki can show: "After the plan is written, what happens? Aiki tracks execution, drift, and completion. Superpowers leaves a markdown file."
- **Why now:** Plan-driven agent workflows are becoming the default methodology. The writing-plans skill is one of Superpowers' most referenced capabilities. Aiki needs to clearly position its task system as the *runtime* for plans, not just another plan format.

### 4. Plan Execution (Batch with Human Checkpoints)
- **Overlap: High** — Aiki's `aiki task start`/`close`/`comment` lifecycle is a plan execution system with progress tracking. Superpowers' `executing-plans` skill runs tasks in batches with human review between phases. Both solve "how do we execute multi-step work with oversight?"
- **Threat: High** — Superpowers' execution model is simple and lightweight: run a batch, pause for human review, continue. Aiki's model is richer (persistent state, provenance, subtask hierarchy) but also heavier. For developers who want minimal overhead, Superpowers' approach may win on simplicity.
- **Opportunity: Counter** — Aiki's execution tracking persists across sessions and agents. If a session crashes or an agent is replaced, Aiki's task state shows exactly what was done and what remains. Superpowers' batch execution is ephemeral — if the session ends mid-plan, the state is lost. Position as: "Superpowers executes plans within a session. Aiki tracks execution across sessions."
- **Why now:** As agent sessions grow longer and more autonomous, execution tracking that survives session boundaries becomes critical. The "couple hours at a time" autonomy that Jesse Vincent describes demands infrastructure for recovery and handoff.

### 5. Subagent-Driven Development (Two-Stage Review)
- **Overlap: High** — Aiki's `aiki run` delegates work to subagents with full task context. Superpowers' `subagent-driven-development` dispatches fresh subagents per task with a two-stage review: first for spec compliance, then for code quality. Both orchestrate multi-agent workflows, but Superpowers includes *review as part of the subagent workflow* — directly overlapping with Aiki's autonomous review wedge.
- **Threat: High** — The two-stage review is the most direct threat to Aiki's core wedge. Superpowers bakes review into the development cycle at the subagent level, meaning code is reviewed *before* it even reaches the developer. If developers adopt this workflow, they may not feel the need for a separate review system.
- **Opportunity: Copy** — Aiki should study Superpowers' two-stage review design (spec compliance + code quality as separate passes). This pattern could enhance `aiki review` by structuring review into explicit phases rather than a single pass. The spec-compliance check (does this match the task description?) is particularly well-suited to Aiki's task→diff provenance model.
- **Why now:** Multi-agent workflows are rapidly becoming standard. Superpowers' 71.7K stars validate that developers want structured subagent orchestration. Aiki's `aiki run` is the mechanism; adding structured review phases would make it the most complete solution.

### 6. Parallel Agent Dispatching
- **Overlap: Medium** — Aiki supports parallel delegation via `aiki run <id> --async` + `aiki task wait`. Superpowers' `dispatching-parallel-agents` skill provides a methodology for concurrent subagent workflows. Both enable parallelism, but Aiki's approach is infrastructure (task state + wait semantics) while Superpowers' is behavioral (skill instructions for agents).
- **Threat: Medium** — Parallel dispatch is a feature, not a wedge. Superpowers' approach is more about teaching agents *when* to parallelize; Aiki's is about providing *how* to parallelize with tracking.
- **Opportunity: Counter** — Aiki's `--async` + `wait` + `show` pattern is more robust for production use. Superpowers' parallel dispatch doesn't track completion status or allow inspection of results. Position as: "Superpowers tells agents to run in parallel. Aiki tells you when they're done and what they did."
- **Why now:** As agentic workflows scale, parallel execution becomes a table-stakes requirement. Both tools address this; neither has a decisive advantage.

### 7. Test-Driven Development Enforcement
- **Overlap: Low** — Aiki doesn't enforce TDD. Aiki tracks *whether* work was done; Superpowers enforces *how* code is written (tests first, RED-GREEN-REFACTOR cycle).
- **Threat: Low** — TDD enforcement is a coding methodology, not a workflow infrastructure concern. It doesn't compete with task tracking or review.
- **Opportunity: Ignore** — TDD is orthogonal to Aiki's wedge. Developers can (and should) use TDD skills alongside Aiki's task tracking. No need to replicate.
- **Why now:** No timing signal specific to Aiki. TDD is a perennial best practice.

### 8. Systematic Debugging (4-Phase Root Cause Process)
- **Overlap: Low** — Aiki doesn't include debugging methodology. This is a single-session problem-solving process, not a cross-session workflow concern.
- **Threat: Low** — Debugging skills are useful but tangential to Aiki's value proposition.
- **Opportunity: Ignore** — Not aligned with Aiki's review/orchestration wedge. No action needed.
- **Why now:** No urgent timing signal.

### 9. Code Review Workflow (Request + Receive)
- **Overlap: High** — This is Aiki's core wedge. Superpowers has two skills: `requesting-code-review` (pre-review checklist before requesting human review) and `receiving-code-review` (structured process for responding to feedback). Aiki's `aiki review` provides autonomous review with issue tracking, severity levels, file locations, and followup task generation via `aiki fix`.
- **Threat: High** — Superpowers' review skills are *process guidelines* for agents, not automated review infrastructure. However, at 71.7K stars, Superpowers sets the developer expectation for what "code review in agentic workflows" looks like. If Superpowers adds automated review capabilities (not just process skills), it would be a direct threat.
- **Opportunity: Counter** — Aiki's review is *infrastructure*: it generates structured issues, tracks severity, links to file locations, and creates followup tasks. Superpowers' review is *behavioral*: it teaches agents to check their work against a list. The gap is enormous. Position as: "Superpowers reminds agents to review. Aiki reviews automatically, tracks issues, and generates fix tasks." This is Aiki's clearest differentiation.
- **Why now:** Recent Superpowers issues (#614: "require tool verification before asserting facts", #613: "add hard gates to prevent review skipping") show that review quality is a live pain point even for Superpowers users. Aiki's structured review addresses exactly this gap.

### 10. Git Worktree Isolation
- **Overlap: High (shared approach)** — Both Aiki and Superpowers use git worktrees for isolated development. Aiki creates JJ-backed isolated workspaces per agent session automatically. Superpowers' `using-git-worktrees` skill teaches agents to create worktrees manually.
- **Threat: Medium** — Superpowers' worktree usage is manual and agent-directed; Aiki's is automatic and infrastructure-level. Developers using Superpowers still manage worktrees themselves.
- **Opportunity: Counter** — Aiki's automatic workspace isolation (JJ-backed, conflict resolution, transparent to the user) is more sophisticated than Superpowers' "tell the agent to use worktrees" approach. Position as: "Superpowers teaches agents about worktrees. Aiki handles isolation transparently — agents don't even know they're in a worktree."
- **Why now:** Multi-agent concurrent workflows make isolation essential. Superpowers' manual approach doesn't scale to many concurrent agents; Aiki's automatic approach does.

### 11. Verification Before Completion
- **Overlap: Medium** — Aiki's review system verifies work quality after completion. Superpowers' `verification-before-completion` skill ensures agents actually verify their fixes work (run tests, check output) before declaring success.
- **Threat: Medium** — Verification is a real pain point (agents claiming "done" when the fix doesn't work). Superpowers addresses this at the skill level; Aiki addresses it via structured review.
- **Opportunity: Copy** — Aiki could add a lightweight verification step to `aiki task close` — require that agents confirm tests pass or provide evidence of verification before a task can be closed. This would strengthen the task lifecycle without adding a new skill.
- **Why now:** Agent reliability is a top concern. Issues like Superpowers #614 and #613 show verification failures are common. Building verification into Aiki's task close flow would directly address this.

### 12. Multi-Platform Support (Claude Code, Cursor, Codex, OpenCode)
- **Overlap: Low** — Aiki currently integrates with Claude Code. Superpowers supports four platforms and is expanding to Kiro and Trae IDE.
- **Threat: Medium** — Superpowers' multi-platform strategy means it reaches developers regardless of their IDE/agent choice. If Aiki remains Claude Code-only, it's constrained to one ecosystem while Superpowers grows across all of them.
- **Opportunity: Ignore (for now)** — Multi-platform expansion is a strategic decision, not a feature to copy. Aiki should first nail its wedge in Claude Code before broadening. However, the `aiki` CLI's platform-agnostic design (shell commands, not IDE plugins) means expansion is architecturally feasible.
- **Why now:** The agentic coding market is fragmenting across platforms. Superpowers' multi-platform adoption (requests for Kiro #618, Trae #617) shows developers want tools that work everywhere. This is a medium-term strategic consideration, not an immediate threat.

### 13. Community Skills Marketplace
- **Overlap: Low** — Aiki doesn't have a community contribution model for workflows or skills. Superpowers has a dedicated marketplace repo (583 stars) and a community skills repo (537 stars).
- **Threat: Medium** — Community-driven content creates a flywheel: more skills → more users → more contributions. This is difficult to compete with directly and could make Superpowers the default "skill layer" for all agentic tools.
- **Opportunity: Ignore** — Aiki's value is in infrastructure (task tracking, review, provenance), not in a skill marketplace. Building a marketplace would dilute focus. Instead, ensure Aiki integrates well *with* Superpowers' skills rather than competing against them.
- **Why now:** Superpowers' marketplace is still small (583 stars) but growing. The window to position Aiki as complementary infrastructure (rather than a competing skill system) is now.

## Summary Assessment

**Overall threat level: High.** Superpowers is the most significant competitor in the structured agentic workflow space. At 71.7K stars with multi-platform support, it is defining developer expectations for how agents should work. Its skills directly overlap with Aiki's core capabilities: task decomposition, plan execution, code review, and subagent orchestration.

**Critical distinction:** Superpowers is a **behavioral layer** (skills that shape how agents act within a session). Aiki is an **infrastructure layer** (persistent state, cross-session tracking, automated review, provenance). This is Aiki's key differentiator and must be the center of all positioning.

**Key risk:** Superpowers' two-stage subagent review and code review skills overlap directly with Aiki's autonomous review wedge. If Superpowers evolves from behavioral guidelines into infrastructure (adding persistent state, automated issue tracking, or cross-session provenance), the differentiation collapses. The recent issues (#613, #614) around review quality suggest this evolution is already underway.

**Top 3 opportunities:**

1. **Two-stage review pattern** (Copy) — Adopt Superpowers' spec-compliance + code-quality review phases into `aiki review`. Aiki's task→diff provenance makes spec-compliance checking especially powerful — compare task description against actual changes automatically.

2. **Verification gates on task close** (Copy) — Require evidence of verification (tests pass, build succeeds) before `aiki task close` succeeds. Superpowers' verification-before-completion skill validates this as a real pain point; Aiki can enforce it at the infrastructure level.

3. **Complementary positioning** (Counter) — Position Aiki as the infrastructure that makes Superpowers' skills more effective: "Use Superpowers to teach agents how to work. Use Aiki to track, verify, and review the work they produce." This turns Superpowers' adoption into a distribution channel rather than a threat.

**Counter-positioning:** Superpowers' skills are *ephemeral behavioral prompts* — they shape one session but produce no persistent artifacts. Aiki's task system is *persistent infrastructure* — tasks, reviews, and provenance survive sessions, agents, and context windows. Message: "Skills tell agents what to do. Aiki makes sure it got done."
