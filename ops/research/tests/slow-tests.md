# Slow Test Investigation

**Date:** 2026-03-15

## Summary

Full `cargo test` takes 3+ minutes despite 1781 unit tests completing in 3.3s. Two root causes account for nearly all the wall-clock time.

## Test Suite Timing Breakdown

| Test file | Tests | Time | Per-test | Notes |
|---|---|---|---|---|
| lib (unit tests) | 1781 | 3.3s | 0.002s | Fast, no issues |
| task_tests | 88 | 36s (parallel) / 59s (sequential) | 0.41s | Setup overhead |
| test_spawn_flow | 10 | 3.8s | 0.38s | Normal for integration |
| git_hooks_tests | 15 | 2.4s | 0.16s | OK |
| test_session_end | 11 | 2.5s | 0.23s | OK |
| test_plugins | 18 | 1.2s | 0.07s | OK |
| multi_editor_tests | 7 | 1.5s | 0.21s | OK |
| end_to_end_tests | 1 | 1.8s | 1.8s | Single test, OK |
| blame_tests | 1 | 1.8s | 1.8s | Single test, OK |
| cli_tests | 9 | 1.0s | 0.11s | OK |
| doc tests | 17 | 2.4s | — | OK |
| tui_snapshot_tests | 14 | 0.1s | 0.006s | Fast |
| test_acp_session_flow | 25 | 0.0s | <0.001s | Fast |
| test_task_events | 28 | 0.02s | <0.001s | Fast |
| test_codex_hooks | 8 | 0.0s | <0.001s | Fast |
| test_flow_statements | 7 | 0.01s | <0.001s | Fast |
| test_flow_yaml_parsing | 4 | 0.0s | <0.001s | Fast |
| stderr_guardrail | 2 | 0.05s | 0.025s | Fast |
| claude_integration_test | 1 | 0.0s | <0.001s | Fast |
| **test_async_tasks** | **40** | **120s+** | — | **OUTLIER** |

## Outlier 1: `test_async_tasks` — Absorption Timeout (120s wasted)

### Problem

Two tests each block for **60 seconds** hitting the absorption timeout:

- `test_wait_with_closed_task_exits_immediately`
- `test_wait_with_stopped_task_returns_error`

### Root Cause

In `cli/src/commands/task.rs:5890`:

```rust
const WAIT_ABSORPTION_TIMEOUT_SECS: u64 = 60;
```

The `run_wait()` function (line 5892) has two phases:
1. Poll until the task reaches a terminal state (Closed/Stopped) — this completes instantly for these tests
2. Wait for an `Absorbed` event for any terminal task that has a `session_id` — **this is where it hangs**

In the test environment, `aiki task close` / `aiki task stop` emits events with a `session_id` (because the CLI always sets one), but no `Absorbed` event ever fires because there's no real isolated workspace to absorb. The inner loop (lines 5938–5983) polls until `WAIT_ABSORPTION_TIMEOUT_SECS` expires.

### Fix Options

1. **Environment variable override** — Check for something like `AIKI_WAIT_ABSORPTION_TIMEOUT` and use it if set. Tests set it to 0 or 1.
2. **Skip absorption when no workspace exists** — Before entering the absorption loop, check if the session workspace directory actually exists. If it doesn't, skip the wait.
3. **Don't set `session_id` when not in an isolated workspace** — The close/stop commands could omit `session_id` unless running in a real workspace, so `needs_absorption` would be empty and the loop would be skipped.

Option 2 is the cleanest fix — it's correct behavior (nothing to absorb if the workspace doesn't exist) and doesn't require test-specific configuration.

## Outlier 2: `task_tests` — Per-Test Setup Overhead (36s)

### Problem

88 integration tests at ~0.4s each = 36s total. Each test calls `init_aiki_repo()` which runs `git init` + `aiki init`, creating a fresh temp directory with a full git+jj repo.

### Root Cause

This is structural — the integration test pattern requires a clean repo per test. The ~0.4s per-test cost comes from:
- `git init` + git config (~0.05s)
- `aiki init` (jj init, branch setup, template copying) (~0.35s)

### Fix Options

1. **Fixture caching** — Create one "golden" initialized repo and `cp -r` it for each test instead of running `git init` + `aiki init`. Copying a directory is ~10x faster than running init.
2. **Shared fixture with `once_cell`** — Initialize once, then `cp -r` the template dir per test.
3. **Accept the cost** — 36s for 88 tests is not terrible. Focus on the 120s absorption timeout first.

## Recommended Priority

1. **Fix absorption timeout** — Saves 120s immediately, clear bug (test says "exits immediately" but takes 60s)
2. **Consider fixture caching** — Would cut task_tests from 36s to ~5-10s, but lower priority
