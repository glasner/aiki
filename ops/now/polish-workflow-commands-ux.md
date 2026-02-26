---
status: draft
---

# Polish Workflow Commands UX

**Date**: 2025-02-14
**Status**: Draft
**Purpose**: Improve the UX and output of `aiki plan`, `aiki build`, `aiki review`, and `aiki fix` for wider release. Migrate terminal UI from raw crossterm to ratatui.

**Related Documents**:
- [TUI Kanban Board](tui.md) - Full TUI vision (ratatui)

---

## Executive Summary

The four workflow commands (`plan`, `build`, `review`, `fix`) are functional but have inconsistent output formatting, a confirmed bug in `plan`, and several UX rough edges that would confuse new users. Additionally, the two places that use raw `crossterm` for terminal control (plan prompt and status monitor) should be migrated to ratatui, which will both improve the immediate UX and lay groundwork for the full TUI kanban board.


---

## Ratatui Migration

### Current State

`crossterm` 0.28 is used directly in two files:

| File | Usage | What It Does |
|------|-------|-------------|
| `cli/src/commands/plan.rs` | `prompt_multiline_input()` | Raw mode text input (Enter/Shift+Enter/Esc/Backspace handling) |
| `cli/src/tasks/status_monitor.rs` | `StatusMonitor` | Cursor save/restore + clear for live task tree display during `task run` |

### Why Migrate

1. **ratatui includes crossterm** — ratatui 0.30 re-exports crossterm (0.28 or 0.29 via feature flags), so adding ratatui and removing the direct crossterm dep is a clean swap
2. **Better input widgets** — The hand-rolled `prompt_multiline_input` in plan.rs is fragile (no cursor movement, no word wrap, no paste support). Ratatui's ecosystem has proper text input widgets (tui-textarea, tui-input)
3. **Better status display** — The status monitor's manual cursor save/restore is brittle. Ratatui's frame-based rendering handles redraws cleanly
4. **Foundation for TUI** — Both components become natural building blocks for the full kanban TUI (see tui.md)

### Migration Plan

**Dependency change** in `cli/Cargo.toml`:
```toml
# Remove:
crossterm = "0.28"

# Add:
ratatui = { version = "0.30", default-features = true }  # includes crossterm backend
tui-textarea = "0.7"  # for plan prompt input widget
```

**plan.rs prompt → ratatui text area:**
- Replace `prompt_multiline_input()` with a ratatui mini-app
- Use `tui-textarea` for proper multi-line input with cursor movement, word wrap, paste
- Render in an inline area (not full-screen) — just the prompt section
- Keep the same keybindings: Enter to submit, Shift+Enter for newline, Esc to skip
- Much better UX: arrow keys work, Home/End work, multi-line editing works

**status_monitor.rs → ratatui frame rendering:**
- Replace manual cursor save/restore/clear with ratatui's `Terminal::draw()`
- Use inline viewport (`viewport::Inline`) to render in-place without taking over the whole screen
- Task tree rendered as a ratatui `List` or custom widget
- Progress bar widget for elapsed time
- Clean teardown on exit (ratatui handles terminal restore)

### Inline Viewport Pattern

Both the plan prompt and status monitor should use ratatui's **inline viewport** — this renders a fixed-height region in the terminal without taking over the full screen. Perfect for CLI tools that need a small interactive region within normal command output.

```rust
// Inline viewport: renders N lines at current cursor position
let backend = CrosstermBackend::new(stderr());
let terminal = Terminal::with_options(
    backend,
    TerminalOptions {
        viewport: Viewport::Inline(height),
    },
)?;
```

---

## Output Inconsistencies

### Problem 1: Inconsistent Output Structure Across Commands

Each command formats its output differently:

| Command | Output Style | Uses `CommandOutput`? | Uses `MdBuilder`? |
|---------|-------------|----------------------|-------------------|
| `plan` | Custom markdown with `## Plan Started/Completed/Error` | No | Yes (wrapper only) |
| `build` | Custom markdown with `## Build Started/Completed` | No | Yes (wrapper only) |
| `review` | Structured via `CommandOutput` | Yes | Yes |
| `fix` | Structured via `CommandOutput` | Yes | Yes |

`review` and `fix` share the `CommandOutput` struct from `output.rs`, but `plan` and `build` have their own ad-hoc formatting.

### Fix

Extend `CommandOutput` to be usable by all four commands, or create a shared output pattern. Key fields all commands need:
- Heading (action name)
- Task ID
- Status message
- Optional scope/target info
- Optional hint/next-step

### Problem 2: Full 32-char Task IDs in Output

All four commands show the full 32-character task ID in output, e.g.:
```
- **Task:** qotysworupowzkxyknzkworuwlyksmls
```

But `task` commands use `short_id()` (7 chars). Users can't easily copy/paste or remember either form. The output should use short IDs consistently and show the full ID only in `show` commands.

### Fix

Use `short_id()` in all command output. The `md.rs` module already has this function.

### Problem 3: Build Output Shows Raw Plan/Build IDs

Build output shows:
```
## Build Started
- **Build ID:** <32 chars>
- **Plan ID:** pending
```

This is confusing — "pending" isn't helpful, and showing both IDs is noisy. A simpler output:
```
Build started for ops/now/feature.md
Task: abcdefg
```

### Problem 4: Review/Fix Output Shows Internal Type/Scope Fields

```
- **Type:** task
- **Scope:** Task (xqrmnpst)
```

These are internal concepts. Users care about *what* is being reviewed, not the taxonomy. Better:
```
Review started for task abcdefg
```

### Problem 5: XML Artifacts Still in Fix Command

`fix.rs:52-62` still parses XML `task_id=""` attributes from stdin, a leftover from the pre-markdown output era. This should be cleaned up to only handle plain task IDs (or short IDs).

**Deprecation strategy:** Before removing XML parsing, audit whether any existing automation, scripts, or agent prompts rely on the XML `task_id=""` format. Specifically:
1. Search the codebase and templates for anything that emits XML-formatted task IDs
2. If callers exist, add a deprecation warning (to stderr) when XML input is detected: `"Warning: XML task_id format is deprecated, use plain task IDs instead"`
3. Keep XML parsing functional for one release cycle with the warning
4. Remove XML parsing in the following release

If no callers are found (likely, since output was already migrated to markdown), the XML parsing can be removed directly — but document this in the commit message as a breaking change for anyone who may have been scraping the old output format.

---

## UX Issues

### Issue 1: `aiki plan` with No Args Returns an Error

Running `aiki plan` with no arguments returns:
```
Error: No plan path or description provided.
```

For an interactive tool, this is unfriendly. It should prompt for input or show usage examples.

### Issue 2: `aiki build` Output Doesn't Show Plan Path

When running `aiki build ops/now/feature.md`, the output says:
```
## Build Started
- **Build ID:** ...
- **Plan ID:** pending
```

But doesn't echo back the plan path the user provided. The user should see confirmation of *what* is being built.

### Issue 3: Progress During Long-Running Commands

`build` and `review` (default mode) block until the agent finishes. During that time, there's no progress indication visible to the user. The spawned agent session handles its own output, but the wrapper command is silent.

### Issue 4: `aiki review` Session Scope Confusion

Running `aiki review` (no target) reviews "all closed tasks in the current session." But if the user is in a different session context, this is confusing. The "nothing to review" message should explain the scope.

### Issue 5: `aiki build show` is Hidden

`aiki build show <path>` is useful but not discoverable. Should be mentioned in build's started/completed output as a next step.

---

## Implementation Plan

### Phase 1a: Fix the Plan Guidance Bug (P0)

This phase is a standalone bugfix that can be landed independently without any dependency changes.

1. **Simplify plan data flow** — Collapse `args_idea`/`user_text`/`initial_idea` into single `initial_idea`. Remove `user_text` param from `create_plan_task()`, remove `build_user_context()`. Update plan template: `{{data.user_context}}` → `{{data.initial_idea}}`. Include `initial_idea` in Claude prompt.
2. **Test**: Run `aiki plan new-feature.md`, type guidance in the interactive prompt, verify text reaches the spawned agent. Test all scenarios from the table above (CLI text, filename only, interactive, Esc, autogen).

### Phase 1b: Ratatui Migration + Non-TTY Hardening (P1)

Depends on Phase 1a being landed. This is a larger change that swaps the terminal rendering layer and adds non-TTY support.

3. **Add ratatui dependency** — Add `ratatui = "0.30"` and `tui-textarea = "0.7"` to Cargo.toml, remove direct `crossterm = "0.28"` dep
4. **Migrate plan prompt to ratatui** — Replace `prompt_multiline_input()` with ratatui inline viewport + tui-textarea widget
   - Proper multi-line editing (arrow keys, Home/End, paste)
   - Same keybindings: Enter = submit, Shift+Enter = newline, Esc = skip
   - Renders inline (not full-screen)
   - Non-TTY: skip prompt with feedback message
5. **Migrate status monitor to ratatui** — Replace manual cursor ops in `StatusMonitor` with ratatui inline viewport
   - Task tree as ratatui widget
   - Clean terminal restore on exit
   - `RenderMode::Ratatui` vs `RenderMode::PlainText` — internal TTY guard
   - Plain text fallback: one-line appended updates (no cursor manipulation)
6. **Non-TTY hardening** — Add `is_interactive()` utility, `--no-interactive` flag / `AIKI_NO_INTERACTIVE` env var. Auto-resume incomplete plans when stdin is not a TTY. Add feedback messages when interactive prompts are skipped.
7. **Test**: Run `aiki plan new-feature.md`, verify ratatui prompt works. Run `aiki task run`, verify status monitor renders correctly. Test with piped stdin/stdout/stderr to verify graceful degradation.

### Phase 2: Unify Output Format (P1)

6. **Extend `CommandOutput` for all commands** — Make it usable by `plan` and `build` too
   - Add optional `target` field (plan path, plan ID)
   - Make `scope` more generic (not review-specific)
7. **Use short IDs everywhere** — Replace full task IDs with `short_id()` in all output
8. **Simplify build output** — Show plan path, use short IDs, drop "pending" plan ID
9. **Simplify review/fix output** — Replace Type/Scope with natural language description

### Phase 3: Polish Interactions (P2)

10. **Better no-args behavior for `plan`** — Show usage examples or prompt interactively
11. **Deprecate XML parsing in `fix`** — Audit callers of the XML `task_id=""` format. If callers exist, add a deprecation warning and keep parsing for one release, then remove. If no callers exist, remove directly with a breaking-change note in the commit (see Problem 5 deprecation strategy above)
12. **Add next-step hints** — All commands should suggest what to do next:
    - `plan` completed → "Run `aiki build <path>` to implement"
    - `build` completed → "Run `aiki review <task-id>` to review"
    - `review` completed → "Run `aiki fix <review-id>` to remediate" (already done)
    - `fix` completed → "Run `aiki review` to verify"
13. **Confirm plan path in build output** — Echo the plan being built

### Phase 4: Enhanced Rendering (P3)

14. **Ratatui status monitor widgets** — Progress bar, spinner, color-coded status symbols
15. **Better session review messaging** — Explain what "session review" means
16. **Discoverable subcommands** — Mention `build show`, `review list`, `review show` in output hints
17. **Plan prompt enhancements** — Syntax highlighting, auto-complete for file paths

---

## Files to Modify

| File | Changes |
|------|---------|
| `cli/Cargo.toml` | Add ratatui + tui-textarea, remove direct crossterm dep |
| `cli/src/commands/plan.rs` | Simplify to single initial_idea, fix prompt passthrough, replace crossterm prompt with ratatui, non-TTY feedback |
| `.aiki/templates/aiki/plan.md` | Replace `{{data.user_context}}` with `{{data.initial_idea}}` |
| `cli/src/tasks/status_monitor.rs` | Replace crossterm cursor ops with ratatui inline viewport, add `RenderMode` + plain text fallback |
| `cli/src/commands/build.rs` | Unify output format, show plan path, use short IDs, auto-resume in non-TTY |
| `cli/src/commands/review.rs` | Simplify output, use short IDs |
| `cli/src/commands/fix.rs` | Simplify output, use short IDs, clean up XML parsing |
| `cli/src/commands/output.rs` | Extend `CommandOutput` for all commands |
| `cli/src/tasks/md.rs` | May need minor adjustments for shared formatting |
| `cli/src/main.rs` | Add `--no-interactive` global flag |
| `cli/src/commands/mod.rs` or `lib.rs` | Add `is_interactive()` utility function |

---

## Ratatui Showcase Inspiration

Reviewed apps from [ratatui.rs/showcase/apps](https://ratatui.rs/showcase/apps/):

| App | Relevant Pattern |
|-----|-----------------|
| [gitui](https://github.com/extrawurst/gitui) | Multi-panel tabs, async background ops, context-sensitive keybindings. Architecture (async ops + TUI render loop) maps to aiki's agent+kanban model |
| [taskwarrior-tui](https://github.com/kdheepak/taskwarrior-tui) | Task list with live filtering, vim navigation, detail panel. Close to what `aiki task` needs |
| [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) | Kanban board view, dependency graph view, split-pane list+detail. Almost exactly the tui.md spec |
| [television](https://github.com/alexpasmantier/television) | Channel-based data sources, fuzzy search, preview pane. "Channel" concept maps to aiki's scope/source filtering |

**Architecture recommendation**: Component Architecture (per [ratatui docs](https://ratatui.rs/recipes/apps/)) — each panel as an isolated component with its own state. gitui uses this pattern.

---

## Output Style Guide (Target)

### Principles

- First line tells you what happened
- Short task IDs (7 chars), not full 32-char IDs
- Natural language, not field dumps
- Hint suggests the next action
- No markdown headers (`##`) in stderr output — keep it clean for terminal

---

## Output Examples: Current vs Proposed

### Full Lifecycle: `aiki plan` → `aiki build` → `aiki review` → `aiki fix`

These examples show the complete user experience across a typical workflow, including incremental status updates during long-running operations.

---

### Stage 1: `aiki plan dark-mode.md`

**Prompt stage (ratatui text area):**
```
Plan: ops/now/dark-mode.md (Dark Mode)
┌─────────────────────────────────────────────────────────┐
│ I want a dark mode toggle in the settings               │
│ panel. It should respect the OS preference by           │
│ default but allow manual override.█                     │
│                                                         │
└──────────── Enter: submit  Shift+Enter: newline  Esc ──┘
```

**After submit — spawning:**
```
Plan: ops/now/dark-mode.md (qotyswo)
Spawning interactive session...
```

**Interactive session runs (Claude inherits terminal)...**

**Completed:**
```
Plan completed: ops/now/dark-mode.md (qotyswo)
---
Run `aiki build ops/now/dark-mode.md` to implement.
```

**Error:**
```
Error: Plan session failed (qotyswo): Claude exited with code 1
```

**Cancelled (Ctrl+C):**
```
Plan session cancelled by user (qotyswo).
```

---

### Stage 2: `aiki build ops/now/dark-mode.md`

**Initial output (before agent starts):**
```
Build: ops/now/dark-mode.md (mvslrsp)
Planning and executing...
```

**Status monitor — planning stage (live, redraws in place):**
```
● [mvslrsp] Build: dark-mode.md                           5s
├─ ●  .0) Review plan and create subtasks                      5s
│      └─ Reading plan file...
└─ (planning...)
                                                  Ctrl+C: detach
```

**Status monitor — plan created, subtasks appear:**
```
● [mvslrsp] Build: dark-mode.md                          18s
├─ ✓  .0) Review plan and create subtasks
│      └─ Created 3 subtasks
├─ ●  .1) Implement theme context and CSS variables       12s
│      └─ Adding CSS custom properties
├─ ○  .2) Add settings toggle component
└─ ○  .3) Wire up OS preference detection
                                                  Ctrl+C: detach
```

**Status monitor — subtask 1 done, subtask 2 in progress:**
```
● [mvslrsp] Build: dark-mode.md                          47s
├─ ✓  .0) Review plan and create subtasks
│      └─ Created 3 subtasks
├─ ✓  .1) Implement theme context and CSS variables
│      └─ Added ThemeProvider with light/dark tokens
├─ ●  .2) Add settings toggle component                   18s
│      └─ Building ToggleSwitch with animation
└─ ○  .3) Wire up OS preference detection
                                                  Ctrl+C: detach
```

**Status monitor — all subtasks done:**
```
✓ [mvslrsp] Build: dark-mode.md                        1m 23s
├─ ✓  .0) Review plan and create subtasks
│      └─ Created 3 subtasks
├─ ✓  .1) Implement theme context and CSS variables
│      └─ Added ThemeProvider with light/dark tokens
├─ ✓  .2) Add settings toggle component
│      └─ Built ToggleSwitch with smooth CSS transition
└─ ✓  .3) Wire up OS preference detection
       └─ prefers-color-scheme media query + manual override
                                                  Ctrl+C: detach
```

**Final output (after monitor clears):**
```
Build completed: ops/now/dark-mode.md (mvslrsp)
  ✓ Implement theme context and CSS variables
  ✓ Add settings toggle component
  ✓ Wire up OS preference detection
---
Run `aiki review mvslrsp` to review.
```

**Async mode:**
```
Build: ops/now/dark-mode.md (mvslrsp)
Running in background. Use `aiki build show ops/now/dark-mode.md` to check status.
```

**Existing plan prompt (interactive):**
```
Incomplete plan for ops/now/dark-mode.md (2/3 done)
  ✓ Implement theme context and CSS variables
  ✓ Add settings toggle component
  ○ Wire up OS preference detection

  1. Resume (build remaining subtasks)
  2. Start fresh (undo and re-plan)

Choice [1-2]:
```

**Non-TTY auto-resume:**
```
Resuming incomplete plan for ops/now/dark-mode.md (2/3 done).
```

---

### Stage 3: `aiki review mvslrsp`

**Initial output:**
```
Review: task mvslrsp (luppzup)
Running to completion...
```

**Status monitor — review in progress:**
```
● [luppzup] Review: task mvslrsp                         15s
├─ ●  .0) Review changes and write findings               15s
│      └─ Examining diff for theme context...
└─ (reviewing...)
                                                  Ctrl+C: detach
```

**Status monitor — findings written:**
```
● [luppzup] Review: task mvslrsp                         32s
├─ ✓  .0) Review changes and write findings
│      └─ Found 2 issues, writing summary
└─ (closing...)
                                                  Ctrl+C: detach
```

**Completed — issues found:**
```
Review completed: task mvslrsp (luppzup), 2 issues
---
Run `aiki fix luppzup` to remediate.
```

**Completed — no issues:**
```
Review completed: task mvslrsp (luppzup)
No issues found — approved.
```

**With --start (hand-off to user):**
```
Review started: task mvslrsp (luppzup)
You are now reviewing. Run `aiki task show luppzup` for instructions.

Ready (2):
[p2] abcdefg  Some task
[p2] zyxwvut  Another task
```

**Nothing to review (session scope):**
```
Nothing to review — no closed tasks in this session.
```

---

### Stage 4: `aiki fix luppzup`

**Initial output — analyzing review:**
```
Fix: review luppzup (tnslzmp)
Analyzing review findings...
```

**Status monitor — creating fix subtasks:**
```
● [tnslzmp] Fix: review luppzup                          8s
├─ ●  .0) Analyze findings and create fix tasks            8s
│      └─ Parsing 2 review comments...
└─ (analyzing...)
                                                  Ctrl+C: detach
```

**Status monitor — fixing issues:**
```
● [tnslzmp] Fix: review luppzup                         35s
├─ ✓  .0) Analyze findings and create fix tasks
│      └─ Created 2 fix subtasks
├─ ●  .1) Add null check for theme preference             20s
│      └─ Updating ThemeProvider.tsx
└─ ○  .2) Handle missing matchMedia API gracefully
                                                  Ctrl+C: detach
```

**Status monitor — all fixes applied:**
```
✓ [tnslzmp] Fix: review luppzup                        1m 05s
├─ ✓  .0) Analyze findings and create fix tasks
│      └─ Created 2 fix subtasks
├─ ✓  .1) Add null check for theme preference
│      └─ Added fallback to light theme
└─ ✓  .2) Handle missing matchMedia API gracefully
       └─ Added feature detection with SSR support
                                                  Ctrl+C: detach
```

**Completed:**
```
Fix completed: review luppzup (tnslzmp), 2 issues resolved
  ✓ Add null check for theme preference
  ✓ Handle missing matchMedia API gracefully
---
Run `aiki review` to verify.
```

**Approved (no issues to fix):**
```
Approved: review luppzup passed — no issues found.
```

**With --start (hand-off to user):**
```
Fix started: review luppzup (tnslzmp), 2 issues
  1. Add null check for theme preference
  2. Handle missing matchMedia API gracefully

Ready (2):
[p2] abcdefg  Some task
[p2] zyxwvut  Another task
```

---

### Non-TTY Status Monitor (Tier 3: Plain Text Fallback)

When stderr isn't a TTY (agent-spawned, CI/CD, piped), the status monitor emits appended lines instead of live redraws:

```
[0s] ● Build: dark-mode.md — Review plan and create subtasks
[15s] ✓ Build: dark-mode.md — Review plan and create subtasks (Created 3 subtasks)
[15s] ● Build: dark-mode.md — Implement theme context and CSS variables
[38s] ✓ Build: dark-mode.md — Implement theme context and CSS variables
[38s] ● Build: dark-mode.md — Add settings toggle component
[52s] ✓ Build: dark-mode.md — Add settings toggle component
[52s] ● Build: dark-mode.md — Wire up OS preference detection
[1m 23s] ✓ Build: dark-mode.md — Wire up OS preference detection
[1m 23s] ✓ Build completed
```

No cursor manipulation, no clearing — just appended lines that work in any log sink.

---

### Machine-Readable stdout (Piped Output)

When stdout is piped, commands emit bare task IDs for scripting:

```bash
# Capture plan task ID
PLAN_TASK=$(aiki plan dark-mode.md "Add dark mode toggle")
echo $PLAN_TASK  # → qotyswo...

# Capture build task ID
BUILD_TASK=$(aiki build ops/now/dark-mode.md)
echo $BUILD_TASK  # → mvslrsp...

# Chain review → fix
REVIEW_TASK=$(aiki review $BUILD_TASK)
aiki fix $REVIEW_TASK
```

Stderr still shows human-readable status (or Tier 3 plain text if also piped).

---

### Summary of Changes

| Aspect | Current | Proposed |
|--------|---------|----------|
| Headers | `## Markdown Headers` in stderr | Plain text, no headers |
| Fields | `- **Field:** value` dumps | Natural sentence: `action: target (id)` |
| Task IDs | Full 32-char IDs | 7-char short IDs |
| Next steps | Only review→fix has a hint | Every command suggests what's next |
| Build IDs | Build ID + Plan ID shown | Single task ID + plan path |
| Scope info | `Type: task`, `Scope: Task(...)` | Just the target: `task xqrmnps` |
| Status symbols | Emoji (💬) that misalign | Unicode symbols (✓ ● ○ ✗) |
| Input prompt | Raw crossterm, no cursor nav | Ratatui text area with full editing |
| Status monitor | Manual cursor save/restore | Ratatui inline viewport, clean redraws |
| Incremental updates | Status monitor only (build) | All blocking commands show live progress |
| Non-TTY | Inconsistent, status monitor crashes | Three-tier degradation (interactive → watch → plain text) |
| Piped stdout | XML fragments | Bare task IDs for scripting |

---

## Non-TTY Environments

### Context

Aiki commands run in several non-interactive contexts:

| Context | stdin | stdout | stderr | Example |
|---------|-------|--------|--------|---------|
| **Human at terminal** | TTY | TTY | TTY | `aiki plan dark-mode.md` |
| **Agent-spawned** | pipe | pipe | pipe | `aiki task run` spawns Claude, which calls `aiki build` |
| **Piped output** | TTY | pipe | TTY | `aiki review \| jq` |
| **CI/CD** | pipe | pipe | pipe | GitHub Actions running `aiki build` |
| **Script** | pipe | pipe | TTY | `./deploy.sh` calling `aiki build --async` |

The ratatui migration makes this critical: ratatui needs a terminal to render, so anything that uses ratatui must gracefully degrade when stderr isn't a TTY.

### Current Handling (Audit)

| File | Check | What It Gates |
|------|-------|---------------|
| `plan.rs:372` | `stdin.is_terminal()` | Interactive text prompt — skips if no TTY |
| `plan.rs:534` | `stdout.is_terminal()` | Writes bare task ID to stdout when piped |
| `build.rs:198,231,310,334` | `stdout.is_terminal()` | Machine-readable output (task ID) when piped |
| `build.rs:595` | `stdin.is_terminal()` | Resume/fresh plan prompt — skips if no TTY |
| `review.rs:516,525,566` | `stdout.is_terminal()` | Machine-readable output when piped |
| `fix.rs:183,192,248` | `stdout.is_terminal()` | Machine-readable output when piped |
| `plan.rs:137,192,203` | `stdout.is_terminal()` | Machine-readable output when piped |
| `plan.rs:465` | `stdin.is_terminal()` | Interactive choice prompt |
| `runner.rs:128` | `stderr.is_terminal()` | Status monitor — falls back to quiet mode |
| **status_monitor.rs** | **NONE** | Renders crossterm cursor ops unconditionally |

### Gaps

1. **Status monitor has no TTY guard internally** — `runner.rs` gates it externally, but `StatusMonitor` itself blindly writes cursor escape sequences. If anyone constructs a `StatusMonitor` directly without the `runner.rs` gate, it'll corrupt piped stderr. The ratatui migration should add an internal guard.

2. **Plan prompt fails silently** — When stdin isn't a TTY, the interactive prompt is skipped and `user_text` defaults to empty. This is correct behavior but isn't communicated — the user gets no feedback that the prompt was skipped. In agent-spawned contexts, a debug log line would help.

3. **Build resume prompt has no non-TTY fallback** — When `build.rs:595` detects non-TTY stdin and an incomplete plan exists, it should auto-resume (or auto-fresh with a flag) rather than silently falling through.

4. **No `--no-interactive` flag** — Some environments are technically a TTY but shouldn't get interactive prompts (e.g., CI with `script` wrapper). A global `--no-interactive` / `AIKI_NO_INTERACTIVE=1` env var would be useful.

### Strategy: Three-Tier Degradation

```
Tier 1: Full interactive (stdin+stderr are TTY)
  → Ratatui inline viewport for prompts and status monitor
  → Interactive choice prompts (resume/fresh, etc.)
  → Color, Unicode symbols, progress indicators

Tier 2: Watch-only (stderr is TTY, stdin is not)
  → Status monitor renders to stderr (read-only, no input)
  → No interactive prompts — use defaults or flags
  → Still get color and symbols

Tier 3: Machine-readable (stderr is not TTY)
  → Plain text to stderr (no cursor manipulation, no ratatui)
  → One-line status updates instead of live tree
  → Task IDs on stdout for piping
```

### Implementation

**1. Internal TTY guard in StatusMonitor / ratatui widgets**

```rust
impl StatusMonitor {
    pub fn new(task_id: &str) -> Self {
        let is_tty = std::io::stderr().is_terminal();
        Self {
            // ...
            render_mode: if is_tty { RenderMode::Ratatui } else { RenderMode::PlainText },
        }
    }
}

enum RenderMode {
    Ratatui,    // Full inline viewport with live redraws
    PlainText,  // Simple eprintln! lines, no cursor manipulation
}
```

**2. PlainText fallback for status monitor**

When stderr isn't a TTY, the monitor prints one-line updates on state change instead of redrawing:

```
[12s] ● Build: dark-mode.md — Implement auth module
[18s] ✓ Build: dark-mode.md — Implement auth module (done)
[18s] ● Build: dark-mode.md — Add tests
```

No cursor manipulation, no clearing — just appended lines. This works in CI logs, piped output, and agent contexts.

**3. Plan prompt: non-TTY means no prompt, with feedback**

```rust
if initial_idea_needs_input(&mode, &initial_idea) {
    if io::stdin().is_terminal() {
        // Tier 1: Full ratatui text area
        if let Some(text) = prompt_multiline_input(&header)? {
            // ...
        }
    } else {
        // Tier 2/3: Skip prompt, log why
        eprintln!("Skipping interactive prompt (non-TTY stdin). Pass text as args.");
    }
}
```

**4. Build resume prompt: auto-resume in non-TTY**

```rust
if !stdin.is_terminal() {
    // Non-interactive: auto-resume incomplete plans
    eprintln!("Resuming incomplete plan (non-TTY stdin).");
    return resume_plan(cwd, &plan_path, &plan_task).await;
}
```

**5. Global `--no-interactive` escape hatch**

```rust
// In main.rs CLI args
#[arg(long, env = "AIKI_NO_INTERACTIVE")]
no_interactive: bool,

// Utility function
pub fn is_interactive() -> bool {
    !std::env::var("AIKI_NO_INTERACTIVE").is_ok_and(|v| v == "1")
        && io::stdin().is_terminal()
}
```

### Machine-Readable stdout Convention

All commands already follow this pattern but it should be codified:

| stdout (piped) | stderr (always) |
|----------------|-----------------|
| Bare task ID: `qotyswo` | Human-readable status messages |
| One value per line | Progress updates, hints, errors |
| No formatting, no color | Color when TTY, plain when not |

This enables scripting:
```bash
TASK_ID=$(aiki plan dark-mode.md)
aiki build ops/now/dark-mode.md
aiki review $TASK_ID
```

### Files Affected

| File | Change |
|------|--------|
| `cli/src/tasks/status_monitor.rs` | Add `RenderMode`, internal TTY guard, plain text fallback |
| `cli/src/commands/plan.rs` | Add non-TTY feedback message |
| `cli/src/commands/build.rs` | Auto-resume in non-TTY, log message |
| `cli/src/main.rs` | Add `--no-interactive` flag |
| `cli/src/commands/mod.rs` or `lib.rs` | Add `is_interactive()` utility |

### Phase Placement

This work fits into **Phase 1** (alongside the ratatui migration) since:
- The ratatui migration creates the need for graceful degradation
- The `RenderMode` enum is part of the ratatui status monitor rewrite
- Non-TTY plan prompt feedback is part of the data flow fix
- Auto-resume is a small addition to the build prompt logic
