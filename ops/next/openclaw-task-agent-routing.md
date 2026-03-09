# Plan: Targeted openclaw agent routing for task execution

**Date**: 2026-03-09  
**Status**: Draft  
**Purpose**: Support namespaced bot targeting in `aiki task run` and prepare headless openclaw execution parity with `claude-code`/`codex`.

**Related docs**: 
- `ops/now/learn-from-skill.md` (template pattern)

---

## Executive Summary
`aiki task run --agent` currently accepts only fixed built-in agent types (`claude-code`, `codex`, `cursor`, `gemini`, `unknown`). To support routing to individual clawbots in openclaw, we need a namespaced agent target format and runtime dispatch path for `openclaw:<id>` (e.g. `openclaw:tu`).

This plan proposes a minimal change set:
1. add a typed agent target parser (`builtin` + `openclaw:<id>`),
2. pass that target through existing `run` execution flow,
3. add an `OpenClawRuntime` adapter, and
4. update CLI help + tests + docs.

## Problem
- `aiki task run --agent` rejects `openclaw:tu`-style values today because `AgentType::from_str` only supports hardcoded agents.
- `get_runtime` currently accepts only `AgentType` and only supports `claude-code` / `codex` execution.
- Existing event fields (`TaskEvent::Started.agent_type`) can carry any string, but task resolution/selection is still enum-driven, so actual routing cannot reach openclaw bot instances.

## Goal
Make `aiki task run --agent openclaw:<id>` (example `openclaw:tu`) route to a dedicated openclaw bot in headless mode, while preserving existing built-ins behavior.

## Scope
### In scope
- `aiki task run --agent` parsing/validation for namespaced openclaw identities.
- Execution-time dispatch from task run path (`TaskRunOptions` → agent resolution → runtime lookup).
- New `OpenClawRuntime` implementation under `cli/src/agents/runtime`.
- Help/docs updates showing `openclaw:<id>`.
- Unit tests for parse/dispatch behavior (and optional runtime constructor smoke behavior).

### Out of scope
- UI for selecting bots in `aiki task add/start`.
- New task assignee semantics for openclaw in shared task visibility (can be separate phase).
- Changes to openclaw-side storage semantics.

## Proposed behavior
- `aiki task run --agent openclaw:tu ...` is accepted.
- `aiki task run` still accepts existing built-ins (`claude-code`, `codex`) unchanged.
- If parser can’t recognize an `openclaw:` target or runtime launch fails, user gets explicit, actionable errors:
  - `UnknownAgentType` for malformed targets
  - `AgentNotSupported` for known but non-executable agent kinds
  - `OpenClawRuntimeUnavailable`-style error if CLI binary/config missing (new error class if needed)

## Implementation plan

### Phase 1 — Target parsing and execution model
1. Update `cli/src/agents/types.rs`
   - Keep `AgentType` as-is for existing built-ins.
   - Add a new enum/type for execution targets, e.g. `AgentTarget`:
     - `Builtin(AgentType)`
     - `OpenClaw { bot_id: String }`
   - Add parser:
     - `openclaw:<id>` => `AgentTarget::OpenClaw { bot_id: <id> }`
     - keep existing built-in aliases (`claude`, `claude-code`, `codex`, ...).
   - Add helper methods for canonical string render (`agent_target.as_str()`) used by event/task metadata.

2. Update `cli/src/tasks/runner.rs`
   - Change `TaskRunOptions.agent_override` to hold `Option<AgentTarget>`.
   - Update `resolve_agent_type` (renamed or refactored) to return `AgentTarget`:
     - explicit `--agent` override first,
     - task assignee second (currently maps from `Assignee` -> `AgentType`),
     - active session/process fallback third.
   - Keep `AgentType`-based logic for current assignee resolution paths.
   - Ensure downstream event-writing (`TaskEvent::Started.agent_type`) can carry `openclaw:tu` where appropriate.

3. Update `cli/src/commands/task.rs`
   - In `Run` CLI definition, expand docs from `claude-code, codex` to include `openclaw:<id>`.
   - In `run_run()`, replace `AgentType::from_str(agent_str)` parsing with `AgentTarget::from_str(agent_str)`.

### Phase 2 — Runtime dispatch + spawn adapter
1. Update `cli/src/agents/runtime/mod.rs`
   - Update `get_runtime` to accept/dispatch `AgentTarget`.
   - Return existing `ClaudeCodeRuntime`/`CodexRuntime` for builtin targets.
   - Return new `OpenClawRuntime` for `AgentTarget::OpenClaw { .. }`.

2. Add `cli/src/agents/runtime/openclaw.rs`
   - Implement `AgentRuntime` methods (`spawn_blocking`, `spawn_background`, `spawn_monitored`).
   - Define headless invocation contract in one place (`openclaw` binary + args + env):
     - carry `task_prompt()` via argument or stdin
     - carry `options.task_id` via env (`AIKI_TASK`) and session markers (matching existing convention)
     - carry `bot_id` metadata via env (e.g. `OPENCLAW_BOT_ID=<id>`), with a TODO/fallback for CLI param location if invocation shape differs.
   - Add extraction summary/error handling consistent with existing runtimes.

3. Update `cli/src/agents/runtime/mod.rs` exports (`mod openclaw; pub use ...`).

### Phase 3 — Validation and docs
1. Add tests:
   - parse tests in `cli/src/agents/types.rs`:
     - `openclaw:tu` accepted,
     - case variants invalid values rejected.
   - dispatch tests in runtime module:
     - builtin agent still resolves,
     - `get_runtime` returns runtime for `openclaw:tu` target.

2. Add/update CLI output examples in `ops/next`/`ops/now` doc:
   - `aiki task run --agent openclaw:tu --template ...`

3. Quick smoke check:
   - `cargo test -p aiki` for parser/runtime tests;
   - if environment cannot execute `openclaw` binary, test should assert missing-tool handling path (no hanging, clear error).

## Acceptance criteria
1. `aiki task run --agent openclaw:tu` is parse-valid and does not reject at validation time.
2. Existing `--agent codex|claude-code` behavior remains unchanged.
3. `run` execution path passes target through to runner/runtime and selects openclaw runtime for `openclaw:*`.
4. `agent_type` metadata for started runs remains meaningful, with started events showing `openclaw:tu` when requested.
5. Unit tests cover parse and dispatch; no regressions in current codex/claude flows.

## Risks / caveats
- Runtime command contract depends on stable headless invocation surface for openclaw; if no stable contract is confirmed, this should remain blocked at a “manual gate” until a concrete CLI shape is validated.
- `AgentSessionResult` mapping depends on reliable exit/error signaling from openclaw process; we need a short real payload/exit audit before enabling strict automation.

## Next
- Confirm openclaw headless command contract (args/env/exit semantics).
- Then implement phases 1–3 in small patches with targeted tests after each.
