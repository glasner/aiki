# ERSC — Aiki Relevance Map

## Capability Matrix

| Capability | Overlap | Threat | Opportunity | Why Now |
|-----------|---------|--------|-------------|---------|
| Jujutsu-native forge | Medium — both build on jj, but different layers (forge vs workflow) | med | counter | JJ has 26k+ stars with no forge; ERSC will define the collaboration model Aiki must integrate with |
| High-quality code review | High — Aiki's core wedge is autonomous review; ERSC builds human review UX on same substrate | high | counter | ERSC lists "high-quality code review" as a core pillar; if they add AI review features, direct competition |
| Codebase evolution understanding | Low — Aiki tracks task-level changes, not codebase-level history/analytics | low | ignore | No immediate signal; different problem space |
| Source control as infrastructure (API-first) | Medium — both treat jj as infrastructure, but for different purposes (platform vs workflow) | med | copy | ERSC is defining the API surface now; Aiki's jj integration should align with whatever ERSC standardizes |
| Stacked PRs | Low — Aiki doesn't handle stacked PRs directly, but task/subtask model maps naturally to stacked changes | low | copy | JJ natively supports stacked changes; ERSC will build first-class stacked PR UX that Aiki could leverage |
| Virtualized remote backend (jj-yak) | Low — Aiki uses jj locally; jj-yak is about remote/centralized repository access | low | ignore | Experimental project, not yet mature |
| Agent-friendly workflows ("humans and machines") | High — Aiki IS the machine/agent layer for development; ERSC's tagline signals intent to serve same users | high | counter | Tagline "source control for humans and machines" explicitly positions for AI agent workflows |

## Detailed Analysis

### Jujutsu-native forge
- **What ERSC does:** Builds the missing collaboration/hosting layer for jj — analogous to GitHub for Git. Code hosting, pull requests, team collaboration, all purpose-built for jj's model (change-ids, patch-based workflows). ([research.md: "collaboration/hosting layer purpose-built for jj, analogous to what GitHub is for Git"])
- **What Aiki does:** Uses jj for workspace isolation (isolated JJ workspaces per agent session) and change tracking, but provides no forge/hosting/collaboration layer. Aiki is a CLI tool that sits on top of whatever forge exists.
- **Gap/overlap:** Complementary layers. ERSC is the platform (forge); Aiki is the automation layer (agent workflows). However, if ERSC controls the forge, they control the platform Aiki depends on — risk of platform dependency.
- **Recommendation:** counter
- **Rationale:** Aiki should position as the best autonomous agent client for ERSC's platform, not try to build forge features. Ensure deep integration with ERSC's forge APIs once available. The risk is ERSC bundling agent workflow features into the forge itself — counter by being the specialized, best-in-class agent orchestration layer that ERSC can't replicate as a side feature.

### High-quality code review
- **What ERSC does:** Building review UX leveraging jj's change-id tracking and patch-based workflows for better interdiff and stacked-PR review. Focused on human reviewers with better tooling. ([research.md: "review UX that leverages jj's change-id tracking and patch-based workflows for better interdiff and stacked-PR review"])
- **What Aiki does:** Autonomous code review (`aiki review`) — AI agents review code changes, identify issues, and create follow-up fix tasks. The review is performed by agents, not humans.
- **Gap/overlap:** Both solve code review, but for different users (humans vs agents). The overlap is high in concept but the execution is different. The threat materializes if ERSC adds AI-powered review suggestions or automated review features to their forge.
- **Recommendation:** counter
- **Rationale:** Position Aiki's autonomous review as the pre-human-review layer — agents catch issues before human reviewers see the code. Aiki reviews are complementary to ERSC's human review UX, not competing. If ERSC adds AI review features, Aiki's advantage is deeper integration with task management, build/fix loops, and full autonomous workflow (not just review comments).

### Codebase evolution understanding
- **What ERSC does:** History visualization or analytics — understanding how codebases change over time. ([research.md: "help users understand how codebases change over time, suggesting history visualization or analytics features"])
- **What Aiki does:** Nothing in this space. Aiki tracks task completion and review outcomes but doesn't visualize or analyze codebase evolution.
- **Gap/overlap:** Minimal. Different problem domains.
- **Recommendation:** ignore
- **Rationale:** Not relevant to Aiki's autonomous review and task management wedge. Codebase analytics is a product feature for human developers, not for agent workflows.

### Source control as infrastructure (API-first)
- **What ERSC does:** Positioning SCM as a foundational platform layer with API-first design, integrations, and treating SCM as a service for both developers and CI/machine consumers. ([research.md: "API-first design, integrations, and treating SCM as a service for both human developers and CI/machine consumers"])
- **What Aiki does:** Uses jj as local infrastructure for workspace isolation and change tracking. Aiki's `aiki task` system stores task state on a dedicated jj branch (`aiki/tasks`), treating jj as a persistence layer.
- **Gap/overlap:** Both treat jj as infrastructure, but at different scales. ERSC builds the networked platform; Aiki builds the local workflow. If ERSC defines standard APIs for jj operations, Aiki should adopt them.
- **Recommendation:** copy
- **Rationale:** Monitor ERSC's API design closely. When they publish APIs for change management, review workflows, and repository operations, Aiki should adopt compatible patterns. This future-proofs Aiki's jj integration and enables seamless Aiki-on-ERSC workflows. Specifically, Aiki's task-to-change mapping could leverage ERSC's change-id APIs rather than rolling its own.

### Stacked PRs
- **What ERSC does:** Likely building deeply integrated stacked PR support leveraging jj's native stacked change model. ([research.md: "stacked PRs, deeply integrated developer tooling" inferred from team background and jj's natural support])
- **What Aiki does:** Aiki's task/subtask model and session isolation create natural stacked change patterns, but Aiki doesn't explicitly model or manage stacked PRs as a workflow concept.
- **Gap/overlap:** Low direct overlap. Aiki's subtask model could map to stacked PRs if the forge supports them natively.
- **Recommendation:** copy
- **Rationale:** Aiki should adopt stacked PR patterns for its task-to-change workflow. When an agent works through subtasks sequentially, each subtask's changes could map to a stacked change in ERSC's model. This would give Aiki users granular, reviewable change stacks rather than monolithic diffs. Watch ERSC's stacked PR implementation and mirror the workflow in `aiki task` semantics.

### Virtualized remote backend (jj-yak)
- **What ERSC does:** gRPC + NFS virtualized remote backend for jj — centralizes repository storage with a local NFS mount and caching layer. Experimental. ([research.md: "(1) a CLI communicating via gRPC, (2) a daemon implementing an NFS server + caching layer, and (3) a centralized backend storing all commits"])
- **What Aiki does:** Uses jj locally with standard file-based storage. Workspace isolation is local JJ workspaces.
- **Gap/overlap:** Different layer entirely. jj-yak is network/storage infrastructure; Aiki is workflow orchestration.
- **Recommendation:** ignore
- **Rationale:** jj-yak is experimental infrastructure plumbing. If it becomes the standard jj backend, Aiki should work transparently on top of it (as it should with any jj backend). No action needed now.

### Agent-friendly workflows ("humans and machines")
- **What ERSC does:** Tagline "source control for humans and machines" signals explicit intent to support AI agents and automation as first-class users. Suggests API-first architecture, agent-friendly collaboration patterns, or CI-native workflows. ([research.md: "'Humans and machines' framing is notable... could mean API-first architecture, agent-friendly workflows, or CI-native collaboration patterns"])
- **What Aiki does:** This IS Aiki — orchestrating autonomous AI agent development workflows (task tracking, code review, build/fix loops, session isolation). Aiki is the "machine" side of development collaboration.
- **Gap/overlap:** Very high. If ERSC builds agent-native collaboration features into the forge (agent identities, agent-initiated reviews, automated change management), they could absorb significant parts of Aiki's value proposition into the platform layer.
- **Recommendation:** counter
- **Rationale:** This is the highest-threat capability. ERSC's "machines" framing suggests they see agents as forge users, not just API consumers. Aiki must differentiate by being the orchestration and intelligence layer — agents don't just need a forge to push to (what ERSC provides), they need autonomous decision-making, review intelligence, task decomposition, and build/fix loops (what Aiki provides). Counter by being deeper than ERSC can go at the workflow layer while being the best agent client for ERSC's platform.

## Summary
- Total capabilities analyzed: 7
- High threat: 2 (High-quality code review, Agent-friendly workflows)
- Copy opportunities: 2 (Source control as infrastructure / API-first, Stacked PRs)
- Counter opportunities: 3 (Jujutsu-native forge, High-quality code review, Agent-friendly workflows)
- Ignore: 2 (Codebase evolution understanding, Virtualized remote backend)

**Strategic takeaway:** ERSC is building the *platform* layer for jj; Aiki builds the *agent workflow* layer on top of jj. These are complementary until ERSC moves into agent-native features — then they compete directly on Aiki's core wedge. The counter strategy is: be the best agent client for ERSC's forge, go deeper on autonomous workflow intelligence than a forge vendor can as a side feature, and adopt ERSC's API patterns (copy) to ensure seamless integration.
