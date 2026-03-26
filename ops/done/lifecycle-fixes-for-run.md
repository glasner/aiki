# Lifecycle Fixes For `aiki run`

## Goal

Tighten the lifecycle semantics of direct `aiki run <task-id>` so it does
not silently launch work for tasks that are already reserved or already in
progress.

The current behavior is too permissive:

- `Open` tasks are reserved and then spawned, which is correct
- `Reserved` tasks can still be run directly
- `InProgress` tasks can still be run directly
- only `Closed` is rejected

That creates ambiguous ownership and makes it too easy to start duplicate
work on tasks that are already claimed.

## Desired Behavior

### Direct `aiki run <task-id>`

- `Open`:
  reserve, spawn, then let the child agent transition `Reserved -> InProgress`
- `Reserved`:
  reject by default with a clear error that the task is already pending a run
- `InProgress`:
  reject by default with a clear error that the task is already running
- `Stopped`:
  allow
- `Closed`:
  reject as today

### Direct `aiki run <task-id> --force`

`--force` is an escape hatch for recovering from bad state.

- `Open`:
  no special handling, proceed normally
- `Reserved`:
  emit `Released`, returning it to `Open`, then continue through the normal
  reserve + spawn flow
- `InProgress`:
  emit `Stopped`, moving it to `Stopped`, then continue through the normal
  stopped-task run flow
- `Stopped`:
  allow
- `Closed`:
  still reject

### `aiki run --next-session`

No change intended.

`--next-session` already works from the ready queue and only selects `Open`
tasks, so the stricter direct-run behavior should not alter next-session
selection semantics.

## Design

### 1. Add `--force` to `aiki run`

Add a `--force` flag on the top-level `Run` command in `cli/src/main.rs`
and thread it through `commands::run::run()` and `run_impl()`.

This flag is only for direct run recovery. It should not change ready-queue
selection rules for `--next-session`.

### 2. Normalize task state in `commands/run.rs`

For direct `aiki run <task-id>`:

- load the task graph before calling the runner
- resolve the target task
- inspect its current status
- enforce the default guards:
  - reject `Reserved`
  - reject `InProgress`
- when `--force` is provided:
  - `Reserved`: write `TaskEvent::Released`
  - `InProgress`: write events that bring the task back to `Open`

After normalization, call the existing runner entry point so downstream
behavior remains unchanged.

This keeps the policy in one place instead of scattering direct-run
lifecycle rules across multiple runner entry points.

### 3. `InProgress --force` uses `Stopped`

The forced recovery path for an actively running task should be:

`InProgress -> [Stopped] -> Stopped`

Then the normal direct-run flow continues from `Stopped`.

This fits the existing lifecycle model better than trying to force an
`InProgress -> Open` transition:

- `Stopped` already means work had started and was interrupted or paused
- `Released` remains specific to pre-start reservation rollback
- no new lifecycle event is needed

Implementation detail:

- emit `TaskEvent::Stopped` with a clear forced-reset reason such as
  `Force-stopped by aiki run --force`
- after that event is written, continue through the standard run path
- because the task is now `Stopped`, downstream behavior should remain
  unchanged

## Proposed Error Messages

For direct run without `--force`:

- `Reserved`:
  `Task '<id>' is reserved and already pending a run. Use --force to release and re-run it.`
- `InProgress`:
  `Task '<id>' is already in progress. Use --force to reset and re-run it.`

These should be direct and operational, not generic.

## Tests

### Unit / command-level coverage

Add tests for direct `aiki run <task-id>`:

- open task: still reserves and runs
- reserved task: fails without `--force`
- in-progress task: fails without `--force`
- stopped task: still runs
- closed task: still fails

### Force-path coverage

Add tests for:

- reserved + `--force`:
  emits `Released`, then normal reserve/spawn proceeds
- in-progress + `--force`:
  emits `Stopped`, then normal stopped-task run behavior proceeds
- spawn failure after force-reset:
  final state is `Open`, not `Reserved`

### Regression coverage

Preserve the recent reserved rollback fix:

- if this run invocation created the reservation and spawn fails,
  emit `Released`
- if it did not create the reservation, do not release someone else’s lock

## Work Breakdown

1. Add CLI plumbing for `aiki run --force`
2. Implement direct-run status guards in `commands/run.rs`
3. Implement the `InProgress -> Stopped` force-stop transition
4. Keep `--next-session` behavior unchanged
5. Add focused tests for reserved/in-progress direct-run behavior
6. Add regression tests for rollback after forced normalization
