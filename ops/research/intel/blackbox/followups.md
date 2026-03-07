# blackbox.ai — Execution Tasks

## positioning/quality-narrative-anti-breadth
**Source opportunity:** Quality Narrative (Anti-Breadth Positioning) — Score: 4.30
**Hypothesis:** Positioning Aiki as "the quality layer breadth-first platforms lack" will resonate with teams who have been burned by unreliable AI output (evidenced by Blackbox's 2.1/5 Trustpilot score), driving organic interest and trial signups from quality-sensitive engineering teams.
**Success metric:** Landing page + content assets published; measurable increase in inbound interest from developers citing quality concerns with existing tools (track via signup survey "why are you trying Aiki?" responses mentioning quality/reliability).
**Scope guardrails:**
- In scope: Messaging framework, landing page copy, 2-3 comparison content pieces (blog/social), updated tagline and positioning statement, case study outline template
- Out of scope: Product changes, new features, paid advertising campaigns, enterprise sales collateral
**Effort:** S

## feature/multi-agent-quality-gates
**Source opportunity:** Multi-Agent Quality Gates — Score: 4.25
**Hypothesis:** Adding review gates between multi-agent task steps will catch quality regressions that best-of-N selection misses, reducing defect rate in multi-agent codebases by >50% compared to unreviewed multi-agent output.
**Success metric:** Review gates intercept and flag issues in >30% of multi-agent task completions during internal dogfooding; zero unreviewed merges from multi-agent workflows when gates are enabled.
**Scope guardrails:**
- In scope: Review gate triggers between agent task boundaries, configurable review policies (auto-approve / human-in-loop / agent-review), workspace isolation hardening for concurrent agent writes, integration with existing `aiki review` infrastructure
- Out of scope: External agent platform integrations (Blackbox API, third-party agent SDKs), building a competitive multi-agent orchestrator, UI/dashboard for review gate management
**Effort:** L

## feature/governance-layer-remote-agents
**Source opportunity:** Governance Layer for Autonomous Remote Agents — Score: 4.25
**Hypothesis:** Providing approval gates and audit trails for autonomous remote agent PRs will address the top enterprise concern with agent-at-scale adoption — unreviewed code reaching production — and position Aiki as the required governance layer before teams can safely scale autonomous agents.
**Success metric:** End-to-end flow demonstrated: remote agent creates PR → Aiki review gate triggers → review issues surfaced → approval/rejection logged with full provenance. Target: working prototype with at least one remote agent platform (e.g., GitHub Actions-based agents).
**Scope guardrails:**
- In scope: Webhook/event-driven review triggers for incoming PRs, provenance tracking for agent-authored changes, approval/rejection workflow with audit log, integration with one remote agent platform as proof of concept
- Out of scope: SOC2 compliance certification, role-based access control, multi-tenant enterprise deployment, integration with more than one remote agent platform in this phase
**Effort:** M

## experiment/structured-dev-review-complement
**Source opportunity:** Structured Development Review Loop — Score: 4.00
**Hypothesis:** Teams using structured development workflows (like Blackbox Conductor's context → plan → implement pipeline) will adopt Aiki's review loop as the missing "verify before merge" step, reducing post-merge defects by >40% compared to unreviewed structured dev output.
**Success metric:** Run 3-5 internal projects through a Conductor-style structured workflow with and without Aiki review loop; measure defect escape rate (issues found post-merge) in both conditions. Target: statistically meaningful reduction in defect escapes.
**Scope guardrails:**
- In scope: Define a structured dev workflow template that includes Aiki review as a stage, run controlled comparison on internal projects, document results and write up findings, identify integration points where Aiki review adds most value
- Out of scope: Building Conductor-compatible plugins, formal partnership with Blackbox, external user testing, changes to core Aiki review infrastructure
**Effort:** S
