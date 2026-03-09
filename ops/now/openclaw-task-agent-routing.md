# Plan: Targeted openclaw agent routing for task execution

**Date**: 2026-03-09  
**Status**: Draft  
**Purpose**: Treat `openclaw:<id>` as a first-class `--agent` target for `aiki task run`, with clean separation between target parsing and built-in agent runtime families.

**Related docs**: 
- `ops/now/learn-from-skill.md` (template pattern)

---

## Executive Summary
`aiki task run --agent` currently accepts only fixed built-in agent types (`claude-code`, `codex`, `cursor`, `gemini`, `unknown`). To route work to a specific clawbot, we need a namespaced execution target syntax (`openclaw:<id>`, e.g. `openclaw:tu`) and a runtime dispatch model that preserves existing semantics.

This plan keeps behavior stable for existing flows while making `openclaw:tu` feel like a first-class agent selector.

The change set is:
1. model **execution targets** as `AgentTarget`
   - builtins (`AgentType`) and
   - namespaced `openclaw:<id>`
2. thread that target through `run` execution paths
3. dispatch to new `OpenClawRuntime`
4. update docs/tests with clear contract gates.

## Problem
- `aiki task run --agent` rejects `openclaw:tu` today because `AgentType::from_str` only supports built-in identifiers.
- `get_runtime` currently accepts only `AgentType` and only supports `claude-code` / `codex` execution.
- Runtime selection and event metadata share the same `AgentType` pathway, which blocks introducing named bot targets without overloading enums.
- `aiki task` currently has multiple agent-like knobs (`--assignee` and task filters), and command surface consistency is important before broadening scope.

## Goal
Make `aiki task run --agent openclaw:<id>` (example `openclaw:tu`) route to a dedicated openclaw bot in headless mode, while preserving existing built-in execution behavior.

## Scope
### In scope
- `aiki task run --agent` parsing/validation for namespaced openclaw identities.
- Execution-time dispatch from task run path (`TaskRunOptions` → resolution → runtime lookup).
- New `OpenClawRuntime` implementation under `cli/src/agents/runtime`.
- Help/docs updates showing `openclaw:<id>`.
- Unit tests for parse/dispatch behavior.

### Out of scope
- UI for selecting bots in `aiki task add/start`.
- Changes to `--assignee` behavior on add/set/close/other task commands.
- Changes to openclaw-side storage semantics.
- Changing task visibility/assignee model for `openclaw:<id>` in the core DAG (separate phase).

## Command-surface alignment (important)
Current task command surface is split:
- **`aiki task run`** has `--agent` and is the only command intended to accept openclaw execution targets.
- **`aiki task add/start`** uses `--assignee` (task ownership/filtering) and does *not* currently accept `openclaw:*` as routing directives.

Decision:
- For this change, keep `--agent` support confined to `aiki task run`.
- Add only run-path execution parsing/routing for this request.
- Defer any additional `--agent` support on other commands unless explicitly requested.

## Proposed behavior
- `aiki task run --agent openclaw:tu ...` is accepted and resolved as an explicit target.
- User-facing semantics are still “agent-style” (`openclaw:tu`), but internal routing uses a separate target model so assignee visibility logic is not exploded with bot ids.
- Existing built-in behavior (`claude-code`, `codex`) is unchanged.
- If parser can’t recognize a target or runtime launch fails, return explicit errors:
  - `UnknownAgentType` for malformed targets
  - `AgentNotSupported` for known but non-executable kinds
  - `OpenClawRuntimeUnavailable` (new if needed) for missing CLI/binary/contract mismatch

## Implementation plan

### Phase 1 — Target parsing and execution model
1. Update `cli/src/agents/types.rs`
   - Keep `AgentType` as-is for built-in family/runtime choices (`claude-code`, `codex`, `cursor`, `gemini`, `unknown`).
   - Add new execution target type `AgentTarget`:
     - `Builtin(AgentType)`
     - `OpenClaw { bot_id: String }`
   - Add parser:
     - `openclaw:<id>` => `AgentTarget::OpenClaw { bot_id: <id> }`
     - existing built-in aliases (`claude`, `claude-code`, `codex`, ...).
   - Add canonical render helper (`AgentTarget::as_str`) for logging/event metadata.

2. Update `cli/src/tasks/runner.rs`
   - Change `TaskRunOptions.agent_override` to hold `Option<AgentTarget>`.
   - Update the resolver (currently `resolve_agent_type`) to return `AgentTarget`:
     - explicit `--agent` override first,
     - task assignee fallback second (maps existing task assignee to `AgentType`)
     - active session/process fallback third.
   - Keep current `AgentType` visibility/assignee rules intact.
   - Ensure downstream `TaskEvent::Started.agent_type` can represent target strings like `openclaw:tu` where applicable.

3. Update `cli/src/commands/task.rs`
   - In `Run` CLI doc for `--agent`, include `openclaw:<id>` explicitly.
   - In `run_run()`, replace `AgentType::from_str(agent_str)` with `AgentTarget::from_str(agent_str)` for override validation.

### Phase 2 — Runtime dispatch + spawn adapter
1. Update `cli/src/agents/runtime/mod.rs`
   - Change `get_runtime` to dispatch on `AgentTarget`.
   - `AgentTarget::Builtin(AgentType::ClaudeCode | Codex)` => existing runtimes.
   - `AgentTarget::OpenClaw { .. }` => new `OpenClawRuntime`.

2. Add `cli/src/agents/runtime/openclaw.rs`
   - Implement `AgentRuntime` (`spawn_blocking`, `spawn_background`, `spawn_monitored`).
   - Define headless invocation contract in one place (`openclaw` binary + args + env):
     - pass prompt via arg or stdin as needed
     - pass task context via `AIKI_TASK`
     - pass bot identifier via env (e.g. `OPENCLAW_BOT_ID=<id>`)
   - Map summary/error output to `AgentSessionResult` consistent with existing runtimes.

3. Export `openclaw` runtime in `cli/src/agents/runtime/mod.rs`.

### Phase 3 — Validation and docs
1. Add tests:
   - parse/format tests in `cli/src/agents/types.rs`:
     - `openclaw:tu` accepted
     - malformed ids rejected (including `openclaw`, `openclaw:`)
   - dispatch tests in runtime module:
     - builtin target still resolves
     - `openclaw:tu` dispatches to new runtime
   - command-surface tests:
     - `aiki task run --agent openclaw:tu` succeeds parse path
     - no expectation that non-run commands gain `--agent` (explicitly rejected or ignored by clap as currently defined)

2. Update CLI docs/examples (and `ops/now` notes):
   - `aiki task run --agent openclaw:tu --template ...`
   - explicit note: `openclaw:*` is run-only in this phase.

3. Smoke checks:
   - `cargo test -p aiki` for parser/runtime tests.
   - if binary invocation is unavailable in CI/local, tests must exercise graceful failure path only.

## Acceptance criteria
1. `aiki task run --agent openclaw:tu` is accepted (no early rejection as unknown built-in).
2. Existing `--agent codex|claude-code` behavior remains unchanged.
3. Run execution passes execution targets through to runtime selection and picks openclaw runtime for `openclaw:*` targets.
4. Started metadata/events preserve meaningful target labels (`openclaw:tu`).
5. Command-surface expectation is explicit: only `aiki task run` has `--agent` routing support for this phase.
6. Unit tests for parse + dispatch and command-surface behavior pass with no regressions in Claude/Codex paths.

## Risks / caveats
- Implementation is blocked until openclaw headless contract is verified (binary path, args, env, and exit semantics).
- `AgentSessionResult` mapping relies on reliable openclaw exit/error signals; run a short real payload/exit audit before strict automation assumptions.

## Next
- Confirm headless `openclaw` contract (args/env/exit semantics).
- Then implement phases 1–3 in minimal patches, each with targeted tests.
