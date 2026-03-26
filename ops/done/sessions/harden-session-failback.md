# Harden Session Fallback

## Problem

Interactive Codex task attribution can attach task events to the wrong historical
session when the current Codex thread does not have a corresponding Aiki session
file in `~/.aiki/sessions/`.

Observed failure:

1. Current Codex thread existed with external session ID `019d299e-2078-75f3-835b-fbba9eaf57b0`
2. Expected Aiki session ID `f32b1592` was missing from `~/.aiki/sessions/`
3. `aiki task start` could not match the current process by PID
4. `find_active_session()` fell back to generic Codex session lookup
5. An older unrelated Codex session file (`2242d4fb`) was selected
6. Task `vuosmlvlzpmnymuywyyvvnkspwlyxnnp` was started and closed with `session_id=2242d4fb`

This is a correctness bug. It causes historical task attribution to drift across
sessions and can lead to the wrong session claiming ownership of interactive work.

## Root Cause

There are two independent weaknesses:

### 1. Current Codex session file can be missing

The current Codex rollout had a real Codex session record and shell snapshot, but
no matching Aiki session file under `~/.aiki/sessions/`.

That means the SessionStart hook either:

- did not fire
- fired but did not dispatch `session.started`
- dispatched `session.started` but failed before creating the session file

Without the session file, PID-based matching has nothing current to match.

### 2. Codex fallback selection is too permissive and poorly ranked

`find_active_session()` falls back to `find_session_by_agent_type()` for Codex.
That fallback currently scans all Codex session files and picks the "most recent"
candidate by calling `query_latest_event()`.

Problems:

- `query_latest_event()` shells out to `jj log` in the current repo, but session
  conversation history lives in the global Aiki repo under `~/.aiki/.jj`
- if the JJ query fails or finds nothing, candidate sessions collapse to
  `UNIX_EPOCH`
- once that happens, selection becomes effectively arbitrary among matching
  Codex session files
- the fallback does not require repo affinity before selecting a Codex session

So a stale session can win even when it belongs to a different repo and a
different external session.

## Goals

- Never attribute interactive task work to an unrelated historical session
- Prefer "no active session found" over attaching to the wrong session
- Improve observability when session detection fails

## Design Decision

The original plan proposed fixing the ranked fallback (repo affinity, better
timestamp queries, safe failure). On reflection, the ranked fallback is
fundamentally guesswork — even with improvements it's still picking from
candidates rather than matching deterministically. Wrong attribution is strictly
worse than no attribution.

**Simplified approach:** remove the ranked fallback entirely for Codex. Only
match by exact external session ID. If that fails, return `None`.

## Plan

### 1. Thread external session ID through session detection

**Files:** `cli/src/editors/codex/events.rs`, `cli/src/session/mod.rs`

When the Codex hook payload or surrounding runtime provides the external session
ID, pass it through to `find_active_session()` so it can attempt an exact match.

The external session ID is hashed deterministically to produce the Aiki session
ID. If a session file exists at `~/.aiki/sessions/<hashed-id>`, that's the
match — no scanning needed.

Detection order:

1. PID match (existing — works when session file exists and PID is live)
2. Exact external-session-ID match (deterministic hash lookup)
3. `None`

The generic `find_session_by_agent_type()` fallback is removed from the Codex
path.

### 2. Remove ranked Codex fallback

**File:** `cli/src/session/mod.rs`

Remove or bypass `find_session_by_agent_type()` for Codex sessions. This
eliminates:

- `query_latest_event()` usage for session ranking
- cross-repo session selection
- arbitrary candidate selection when timestamps are unknown

Non-Codex agent types that rely on the ranked fallback are unaffected.

### 3. Add debug logging for failed matches

**Files:** `cli/src/session/mod.rs`, `cli/src/events/session_started.rs`

When session detection returns `None`, log enough context to diagnose why:

- external session ID (if available)
- computed Aiki session ID
- whether the session file existed
- PID match result

Around Codex SessionStart handling, log to distinguish:

- hook not invoked
- event parse failure
- `session.started` dispatch failure
- session file create failure

Include external session ID, computed Aiki session ID, cwd, and parent PID
where available.

## Testing

### Unit tests

- exact external-session match returns the correct session
- missing session file with valid external ID returns `None` (not a stale session)
- PID match still works and takes priority over external ID match
- non-Codex agent types still use existing fallback behavior

### Integration tests

- Codex session with valid SessionStart creates expected `~/.aiki/sessions/<uuid>`
- missing current session file does not cause task start to claim an unrelated
  older session
- when no match exists, `aiki task start` behaves as "no active session"
  instead of assigning the wrong one

## Not in Scope

- Retroactively rewriting old task events
- Changing task event schema (`session_id` is sufficient once detection is correct)
- Solving all non-Codex session attribution issues in one pass
- Fixing the ranked fallback (repo affinity, timestamp queries) — replaced by removal
