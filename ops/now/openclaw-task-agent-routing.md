# Plan: Targeted openclaw task routing for execution and assignee workflows

**Date**: 2026-03-09  
**Status**: Draft  
**Purpose**: Treat `openclaw:<id>` as a first-class identity target across `aiki task run --agent` and `--assignee` task ownership flows, while preserving existing built-in agent behavior.

**Related docs**: 
- `ops/now/learn-from-skill.md` (template pattern)

---

## Executive Summary
`aiki task run --agent` currently accepts only fixed built-in agent types (`claude-code`, `codex`, `cursor`, `gemini`, `unknown`), while task ownership flags (`--assignee` on `add`, `start`, `set`, etc.) also only accept built-in names.

To avoid half-implemented behavior, this change should support the same namespaced target model (`openclaw:<id>`, e.g. `openclaw:tu`) in both execution and assignment paths.

The change set is:
1. model execution/ownership as a shared **target** concept (`AgentTarget`), while keeping `AgentType` as fixed family/runtime family.
2. thread that target through `task run` execution and `task` ownership flows.
3. dispatch to a new `OpenClawRuntime` for headless openclaw execution.
4. update docs/tests so `openclaw:<id>` is coherent across command surfaces.

## Problem
- `aiki task run --agent` rejects `openclaw:tu` today because `AgentType::from_str` only supports built-in identifiers.
- `--assignee` parsing (`Assignee::from_str`) only supports built-in agents (`claude-code`, `codex`, `cursor`, `gemini`) and `human`.
- `get_runtime` accepts only `AgentType`, so execution dispatch cannot reach openclaw identities.
- Runtime selection, event metadata, and task visibility are coupled to `AgentType` and currently lack an identity channel for clawbot ids.

## Goal
Enable `openclaw:<id>` as a first-class target for:
- execution routing via `aiki task run --agent openclaw:<id>`
- explicit ownership/filtering via task `--assignee` (`add`, `start`, `set`, and related task commands that already accept assignee)

while preserving existing built-ins and minimizing semantics drift.

## Scope
### In scope
- `aiki task run --agent` parsing/validation for namespaced openclaw identities.
- `aiki task --assignee` parsing/validation for namespaced openclaw identities in command paths that currently support assignee flags.
- Execution-time dispatch from run path (`TaskRunOptions` → resolution → runtime lookup).
- Assignee-aware run fallback: when task assignee is openclaw target, run should resolve to that runtime target.
- New `OpenClawRuntime` implementation under `cli/src/agents/runtime`.
- Help/docs updates showing `openclaw:<id>` in both run + assignee contexts.
- Unit tests for parse/dispatch and assignee-run resolution behavior.

### Out of scope
- Changes to openclaw-side storage semantics.
- Deep refactor of visibility/filtering policy beyond minimal compatibility (`openclaw:<id>` should behave like existing agent ownership in the existing CLI surfaces).
- Any new `--assignee` surfaces on commands that do not currently expose assignee.

## Command-surface alignment (important)
Current task command surfaces include two paths:
- `--agent` (primarily `aiki task run`)
- `--assignee` (add/start/set/run path ownership and filtering logic)

Decision:
- Support openclaw identifiers on both in this change:
  - `aiki task run --agent openclaw:<id>`
  - `aiki task add/start/set ... --assignee openclaw:<id>` (where `--assignee` is already supported).
- No changes to create new assignee flags on unrelated commands.

## Proposed behavior
- `aiki task run --agent openclaw:tu ...` is accepted and resolved as an explicit runtime target.
- `aiki task add/start --assignee openclaw:tu ...` stores a valid assignee target.
- Run resolution prioritizes explicit `--agent` override first, then task assignee (`openclaw:<id>` supported), then active session/process fallback.
- Existing built-in behavior (`claude-code`, `codex`) remains unchanged.
- Task event metadata can include meaningful labels such as `openclaw:tu`.
- If parser can’t recognize a target or runtime launch fails, return explicit errors:
  - `UnknownAgentType` for malformed targets
  - `AgentNotSupported` for known but non-executable kinds
  - `OpenClawRuntimeUnavailable` (new if needed) for missing CLI/binary/contract mismatch

## Implementation plan

### Phase 1 — Shared target model + parsing
1. Update `cli/src/agents/types.rs`
   - Keep `AgentType` as-is for built-in family/runtime choices (`claude-code`, `codex`, `cursor`, `gemini`, `unknown`).
   - Add new execution/ownership target type `AgentTarget`:
     - `Builtin(AgentType)`
     - `OpenClaw { bot_id: String }`
   - Add parser:
     - `openclaw:<id>` => `AgentTarget::OpenClaw { bot_id: <id> }`
     - existing built-in aliases (`claude`, `claude-code`, `codex`, ...).
   - Extend `Assignee` parsing to support `openclaw:<id>`; either by:
     - adding `Assignee::Target(AgentTarget)` (preferred), or
     - introducing a parallel helper that safely converts assignee strings to `AgentTarget` in runner resolution.
   - Add canonical render helper (`AgentTarget::as_str`) for logging/event metadata.

2. Update `cli/src/tasks/runner.rs`
   - Change `TaskRunOptions.agent_override` to hold `Option<AgentTarget>`.
   - Update resolver (`resolve_agent_type`) to return `AgentTarget`:
     - explicit `--agent` override first,
     - task assignee fallback second (including `openclaw:<id>`),
     - active session/process fallback third.
   - Ensure started events can represent target labels like `openclaw:tu` where applicable.

3. Update assignee-consuming command paths in `cli/src/commands/task.rs`
   - `Run` path: parse/validate `--agent` via `AgentTarget::from_str`.
   - Assignee-bearing paths (`add`, `start`, `set`, etc.): accept `openclaw:<id>` via updated assignee parser.
   - Keep existing built-in assignee constraints where required by UX/docs.

### Phase 2 — Runtime dispatch + spawn adapter
1. Update `cli/src/agents/runtime/mod.rs`
   - Change runtime dispatch to accept `AgentTarget`.
   - `AgentTarget::Builtin(AgentType::ClaudeCode | Codex)` => existing runtimes.
   - `AgentTarget::OpenClaw { .. }` => new `OpenClawRuntime`.

2. Add `cli/src/agents/runtime/openclaw.rs`
   - Implement `AgentRuntime` methods (`spawn_blocking`, `spawn_background`, `spawn_monitored`).
   - Define headless invocation contract (`openclaw` binary + args + env):
     - pass prompt/task context via existing contract (`AIKI_TASK` et al.)
     - pass bot identifier via env (e.g. `OPENCLAW_BOT_ID=<id>`)
   - Map summary/error output to `AgentSessionResult` consistent with existing runtimes.

3. Export `openclaw` runtime in `cli/src/agents/runtime/mod.rs`.

### Phase 3 — Validation and docs
1. Add tests:
   - `cli/src/agents/types.rs`
     - `openclaw:tu` accepted by target parser
     - malformed ids rejected (`openclaw`, `openclaw:`)
   - `cli/src/commands/task.rs` parse tests / integration:
     - `aiki task add --assignee openclaw:tu ...`
     - `aiki task run --agent openclaw:tu ...`
   - `cli/src/tasks/runner.rs` behavior tests:
     - `TaskRunOptions` resolves explicit `--agent` target before task assignee
     - task assignee `openclaw:<id>` resolves to openclaw runtime when no explicit override

2. Update CLI docs/examples:
   - `aiki task run --agent openclaw:tu --template ...`
   - `aiki task add --assignee openclaw:tu ...`
   - `aiki task start --assignee openclaw:tu ...`

3. Smoke checks:
   - `cargo test -p aiki` for parser/runtime tests.
   - if binary invocation unavailable in CI/local, tests should verify graceful failure path.

## Acceptance criteria
1. `aiki task run --agent openclaw:tu` is accepted and resolves to OpenClaw runtime.
2. `--assignee openclaw:tu` is accepted wherever `--assignee` is supported (`add`, `start`, `set`, etc. as currently defined).
3. Existing `--agent`/`--assignee` behavior for built-ins remains unchanged.
4. Run resolution still prefers explicit `--agent` override before assignee fallback.
5. Metadata/events preserve meaningful target labels (`openclaw:tu`).
6. Unit tests for parser, command surfaces, and dispatch pass with no regressions.

## Risks / caveats
- Implementation is blocked until openclaw headless contract is confirmed (binary path, args/env/exit semantics).
- `Assignee`/visibility behavior for openclaw identities must avoid breaking existing task filtering and human visibility; treat this as a compatibility-focused change (default behaviors should not regress).
- `AgentSessionResult` mapping depends on reliable openclaw exit/error signals; run a short real payload/exit audit before hard automation assumptions.

## Next
- Confirm headless `openclaw` contract (args/env/exit semantics).
- Then implement phases 1–3 in minimal patches, with targeted tests after each stage.
