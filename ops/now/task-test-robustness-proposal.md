# Task/Test Robustness Proposal (Aiki CLI)

## Investigation summary (what feels brittle today)

I reviewed the Aiki CLI test surface and found most fragility comes from **text-coupled assertions**, not logic correctness.

### What I found
1. **Task tests parse human output format**
   - `cli/tests/task_tests.rs` extracts IDs by parsing lines like:
     - `Added <id>`
     - `Started <id>`
     - list lines like `[pN] <id>  <name>`
   - Any formatting change (spacing, punctuation, prefixes, localization) breaks parsing.

2. **Limited machine-friendly output mode**
   - `OutputFormat` today is effectively only `id` in `cli/src/commands/mod.rs`.
   - `aiki` CLI has command-specific `--output` in many paths, but no consistent structured output surface for most operations.

3. **UI/output surface is coupled to implementation details**
   - Tests assert exact phrases (`"Ready (1):"`, task statuses rendered in specific blocks, etc.).
   - This is sensitive to any non-functional refactor in renderer/reporting layer.

4. **Async and timing behavior is asserted in synchronous shells**
   - Several flows rely on immediate availability of output/state, while command execution can be backgrounded/delayed in this environment.
   - This creates test flakiness unrelated to product behavior.

5. **Ordering assumptions**
   - List-based tests assume stable ordering and deterministic prefixes; this tends to become brittle as scheduling / session semantics evolve.

---

## Proposal: make tests protocol-driven instead of prose-driven

### Phase 0 — Baseline hardening (1–2 days)
- Add a shared integration-test helper:
  - `run_aiki_json(cmd, args, cwd)` returning parsed JSON payload + raw stream.
- Introduce command contract expectations:
  - `status`, `id`, `state`, `error_code`, `message`.
- Refactor existing task add/start/list tests to use ID fields from structured output, not regex against prose.

### Phase 1 — Structured output in CLI (1–3 days)
- Expand `OutputFormat` in `cli/src/commands/mod.rs`:
  - `Id`, `Json`, optionally `Plain`.
- Add consistent `--output json` flag on user-facing command families:
  - `task add|start|list|show|wait`
  - `build|decompose|loop|plan|review`
  - `epic` + `fix` where applicable
- Keep existing plain output default unchanged for TUI/interactive behavior.

### Phase 2 — Test migration by command domain (3–5 days)
- Migrate tests to schema-based assertions:
  - `assert_json_eq!(...)` / key-path checks on fields.
  - Keep only 1–2 human-string assertions per command for backward compatibility.
- Standardize fixtures for task identity in tests:
  - capture created IDs via JSON and reuse downstream.
- Replace sleeps with explicit polling helpers that cap retries/time.

### Phase 3 — Async resilience (2–4 days)
- Add helpers for eventual consistency:
  - `wait_for_task_state(task_id, desired_state, timeout_ms, poll_ms)`.
- Tests assert converged state transitions instead of immediate text snapshots.

### Phase 4 — Snapshot discipline (1–2 days)
- Reserve snapshot-style assertions only for pure markdown/UI docs.
- Avoid snapshots for command state for tests that need behavior guarantees.
- Mark brittle tests with explicit `#[ignore]` + regression ticket only if no alternate contract exists.

---

## Concrete acceptance criteria
1. Core task tests do not fail when human-facing task list text is reworded.
2. Running tests in different terminal/tty conditions yields stable pass/fail.
3. New `--output json` exists and all state-changing task commands can be tested without parsing prose.
4. Flaky test count (manual baseline) drops by >70% after migrating existing suite.

---

## Immediate next steps
1. Add first pass JSON output for:
   - `aiki task add --output json`
   - `aiki task start --output json`
   - `aiki task list --output json`
2. Refactor `cli/tests/task_tests.rs` only (highest ROI slice) to prove pattern.
3. Extend same pattern to `epic`, `loop`, and `build` tests.
4. Track deltas with a CI job that compares historical flaky count.

---

## Owner + priority
- **Priority:** P1 (test reliability
- **Owner:** Aiki core testing bundle
- **Risk if delayed:** regressions sneak in as command output evolves; slower confidence loop on task orchestration changes.
