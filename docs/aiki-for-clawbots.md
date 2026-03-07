# Aiki for Claw Bots

_Status: internal dogfood guide (v0.1)_

This guide is for claw agents using Aiki to automate recurring work.

## Why this exists

We want two outcomes:
1. Help agents automate common tasks **reliably** right now.
2. Build a concrete, evidence-backed base for later public communication.

This is not a hype doc. It is an operator playbook.

## What Aiki is (agent framing)

Aiki is an automation layer for repeatable workflows.

Use it when work is:
- predictable,
- triggerable (time/event/state), and
- better when executed consistently with audit logs.

For agents, Aiki turns:
- “I should remember to do this later”
into
- “This runs automatically with clear outputs and escalation rules.”

## Use Aiki vs. not

### Use Aiki when
- The task repeats on a schedule.
- Inputs/outputs are clear.
- The workflow can be expressed as deterministic steps.
- Consistency matters more than improvisation.

### Do not use Aiki when
- The task is one-off exploration.
- Requirements are changing rapidly.
- Human judgment is required at every step.
- Automation side-effects are risky without review gates.

## High-value automation patterns

### 1) Daily operational sweep
**Use case:** summarize only actionable deltas from routine checks.
- Input: inbox/calendar/notifications/state checks
- Output: concise “what needs action now” list

### 2) Docs freshness monitor
**Use case:** detect stale docs and open/update remediation tasks.
- Input: docs tree + change history
- Output: stale-doc report with file-level suggestions

### 3) Release/watch digest
**Use case:** weekly summary of upstream changes and impact.
- Input: release notes/issues/changelogs
- Output: high-signal digest + recommended responses

### 4) Task lifecycle hygiene
**Use case:** detect stalled tasks and trigger nudges/escalation.
- Input: task metadata + last-updated timestamps
- Output: “stuck tasks” list with owners and next action

### 5) Environment health checks
**Use case:** run non-destructive diagnostics on schedule.
- Input: health commands / probes
- Output: pass/fail summary with drift/regression notes

### 6) PR/review prep
**Use case:** generate reviewer-ready context.
- Input: changed files, CI status, risk flags
- Output: review brief (risks, hotspots, validation status)

### 7) Knowledge capture loop
**Use case:** promote validated daily learning into durable memory.
- Input: daily notes
- Output: durable memory updates + superseded assumptions

### 8) Triage routing
**Use case:** classify incoming items and route correctly.
- Input: message/event stream
- Output: urgent/actionable/info routing with rationale

## Guardrails (non-negotiable)

- Default to read-only unless explicitly authorized.
- No external side-effects (posting, sending, deleting) without policy + approval path.
- Human-in-the-loop gates for sensitive/irreversible actions.
- Bounded retries and explicit escalation (no silent infinite loops).
- Define success/failure criteria before automation starts.
- Log trigger, action, result, and handoff context.
- Use least-privilege access for every workflow.

## Rollout plan

### Phase 1 — Internal dogfood
- Pick 2–3 repetitive workflows per agent.
- Measure: time saved, error rate, escalation quality, trust.
- Weekly keep/kill/tune decisions per automation.

### Phase 2 — Internal standardization
- Publish canonical patterns + guardrails.
- Create reusable templates.
- Define ownership and escalation contracts.

### Phase 3 — External communication
- Publish proven playbooks with real outcomes.
- Lead with specific before/after results.
- Include limits/failures to keep claims credible.

## First-day minimum useful path

- [ ] Pick one repetitive task with clear output.
- [ ] Define trigger and stop condition.
- [ ] Define success criteria + escalation path.
- [ ] Run read-only first.
- [ ] Validate quality on 2–3 runs.
- [ ] Add timeout/retry guardrails.
- [ ] Document the pattern in repo.
- [ ] Promote one durable lesson to memory.

## Success criteria for this guide

This guide is working if agents can:
1. choose one useful automation in <15 minutes,
2. run safely with clear escalation,
3. produce measurable value within one day.
