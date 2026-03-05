# Task Contracts: Typed Inputs, Typed Outputs, and Safe Handoffs

## Why now

The task system already has strong decomposition and review semantics. The main reliability gap is contract rigor at task boundaries.

This plan adds typed contracts to prevent invalid runs, reduce integration drift, and make task outputs safely consumable by downstream automation.

## Goals

1. Add strict, explicit input contracts to task templates.
2. Add enforceable output contracts for artifacts.
3. Add typed task-to-task handoff validation.
4. Add contract versioning and compatibility checks.
5. Improve operator UX with preflight validation and runtime contract logs.

## Non-goals (v1)

- Full plugin registry semantics.
- Cross-repo dependency resolver.
- Advanced schema features beyond high-value constraints.

## Design principles

- Fail fast before expensive agent work.
- Be strict by default; coercion is explicit.
- Contracts are local, human-readable, and versioned.
- Validation errors are actionable (field + reason + fix hint).
- Preserve backward compatibility via versioned contracts.

## Scope

### 1) Template Input Schema (v1 required)

Add a `contract.inputs` block in template metadata/frontmatter.

Supported v1 schema features:
- Primitive types: `string`, `integer`, `number`, `boolean`, `array`, `object`
- `required`
- `default`
- `enum`
- Numeric bounds: `min`, `max`
- String rules: `pattern`, `minLength`, `maxLength`
- Array rules: `minItems`, `maxItems`, `items`
- `nullable`
- `description` for docs/errors

Example:

```yaml
contract:
  name: intel.monitor
  version: 1
  strict: true
  inputs:
    window_days:
      type: integer
      min: 1
      max: 90
      required: true
    targets:
      type: array
      minItems: 1
      items:
        type: string
      required: true
    mode:
      type: string
      enum: [daily, weekly]
      default: weekly
```

### 2) Cross-field constraints (v1 required)

Add declarative rules:
- `if/then`
- `xor`
- `oneOf` for mutually exclusive patterns

Examples:
- If `mode = daily`, then `window_days <= 7`
- Exactly one of `window_days` or `start_date`

### 3) Output contract enforcement (v1 required)

Add `contract.outputs`:
- Required artifact paths (glob support)
- Required sections/headings for markdown artifacts
- Required keys/types for JSON artifacts

Build/run fails if outputs do not satisfy contract.

Example:

```yaml
contract:
  outputs:
    - path: reports/weekly/*.md
      kind: markdown
      required_sections: [Summary, Signals, Recommendations, Risks]
    - path: reports/weekly/*.json
      kind: json
      required_keys:
        generated_at: string
        findings: array
```

### 4) Typed task handoffs (v1.1)

When parent tasks pass context/vars to subtasks, validate payload against the callee input contract.

- Pre-dispatch validation at orchestration boundary.
- Explicit error if parent payload mismatches child schema.

### 5) Contract versioning + compatibility (v1.1)

Add explicit identity:
- `contract.name`
- `contract.version`

Policy:
- Major breaking change => new version.
- Callers can pin required contract versions.
- Clear deprecation warnings with migration hints.

### 6) Strict mode + coercion policy (v1 required)

- `strict: true` default.
- No implicit coercion (`"7"` != `7`) unless template enables `coercion: safe`.
- Coercion actions are logged.

### 7) Operator UX: preflight and explainability (v1 required)

New command:

```bash
aiki task validate --template <template> --vars <vars>
```

Behavior:
- Validate inputs and cross-field constraints without execution.
- Show resolved defaults.
- Show contract/version.
- Return non-zero on failure with clear error details.

### 8) Runtime observability (v1 required)

At run start, log:
- Contract name/version
- Validated input summary (redacted sensitive fields)
- Defaults applied
- Strict/coercion mode

On failure, log:
- Field path
- Violated rule
- Suggested fix

## Implementation plan

### Phase 0 — RFC + schema draft (2–3 days)

- Finalize contract metadata shape.
- Decide minimal JSON Schema subset.
- Define error taxonomy and CLI output format.

Deliverables:
- `references/task-contracts-rfc.md`
- `references/task-contract-schema-v1.md`

### Phase 1 — Input contracts + preflight CLI (4–6 days)

- Parse `contract.inputs` from templates.
- Implement validator engine for v1 subset.
- Add `aiki task validate` command.
- Integrate preflight check into `task run`.

Acceptance:
- Invalid input blocks run before spawning agents.
- Errors are deterministic and actionable.

### Phase 2 — Cross-field constraints + strict/coercion (3–4 days)

- Add `if/then`, `xor`, `oneOf` rule evaluation.
- Implement strict mode default and explicit coercion option.

Acceptance:
- Contract rule violations return precise field/rule context.

### Phase 3 — Output contracts (4–6 days)

- Implement artifact existence checks.
- Add markdown required section validator.
- Add JSON required keys/type validator.
- Enforce on task completion before close.

Acceptance:
- Task cannot close “green” with missing/invalid required outputs.

### Phase 4 — Handoffs + versioning (5–7 days)

- Validate parent->child payloads.
- Add contract version fields and compatibility checks.
- Add deprecation warning path.

Acceptance:
- Incompatible handoffs fail before subtask dispatch.

### Phase 5 — Observability polish (2–3 days)

- Improve logs and failure messages.
- Add concise contract summaries to run output.

Acceptance:
- Operators can diagnose contract failures from run logs alone.

## Milestones

- **M1 (v1)**: Input schema + preflight + cross-field + strict/coercion + output contracts.
- **M2 (v1.1)**: Typed handoffs + contract versioning.

## Risks and mitigations

1. **Over-engineering schema surface**
   - Mitigation: keep v1 schema small; expand only from real failures.

2. **Template author friction**
   - Mitigation: generate contract stubs and clear error docs.

3. **Backward compatibility breakage**
   - Mitigation: opt-in default for existing templates; migration tooling.

4. **False confidence from weak output checks**
   - Mitigation: enforce both existence and structural requirements.

## Success metrics

- % of runs failing preflight vs runtime (want more early failures).
- Reduction in reruns caused by missing/invalid vars.
- Reduction in downstream failures from malformed artifacts.
- Time-to-diagnose contract failures from logs.

## Open questions

1. Should existing templates default to `strict: false` for one release window?
2. How much JSON Schema parity is worth carrying in-core vs keeping minimal?
3. Do we support reusable named schema fragments in v1 or v1.1?
4. Should output contracts support semantic checks (e.g., non-empty findings) in v1?

## Immediate next steps

1. Approve this plan and lock v1 schema subset.
2. Draft RFC docs and sample contract-enabled templates.
3. Implement `aiki task validate` first to unlock fast feedback.
4. Gate `task run` on contract preflight.
