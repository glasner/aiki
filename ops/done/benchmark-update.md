# Benchmark Command Expansion Plan

**Status**: Implemented
**Date**: 2025-12-11

## Executive Summary

The `aiki benchmark` command currently tests only 2 of 7 event types (SessionStart, PostFileChange) using only Claude Code hook simulation. This plan expands coverage to all tracked events with a convention-over-configuration approach.

**Design Principle**: The benchmark should "just work" - always test everything, use sensible defaults, minimize flags.

**Vendor Limitations**:
- Cursor has no native `SessionStart` hook - `beforeSubmitPrompt` maps to `PrePrompt` only
- `SessionEnd` is triggered internally by `PostResponse` when no autoreply, not a separate hook
- Unsupported events are omitted from metrics (not recorded as "0 ms")

---

## Current State

### What's Tested

| Event | Tested | Method |
| --- | --- | --- |
| SessionStart | ✅ | 1x during init |
| PostFileChange | ✅ | Nx in hot path (PostToolUse hook) |
| PrePrompt | ❌ | - |
| PreFileChange | ❌ | - |
| PostResponse | ❌ | - |
| SessionEnd | ❌ | - |
| PrepareCommitMessage | ❌ | Triggered by git commit but not profiled |

### Vendor Coverage

| Vendor | Tested |
| --- | --- |
| Claude Code hooks | ✅ (PostToolUse only) |
| Cursor hooks | ❌ |

### Current Implementation

Location: `cli/src/commands/benchmark.rs` (662 lines)

**Workflow:**
1. Init temp Git repo + `aiki init` (measures SessionStart)
2. Create initial commit with test files
3. Hot path: N edits via PostToolUse hook (measures PostFileChange)
4. Query operations: `aiki blame`, `aiki authors`
5. Final git commit (triggers but doesn't measure PrepareCommitMessage)

**Output:** `.aiki/benchmarks/{flow_name}/{timestamp}/` with `results.txt` and `metrics.json`

---

## Design: Convention Over Configuration

### CLI Interface

```bash
# That's it. One command.
aiki benchmark [--edits N]
```

**Conventions:**
- Always uses `aiki/core` flow (the standard flow)
- Always tests all events
- Always tests both vendors (Claude Code + Cursor)
- Always runs 3 warmup iterations
- Always runs 3 measurement passes
- Default: 50 edits (enough for statistical significance, fast enough for iteration)

**Why no flags:**
- `--vendor`: Always test both - they share most code paths, differences are interesting
- `--events`: Always test all - partial coverage hides problems
- `--warmup`: 3 is the right number (JIT warm, caches hot, not wasteful)
- `--runs`: 3 balances noise reduction vs speed

### Benchmark Workflow

```
+------------------------------------------------------------------+
| Phase 1: Setup                                                    |
|   - Create temp repo                                              |
|   - Run `aiki init`                                               |
|   - Create test files + initial commit                            |
+------------------------------------------------------------------+
                              |
                              v
+------------------------------------------------------------------+
| Phase 2: Session Lifecycle (per vendor)                           |
|   For vendor in [claude-code, cursor]:                            |
|     1. SessionStart (1x) - Claude Code only, Cursor = N/A         |
|     2. PrePrompt (1x) - simulate user prompt                      |
|     3. Hot Path Loop (N iterations):                              |
|        a. PreFileChange                                           |
|        b. [actual file edit]                                      |
|        c. PostFileChange                                          |
|     4. PostResponse (1x) - no autoreply, includes SessionEnd      |
|     5. PostResponse+Autoreply (1x) - with autoreply, no SessionEnd|
+------------------------------------------------------------------+
                              |
                              v
+------------------------------------------------------------------+
| Phase 3: Git Integration                                          |
|   - PrepareCommitMessage (measured explicitly)                    |
|   - Final commit with co-authors                                  |
+------------------------------------------------------------------+
                              |
                              v
+------------------------------------------------------------------+
| Phase 4: Query Operations                                         |
|   - `aiki blame src/file_1.rs`                                    |
|   - `aiki authors`                                                |
+------------------------------------------------------------------+
```

**Notes**:
- SessionStart: Cursor has no native hook, marked N/A (not recorded)
- PostResponse: Includes SessionEnd cleanup (no autoreply path)
- PostResponse+Autoreply: Session continues, no SessionEnd (autoreply path)
- SessionEnd overhead = PostResponse - PostResponse+Autoreply

---

## Implementation

### Event Simulation

Each event type needs a simulation function that creates the appropriate payload and invokes the hook handler.

#### PrePrompt

```rust
fn simulate_pre_prompt(vendor: Vendor, session: &Session, repo_path: &Path) -> Result<Duration> {
    let start = Instant::now();

    match vendor {
        Vendor::ClaudeCode => {
            let payload = json!({
                "session_id": session.id,
                "hook_event_name": "UserPromptSubmit",
                "prompt": "Add error handling to the parse function",
                "cwd": repo_path,
                "transcript_path": "/dev/null"
            });
            invoke_hook("claude-code", &payload)?;
        }
        Vendor::Cursor => {
            let payload = json!({
                "hook_event_name": "beforeSubmitPrompt",
                "prompt": "Add error handling to the parse function",
                "conversation_id": session.id,
                "workspace_roots": [repo_path],
            });
            invoke_hook("cursor", &payload)?;
        }
    }

    Ok(start.elapsed())
}
```

#### PreFileChange

```rust
fn simulate_pre_file_change(vendor: Vendor, session: &Session, file_path: &Path) -> Result<Duration> {
    let start = Instant::now();

    match vendor {
        Vendor::ClaudeCode => {
            let payload = json!({
                "session_id": session.id,
                "hook_event_name": "PreToolUse",
                "tool_name": "Edit",
                "tool_input": {
                    "file_path": file_path,
                    "command": "str_replace"
                },
                "cwd": session.cwd,
                "transcript_path": "/dev/null"
            });
            invoke_hook("claude-code", &payload)?;
        }
        Vendor::Cursor => {
            let payload = json!({
                "hook_event_name": "beforeShellExecution",
                "conversation_id": session.id,
                "workspace_roots": [session.cwd],
                "shell_command": format!("edit {}", file_path.display()),
            });
            invoke_hook("cursor", &payload)?;
        }
    }

    Ok(start.elapsed())
}
```

#### PostFileChange (existing, expanded)

```rust
fn simulate_post_file_change(vendor: Vendor, session: &Session, file_path: &Path, edit_num: usize) -> Result<Duration> {
    let start = Instant::now();
    let new_content = format!("    println!(\"Edit {}\");", edit_num);

    match vendor {
        Vendor::ClaudeCode => {
            let payload = json!({
                "session_id": session.id,
                "hook_event_name": "PostToolUse",
                "tool_name": "Edit",
                "tool_input": {
                    "file_path": file_path,
                    "old_string": "",
                    "new_string": new_content,
                    "cwd": session.cwd,
                },
                "cwd": session.cwd,
                "transcript_path": "/dev/null"
            });
            invoke_hook("claude-code", &payload)?;
        }
        Vendor::Cursor => {
            let payload = json!({
                "hook_event_name": "afterFileEdit",
                "conversation_id": session.id,
                "workspace_roots": [session.cwd],
                "file_path": file_path,
                "edits": [{
                    "old_string": "",
                    "new_string": new_content
                }]
            });
            invoke_hook("cursor", &payload)?;
        }
    }

    Ok(start.elapsed())
}
```

#### PostResponse

```rust
fn simulate_post_response(vendor: Vendor, session: &Session) -> Result<Duration> {
    let start = Instant::now();

    match vendor {
        Vendor::ClaudeCode => {
            let payload = json!({
                "session_id": session.id,
                "hook_event_name": "Stop",
                "stop_hook_active": true,
                "cwd": session.cwd,
                "transcript_path": "/dev/null"
            });
            invoke_hook("claude-code", &payload)?;
        }
        Vendor::Cursor => {
            let payload = json!({
                "hook_event_name": "stop",
                "status": "completed",
                "loop_count": 0,
                "conversation_id": session.id,
                "workspace_roots": [session.cwd],
            });
            invoke_hook("cursor", &payload)?;
        }
    }

    Ok(start.elapsed())
}
```

#### PrepareCommitMessage

```rust
fn simulate_prepare_commit_msg(repo_path: &Path) -> Result<Duration> {
    let commit_msg_file = repo_path.join(".git/COMMIT_EDITMSG");
    std::fs::write(&commit_msg_file, "Test commit\n")?;

    let start = Instant::now();

    // Call handler directly (not via subprocess - this is a git hook)
    let payload = AikiPrepareCommitMessagePayload {
        agent_type: AgentType::ClaudeCode,
        cwd: repo_path.to_path_buf(),
        timestamp: Utc::now(),
        commit_msg_file: Some(commit_msg_file),
    };
    events::handle_prepare_commit_msg(payload)?;

    Ok(start.elapsed())
}
```

### Statistics (Fixed Configuration)

```rust
const WARMUP_ITERATIONS: usize = 3;
const MEASUREMENT_RUNS: usize = 3;
const DEFAULT_EDITS: usize = 50;

struct EventStats {
    samples: Vec<Duration>,
}

impl EventStats {
    fn p50(&self) -> Duration { self.percentile(50) }
    fn p95(&self) -> Duration { self.percentile(95) }
    fn min(&self) -> Duration { *self.samples.iter().min().unwrap() }
    fn max(&self) -> Duration { *self.samples.iter().max().unwrap() }
}
```

---

## Output Format

### Terminal Output

```
Aiki Benchmark
==============
Date: 2025-12-11 14:30:00 UTC
Edits: 50 x 3 runs = 150 samples per event

Warmup: 3 iterations... done

Running benchmarks...
  Claude Code: [====================] 100%
  Cursor:      [====================] 100%

Results by Event (p50 / p95 / max):
+------------------------+---------------------+---------------------+
| Event                  | Claude Code         | Cursor              |
+------------------------+---------------------+---------------------+
| SessionStart           | 135 / 142 / 145 ms  | N/A                 |
| PrePrompt              |  12 /  18 /  24 ms  |   8 /  12 /  15 ms  |
| PreFileChange          |   5 /   8 /  12 ms  |   6 /   9 /  14 ms  |
| PostFileChange         |  98 / 125 / 142 ms  |  95 / 120 / 138 ms  |
| PostResponse           |  45 /  52 /  58 ms  |  42 /  48 /  55 ms  |
| PostResponse+Autoreply |  12 /  15 /  18 ms  |  10 /  13 /  16 ms  |
| PrepareCommitMessage   |  45 /  52 /  55 ms  | (same)              |
+------------------------+---------------------+---------------------+

SessionEnd overhead (calculated): ~33ms (PostResponse - PostResponse+Autoreply)

Query Operations:
  blame:   123ms
  authors:  89ms

Total: 12.34s

vs Previous Run:
  PostFileChange p50: 102ms -> 98ms  -3.9%
  PrePrompt p50:       11ms -> 12ms  +9.1%
  Overall:          12.89s -> 12.34s -4.3%
```

### metrics.json

```json
{
  "version": 2,
  "timestamp": "2025-12-11T14:30:00Z",
  "config": {
    "edits": 50,
    "warmup": 3,
    "runs": 3
  },
  "vendors": {
    "claude-code": {
      "SessionStart":          { "p50": 135, "p95": 142, "max": 145, "samples": 3 },
      "PrePrompt":             { "p50": 12,  "p95": 18,  "max": 24,  "samples": 3 },
      "PreFileChange":         { "p50": 5,   "p95": 8,   "max": 12,  "samples": 150 },
      "PostFileChange":        { "p50": 98,  "p95": 125, "max": 142, "samples": 150 },
      "PostResponse":          { "p50": 45,  "p95": 52,  "max": 58,  "samples": 3 },
      "PostResponseAutoreply": { "p50": 12,  "p95": 15,  "max": 18,  "samples": 3 }
    },
    "cursor": {
      "PrePrompt":             { "p50": 8,   "p95": 12,  "max": 15,  "samples": 3 },
      "PreFileChange":         { "p50": 6,   "p95": 9,   "max": 14,  "samples": 150 },
      "PostFileChange":        { "p50": 95,  "p95": 120, "max": 138, "samples": 150 },
      "PostResponse":          { "p50": 42,  "p95": 48,  "max": 55,  "samples": 3 },
      "PostResponseAutoreply": { "p50": 10,  "p95": 13,  "max": 16,  "samples": 3 }
    }
  },
  "shared": {
    "PrepareCommitMessage": { "p50": 45, "p95": 52, "max": 55, "samples": 3 }
  },
  "queries": {
    "blame_ms": 123,
    "authors_ms": 89
  },
  "total_ms": 12340,
  "previous_run": "2025-12-10T10:15:00Z"
}
```

**Note**: Cursor's `SessionStart` is absent from metrics.json (not "0 ms") because the vendor doesn't support it. Unsupported events are omitted entirely.

---

## Implementation Plan

### Phase 1: Full Event Coverage

1. [x] Add `simulate_pre_prompt()` for both vendors
2. [x] Add `simulate_pre_file_change()` for both vendors
3. [x] Add `simulate_post_response()` for both vendors
4. [x] Add `simulate_prepare_commit_msg()` (shared)
5. [x] Update main loop to call all events in lifecycle order

### Phase 2: Multi-Vendor (Always On)

1. [x] Add `Vendor` enum and iteration
2. [x] Run full lifecycle for each vendor sequentially
3. [x] Store results per-vendor in metrics
4. [x] Add side-by-side comparison in output

### Phase 3: Statistics & Reliability

1. [x] Add warmup phase (3 iterations, not measured)
2. [x] Add multiple runs (3x) with aggregation
3. [x] Implement p50/p95/max percentile calculations
4. [x] Add comparison against previous run

---

## Files to Modify

- `cli/src/commands/benchmark.rs` - Main implementation + fixes
- `cli/src/event_bus.rs` - Add AIKI_BENCHMARK_FORCE_AUTOREPLY check

## Fixes Applied (see fix.md)

1. [x] Issue 1: Return `Option<Duration>` for unsupported events, omit from metrics
2. [x] Issue 2: Fix Cursor PreFileChange payload (`toolName` not `tool_name`)
3. [x] Issue 3: Set `AIKI_COMMIT_MSG_FILE` env var for PrepareCommitMessage
4. [x] Issue 4: Add PostResponse+Autoreply benchmark with env var control

## Files for Reference

- `cli/src/vendors/claude_code.rs` - Payload structures
- `cli/src/vendors/cursor.rs` - Payload structures
- `cli/src/events/*.rs` - Event handlers
