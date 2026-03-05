---
draft: false
---

# Competitive Intelligence Plugin — Implementation Spec

**Date**: 2026-03-05  
**Status**: Ready for implementation  
**Audience**: Claude (implementation)

## Goal
Implement reusable Aiki templates for competitor research and monitoring.

## Create these files
- `.aiki/templates/intel/new-target.md.tmpl`
- `.aiki/templates/intel/monitor-releases.md.tmpl`
- `.aiki/templates/intel/monitor-issues.md.tmpl`
- `.aiki/templates/intel/weekly-synthesis.md.tmpl`

## Output contract (per target)
Templates must write outputs under:
- `ops/research/<target-slug>/`

Required outputs:
- `research.md`
- `aiki-map.md`
- `opportunities.md`
- `followups.md`
- `brief.md`
- `release-log.md`
- `issue-log.md`
- `weekly-release-synthesis.md`
- `weekly-issue-synthesis.md`
- `weekly-decision-memo.md`

## Locked v1 decisions (non-negotiable)
- Pricing/packaging tracking (**PnP**) is out of scope.
- Release monitoring scope is only:
  - `core-capability`
  - `devex`
  - `enterprise/compliance`
- Opportunity scoring weights are fixed:
  - `0.35*pain + 0.35*fit + 0.20*gtm + 0.10*(6-complexity)`
- Weekly decision memo max length is **1 page**.
- Follow-up tasks are **human-confirmed** in v1:
  - templates propose tasks,
  - templates do not auto-create task records.

---

## Template content (implement exactly)

### 1) `new-target.md.tmpl`

```markdown
---
version: 1.3.0
type: intel
---

# Intel: New Target — {{data.target_name}}

**Role**: Competitive analyst for Aiki. Produce source-backed intelligence and recommendations. Do not make product/code changes in this task.

When all subtasks are closed, close this task with:

```bash
aiki task close {{id}} --summary "Intel complete (target={{data.target_name}}, opportunities=N, confidence=H/M/L)"
```

## Objective
Produce a decision-grade brief for `{{data.target_name}}` from `{{data.target_url}}`, ending with explicit `copy | counter | ignore` decisions.

For every `copy/counter/ignore` decision include:
- one-sentence rationale,
- confidence (`H/M/L`),
- expected downside if wrong.

## Inputs
- target_name: `{{data.target_name}}`
- target_url: `{{data.target_url}}`
- target_slug: `{{data.target_slug}}`
- github_repo_or_unknown: `{{data.github_repo_or_unknown}}`
- date_opened: `{{data.date_opened}}`

## Output Files
Write to `ops/research/{{data.target_slug}}/`:
- `research.md`
- `aiki-map.md`
- `opportunities.md`
- `followups.md`
- `brief.md`

# Subtasks

## Normalize target profile
---
slug: normalize
---

Create concise target profile with:
- one-sentence product claim,
- primary persona,
- workflow entrypoint,
- pricing model (if public),
- canonical repo/org (if discoverable).

Write to `research.md` section: `## Target Profile`.

Close with:
```bash
aiki task close {{id}} --summary "Profile normalized: {{data.target_name}}"
```

## Gather evidence
---
slug: evidence
needs-context: subtasks.normalize
---

Collect evidence from:
- product site + docs,
- changelog/release notes,
- GitHub repos/issues/discussions,
- demos/technical writeups.

Rules:
- every non-obvious claim includes source URL,
- first-party + implementation evidence preferred,
- separate **facts** from **interpretation**.

Write to `research.md` sections:
- `## Evidence`
- `## Facts vs Interpretation`

Close with:
```bash
aiki task close {{id}} --summary "Evidence captured (sources=N, confidence=H/M/L)"
```

## Map to Aiki
---
slug: map
needs-context: subtasks.evidence
---

For each major capability, classify:
- overlap with Aiki wedge (autonomous review): low/med/high,
- threat level: low/med/high,
- decision: copy/counter/ignore,
- timing signal: why now.

Write to `aiki-map.md` as a table.

Close with:
```bash
aiki task close {{id}} --summary "Aiki map complete (capabilities=N)"
```

## Score opportunities
---
slug: score
needs-context: subtasks.map
---

Generate up to 10 opportunities. Score each:
- user pain severity (1-5)
- strategic fit to Aiki (1-5)
- build complexity (1-5, inverse)
- GTM leverage (1-5)

Formula:
`score = 0.35*pain + 0.35*fit + 0.20*gtm + 0.10*(6-complexity)`

Write ranked table to `opportunities.md` with rationale and confidence.

Close with:
```bash
aiki task close {{id}} --summary "Opportunities scored (count=N, top_score=X)"
```

## Produce followups and brief
---
slug: outputs
needs-context: subtasks.score
---

Create in `followups.md`:
- 2 feature proposals,
- 1 experiment proposal,
- 1 positioning proposal.

Each proposal must include:
- hypothesis,
- success metric,
- effort (S/M/L),
- scope guardrails,
- owner suggestion.

Write `brief.md` with exactly:
1. What this product is
2. What matters for Aiki
3. Top 3 recommendations this week
4. Risk if we do nothing

Do not auto-create tasks in v1; write proposals only in `followups.md`.

Close with:
```bash
aiki task close {{id}} --summary "Outputs complete (brief + followups written)"
```
```

---

### 2) `monitor-releases.md.tmpl`

```markdown
---
version: 1.3.0
type: intel
---

# Intel Monitor: Releases — {{data.target_name}}

**Role**: Monitor release movement and propose action when warranted. Do not rewrite historical logs beyond additive corrections.

When all subtasks are closed, close this task with:

```bash
aiki task close {{id}} --summary "Release monitor run complete (deltas=N, act-now=M)"
```

## Objective
Track release/changelog deltas and surface what Aiki should do now.

## Inputs
- target_name: `{{data.target_name}}`
- target_slug: `{{data.target_slug}}`
- cadence: `{{data.cadence}}` (default 3x/week)

## Output Files
- append: `ops/research/{{data.target_slug}}/release-log.md`
- update: `ops/research/{{data.target_slug}}/weekly-release-synthesis.md`

# Subtasks

## Detect release deltas
---
slug: detect
---

Find all new releases/tags/changelog entries since prior run.
For each delta capture:
- date/time,
- source URL,
- summary (<= 2 lines).

Close with:
```bash
aiki task close {{id}} --summary "Release deltas detected (count=N)"
```

## Assess impact
---
slug: impact
needs-context: subtasks.detect
---

Tag each delta:
- `core-capability`
- `devex`
- `enterprise/compliance`

Assign impact label:
- `none`
- `watch`
- `act-now`

Close with:
```bash
aiki task close {{id}} --summary "Release impact assessed (act-now=M)"
```

## Write logs + recommendation
---
slug: output
needs-context: subtasks.impact
---

Append run entry to `release-log.md`.
If `act-now`, include one concrete proposed Aiki task.
(Proposal only in v1; do not auto-create task records.)

Update `weekly-release-synthesis.md` with:
- top directional shifts,
- change vs prior week,
- max 3 recommended actions.

Close with:
```bash
aiki task close {{id}} --summary "Release logs updated (deltas=N, act-now=M, confidence=H/M/L)"
```
```

---

### 3) `monitor-issues.md.tmpl`

```markdown
---
version: 1.3.0
type: intel
---

# Intel Monitor: Issues — {{data.target_name}}

**Role**: Detect recurring pain and actionable adoption signals. Do not dump raw issue feeds.

When all subtasks are closed, close this task with:

```bash
aiki task close {{id}} --summary "Issue monitor run complete (clusters=N, triggers=M)"
```

## Objective
Track recurring user pain, feature demand, and adoption blockers from issue activity.

## Inputs
- target_name: `{{data.target_name}}`
- target_slug: `{{data.target_slug}}`
- github_repo_or_unknown: `{{data.github_repo_or_unknown}}`

## Output Files
- append: `ops/research/{{data.target_slug}}/issue-log.md`
- update: `ops/research/{{data.target_slug}}/weekly-issue-synthesis.md`

# Subtasks

## Establish source of truth
---
slug: source
---

If repo unknown, discover canonical repo from official links and record it in run entry.

Close with:
```bash
aiki task close {{id}} --summary "Issue source-of-truth established"
```

## Cluster issue signals
---
slug: cluster
needs-context: subtasks.source
---

Cluster new/updated issues into:
- bug pain,
- feature requests,
- adoption friction,
- enterprise blockers.

For each cluster include:
- representative links,
- frequency/velocity,
- severity (low/med/high),
- Aiki relevance (low/med/high).

Close with:
```bash
aiki task close {{id}} --summary "Issue clusters generated (clusters=N)"
```

## Apply trigger rules
---
slug: trigger
needs-context: subtasks.cluster
---

Trigger action when any are true:
- same pain appears in >=3 issues within 14 days,
- high-severity blocker aligns with Aiki wedge,
- competitor ships capability Aiki currently lacks.

When triggered, propose:
- problem statement,
- why now,
- copy/counter/ignore,
- validation metric.

Close with:
```bash
aiki task close {{id}} --summary "Trigger evaluation complete (triggers=M)"
```

## Write logs + synthesis
---
slug: output
needs-context: subtasks.trigger
---

Append to `issue-log.md`.
Update `weekly-issue-synthesis.md` with:
- top 5 signals,
- risks to Aiki position,
- 1-3 backlog recommendations.

Recommendations are proposals only in v1; task creation remains human-confirmed.

Close with:
```bash
aiki task close {{id}} --summary "Issue logs updated (clusters=N, triggers=M, confidence=H/M/L)"
```
```

---

### 4) `weekly-synthesis.md.tmpl`

```markdown
---
version: 1.3.0
type: intel
---

# Intel Weekly Synthesis — {{data.target_name}}

**Role**: Strategy synthesizer. Merge weekly signals into decisions and concrete next work.

When all subtasks are closed, close this task with:

```bash
aiki task close {{id}} --summary "Weekly decision memo complete for {{data.target_name}}"
```

## Objective
Merge release + issue signals into one decision memo for Aiki.

## Inputs
- `ops/research/{{data.target_slug}}/weekly-release-synthesis.md`
- `ops/research/{{data.target_slug}}/weekly-issue-synthesis.md`
- optional: `brief.md`, `opportunities.md`

## Output File
- `ops/research/{{data.target_slug}}/weekly-decision-memo.md`

# Subtasks

## Aggregate material deltas
---
slug: aggregate
---

Extract only material deltas from weekly syntheses. No raw feed dump.

Close with:
```bash
aiki task close {{id}} --summary "Weekly material deltas aggregated"
```

## Produce decision memo
---
slug: memo
needs-context: subtasks.aggregate
---

Write `weekly-decision-memo.md` with:
1. What changed this week (max 10 bullets)
2. Ranked threats to Aiki
3. Ranked opportunities for Aiki
4. Decisions needed now (`copy/counter/ignore`)
5. Recommended next Aiki tasks (max 5)

Hard constraints:
- memo length must be <= 1 page,
- evidence-linked claims,
- confidence notes,
- concrete owners and next actions.

Close with:
```bash
aiki task close {{id}} --summary "Weekly memo published"
```
```

---

## Validation target (first run)
- `target_name`: TasteMatter
- `target_url`: https://tastematter.dev/
- `target_slug`: tastematter
- `github_repo_or_unknown`: unknown (discover during run)
- `date_opened`: 2026-03-05
- `cadence`: 3x/week

## Acceptance criteria
- All four template files are created at `.aiki/templates/intel/`.
- Template content matches this spec.
- All required output paths are referenced correctly.
- v1 locked decisions are enforced in template behavior.

## Execute
```bash
aiki plan ops/now/competitive-intellegence.md
aiki build --fix ops/now/competitive-intellegence.md
```
