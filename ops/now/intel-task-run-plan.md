# Intel Intake & Execution Plan

## Goal
Make URL-based Intel intake run as a one-command `aiki task` flow (no manual `build --fix` needed).

## Current constraint
`aiki task run` **does not accept `--template`** (it only accepts task IDs).

Reference:
- `aiki task run --help`

## New workflow (recommended)
Use a one-liner that combines creation + run:

```bash
# Intake + execute in one command:
aiki task add --template intel/new-target \
  --data target_name="<NAME>" \
  --data target_url="<URL>" \
  --data target_slug="<slug>" \
  --data github_repo_or_unknown="<repo-or-unknown>" \
  --data date_opened="$(date +%F)" -o id | \
  xargs -I{} sh -c 'aiki task run {}'
```

## Standard data fields (required by template)
- `target_name` — Human name of target
- `target_url` — Source URL
- `target_slug` — folder slug under `ops/now/intel/<slug>/`
- `github_repo_or_unknown` — repo URL or `unknown`
- `date_opened` — `YYYY-MM-DD`

## Recommended shell helper
Create an operator-side wrapper command if you want it fully ergonomic:

```bash
# usage: aiki-intel <name> <url> [repo]
aiki-intel() {
  local name="$1" url="$2" repo="${3:-unknown}";
  local slug=$(echo "$name" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-//;s/-$//' );
  cd /Users/glasner/code/aiki;
  aiki task add --template intel/new-target \
    --data target_name="$name" \
    --data target_url="$url" \
    --data target_slug="$slug" \
    --data github_repo_or_unknown="$repo" \
    --data date_opened="$(date +%F)" -o id | \
    xargs -I{} sh -c 'aiki task run {}'
}
```

## Optional quality guard
After launching, verify artifacts in:
- `ops/now/intel/<slug>/research.md`
- `ops/now/intel/<slug>/aiki-map.md`
- `ops/now/intel/<slug>/opportunities.md`
- `ops/now/intel/<slug>/followups.md`
- `ops/now/intel/<slug>/brief.md`

## Done criteria
- This flow creates the task and starts execution in one step.
- No `aiki build --fix` needed for URL intake.
