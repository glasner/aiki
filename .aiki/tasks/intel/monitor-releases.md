# tu/intel/monitor-releases — {{target_name}}

## Goal
Track product and release movement from {{target_name}} and surface implications for Aiki.

## Cadence
- Run {{cadence}}.
- If high activity, escalate to daily.

## Sources
- Official release/changelog endpoints
- GitHub releases/tags (if repo exists)
- Product update posts/newsletters

## Per-run tasks
1. Detect new releases since last run.
2. Summarize changes in <= 8 bullets.
3. Tag each change:
   - `core-capability`
   - `devex`
   - `enterprise/compliance`
   - `pricing/packaging`
4. Mark Aiki impact: `none | watch | act-now`.
5. If `act-now`, propose one concrete Aiki response task.

## Output
Append to:
- `ops/now/monitor/{{target_slug}}/release-log.md`

Entry format:
- Date
- Source links
- Delta summary
- Aiki impact
- Proposed action

## Weekly synthesis hook
Every Friday, produce/update:
- `ops/now/monitor/{{target_slug}}/weekly-release-synthesis.md`
with:
- Top 3 directional shifts
- What changed vs prior week
- Recommended Aiki response (max 3 items)

## Definition of done
- New release deltas captured with sources.
- Clear impact label provided for each run.
