# tu/intel/monitor-issues — {{target_name}}

## Goal
Track recurring user pain, feature demand, and enterprise blockers from {{target_name}} issue activity.

## Cadence
- Run daily.
- Weekly synthesis every Friday.

## Source discovery
If repo is unknown:
1. Discover official GitHub org/repo from product/docs links.
2. Record canonical repo in `ops/now/monitor/{{target_slug}}/source-of-truth.md`.

## Per-run issue scan
Capture newly opened/updated issues and cluster into:
- Bug pain clusters (repeat failures)
- Feature request clusters
- Adoption friction (setup, auth, integrations)
- Enterprise blockers (policy, auditability, permissions, compliance)

For each cluster include:
- Representative issue links
- Frequency/velocity signal
- Severity: low/med/high
- Aiki relevance: low/med/high

## Output
Append run output to:
- `ops/now/monitor/{{target_slug}}/issue-log.md`

## Trigger rules
Create/update an Aiki action item when any condition is met:
- Same pain appears in >= 3 issues in 14 days
- A high-severity blocker aligns with Aiki wedge
- Competitor ships requested capability Aiki lacks

Action item format:
- Problem statement
- Why now
- Proposed Aiki response (copy/counter/ignore)
- Metric to validate

## Weekly synthesis
Update:
- `ops/now/monitor/{{target_slug}}/weekly-issue-synthesis.md`

Must include:
- Top 5 issue signals
- Emerging risks to Aiki position
- 1-3 concrete backlog recommendations

## Definition of done
- New issue signals clustered (not raw dump).
- Clear copy/counter/ignore recommendation produced when trigger rules fire.
