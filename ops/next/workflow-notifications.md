---
status: draft
---

# Workflow Notifications: Desktop Alerts for CLI Commands

**Status**: Draft
**Priority**: P2
**Location**: ops/next

---

## Summary

Send desktop notifications automatically when long-running aiki CLI commands complete. No configuration — if the OS supports it, notifications fire. This gives users awareness of background work without watching the terminal.

---

## Problem

Long-running commands (`aiki task run`, `aiki build`, `aiki task wait`) block in the terminal while an agent works. Users switch to other windows and lose track of when work finishes. Today there's no built-in way to get notified — you either watch the terminal or check back manually.

The flow engine has example `notify:` actions in docs (`workflow-hook-commands.md`, `slack-notify` plugin) but these are user-authored hooks. This spec covers **built-in CLI notifications** — zero setup, just works.

---

## Design

### Which commands notify

| Command | When | Message |
|---------|------|---------|
| `aiki task run <id>` | Agent session completes | "Task completed: {task_name}" or "Task failed: {task_name}" |
| `aiki task wait <id>` | Polled task reaches terminal state | "Task ready: {task_name}" |
| `aiki build <spec>` | Build orchestration finishes | "Build complete: {spec_name}" or "Build failed: {spec_name}" |
| `aiki review <id> --start` | Review finishes | "Review complete: {task_name}" |
| `aiki fix <id>` | Fix task finishes | "Fix complete: {task_name}" |

### When NOT to notify

- Commands that return instantly (< 2s) — no notification needed
- Commands already running in the foreground interactively (agent is in the same terminal the user is watching)
- When stdout is not a TTY (piped output, CI) — skip notifications

The 2-second threshold avoids spamming for fast operations. Start a timer when the command begins; only notify if the wall-clock duration exceeds the threshold.

### Notification content

Keep it minimal:

```
Title: Aiki
Body:  ✓ Task completed: Fix auth handler  (1m 23s)
```

```
Title: Aiki
Body:  ✗ Build failed: ops/now/feature.md  (4m 12s)
```

Include:
- Success/failure indicator
- Command type + target name (task name, spec filename)
- Duration

Omit:
- Task IDs (not useful in a desktop notification)
- Detailed error messages (user will see those in the terminal)

### Platform support

| Platform | Mechanism | Notes |
|----------|-----------|-------|
| macOS | `osascript -e 'display notification ...'` | No dependencies, works everywhere |
| Linux | `notify-send` (libnotify) | Best-effort — skip silently if not installed |

Windows is out of scope (aiki doesn't support Windows currently).

### Suppression

- `--quiet` / `-q` flag on commands suppresses notifications (already exists on some commands for other output)
- `AIKI_NOTIFY=0` environment variable disables globally
- No notification if command duration < 2 seconds

---

## Implementation

### `cli/src/notify.rs` — notification helper module

A small module with a single public function:

```rust
use std::process::Command;
use std::time::Duration;

/// Send a desktop notification (best-effort, never fails the caller)
pub fn desktop_notify(title: &str, body: &str) {
    if std::env::var("AIKI_NOTIFY").as_deref() == Ok("0") {
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            body.replace('"', "\\\""),
            title.replace('"', "\\\""),
        );
        let _ = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("notify-send")
            .arg(title)
            .arg(body)
            .output();
    }
}

/// Format a notification body for a completed command
pub fn format_completion(success: bool, kind: &str, name: &str, duration: Duration) -> String {
    let icon = if success { "✓" } else { "✗" };
    let verb = if success {
        match kind {
            "build" => "complete",
            "review" => "complete",
            "fix" => "complete",
            _ => "completed",
        }
    } else {
        "failed"
    };
    let elapsed = format_duration(duration);
    format!("{icon} {kind} {verb}: {name}  ({elapsed})")
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}
```

### Wiring into commands

Each long-running command wraps its work with a timer and calls `desktop_notify` after:

```rust
// In task run, build, etc.
let start = std::time::Instant::now();

// ... do the actual work ...

let duration = start.elapsed();
if duration > Duration::from_secs(2) && std::io::stdout().is_terminal() {
    let body = notify::format_completion(success, "task", &task_name, duration);
    notify::desktop_notify("Aiki", &body);
}
```

### Files to create/modify

| File | Change |
|------|--------|
| `cli/src/notify.rs` | **New** — notification helper module |
| `cli/src/lib.rs` | Add `pub mod notify;` |
| `cli/src/commands/task.rs` | Wire notification into `run` subcommand handler |
| `cli/src/commands/build.rs` | Wire notification into `run_build_spec` and `run_build_plan` |
| `cli/src/commands/review.rs` | Wire notification into review-with-start path |
| `cli/src/commands/fix.rs` | Wire notification into fix execution |
| `cli/src/commands/wait.rs` | Wire notification into `poll_task_status` completion |

### Testing

- Unit test `format_completion` for various inputs
- Unit test `format_duration` edge cases (0s, 59s, 60s, 90s)
- Integration: mock/stub the `osascript`/`notify-send` call to verify it's invoked with correct arguments
- Verify `AIKI_NOTIFY=0` suppresses the call
- Verify non-TTY suppresses the call

---

## Non-goals

- **Flow engine `notify:` action** — that's a separate concern (user-authored hooks). This spec only covers built-in CLI notifications.
- **Slack/webhook notifications** — out of scope. Users who want Slack alerts use the flow engine with `shell:` or `http:` actions.
- **Notification configuration** — no config file, no channel selection. Desktop only, always on (unless suppressed).
- **TUI integration** — no notification center or toast UI.
- **Sound** — OS default notification sound is fine (controlled by OS settings).

## Resolved Questions

1. **`aiki task run --async` does not notify.** Only synchronous (blocking) commands send desktop notifications. Async has no parent process watching — a different mechanism (e.g., `task.closed` hook) would be needed later if desired.
2. **No `--notify` flag.** Notifications are automatic for TTY contexts only. No flag to force them in non-TTY/CI.
