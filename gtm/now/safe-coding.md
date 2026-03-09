# Aiki FastTrack (2-track offer)

**Date:** 2026-03-09  
**Owner:** Tu/Jordan  
**Goal:** Expand `Aiki FastTrack` into two practical GTM entry tracks for faster closes.

## Why split it
Current buyers are mixed:
- some already run AI coding bots and just need safety at scale,
- others are still human-first and need Aiki adoption before scaling bot usage.

A single offer family with two tracks reduces confusion and improves conversion speed.

---

## Aiki FastTrack — Human Track

**Position:** "Set your team up with a safe, enforceable coding workflow in Aiki."  
**Audience:** teams where most implementation is still human-driven (AI adoption is early or inconsistent).

### What is included (1–2 weeks)
- Aiki bootstrap/install in repo/workspace (tooling + baseline configs)
- Human review workflow hardening (PR gates, escalation, ownership)
- Policy + safety templates (what can be auto-merged vs requires human review)
- Weekly delivery cadence and handoff playbook
- 1–2 team training sessions for onboarding and usage discipline
- 30-day tuning pass (1 follow-up checkpoint)

### Commercials
- **Fixed project:** **$3k–$6k**
- **Optional retainer:** **$1.5k–$3k/mo** (policy tuning + support + optimization)

### Exit criteria
- Team can run a stable Aiki review flow without manual drift
- Explicit rules for when AI suggestions are allowed/blocked are in place
- Reduction in review ambiguity and repeated defects attributable to workflow gaps

---

## Aiki FastTrack — Clawbot Track

**Position:** "Make your coding bots productive at scale without turning into technical debt."  
**Audience:** teams already running custom/internal bots (or integrating public clawbots).

### What is included (1–2 weeks)
- Bot onboarding for 1–3 bots
- Hook/provenance integration and safety boundaries for each bot
- Diff/patch policy enforcement + auto-rollback/escalation rules
- Failure/fatigue signal instrumentation and operational dashboard
- Incident triage and override playbook for bot-driven churn
- 30-day optimization pass

### Commercials
- **Fixed project:** **$4k–$7k**
- **Optional retainer:** **$2k–$3k/mo**

---

## Shared add-ons for both tracks

- **General Workflows Pack (add-on):** issue/PR/PM workflows and release operations integration, **$2k–$4k**
- **Pilot-to-retainer conversion:** optional managed operations support after sprint

---

## Suggested GTM sequence (next 60 days)

1. Sell **Human Track** to widen funnel and reduce entry friction.
2. Expand to **Clawbot Track** where teams already have bots.
3. Upsell **General Workflows + retainer** as recurrence layer.

---

## Success signals
- Signed sprint within 10 outreach touches.
- 1 paid pilot started within 14 days.
- >40% of paid pilots converting to retainer by day 30.
- Clear migration path from Human → Clawbot for teams that scale AI usage.

## Clawbot TOFU Motion (bottoms-up)

The Clawbot track should be distributed where bots execute, not where enterprise buyers browse.

### 1) Distribution surfaces
- Publish Aiki FastTrack as bot-facing integration packages in ecosystems these bots already use.
- Ship installable templates/CLI entrypoints so the bot maintainer can adopt with minimal human process.
- Place examples in docs/readmes/community examples where bot builders search for tooling.

### 2) Bot-native lead capture
- The first bot command runs a risk audit and emits:
  - risk scorecard (rollback/loop/intervention indicators)
  - recommended policy baseline
  - owner-approval packet draft (problem, expected 14-day outcome, pricing band)
- If risk is above threshold, automatically draft an owner proposal artifact.

### 3) Owner escalation path
- Bot proposes directly to maintainer + owner, but payment/pilot kickoff still requires owner sign-off.
- The packet is short and measurable:
  - current failure/rework signals
  - expected reduction hypothesis
  - 2-week pilot scope
  - retainer pathway

### 4) TOFU→close funnel
- **Expose:** package listing / registry / template discovery.
- **Run:** one-click install + audit.
- **Prove:** owner packet generated from real bot metrics.
- **Escalate:** maintainer requests ownership approval.
- **Close:** Aiki FastTrack Clawbot Track pilot.

### Funnel targets (Clawbot)
- 20 bot-facing installations/templates indexed/posted in 14 days.
- 5 audit runs triggered.
- 2 owner-directed proposals sent.
- 1–2 paid pilots in 60 days.


## Presence Continuity
- Long-term persona and distribution guardrails are maintained in `../meta/tu.md`.
