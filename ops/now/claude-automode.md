# Migrate Background Runner from `--dangerously-skip-permissions` to Automode

## Status: Planning

## Context

Aiki spawns Claude Code sessions in three modes (blocking, background, monitored) via
`src/agents/runtime/claude_code.rs`. All three currently pass `--dangerously-skip-permissions`,
which blindly auto-approves every tool call with zero safety checks. This was acceptable
for early development but is a liability for production use — a prompt-injected file or
malicious web content can exfiltrate secrets, destroy data, or modify infrastructure
without any guardrail.

Claude Code now ships **automode** (`--permission-mode auto`), a safer alternative that
uses an AI classifier (Sonnet 4.6) to review each tool call before execution. It blocks
destructive or off-task actions while still allowing autonomous operation.

## What Automode Does

Before each tool call executes, a dedicated classifier evaluates:

1. **Task scope** — Is this action within what was actually requested?
2. **Target trust** — Is it operating on trusted infrastructure?
3. **Intent shift** — Are there signs Claude was influenced by hostile content?

### Default behavior

| Allowed by default | Blocked by default |
|---|---|
| File edits in working directory | `curl \| bash` (download-and-execute) |
| Read-only operations | Sending secrets to unknown endpoints |
| Bash commands matching user intent | Production deploys / migrations |
| Installing deps from lock files | Mass deletion on cloud storage |
| Reading `.env` and sending creds to matching APIs | Granting IAM / repo permissions |
| Pushing to current branch or Claude-created branch | Force push, push to main |

### Graceful fallback

If the classifier blocks an action 3× in a row or 20× total in a session, automode
pauses and switches back to interactive prompting. Approving a prompted action resets
the counters. **This means automode is not fully non-interactive** — it can fall back
to prompting, which will hang a headless runner.

## Migration Plan

### Phase 1: Configuration infrastructure

**Goal:** Add automode settings without changing runtime behavior yet.

- [ ] Add `autoMode` block to Aiki's managed Claude Code settings
  (`src/config.rs`, the hook-installation path that writes `~/.claude/settings.json`):

  ```json
  {
    "autoMode": {
      "environment": [
        "Tool: Aiki — AI change-tracking layer over JJ/Git",
        "Source control: JJ (internal) and Git (user-facing)",
        "All operations are local, no cloud infrastructure"
      ],
      "allow": [
        "Writing files anywhere in the working directory is allowed",
        "Running cargo build, cargo test, cargo clippy is allowed",
        "Running jj and git read-only commands is allowed",
        "Creating JJ changes via jj describe / jj new is allowed"
      ],
      "soft_deny": [
        "Never run git push or jj git push",
        "Never run rm -rf on directories outside the working copy",
        "Never modify ~/.ssh, ~/.gitconfig, or ~/.claude/settings.json",
        "Never install global packages (cargo install, npm -g, pip install)"
      ]
    }
  }
  ```

- [ ] Add a CLI flag / config option to `AgentSpawnOptions` to select permission mode:
  `permission_mode: PermissionMode` enum with variants `DangerouslySkip`, `Auto`, `DontAsk`.

- [ ] Default to `DangerouslySkip` initially so behavior is unchanged.

### Phase 2: Swap the flag in `claude_code.rs`

**Goal:** Replace `--dangerously-skip-permissions` with `--permission-mode auto`.

The change is mechanical — in all three spawn methods:

```rust
// Before
.args(["--print", "--dangerously-skip-permissions", &prompt])

// After (when permission_mode == Auto)
.args(["--print", "--permission-mode", "auto", &prompt])
```

- [ ] Update `spawn_blocking()` — uses the new `permission_mode` field.
- [ ] Update `spawn_background()` — same.
- [ ] Update `spawn_monitored()` — same.
- [ ] Keep `--dangerously-skip-permissions` path behind the `DangerouslySkip` variant
  as an escape hatch (feature-flagged, not default).

### Phase 3: Handle the non-interactive fallback problem

**Critical issue:** When automode hits its block threshold it falls back to interactive
prompting. In a headless background runner (`stdin = Stdio::null()`), this will hang
the process indefinitely.

Options to evaluate:

1. **`--permission-mode dontAsk` with explicit allow rules** — Fully non-interactive,
   never prompts, but requires exhaustive allow-list. Safest for CI/headless.
   Claude Code will deny (not hang) on anything not explicitly allowed.

2. **Automode + timeout watchdog** — Keep automode but wrap spawn with a timeout.
   If the process hangs (blocked waiting for input), kill and report the block.
   Already partially handled by `MonitoredChild`.

3. **Hybrid: automode for monitored, `dontAsk` for background** — Use automode where
   we can detect hangs (monitored mode), use `dontAsk` for fully detached background
   sessions where we can't intervene.

**Recommendation:** Option 3 (hybrid). Monitored sessions already capture stdout/stderr
and can detect hangs. Background sessions are fire-and-forget and must never block.

- [ ] Implement timeout detection in `MonitoredChild` for stdin-blocked processes.
- [ ] Use `--permission-mode auto` for monitored sessions.
- [ ] Use `--permission-mode dontAsk` with allow rules for background sessions.
- [ ] Document the tradeoff in code comments.

### Phase 4: Tune and validate

- [ ] Run `claude auto-mode defaults` and review built-in rules against Aiki's use cases.
- [ ] Run `claude auto-mode critique` on our custom rules to check for ambiguity.
- [ ] Integration test: spawn a monitored session with automode, verify it completes
  a file-edit task without prompting.
- [ ] Integration test: verify automode blocks `git push` (per our `soft_deny` rules).
- [ ] Integration test: verify background `dontAsk` session completes without hanging.
- [ ] Update `tests/README_CLAUDE_INTEGRATION.md` with new flag expectations.

### Phase 5: Remove `--dangerously-skip-permissions` entirely

- [ ] Remove the `DangerouslySkip` variant from `PermissionMode` enum.
- [ ] Remove all `--dangerously-skip-permissions` references from code and tests.
- [ ] Update CLAUDE.md if it references the old flag.

## Files to Modify

| File | Change |
|---|---|
| `src/agents/runtime/claude_code.rs` | Swap CLI args based on permission mode |
| `src/agents/runtime/mod.rs` | Add `PermissionMode` to `AgentSpawnOptions` |
| `src/config.rs` | Add `autoMode` block to managed settings |
| `src/tasks/runner.rs` | Pass permission mode through to spawn options |
| `tests/claude_integration_test.rs` | Update expected CLI args |
| `tests/README_CLAUDE_INTEGRATION.md` | Document new permission modes |

## Key Risks

1. **Automode is research preview** — API/behavior may change. Pin to known Claude Code
   version in CI if possible.

2. **Classifier false positives** — May block legitimate Aiki operations (e.g., `jj describe`
   modifying change descriptions). Mitigate with explicit `allow` rules.

3. **Non-interactive hang** — The fallback-to-prompting behavior is the biggest risk for
   background runners. The hybrid approach (Phase 3) addresses this directly.

4. **Performance overhead** — Each tool call now has a classifier round-trip. Unlikely to
   matter for Aiki's workloads but worth measuring.

## References

- [Claude Code Permission Modes docs](https://code.claude.com/docs/en/permission-modes.md)
- [Auto mode blog post](https://claude.com/blog/auto-mode)
- [Claude Code Permissions config](https://code.claude.com/docs/en/permissions.md)
- CLI commands: `claude auto-mode defaults`, `claude auto-mode config`, `claude auto-mode critique`
