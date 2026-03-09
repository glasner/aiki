# tu/intel/new-target — {{target_name}}

## Goal
Produce a complete competitive-intel brief on {{target_name}} and convert findings into Aiki-relevant opportunities.

## Target
- Name: {{target_name}}
- URL: {{target_url}}
- GitHub: {{github_repo_or_unknown}}
- Date opened: {{date_opened}}

## Tasks

### 1) Intake + normalization
- Capture core product claim in one sentence.
- Identify primary user persona and workflow entrypoint.
- Extract pricing/packaging if publicly available.

### 2) Deep research pass
Collect evidence (URLs + short notes) from:
- Product website and docs
- Blog/changelog/release notes
- Public repo(s) and org page (if any)
- Public demos/videos/social threads where product behavior is shown

Output artifact: `ops/research/intel/{{target_slug}}/research.md`

### 3) Aiki relevance map
For each major capability, classify:
- Overlap with Aiki autonomous review wedge
- Potential threat level (low/med/high)
- Opportunity type: copy / counter / ignore
- Why now (timing signal)

Output artifact: `ops/research/intel/{{target_slug}}/aiki-map.md`

### 4) Opportunity scoring
Create a ranked list of opportunities (top 10 max) scored on:
- User pain severity (1-5)
- Strategic fit to Aiki (1-5)
- Build complexity (1-5, inverse)
- GTM leverage (1-5)

Scoring formula:
`score = 0.35*pain + 0.35*fit + 0.20*gtm + 0.10*(6-complexity)`

Output artifact: `ops/research/intel/{{target_slug}}/opportunities.md`

### 5) Convert top opportunities to execution tasks
Create exactly:
- 2 `feature/*` tasks
- 1 `experiment/*` task
- 1 `positioning/*` task

Each task must include:
- Hypothesis
- Success metric
- Scope guardrails
- Estimated effort (S/M/L)

Output artifact: `ops/research/intel/{{target_slug}}/followups.md`

## Deliverable
`ops/research/intel/{{target_slug}}/brief.md` containing:
1. What {{target_name}} is
2. What matters for Aiki
3. Top 3 recommendations this week
4. Risks if we do nothing

## Definition of done
- All artifact files above created.
- Every claim backed by a source URL.
- Final brief includes explicit copy/counter/ignore decisions.
