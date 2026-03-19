# TUI: Two-Pane Plan Navigator

**Date**: 2026-03-18
**Status**: Proposed mockup — replaces v4 dashboard concept for plan...fix pipeline
**Prereqs**: [chatty-output.md](chatty-output.md) — narrative rendering layer (must land first)

---

## Problem

The current pipeline TUI is a status dashboard — symbols, progress bars, DAGs. It answers "where is this build at?" but doesn't answer:

1. What should I work on next?
2. What happened during this build/review?
3. What went wrong and what's being done about it?

The v4 mockups add navigation but keep the same dashboard aesthetic. We want something that feels more like reading a conversation log than staring at a control panel.

> **Note**: The chatty narrative style used in the right pane is implemented by `chatty-output.md`. This plan focuses on the two-pane layout and navigation that consumes it.

---

## Design: Plans Left, Story Right

Two vertical panes. Left is a narrow file picker. Right is a scrollable narrative of what's happening with the selected plan.

### Core Mockup — Active build

```
[120×28]
 Plans                          │                                                          ← dim "Plans", dim │
                                │  webhooks.md                                             ← hi+bold filename
 ▸ webhooks.md            0:34  │                                                          ← yellow ▸ selected, dim time
   auth-refactor.md       1:42  │  Started build  3/6                           0:34       ← fg "Started build", dim progress+time
   realtime.md            3:08  │                                                          ←
 ──────────────────────────     │  Decomposed into 6 subtasks                  12s        ← dim, past tense = done
   payment-migration.md         │  ✓ Explore webhook requirements              8s  cc     ← green ✓, dim time, cyan agent
   api-versioning.md            │  ✓ Create implementation plan                6s  cc     ←
   caching-layer.md             │  ▸ Implement webhook route handler               cur    ← yellow ▸ active, magenta agent
 ──────────────────────────     │  ▸ Verify Stripe signatures                      cc     ←
   ✓ fix-login-bug.md          │  ○ Add idempotency key tracking                          ← dim ○ pending
   ✓ update-deps.md            │  ○ Write integration tests                               ←
                                │                                                          ←
                                │  Waiting on review...                                    ← dim, forward-looking
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
 [j/k] navigate  [b] build     │  [Enter] drill in  [d] diff  [Esc] quit                  ← dim key hints
```

**Left pane (30 cols):**
- Three sections separated by dim `──────` dividers: Active / Pending / Done
- No section headers — the symbols tell the story (▸ active, plain = pending, ✓ done)
- Selected plan highlighted with `▸` and elapsed time right-aligned
- Simple, scannable list — you see your whole ops/now at a glance

**Right pane (remaining cols):**
- Plan filename as header
- A **narrative** of what happened/is happening, in chronological order
- Each event is a line: past tense for done things, present tense for active, future for pending
- Agent badges and times are right-aligned metadata, not the focus
- The story reads top-to-bottom like a log, not a dashboard

---

### Review found issues

```
[120×28]
 Plans                          │                                                          ←
                                │  auth-refactor.md                                        ← hi+bold
 ▸ auth-refactor.md       1:42  │                                                          ←
   webhooks.md            0:34  │  Built 8/8 subtasks                          1m54       ← green text, completed
   realtime.md            3:08  │                                                          ←
 ──────────────────────────     │  Review found 2 issues                       42s        ← yellow text, attention
   payment-migration.md         │                                                          ←
   api-versioning.md            │  1. Missing null check in auth handler                   ← yellow number, fg text
 ──────────────────────────     │     auth.rs:42 — token.claims.sub accessed               ← dim location, dim desc
   ✓ fix-login-bug.md          │     without None check. Panics on expired tokens.         ← dim wrapped text
                                │     ▸ fixing...                              cur         ← yellow ▸, magenta agent
                                │                                                          ←
                                │  2. Error message not user-friendly                      ← yellow number
                                │     api/errors.rs:18 — returns raw internal error        ← dim
                                │     to client instead of user-facing message.             ←
                                │     ▸ fixing...                              cc          ← yellow ▸, cyan agent
                                │                                                          ←
                                │  Fixing 2 issues, then re-reviewing...                   ← dim, forward-looking
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
 [j/k] navigate  [b] build     │  [Enter] drill in  [f] re-fix  [Esc] quit                ← dim key hints
```

**Key difference from v4**: Issues are inlined as a readable paragraph, not a table row. You can understand what went wrong without drilling into a detail screen. The right pane tells you the _story_ of this plan.

---

### All done — victory

```
[120×24]
 Plans                          │                                                          ←
                                │  auth-refactor.md                                        ← hi+bold
 ▸ auth-refactor.md       4:48  │                                                          ←
   webhooks.md            0:34  │  Built 8 subtasks                            1m54       ← green
   realtime.md            3:08  │  Review found 2 issues                       42s        ← green (resolved)
 ──────────────────────────     │  Fixed both issues                           1m14       ← green
   payment-migration.md         │  Re-review passed — approved                 12s        ← green "approved"
   api-versioning.md            │                                                          ←
 ──────────────────────────     │  Done in 2 iterations, 4m48 total                       ← green, summary
   ✓ fix-login-bug.md          │  Agents: cc ×4, cur ×3                                   ← dim
   ✓ update-deps.md            │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
 [j/k] navigate  [b] build     │  [d] view diff  [Esc] quit                               ← dim key hints
```

The completed plan reads like a summary paragraph. No expandable sections, no drill-down needed — you can see the whole journey in 6 lines.

---

### Pending plan selected (no pipeline yet)

```
[120×20]
 Plans                          │                                                          ←
                                │  payment-migration.md                                    ← hi+bold
   webhooks.md            0:34  │                                                          ←
   auth-refactor.md       1:42  │  # Payment Migration Plan                                ← fg, plan title from file
 ──────────────────────────     │                                                          ←
 ▸ payment-migration.md        │  Migrate from Stripe v1 to v2 billing API.                ← dim, first paragraph preview
   api-versioning.md            │  Handle subscription proration and webhook                ←
   caching-layer.md             │  endpoint changes.                                        ←
 ──────────────────────────     │                                                          ←
   ✓ fix-login-bug.md          │  Ready to build.                                          ← fg
   ✓ update-deps.md            │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
 [j/k] navigate  [b] build     │  [b] build this plan  [e] edit  [Esc] quit               ← dim key hints
```

When you select a plan that hasn't been built yet, the right pane shows a **preview** of the plan file content instead of a pipeline narrative. Pressing `[b]` kicks off `aiki build`. Pressing `[e]` opens `aiki plan` to edit it.

---

### Empty state — just plans, no builds

```
[120×16]
 Plans                          │                                                          ←
                                │  webhooks.md                                             ← hi+bold
 ▸ webhooks.md                  │                                                          ←
   auth-refactor.md             │  # Stripe Webhook Integration                            ← fg, plan title
   payment-migration.md         │                                                          ←
                                │  Add webhook endpoints for Stripe payment                ← dim, preview
                                │  events. Handle payment_intent.succeeded,                ←
                                │  charge.refunded, and subscription lifecycle.             ←
                                │                                                          ←
                                │  Ready to build.                                          ← fg
                                │                                                          ←
                                │                                                          ←
                                │                                                          ←
 [j/k] navigate  [b] build     │  [b] build  [e] edit  [Esc] quit                         ← dim key hints
```

No dividers when there's only one section. Clean and minimal.

---

## Chatty Output (prereq)

The right pane's narrative rendering is defined in [chatty-output.md](chatty-output.md). That plan **replaces** the entire old view stack (WorkflowView, EpicTree, StageList, etc.) with a PipelineChat widget that reads TaskGraph directly and renders the full pipeline — plan creation through fix — as one continuous chat log.

---

## Left Pane Design

Width: 30 columns fixed. Content:

```
section_divider = "──────────────────────────"

For each plan in ops/now/*.md:
  - Active (▸ / building/reviewing/fixing):  " ▸ {filename:<22} {elapsed:>5}"
  - Pending (no pipeline):                    "   {filename:<22}"
  - Done (✓):                                 " ✓ {filename:<22}"
```

Sections:
1. **Active** — plans with an open epic (in build, review, or fix)
2. **Pending** — plan files with no associated epic
3. **Done** — plans with a closed epic (today/session)

Sections separated by `──────` dividers. No section headers — the visual grouping is enough.

Sort: Active by most-recently-updated. Pending by filename. Done by completion time.

---

## Navigation

| Key | Context | Action |
|-----|---------|--------|
| `j`/`k` | Left pane | Move selection up/down |
| `Enter` | Active plan | Drill into subtask/issue detail (pushes screen) |
| `b` | Pending plan | Start `aiki build` on selected plan |
| `b` | Left pane (no selection) | Build selected |
| `e` | Pending plan | Open `aiki plan` to edit |
| `d` | Active/Done plan | View diff in pager |
| `r` | Active plan (build done) | Trigger review |
| `f` | Active plan (review done) | Trigger fix |
| `Esc` | Drilled-in | Pop back to two-pane |
| `Esc` | Two-pane | Quit |
| `q` | Anywhere | Quit |

---

## What Changes from v4 Mockups

| v4 Dashboard | Two-Pane Navigator |
|---|---|
| Full-width pipeline rows | Left: file list, Right: narrative |
| Section headers ("Active", "Pending Plans") | Visual dividers, no text headers |
| Compact inline pipeline track per row | No pipeline track — narrative tells the story |
| Drill-down to separate pipeline detail screen | Right pane IS the detail — no separate screen |
| Breadcrumb navigation | No breadcrumbs needed (plan name in right pane header) |
| Multiple screen types (Dashboard, Pipeline, Detail, Issue) | Two screens: Two-pane (main) and Drill-in (detail) |

**What stays:**
- LiveScreen lifecycle
- Key binding conventions (j/k, Enter, Esc, q)
- Data sources (TaskGraph, PlanGraph, ops/now/*.md scanning)
- Theme system + buffer_to_ansi()

**What's gone** (replaced by chatty-output prereq):
- WorkflowView, EpicView, StageView — replaced by ChatLine model
- EpicTree, StageList, StageTrack, LaneDag, IssueList widgets — replaced by PipelineChat widget
- workflow.rs view composer — replaced by pipeline_chat.rs

---

## New Components Needed

| Component | Purpose |
|---|---|
| `tui/views/two_pane.rs` | Composer: splits 120-col buffer into left (30) + divider (1) + right (89) |
| `tui/widgets/plan_list.rs` | Left pane: file list with selection, sections, elapsed times |
| `tui/widgets/plan_preview.rs` | Right pane: markdown preview for pending plans |
| `tui/app.rs` | Screen stack (TwoPane → DrillIn), key dispatch, state management |

> `pipeline_chat.rs` and `chat_builder.rs` are delivered by the [chatty-output.md](chatty-output.md) prereq. The right pane calls PipelineChat directly for active/done plans.

---

## Implementation Sketch

> **Prereq**: [chatty-output.md](chatty-output.md) must land first — replaces old view stack with PipelineChat widget.

### Phase 1: Static two-pane layout
- `plan_list` widget with hardcoded data
- `plan_preview` widget showing file content for pending plans
- `two_pane` composer that splits the buffer
- Wire `PipelineChat` (from chatty-output) into right pane for active/done plans
- Snapshot tests for layout

### Phase 2: Interactive navigation
- j/k selection in left pane
- Action keys (b, e, d, r, f)
- Drill-in screen for subtask/issue detail

### Phase 3: Plan file discovery
- Scan ops/now/*.md for pending plans
- Match against PlanGraph for active/done status
- Auto-refresh when files change

---

## Open Questions

1. **Right pane width** — 89 cols is generous. Should we cap narrative text at ~72 chars for readability and right-align metadata in the remaining space?
2. **Scroll** — right pane will need scrolling for long builds with many subtasks. j/k in left pane vs scroll in right pane — how to disambiguate? Maybe Tab to switch focus.
3. **Multiple active agents on same plan** — show agent names inline or group by agent?
4. **Plan file preview** — how much of the markdown to show? First N lines? Title + first paragraph?
5. **Resize** — left pane fixed at 30, right pane stretches? Or proportional?
