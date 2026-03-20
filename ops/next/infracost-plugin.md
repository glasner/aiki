# Plan: Build `aiki/infracost` plugin

## Goal
Create an Aiki plugin that brings Infracost-powered cost awareness into day-to-day code flows (mainly Terraform/Terragrunt/CloudFormation repos) while staying read-only by default and avoiding blocking edits unless policy violations are explicit blockers.

## Why now
I reviewed:
- `https://infracost.github.io/agent-skills/` (product and value framing)
- `https://raw.githubusercontent.com/infracost/agent-skills/main/plugins/infracost/skills/{scan,iac-generation,price-lookup,install}.md`
- Aiki plugin docs (`cli/docs/creating-plugins.md`, `cli/docs/customizing-defaults.md`)

Key takeaways:
- Infracost CLI is `infracost-preview`.
- Core actions for automation are:
  - `infracost-preview policies`
  - `infracost-preview scan <path>` (write JSON to stdout)
  - `infracost-preview inspect --file <json> ...`
  - `infracost-preview price`
- `iac-generation` and `scan` skill UX strongly emphasize:
  - policy-first workflow before writing IaC
  - non-blocking, read-only cost checks on saved code
  - budget/tagging/compliance guidance
- Plugin repo model in Aiki is `namespace/name` and can include:
  - `hooks.yaml` (event handlers)
  - `templates/*.md` (task templates)

## Proposed plugin reference and structure
- Namespace/name: **`aiki/infracost`** (as requested)
  - Note: docs currently reserve `aiki` for `glasner`-owned plugins. We should confirm ownership/namespace strategy before publish.

Proposed files:
- `hooks.yaml`
- `templates/cost-review.md`
- `templates/finops-audit.md`
- `templates/iac-budget-check.md`
- `templates/infracost-scan.md`

## Scope
1. Build a plugin that hooks into Aiki lifecycle events and gives cost signal with minimal noise.
2. Add command/task templates for explicit, deeper reviews.
3. Keep behavior safe:
   - no file edits by default
   - no shell side-effects
   - no commit/push
   - explicit instructions for install/login failures

Out of scope for this first pass:
- Full auto-remediation (e.g., mutating IaC based on scan findings)
- Deep provider-specific cost strategy engine
- Cross-cloud chargeback integrations / billing API pulls

## Phase 1 — Reference and architecture (1 day)
### 1.1 Confirm install target
- Decide whether the plugin is published under:
  - `aiki/infracost` (preferred naming path for internal standardization), or
  - a temporary namespace for build (`tu/infracost`) then migrate.
- Confirm CI/permission model for publishing to `aiki` namespace.

### 1.2 Define event-to-behavior matrix
- `session.started`:
  - Inject context reminder: required tooling, expected IaC scope, and “no destructive actions by default.”
- `change.completed`:
  - Detect if changed files are IaC-related (e.g., `*.tf`, `*.tf.json`, `*.hcl`, `*.yaml`, `*.yml`, `*.json` when clearly CloudFormation patterns).
  - If yes, run lightweight scan/inspect path and store latest JSON + summary in project-local artifact.
- `turn.completed`:
  - Optional: surface a short “cost delta this turn” digest when the last change touched IaC.
- `shell.permission_asked` (optional):
  - Preempt expensive/prohibited commands in sensitive infra directories with explicit warning context, not hard block (initially).

### 1.3 Error modes to model
- `infracost-preview` missing.
- `infracost-preview` not logged in / auth error.
- Repo has no recognized IaC files.
- Scan timeout on large repos.
- Non-Terraform file false positives.

## Phase 2 — Hook implementation (2–3 days)
### hooks.yaml plan
Use only documented Aiki actions: `if`, `shell`, `let`, `alias`, `context`, `log`, `autoreply`, `on_failure`, and `continue`.

#### Core flow
- `session.started`
  - `context` with a short policy: “infracost preview integration enabled; keep scans read-only.”
  - Include detection hints: commands and expected file formats.
- `change.completed`
  - `shell`: run file detection (safe diff or path filter from event data).
  - If no IaC changes, skip.
  - Else:
    - `shell`: create isolated temp JSON path under repo (or `.aiki/costs/`) and run:
      - `infracost-preview scan <candidate-root> > <cost_file>`
    - `shell`: run `infracost-preview inspect --summary --file <cost_file>` to derive human-readable summary.
    - `context` or `autoreply`: append concise summary (“est. monthly total, top expensive deltas, policy count”).
  - `on_failure`:
    - `continue` with `log` for parse/missing command/auth failures.
- `turn.completed`
  - Optional second-pass guard for large project edits:
    - if scan ran and exceeded thresholds, run `infracost-preview inspect --top 10` for focused next-step hints.

### Safety constraints
- All shell actions should be bounded with `timeout` (start 60s, configurable by event).
- Avoid commands that write outside repo/workspace.
- Do not auto-block normal coding operations initially; use advisory `autoreply` + `log`.
- If blocking is ever needed, use `on_failure: block` only on clearly policy-critical violations and only in explicit project opt-in.

## Phase 3 — Templates and workflow UX (2 days)
### Template: `templates/cost-review.md`
Purpose: standardized, pasteable task for focused infra cost reviews.

Include:
- required vars: `repo_path`, optional `provider`, `budget`, `currency`
- sections: command outputs, top spenders, policy violations, proposed remediations, risk caveats
- strict output format (table + action list)

### Template: `templates/finops-audit.md`
Purpose: periodic governance review.

Include:
- scan summary + failing policy list
- untagged/mis-tagged resources grouped by policy
- actionable patch plan (no patching by default)

### Template: `templates/iac-budget-check.md`
Purpose: user-requested budget validation.

Include:
- pass/fail against budget
- top 5 high-cost candidates
- tradeoff notes (e.g., performance vs savings)

### Template: `templates/infracost-scan.md`
Purpose: manual command wrapper for one-shot repository scans without waiting for hooks.

Include:
- command steps and output interpretation conventions
- interpretation of `--summary`, `--failing`, `--group-by`, `--top`

## Phase 4 — Installation, docs, and test harness (1–2 days)
### Install + onboarding
- Document prerequisites:
  - `infracost-preview` installed + authenticated (`infracost-preview login`)
  - minimal permissions / read access
- Include quick verification command in template:
  - `infracost-preview version`
  - `infracost-preview policies`

### Test plan
- Unit-level validation (plugin YAML lint/manual schema checks)
- E2E matrix:
  1. New repo with no IaC files → no-op on `change.completed`
  2. TF file change → summary produced
  3. CLI missing → advisory message, no crash
  4. Auth error → explicit recovery hint
  5. Large scan timeout → degrade gracefully with “rerun with scoped path” guidance

## Rollout
1. Build in non-production project with local symlink (`~/.aiki/plugins/...`) to validate behavior.
2. Add to one pilot repo `.aiki/hooks.yml` include list.
3. Monitor for 1 week for false positives and signal/noise.
4. Promote to broader usage if summary quality + performance are stable.

## Acceptance criteria
- Plugin installs successfully via `aiki plugin install aiki/infracost` (or temporary equivalent namespace).
- On recognized IaC file changes, plugin emits:
  - total cost estimate summary,
  - top likely cost drivers,
  - policy/compliance signal,
  - clear remediation suggestions.
- Plugin never mutates code/files in hook execution.
- Failure states are explicit and actionable (install/auth/path/timeout).
- One stable template set exists for manual deep-dive reviews.

## Risks + mitigations
- **Cost of false positives:** default to advisory mode + include thresholds + quick disable switch in `.aiki/hooks.yml`.
- **Plugin noise:** start with turn/session-level summaries only; defer stricter enforcement.
- **CLI brittleness:** capture stderr separately and avoid parsing raw JSON inline.
- **Namespace constraint (`aiki`):** confirm governance before publishing; if blocked, publish in user namespace then migrate.
- **Performance:** keep scan paths scoped to changed directories where possible; avoid full-repo scans every edit.

## Immediate next actions
1. Confirm namespace/ownership (`aiki/infracost` publish path).
2. Scaffold repository or plugin skeleton with `hooks.yaml` + 4 templates.
3. Implement advisory cost scan path and run pilot in one Terraform repo.
4. Iterate on trigger conditions from pilot data before any blocking rules are introduced.