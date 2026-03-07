# ERSC — Opportunity Scoring

## Ranked Opportunities

| Rank | Opportunity | Pain | Fit | GTM | Complexity | Score |
|------|-----------|------|-----|-----|------------|-------|
| 1    | Pre-Human Review Layer | 5 | 5 | 5 | 2 | 4.90 |
| 2    | Autonomous Workflow Intelligence | 5 | 5 | 3 | 1 | 4.60 |
| 3    | Agent-Initiated Reviews on ERSC | 4 | 5 | 5 | 3 | 4.45 |
| 4    | ERSC Forge Integration | 4 | 5 | 4 | 3 | 4.25 |
| 5    | Agent Identity & Authorship | 4 | 5 | 4 | 3 | 4.25 |
| 6    | Build/Fix Loop with ERSC CI | 4 | 4 | 4 | 2 | 4.00 |
| 7    | Patch-Based Review Intelligence | 3 | 4 | 3 | 2 | 3.45 |
| 8    | Task-to-Stacked-Change Mapping | 3 | 4 | 3 | 3 | 3.35 |
| 9    | Change-ID Native Task Tracking | 3 | 4 | 2 | 3 | 3.15 |
| 10   | ERSC API Compatibility Layer | 2 | 4 | 2 | 2 | 2.90 |

## Detailed Scoring

### 1. Pre-Human Review Layer (Score: 4.90)
- **Pain (5/5):** Human reviewers are overwhelmed with PRs. Catching issues before a human ever sees the code saves massive reviewer time and shortens review cycles. This is a widely-felt, daily pain for every team doing code review.
- **Fit (5/5):** This IS Aiki's core wedge — autonomous code review (`aiki review`). Positioning as the pre-human-review step in ERSC's review pipeline is the natural home for Aiki's existing capability.
- **GTM (5/5):** Crystal-clear value proposition: "AI agents review your code before humans do." Easy to communicate, easy to measure (issues caught before human review, review cycle time reduction). ERSC's review UX becomes the perfect distribution channel.
- **Complexity (2/5):** Requires integration with ERSC's review data model — matching their interdiff format, posting review comments via their API, and fitting into their review state machine. ERSC's APIs don't exist yet, adding uncertainty.
- **What we'd build:** An Aiki review integration that runs autonomously on new changes in ERSC, posts findings as review comments before human reviewers are notified, and creates fix tasks for issues found. Complements ERSC's human review UX rather than competing with it.
- **Source evidence:** aiki-map.md recommends "counter" on high-quality code review: "Position Aiki's autonomous review as the pre-human-review layer." research.md confirms ERSC is building "review UX that leverages jj's change-id tracking and patch-based workflows."

### 2. Autonomous Workflow Intelligence (Score: 4.60)
- **Pain (5/5):** AI agents today are reactive — they execute instructions but can't autonomously decide what to review, what to fix, or how to prioritize across a codebase. Teams need agents that can think, not just execute.
- **Fit (5/5):** This is Aiki's deepest differentiator vs. ERSC absorbing agent features. A forge can expose APIs for agents, but building autonomous decision-making, task decomposition, and build/fix loops is a fundamentally different problem. This is the moat.
- **GTM (3/5):** Harder to market than concrete features — "autonomous workflow intelligence" is abstract. Needs to be demonstrated through specific capabilities (auto-prioritization, auto-triage, self-healing builds) rather than sold as a concept.
- **Complexity (1/5):** Very hard. Requires sophisticated AI orchestration: understanding codebases holistically, making quality tradeoff decisions, decomposing ambiguous tasks into subtasks, and learning from outcomes. This is multi-year R&D territory.
- **What we'd build:** An intelligence layer that autonomously decides what needs review, prioritizes issues by impact, decomposes complex tasks into subtask sequences, and adapts its approach based on build/test feedback — all without human prompting.
- **Source evidence:** aiki-map.md identifies "Agent-friendly workflows" as the highest-threat capability: "agents don't just need a forge to push to (what ERSC provides), they need autonomous decision-making, review intelligence, task decomposition, and build/fix loops (what Aiki provides)."

### 3. Agent-Initiated Reviews on ERSC (Score: 4.45)
- **Pain (4/5):** Agents generate code changes but can't participate in the review process — they can't open reviews, respond to comments, or iterate on feedback. This creates a manual handoff bottleneck between agent work and human review.
- **Fit (5/5):** Extends Aiki's existing review capability into the forge layer. Aiki already does autonomous review; the next step is having agents be full participants in the forge's review workflow.
- **GTM (5/5):** Highly visible and compelling: "Your AI agent opens PRs, responds to review feedback, and iterates on changes." This is the dream workflow that every AI-forward team wants.
- **Complexity (3/5):** Moderate. Needs ERSC's review API (which doesn't exist yet) and a bidirectional integration: Aiki pushes review actions to ERSC, and reacts to human reviewer feedback from ERSC.
- **What we'd build:** Aiki agents that can create reviews in ERSC, post review comments, respond to human reviewer feedback, push updated changes, and manage the full review lifecycle autonomously.
- **Source evidence:** aiki-map.md on agent-friendly workflows: "ERSC's 'machines' framing suggests they see agents as forge users, not just API consumers." research.md: tagline "source control for humans and machines" signals "agent-friendly workflows."

### 4. ERSC Forge Integration (Score: 4.25)
- **Pain (4/5):** jj users today have no purpose-built forge. When ERSC launches, it will be THE forge for jj — and Aiki must be a first-class client or risk irrelevance in the jj ecosystem.
- **Fit (5/5):** Directly aligns with Aiki's "agent workflow layer on top of VCS" positioning. Being the best agent client for the dominant jj forge is the natural strategic position.
- **GTM (4/5):** Being the recommended/official agent integration for ERSC gives immediate distribution to every ERSC user. Co-marketing opportunities are strong.
- **Complexity (3/5):** Medium. Need to build API integrations against ERSC's forge APIs (which don't exist yet), handle authentication, and map Aiki's concepts to ERSC's data model.
- **What we'd build:** A deep integration layer that connects Aiki's task/review/session model to ERSC's forge — creating changes, managing reviews, syncing task state, and operating as a first-class forge client.
- **Source evidence:** aiki-map.md recommends "counter" on jj-native forge: "Aiki should position as the best autonomous agent client for ERSC's platform." research.md: "ERSC is building the collaboration/hosting layer purpose-built for jj."

### 5. Agent Identity & Authorship (Score: 4.25)
- **Pain (4/5):** Agent-authored work today is attributed to the human user who invoked the agent. There's no way to distinguish human vs. agent contributions in code history, reviews, or accountability. This matters for audit trails, trust, and team dynamics.
- **Fit (5/5):** Directly supports Aiki's "machines" side of development. If ERSC is "for humans and machines," agents need first-class identities — and Aiki is the agent layer that should own this.
- **GTM (4/5):** Differentiating and visible: "See exactly what the agent did, separate from your own work. Full traceability for every agent action." Resonates with teams concerned about AI accountability.
- **Complexity (3/5):** Moderate. Needs ERSC to support agent identity primitives (agent users, agent-authored commits/reviews). Aiki needs to propagate agent identity through its workflow.
- **What we'd build:** Agent identity propagation: Aiki agents have their own identity in ERSC, their commits/reviews are attributed to the agent (linked to the invoking human), and teams can see agent contribution metrics.
- **Source evidence:** aiki-map.md: "ERSC's 'machines' framing suggests they see agents as forge users." research.md: "'Humans and machines' framing... could mean API-first architecture, agent-friendly workflows, or CI-native collaboration patterns."

### 6. Build/Fix Loop with ERSC CI (Score: 4.00)
- **Pain (4/5):** Agents push code that breaks CI, and fixing it requires human intervention — reading CI logs, diagnosing failures, pushing fixes. Closing this loop autonomously is a major pain point for teams using AI agents.
- **Fit (4/5):** Extends Aiki's autonomous workflow into the CI dimension. Aiki already has build/fix loop concepts; connecting them to ERSC's CI infrastructure is a natural extension.
- **GTM (4/5):** Clear value: "Your agent fixes its own CI failures." Measurable (CI fix rate, time-to-green). Appeals to teams with high CI failure rates from agent-generated code.
- **Complexity (2/5):** Hard. Requires deep CI integration — parsing diverse CI output formats, understanding failure categories, generating targeted fixes, and managing the retry loop. CI systems vary widely.
- **What we'd build:** An integration where Aiki watches ERSC CI results for agent-authored changes, automatically diagnoses failures, generates fixes, and pushes updated changes — a self-healing CI loop for agent work.
- **Source evidence:** aiki-map.md strategic takeaway: Aiki should "go deeper on autonomous workflow intelligence than a forge vendor can as a side feature." Build/fix loops are a concrete example of workflow intelligence ERSC won't build.

### 7. Patch-Based Review Intelligence (Score: 3.45)
- **Pain (3/5):** AI reviews today re-review everything from scratch on each iteration. They don't understand what changed between review rounds, leading to redundant comments and noise. Moderately painful but not a top complaint.
- **Fit (4/5):** Enhances Aiki's core review capability using jj's unique strengths. Leveraging jj's patch model for smarter reviews is a natural technical evolution.
- **GTM (3/5):** Technical improvement with moderate marketability. "Smarter interdiff-aware reviews" resonates with teams doing iterative review, but it's an incremental improvement, not a new capability.
- **Complexity (2/5):** Hard. Requires understanding jj's patch/interdiff model deeply, tracking review state across change iterations, and focusing AI review only on what's new — significant technical work.
- **What we'd build:** Review intelligence that leverages jj's change-id tracking and interdiff capabilities to only review what changed since last review, carry context across iterations, and avoid redundant findings.
- **Source evidence:** research.md: ERSC is building "review UX that leverages jj's change-id tracking and patch-based workflows for better interdiff and stacked-PR review." aiki-map.md: "if ERSC adds AI review features, Aiki's advantage is deeper integration with task management, build/fix loops, and full autonomous workflow."

### 8. Task-to-Stacked-Change Mapping (Score: 3.35)
- **Pain (3/5):** Agent work produces monolithic diffs that are hard to review. Mapping subtasks to stacked changes would give reviewers granular, incremental diffs. Painful for large agent-driven changes, less so for small ones.
- **Fit (4/5):** Natural extension of Aiki's task/subtask model. jj natively supports stacked changes, and ERSC will build stacked PR UX — mapping Aiki subtasks to stacked changes is architecturally clean.
- **GTM (3/5):** Niche but valued by the jj community. "Agent work as reviewable stacked changes" is compelling for teams that care about review granularity, but stacked PRs are still a power-user workflow.
- **Complexity (3/5):** Moderate. Need to map Aiki's task model to jj's stacked change model, ensure each subtask produces a clean, isolated change, and integrate with ERSC's stacked PR UX.
- **What we'd build:** When Aiki works through subtasks, each subtask's changes map to a distinct stacked change in jj/ERSC. Reviewers see a stack of small, focused changes rather than one large diff.
- **Source evidence:** aiki-map.md recommends "copy" on stacked PRs: "Aiki should adopt stacked PR patterns for its task-to-change workflow. When an agent works through subtasks sequentially, each subtask's changes could map to a stacked change."

### 9. Change-ID Native Task Tracking (Score: 3.15)
- **Pain (3/5):** Aiki's current task tracking uses its own ID system on a dedicated jj branch. When ERSC standardizes change-id based workflows, Aiki's task model won't natively interop. Moderate pain — current system works, but won't scale to forge integration.
- **Fit (4/5):** Aligns Aiki's internal data model with jj/ERSC's change-id paradigm. Future-proofs task tracking for forge integration.
- **GTM (2/5):** Infrastructure improvement — not directly marketable. Users don't care about internal ID schemes; they care about features built on top of them.
- **Complexity (3/5):** Moderate. Requires remapping task storage to use jj change-ids as primary identifiers, maintaining backward compatibility, and ensuring the mapping is robust across rebases and amendments.
- **What we'd build:** Refactor Aiki's task system to use jj change-ids as native identifiers, enabling direct linkage between tasks and forge changes without translation layers.
- **Source evidence:** aiki-map.md recommends "copy" on API-first infrastructure: "Aiki's task-to-change mapping could leverage ERSC's change-id APIs rather than rolling its own."

### 10. ERSC API Compatibility Layer (Score: 2.90)
- **Pain (2/5):** Not painful today — ERSC has no public APIs yet. Will become painful once ERSC launches and Aiki's jj operations diverge from ERSC's standardized API surface. Preventive investment.
- **Fit (4/5):** Ensures Aiki works seamlessly with ERSC when it launches. Adopting ERSC's patterns early prevents costly migration later.
- **GTM (2/5):** Pure infrastructure work with no direct GTM impact. Enables other opportunities (forge integration, review integration) but isn't marketable on its own.
- **Complexity (2/5):** Hard. Need to rewrite Aiki's jj integration to match ERSC's API surface, which doesn't exist yet — so this requires monitoring ERSC's development and iterating as their APIs stabilize.
- **What we'd build:** An abstraction layer in Aiki's jj integration that mirrors ERSC's API patterns for change management, review workflows, and repository operations — so switching to ERSC's actual APIs is a drop-in replacement.
- **Source evidence:** aiki-map.md recommends "copy" on source control as infrastructure: "Monitor ERSC's API design closely. When they publish APIs for change management, review workflows, and repository operations, Aiki should adopt compatible patterns."
