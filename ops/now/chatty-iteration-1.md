# Chatty Output: Iteration 1 — Polish

**Date**: 2026-03-20
**Status**: Proposed
**Builds on**: [chatty-output.md](chatty-output.md)

---

## Context

The chatty pipeline view is structurally complete — data model, builder, widget, progressive dimming are all in place. But comparing actual output to the spec reveals several gaps.

**Active build (current):**

```
[aiki] ops/now/fix-rhai-int-conditionals.md

 Created plan                                                             18:52

 Decomposed into 3 subtasks                                                2m22
 ▸ Register __truthy__ function in Rhai engine                              53s
 ○ Implement wrap_bare_boolean_operands() preprocessing pass
 ○ Add tests for non-bool operands in && / ||
```

**Completed build (current):**

```
[aiki] ops/now/fix-rhai-int-conditionals.md

 Created plan                                                             18:52

 Decomposed into 3 subtasks                                                2m22
 ✓ Built 3/3 subtasks                                                      4m44

 ✓ Done in 0 iterations, 4m44 total
```

---

## Issues

### 1. Flat subtasks instead of lane blocks

**What happens:** Subtasks render as flat `ChatChild::Subtask` lines — no surface background, no block grouping, no footer.

**Root cause:** `find_orchestrator()` in `chat_builder.rs:447` only returns an orchestrator that's `InProgress` or `Closed`. When it returns `None`, `build_active_build()` falls through to flat rendering (lines 422-432). Even a single lane should be a block — the surface background is the spec's primary mechanism for "work is happening here" and the footer is the only place for agent/model/cost/context info.

**Fix:** When no orchestrator is found and there are active/pending subtasks, wrap them in a single `LaneBlock` with a footer sourced from the active subtask.

### 2. Elapsed on active subtask line

**What happens:** The `▸` line shows `53s` right-aligned. Per spec, active subtasks inside a lane block don't show elapsed — that belongs on the footer.

**Where:** `pipeline_chat.rs:227` passes `ls.elapsed.as_deref()` to `render_block_line_with_meta()` for all lane subtasks regardless of status.

**Fix:** Suppress elapsed for `Active` and `Pending` lane subtasks. Only `Done` subtasks show elapsed on their line.

### 3. Progressive dimming verification

The code looks correct (`pipeline_chat.rs:81-83`): finds the active stage from the last `Active`/`Attention` message and dims earlier stages. "Created plan" (Stage::Plan) should dim when build (Stage::Build) is active.

**Action:** Verify this works in practice. If "Created plan" isn't dimming, it may be because the empty-text active message (`chat_builder.rs:438-439`, text is `String::new()`) doesn't participate in stage detection. The widget's `active_stage` scan may skip it — needs a test.

### 4. Dead if/else in footer rendering

`pipeline_chat.rs:267-271` — both branches produce `self.theme.dim`. Remove the if/else, use `self.theme.dim` directly.

### 5. "0 iterations" in summary

**What happens:** Summary shows "Done in 0 iterations, 4m44 total" — iteration count is `review_ids.len()` (`chat_builder.rs:864`). With no review, this is 0.

**Fix:** When iterations == 0 (no review cycle), drop the iteration count entirely. Just "Done, 4m44 total". The iteration count only has meaning when there were review rounds.

### 6. ✓ prefix on summary line

**What happens:** "✓ Done in 0 iterations..." — the ✓ comes from `kind_sym_color()` returning `SYM_CHECK` for all `MessageKind::Done` messages.

**Spec says:** "Done in 2 iterations, 4m48 total" — green text, no ✓ symbol. The word "Done" is the signal; ✓ is redundant.

**Fix:** Either give the summary message a different kind (e.g. keep `Done` but check `stage == Summary` in the widget to suppress the symbol), or use `MessageKind::Meta` for the summary and render it green when `stage == Summary`. Simplest approach: add a `Summary` variant to `MessageKind` that renders green text with no symbol.

### 7. Missing agents line

**What happens:** No "Agents: claude ×3" line appears. The builder emits it (`chat_builder.rs:892-905`) but only if `agent_label()` returns `Some` for at least one subtask. This means `agent_label()` is returning `None` for all subtasks — the agent info likely isn't populated in task data.

**Fix:** Investigate what `agent_label()` looks for in task data and ensure the orchestrator/runner populates it. This may be an upstream issue (task data not written) rather than a TUI bug.

### 8. Chat view doesn't persist after build completes

**What happens:** During the build, the chatty pipeline view renders live via `ScreenSession`. When the build finishes, the session drops (clears the terminal), then `output_build_completed()` (`build.rs:970`) prints a plain markdown summary:

```
## Build Completed
- **Build ID:** tqsvknmwouktywtlnkmmntwpqqvnrlzs
- **Epic ID:** ykmvwqqktlztzvppzvtxsmsxotmxlsql
- **Subtasks:** 3

1. Register __truthy__ function in Rhai engine (done)
...
```

The whole narrative disappears and gets replaced by a table of IDs.

**The chatty view is already available as `output_build_show()`** (`build.rs:997`) — it calls `build_pipeline_chat()` → `render_pipeline_chat()` → `buffer_to_ansi()` and renders the final chat state as static ANSI.

**Fix:** Replace `output_build_completed()` calls (lines 437, 589) with `output_build_show()`. The final chat state — "Done, 4m44 total" with agents line — persists in the terminal scrollback. The `aiki review` hint can be appended after the chat output. Delete `output_build_completed()`.

---

## Scope

Eight issues. Builder, widget, and build command output path.

| File | Changes |
|---|---|
| `cli/src/tui/chat_builder.rs` | #1 LaneBlock fallback, #5 drop "0 iterations", #7 agent_label investigation |
| `cli/src/tui/views/pipeline_chat.rs` | #2 suppress elapsed on active, #4 dead if/else, #6 summary symbol |
| `cli/src/tui/types.rs` | #6 possibly add Summary variant to MessageKind |
| `cli/src/commands/build.rs` | #8 replace `output_build_completed()` with `output_build_show()` |
| Tests | Lane block single-lane, dimming, summary formatting |
