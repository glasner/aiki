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

## Running a task from a template (required workflow)

Use `aiki task run --template ...` when you want one-shot create + execute.

### 1) Pick a template
Use namespaced template IDs (example: `tu/intel/new-target`).

Template docs:
- [Task template composition and subtask spawning](tasks/templates/spawn.md)
- [Task kinds](tasks/kinds.md)
- [Task links](tasks/links.md)
- [Template overrides and resolution order](customizing-defaults.md)

### 2) Run sync (wait for completion)
```bash
aiki task run --template tu/intel/new-target \
  --data target_slug=acme \
  --data target_name="Acme" \
  --data target_url="https://acme.dev"
```

### 3) Run async (return immediately)
```bash
aiki task run --template tu/intel/new-target \
  --data target_slug=acme \
  --data target_name="Acme" \
  --data target_url="https://acme.dev" \
  --async
```

### 4) When to pass `--agent`
If template output has no assignee, pass an explicit agent:
```bash
aiki task run --template tu/intel/new-target \
  --data target_slug=acme \
  --data target_name="Acme" \
  --data target_url="https://acme.dev" \
  --agent claude-code \
  --async
```

### 5) Expected output patterns
- Sync: `Added ...`, `Spawning ...`, `Task run complete`, `## Run Completed`
- Async: `Added ...`, `## Run Started`, then returns immediately

### 6) Common errors to avoid
- Missing required `--data` variables.
- Passing `--data` without `--template`.
- Passing both task ID and `--template` at the same time.
- Using stale template IDs (use namespaced IDs like `tu/intel/...`).

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
