# Clawbot Distribution Playbook (2026-03-09)

## Objective
Build **Aiki FastTrack Clawbot Track** as a bottoms-up, maintainer-triggered distribution motion that creates owner-ready evidence first, then converts to paid pilots.

## Distribution thesis
- **Do not sell first.** Sell evidence.
- **Go where claws already execute:** install surfaces, developer ecosystems, and maintainer communities.
- **Let the bot propose to owner.** The maintainer gets the tool; the owner gets the risk/ROI decision packet.

## Channel stack (what to build first)

### 1) ClawHub (primary install surface)
**What to publish:**
- `aiki-fasttrack-clawbot` (main track)
- `aiki-fasttrack-risk-audit` (auxiliary audit starter)
- versioned changelog + `latest` tag

**Entry points required per asset:**
- `aiki-fasttrack.yaml` install manifest
- `owner-packet.md` template
- `risk-score.schema.json`
- `quickstart.md` with 5-step install and first output

**Success signal:**
- Installed assets + first audit run logs generated.

---

### 2) GitHub / skill-readme surfacing (trust + context)
**What to include in each README/landing artifact:**
1. Install command
2. What signal it detects (e.g., rollback, loop depth, manual interventions)
3. One sample output JSON
4. “Owner packet” example with objective + 14-day scope
5. Security scope (what it does/doesn’t do)

**Format:** command-first, no brochure copy.

**Success signal:**
- Maintainer stars/watchers/comments on distribution package.
- External issue/discussion references to output schema.

---

### 3) Community showcase loop (proof velocity)
**Surfaces:**
- OpenClaw showcase page / community posts
- X posts with one command + one artifact screenshot/sample output
- Discord #showcase mention with concise maintainer-facing summary

**Posting cadence:**
- 2 posts/week minimum in the first 2 weeks.

**Post format:**
- Problem signal
- Command and sample output
- “If you maintain this bot, your owner gets this packet format.”

**Success signal:**
- Maintainer responses leading to packet requests.
- Increase in owner conversion-ready packets.

---

### 4) Commercial layer (ClawMart/offer surface)
Use this only once evidence loop is proven.

**What enters paid space:**
- premium support package
- onboarding + fine-tuning sprint
- additional integrations

**Keep free-distribution artifacts decoupled from pricing.**

**Success signal:**
- Maintainer packet accepted by owner and converted into pilot kickoff.

---

## 14-day execution plan

### Days 1–3 (Build)
- Finalize and publish 1st Clawbot package on ClawHub.
- Publish `risk-score.schema.json` + owner packet template.
- Publish 1 maintainer-facing one-pager in `gtm/meta` linking to `aiki-fasttrack.yaml` and quickstart.

### Days 4–7 (Seed)
- Push two maintainer-first outreach messages across 5 target repositories/channels.
- Publish 1 public example: before/after risk score from internal or partner run.
- Capture at least 3 maintainer feedback data points.

### Days 8–10 (Evidence)
- Produce 2 owner packets generated from real test runs (internal accepted if no external yet).
- Publish maintainer FAQ (safety, scope, failure fallback).
- Add 1 template update to reduce setup friction.

### Days 11–14 (Convert)
- Send first batch of owner packets to interested maintainers’ leads.
- Convert top 1–2 responses into pilot proposals with clear close date.
- Measure conversion rate and blockers; adjust messaging.

---

## Channel-by-channel playbook

### ClawHub onboarding copy
```text
This skill auto-generates a clawbot risk profile and owner packet from recent
interventions.
Use this when your bot already generates patches too often without clear escalation.

Install:
  clawhub install ai/aiki-fasttrack-clawbot
Run:
  aiki-fasttrack audit --repo . --owner-packet
```

### GitHub maintainer outreach
```text
Hi — I noticed your bot is handling [domain].
I built a minimal clawbot-ready FastTrack pack that generates:
- risk score,
- intervention summary,
- owner-ready packet for your team lead.
No new agent infra needed, no pitch deck.
Want the one-command install + sample output?
```

### Owner packet trigger
```text
Current signal: repeated risky auto-fix loops and manual re-reads are high.
Expected 14-day outcome: reduced manual interventions + explicit escalation rubric.
Scope: [files/processes], pilot window: 14 days.
```

### Community post template
- “Maintainer-triggered safety rollout in 3 commands”
- “One maintainer-run output example”
- “How an owner packet is produced automatically”

---

## 30-day KPI dashboard
- **Installs:** 15 target
- **Risk audits executed:** 10 target
- **Owner packets produced:** 4 target
- **Maintainer→owner handoffs:** 2 target
- **Pilot conversions:** 1 target

## KPIs that predict survivability (run weekly)
- Time from first maintainer contact → first packet
- Packet-to-pilot conversion rate
- % of packets with owner sign-off within 7 days
- Packet quality score (clarity, measurable outcomes, scope fit)

## Pushback controls
- **Bad lead risk:** only engage where bot has existing usage data.
- **Noise risk:** no long brand narrative; only measurable outputs.
- **Trust risk:** include explicit boundaries and safety guardrails in every artifact.
- **Scope risk:** cap initial pilot to 1–2 bots/repo + 1 workflow.

## Linkages
- This plan operationalizes `gtm/meta/tu.md` presence rules.
- This is the Clawbot channel for **Aiki FastTrack** distribution; Human Track follows separate motion in
  `/gtm/now/safe-coding.md`.