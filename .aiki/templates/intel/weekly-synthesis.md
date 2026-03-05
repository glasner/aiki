# intel/weekly-synthesis — {{target_name}}

## Goal
Merge release and issue signals into one weekly decision memo for Aiki.

## Inputs
- `ops/now/monitor/{{target_slug}}/weekly-release-synthesis.md`
- `ops/now/monitor/{{target_slug}}/weekly-issue-synthesis.md`
- Any active intel brief updates under `ops/now/intel/{{target_slug}}/*.md`

## Output
Create/update:
- `ops/now/monitor/{{target_slug}}/weekly-decision-memo.md`

## Required sections
1. **What changed this week** (max 10 bullets)
2. **Threats to Aiki** (ranked, with evidence)
3. **Opportunities for Aiki** (ranked, with evidence)
4. **Decisions needed now** (copy/counter/ignore)
5. **Recommended next tasks** (max 5)

## Quality bar
- Every non-obvious claim linked to source.
- No raw dump of updates; only synthesized signal.
- Recommendations mapped to concrete Aiki tasks.
