# Step 4a: Remove Old TUI Code

**Date**: 2026-03-21
**Status**: Merged into step 3a (cutover)
**Priority**: P2

> **Note:** This step's work is now done as part of [step-3a](step-3a-status-monitor.md) (Phase 3 cutover). This file is kept as a reference for the complete delete list.

---

## Problem

After all flows are migrated to the new rendering stack, the old TUI code is dead. It should be removed to prevent confusion and reduce maintenance burden.

---

## Fix: Delete old files, update imports

### Files to delete

| File | Lines | Why dead |
|------|-------|----------|
| `cli/src/tui/chat_builder.rs` | 1240 | Replaced by screen-specific builders in `screens/` |
| `cli/src/tui/views/pipeline_chat.rs` | 1110 | Replaced by `render.rs` shared components |
| `cli/src/tui/live_screen.rs` | 400 | Replaced by `inline_renderer.rs` |
| `cli/src/tui/loading_screen.rs` | 278 | Loading state handled by inline renderer |
| `cli/src/tui/types.rs` | 122 | Replaced by Line types in `app.rs` |
| `cli/src/tui/widgets/stage_track.rs` | 490 | No stage track in new design |
| `cli/src/tui/widgets/epic_tree.rs` | 538 | Replaced by `render_subtask_table()` |
| `cli/src/tui/widgets/breadcrumb.rs` | 188 | Not needed |
| `cli/src/tui/widgets/path_line.rs` | 200 | Plan phase renders its own path |
| `cli/src/tui/views/epic_show.rs` | 413 | Replaced by view function using components (step 2e) |
| `cli/src/tui/views/issue_list.rs` | 379 | Replaced by view function using components (step 2e) |
| `cli/src/tui/buffer_ansi.rs` | 125 | Replaced by `Viewport::Inline` + `TestBackend.to_string()` |
| `cli/src/tasks/status_monitor.rs` | 443 | Replaced by `app.rs` Elm event loop |
| `cli/src/tui/widgets/mod.rs` | 3 | Entire widgets directory deleted |
| `cli/src/tui/render_png.rs` | 120 | `insta` text snapshots replace PNG rendering |
| **Total** | **6046** | |

### Files to update

| File | Change |
|------|--------|
| `cli/src/tui/mod.rs` | Remove old `pub mod` lines, add new ones |
| `cli/src/tui/views/mod.rs` | Remove `pipeline_chat` |
| `cli/src/tui/widgets/mod.rs` | Remove deleted widgets |
| `cli/src/commands/build.rs` | Remove imports of old types |
| `cli/src/commands/fix.rs` | Remove imports of old types |
| `cli/src/commands/review.rs` | Remove imports of old types |
| `cli/src/tasks/runner.rs` | Remove `ScreenSession`, `LiveScreen` imports |

### Files to keep

| File | Lines | Why |
|------|-------|-----|
| `theme.rs` | 144 | Colors and symbols |
| `app.rs` | ~200 | New: Elm core + Line types (step 1b) |
| `components.rs` | ~120 | New: reusable view components (step 1c) |
| `render.rs` | ~80 | New: line renderer (step 1c) |
| `screens/*.rs` | ~400 | New: per-screen view functions (phase 2) |

### Verification

After deletion:
1. `cargo build` — no compilation errors
2. `cargo test` — all tests pass (old snapshot tests will be replaced in 4b)
3. No dead imports or unused warnings
