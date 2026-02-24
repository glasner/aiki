---
status: done
---

# Kill Orphaned Zed ACP Processes

**Date**: 2026-02-24
**Status**: Done
**Priority**: P1

---

## Problem

When Zed crashes, force-quits, or fails to clean up Rc references, the `aiki hooks acp` → `claude` process tree is orphaned (reparented to PID 1). These processes accumulate indefinitely — 39 stale `claude` processes were observed consuming memory after a week of normal use.

### Root Cause

The ACP protocol has **no shutdown handshake**. Session lifecycle is: create → prompt → cancel (in-flight only) → transport dies. Zed's termination path relies on Rust `Drop` semantics (`AcpConnection::drop()` → `child.kill()`), which fails when:

1. **Force quit / crash** — Drop never fires, child processes orphaned
2. **Rc reference leaks** — Async tasks hold references, preventing Drop
3. **Panel close** — No explicit cleanup; relies on refcount reaching zero

Zed fixed the normal-quit case in v0.203.4 ([zed#37741](https://github.com/zed-industries/zed/issues/37741)), but crashes and ref leaks remain unaddressed.

### What We Can Detect

| Signal | When it fires | Reliable? |
|--------|---------------|-----------|
| stdin EOF | Zed properly drops AcpConnection | Yes, post-v0.203.4 for normal quit |
| Parent PID gone | Zed crashes or force-quits | Yes — universal fallback |
| ACP `session/cancel` | Never sent as shutdown signal | No — not a shutdown mechanism |
| ACP `session/end` | Does not exist in protocol | N/A |

## Solution: Parent-PID Watchdog Thread

Add a watchdog thread to the ACP proxy (`cli/src/commands/acp.rs`) that periodically checks if the parent process (Zed) is still alive. When Zed disappears, the watchdog triggers a clean shutdown of the `claude` child process and exits the proxy.

### Design

```
┌─────────────────────────────────────────────────┐
│                  ACP Proxy Process               │
│                                                  │
│  ┌──────────────────┐  ┌─────────────────────┐  │
│  │ IDE → Agent       │  │ Agent → IDE          │  │
│  │ Thread            │  │ Thread (main)        │  │
│  └──────────────────┘  └─────────────────────┘  │
│                                                  │
│  ┌──────────────────┐  ┌─────────────────────┐  │
│  │ Autoreply        │  │ Parent-PID Watchdog  │  │ ← NEW
│  │ Forwarder        │  │ Thread               │  │
│  └──────────────────┘  └─────────────────────┘  │
│                                                  │
└─────────────────────────────────────────────────┘
```

### Implementation

**Location**: `cli/src/commands/acp.rs`, spawned early alongside the other threads.

```rust
// Capture parent PID before spawning watchdog.
// IMPORTANT: parent_id() calls getppid(), which returns the *current* parent at call time.
// We must capture it here (before any reparenting could occur) to get Zed's PID.
let parent_pid = std::os::unix::process::parent_id();
let agent_child_id = agent.id();  // PID of the claude process

// Shared shutdown flag for coordinated cleanup between stdin EOF and watchdog
let shutdown = Arc::new(AtomicBool::new(false));
let shutdown_watchdog = Arc::clone(&shutdown);

thread::spawn(move || {
    loop {
        thread::sleep(Duration::from_secs(5));

        // Check if normal shutdown has already occurred
        if shutdown_watchdog.load(Ordering::Relaxed) {
            return;
        }

        // kill(pid, 0) checks if process exists without sending a signal
        let parent_alive = unsafe { libc::kill(parent_pid as i32, 0) } == 0;

        if !parent_alive {
            eprintln!("ACP Proxy: Parent process (PID {}) gone, shutting down", parent_pid);

            // Log watchdog activation for metrics tracking
            eprintln!("ACP Proxy: Watchdog triggered orphan cleanup");

            // Kill the claude child process
            // Note: If claude has already exited, SIGTERM will fail with ESRCH (harmless)
            unsafe { libc::kill(agent_child_id as i32, libc::SIGTERM); }

            // Give it a moment to clean up, then force kill
            thread::sleep(Duration::from_secs(2));
            unsafe { libc::kill(agent_child_id as i32, libc::SIGKILL); }

            // Exit the proxy process (kills all threads, including stdin handler)
            std::process::exit(0);
        }
    }
});

// In the stdin EOF handler (normal shutdown path):
// shutdown.store(true, Ordering::Relaxed);
// ... existing cleanup ...
```

### Behavior

| Scenario | What happens |
|----------|-------------|
| **Zed normal quit** | stdin EOF fires first (existing path). Sets shutdown flag, watchdog exits gracefully. |
| **Zed crash** | stdin hangs. Watchdog detects parent gone within 5s, kills claude, exits. |
| **Zed force quit** | Same as crash — watchdog catches it. |
| **Panel close (Zed alive)** | Parent PID still exists → watchdog does nothing. stdin EOF handles cleanup. |
| **Panel close (Rc leak)** | Parent PID still exists → watchdog does nothing. Process stays alive (matches Zed's intent — session persists for potential resume). |

### Edge Cases

1. **Reparented to PID 1 (init/launchd)**: We capture `parent_id()` at spawn time, before any reparenting. If the parent dies, `kill(captured_parent_pid, 0)` returns `-1` with `errno=ESRCH` (process not found) — correctly detected as "gone."

2. **Race between proxy startup and parent death**: If Zed crashes between `parent_id()` capture and watchdog thread spawn, the watchdog will poll a dead PID and immediately trigger shutdown. This is correct behavior (orphan cleanup).

3. **`agent.id()` when child has exited**: `std::process::Child::id()` returns `u32`, but if the child has already exited before the watchdog fires, the `kill(agent_child_id, SIGTERM)` call will fail with `ESRCH` — harmless, no action needed.

4. **PID reuse**: Extremely unlikely within the 5s polling window. macOS uses PID randomization and a large PID space. Even if it happened, the worst case is the proxy stays alive one extra poll cycle.

5. **macOS sandbox**: `kill(pid, 0)` with signal 0 only checks existence — no actual signal is sent. Works fine regardless of sandbox restrictions.

6. **SIGTERM vs SIGKILL**: We send SIGTERM first to let claude clean up (e.g., save conversation state), then SIGKILL after 2s as a fallback.

7. **Watchdog vs stdin EOF race**: The `AtomicBool` shutdown flag prevents the watchdog from wasting cycles after stdin EOF fires. This avoids redundant cleanup and potential double-kill scenarios.

### Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| **Parent-PID watchdog** (chosen) | Simple, reliable, zero dependencies | 5s detection latency |
| **`prctl(PR_SET_PDEATHSIG)`** | Instant detection | Linux-only, not available on macOS |
| **kqueue/EVFILT_PROC** | Instant, macOS-native | Complex, requires unsafe FFI |
| **launchd periodic cleanup** | Catches all orphans | External dependency, not portable |
| **`aiki cleanup` command** | User-controlled | Manual, easy to forget |

The parent-PID polling approach is the best tradeoff: simple, cross-platform (macOS + Linux), and catches all crash/force-quit scenarios with minimal latency.

### Future: kqueue Upgrade (Skipped for v1)

If the 5s latency matters (unlikely — orphan cleanup doesn't need to be instant), we could upgrade to macOS `kqueue` with `EVFILT_PROC` + `NOTE_EXIT` for instant notification. This would replace the polling loop with an event-driven wait:

```rust
// macOS only — instant parent death notification
let kq = unsafe { libc::kqueue() };
let event = libc::kevent {
    ident: parent_pid as usize,
    filter: libc::EVFILT_PROC,
    flags: libc::EV_ADD | libc::EV_ONESHOT,
    fflags: libc::NOTE_EXIT,
    ..
};
// kevent() blocks until parent exits
```

**Decision**: Skip this optimization unless user reports indicate the 5-7s cleanup latency is problematic. For orphan cleanup, this latency is perfectly acceptable — nobody is watching these processes die.

## Tasks

1. Add `libc` dependency (if not already present)
2. Spawn watchdog thread in `acp.rs` before the IDE→Agent thread
3. Add `Arc<AtomicBool>` shutdown flag, shared between watchdog and stdin EOF handler
4. Update stdin EOF handler to set shutdown flag before cleanup
5. Pass child PID to watchdog (from `agent.id()`)
6. Add log line when watchdog fires (for metrics tracking)
7. Add integration test: spawn proxy with a mock parent, kill parent, verify proxy exits
8. Manual test: force-quit Zed, verify claude processes are cleaned up within ~7s
