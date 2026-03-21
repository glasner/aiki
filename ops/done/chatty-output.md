# Chatty Output: Narrative Pipeline View

**Date**: 2026-03-18
**Status**: Proposed
**Prereq for**: [tui-two-pane-plan.md](tui-two-pane-plan.md)

---

## Problem

The current TUI output is a status dashboard — symbols, progress fractions, structured widget trees. It tells you _where things are_ but not _what happened_. Reading the output feels like monitoring a flight control board, not following a story.

This plan **replaces** the existing pipeline views (WorkflowView, StageList, EpicTree, LaneDag) with a single narrative renderer that reads the TaskGraph directly and tells the story of a plan from creation to completion.

---

## What Gets Replaced

The entire existing view stack for pipeline output goes away:

| Removed | Reason |
|---|---|
| `tui/views/workflow.rs` | Replaced by `pipeline_chat.rs` |
| `tui/widgets/epic_tree.rs` | Subtasks rendered inline in the chat |
| `tui/widgets/stage_list.rs` | Stages are prose, not a widget |
| `tui/widgets/stage_track.rs` | No phase bar — narrative implies phase |
| `tui/widgets/lane_dag.rs` | Parallelism shown via live agent blocks, not abstract DAG |
| `tui/widgets/issue_list.rs` | Issues rendered as paragraphs in the chat |
| `tui/types.rs` WorkflowView, EpicView, StageView, etc. | Replaced by Chat/Message/ThreadedMessage model |
| `tui/builder.rs` (workflow builder) | Replaced by chat builder that reads TaskGraph directly |

**What stays:**
- `theme.rs` — same semantic colors, same symbols
- `buffer_to_ansi()` — same output path
- `path_line.rs` — still useful outside pipeline views
- `live_screen.rs` — still the interactive shell
- `render_png.rs` — test infrastructure

---

## The Entire Pipeline Is the Chat

The narrative doesn't start at "build". It starts when the plan was created and runs through every phase as one continuous log. Here's the full lifecycle, stage by stage.

### 1. Plan — just created

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← dim
```

Just the filename and when it was created. That's it.

### 2. Plan — edited

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← dim
 Edited with claude                                                2m08      ← dim
```

Each edit session gets a line. Multiple edits = multiple lines. Plan lines are always `Meta` kind (dim regardless of stage).

### 3. Build — decompose running

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 ┃  ▸ Decomposing plan                                                         ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 12% · $0.08                                  4s         ← surface bg, dim footer
```

Plan stage dims. The decompose agent gets a block: task line + footer. The footer shows agent/model, context usage, cost, and elapsed time.

### 4. Build — subtasks running (lane blocks show parallelism)

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Decomposed into 6 subtasks                                        12s        ← dim

 ┃  ✓ Explore webhook requirements                             8s              ← surface bg, green ✓
 ┃  ✓ Create implementation plan                               6s              ← surface bg, green ✓
 ┃  ▸ Verify Stripe signatures                                                ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 42% · $0.35                                32s          ← surface bg, dim footer
 ┃  ⎿ Checking signature against test vectors                                  ← surface bg, dim ⎿

 ┃  ▸ Implement webhook route handler                                          ← surface bg, yellow ▸
 ┃  ○ Write integration tests                                                  ← surface bg, dim ○
 ┃  cursor/sonnet-4.6 · 28% · $0.14                              48s          ← surface bg, dim footer
 ┃  ⎿ Writing route handler for /api/webhooks                                  ← surface bg, dim ⎿

 ○ Add idempotency key tracking                                               ← dim ○ (unassigned)

 Waiting on review...                                                          ← dim
```

Each lane block is a **lane from the lane decomposition** — subtasks that must run serially within that lane. **Two blocks = two parallel lanes.** The `theme.surface` background makes each lane pop as a live panel.

The **footer** is the second-to-last line of each block (or last, if no heartbeat yet). It identifies who's doing the work and how it's going:
- `claude/opus-4.6` — agent and model with version
- `42%` — context window usage (how much runway is left)
- `$0.35` — session cost so far
- `32s` — elapsed time (right-aligned)

Inside a lane block:
- `✓` = subtask done (within this lane)
- `▸` = subtask active (agent is working on it now)
- `○` = subtask pending (waiting for the active one to finish)

Subtasks outside any block (`○ Add idempotency key tracking`) are unassigned — not yet claimed by any lane.

The `┃` in mockups represents the background fill — in the actual render, all lines in the block have `bg: theme.surface` on every cell across the full width. No visible border character.

When a lane finishes all its subtasks, the block disappears and its `✓` lines join a flat done list above the remaining blocks.

### 5. Review — agent scanning

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                1m54      ← STAGE-DIM

 ┃  ▸ Reviewing changes                                                         ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 18% · $0.12                                18s          ← surface bg, dim footer
 ┃  ⎿ Scanning auth handler for null checks                                    ← surface bg, dim ⎿
```

Build collapses to "Built 6/6 subtasks" and dims. Review agent gets its own block. The `⎿` heartbeat line shows the agent's latest progress comment — it updates in place as new comments arrive.

### 6. Review — issues found

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                  1m54      ← STAGE-DIM

 Review found 2 issues                                             42s        ← yellow

 1. Missing null check in auth handler                                         ← yellow number
    auth.rs:42 — token.claims.sub accessed without                             ← dim
    None check. Panics on expired tokens.

 2. Error message not user-friendly                                            ← yellow number
    api/errors.rs:18 — returns raw internal error                              ← dim
    to client instead of user-facing message.

 Waiting on fix...                                                             ← dim
```

Agent block gone (review finished). Issues rendered inline with descriptions. If review passes with no issues, this stage is just: `Review passed — approved  42s  ← green`

### 7. Fix — planning

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                1m54      ← STAGE-DIM

 Review found 2 issues                                             42s        ← STAGE-DIM

 ┃  ▸ Planning fix                                                              ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 8% · $0.06                                 12s          ← surface bg, dim footer
```

Review stage dims. Fix starts its own pipeline: plan → decompose → execute → review.

### 7b. Fix — subtasks running (parallel lanes)

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                1m54      ← STAGE-DIM

 Review found 2 issues                                             42s        ← STAGE-DIM

 ✓ Planned fix                                                 12s  claude    ← green ✓
 ✓ Decomposed into 2 subtasks                                   6s  claude    ← green ✓

 ┃  ▸ Fix null check in auth handler                                            ← surface bg, yellow ▸
 ┃  cursor/sonnet-4.6 · 14% · $0.06                               8s          ← surface bg, dim footer

 ┃  ▸ Fix error message format                                                 ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 10% · $0.04                                  4s         ← surface bg, dim footer

 ○ Review fix                                                                  ← dim ○
```

Same lane block pattern as build. Each fix subtask gets its own lane. Two blocks = two agents fixing in parallel.

### 7c. Fix — reviewing fix

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                1m54      ← STAGE-DIM

 Review found 2 issues                                             42s        ← STAGE-DIM

 ✓ Planned fix                                                 12s  claude    ← green ✓
 ✓ Fixed 2/2 subtasks                                          20s             ← green ✓

 ┃  ▸ Reviewing fix                                                             ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 22% · $0.14                                12s          ← surface bg, dim footer
```

Fix subtasks collapse to "Fixed 2/2 subtasks". Review-fix agent gets its own block.

### 8. Re-review

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                1m54      ← STAGE-DIM

 Review found 2 issues                                             42s        ← STAGE-DIM
 Fixed 2 issues                                                    52s        ← STAGE-DIM

 ┃  ▸ Re-reviewing changes                                                      ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 16% · $0.10                                12s          ← surface bg, dim footer
```

Fix stage dims. Re-review agent gets a block.

### 9. Done

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← dim
 Edited with claude                                                2m08      ← dim

 Built 6/6 subtasks                                                  1m54      ← green
 Review found 2 issues                                             42s        ← green
 Fixed both issues                                                 20s        ← green
 Re-review passed — approved                                       12s        ← green

 Done in 2 iterations, 4m48 total                                              ← green
 Agents: claude ×4, cursor ×3                                                  ← dim
```

No active stage, so nothing dims (except plan/edit which are always dim as Meta). All summary lines are green. The whole pipeline reads as a finished story top to bottom.

### Build failed (error case)

**Mid-build: one lane fails while another is still running.** The failing lane keeps its block form with `✗` replacing `▸`. The other lane continues. Previously completed lanes have already collapsed to flat `✓` lines above.

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Decomposed into 6 subtasks                                        12s        ← dim

 ✓ Explore webhook requirements                              8s  claude      ← green (collapsed from earlier lane)
 ✓ Create implementation plan                                6s  claude

 ┃  ✗ Implement webhook route handler                                          ← surface bg, red ✗
 ┃    Error: Connection refused to test database                               ← surface bg, red, indented
 ┃  ○ Write integration tests                                                  ← surface bg, dim ○ (skipped)
 ┃  cursor/sonnet-4.6 · 28% · $0.14                              48s          ← surface bg, dim footer

 ┃  ▸ Verify Stripe signatures                                                ← surface bg, yellow ▸ (still running)
 ┃  claude/opus-4.6 · 42% · $0.35                                32s          ← surface bg, dim footer

 ○ Add idempotency key tracking                                               ← dim ○ (unassigned)
```

**Terminal state: all lanes done/failed, collapsed to flat list.**

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Decomposed into 6 subtasks                                        12s        ← dim
 ✓ Explore webhook requirements                              8s  claude      ← green
 ✓ Create implementation plan                                6s  claude
 ✓ Verify Stripe signatures                                 32s  claude      ← green
 ✗ Implement webhook route handler                          48s  cursor      ← red ✗
   Error: Connection refused to test database                                  ← red, indented
 ○ Write integration tests                                                    ← dim, skipped
 ○ Add idempotency key tracking                                               ← dim, skipped

 Build failed: 1 subtask errored                                               ← red
```

Lane blocks collapse to flat lines on completion (success or failure). The `✗` line retains its error detail. Pending subtasks in the failed lane and unassigned subtasks show as `○` (skipped).

---

The key rhythm: **agent block appears → work happens → block collapses to a summary line → that line dims → next stage's agent block appears**. The `theme.surface` background means your eye always snaps to what's alive right now.

---

## The Chatty Rules

1. **Past tense for done.** "Built 8/8 subtasks" not "✓ build 8/8"
2. **Present tense for active.** "Fixing 1 remaining issue..." not "● fix 1/2"
3. **Future tense for next.** "Waiting on review..." not "○ review"
4. **Inline detail.** Issue descriptions visible without drill-down
5. **No stage widgets.** Pipeline phase is implied by the narrative flow
6. **Active sessions get background.** Running agent sessions get `theme.surface` background. Single-task sessions (decompose, review) are one-line `AgentBlock`s. Multi-task sessions (build loop, fix loop) are `LaneBlock`s with subtasks inside. Multiple blocks = multiple lanes = parallelism.
7. **Done blocks collapse.** Finished lanes disappear; their subtasks join the flat done list. Finished single-task sessions become `✓` lines with agent as right-aligned metadata. No background.
8. **Subtasks live inside lanes.** During build/fix, subtasks are grouped by lane inside `LaneBlock`s. Unassigned subtasks appear outside any block.
9. **Collapse when done.** Active build shows subtask tree with agent blocks; done build collapses to "Built N/N subtasks"
10. **Summary at the end.** "Done in N iterations, Xm total"
11. **The whole pipeline is one scroll.** Plan creation → edit → build → review → fix → re-review, all in order
12. **Previous stages dim.** Completed stages fade so your eye falls on the current action
13. **Failed lanes keep their block.** A lane with a `✗` subtask stays in block form (surface bg) until the build stage resolves. On terminal collapse, `✗` lines retain their error detail in the flat list.

---

## Progressive Dimming

As the pipeline advances, completed stages dim so the active stage visually pops. This is the primary mechanism for delineating stages — no divider lines, no section headers, just brightness.

**Four brightness levels:**

| Level | When | Style |
|---|---|---|
| **Block** | Active agent/lane blocks | `theme.surface` background on all lines, foreground yellow/green/dim |
| **Full brightness** | Active stage + anything pending after it | Normal fg/green/yellow/red as specified, default bg |
| **Dimmed** | Completed stages before the active one | All text in dim (theme.dim), including ✓ symbols |
| **Header** | Plan filename (always) | hi+bold (always full brightness) |

The agent block background is the highest visual weight in the chat — it's the only element with a non-default background. This makes running agents immediately findable.

**How it works:** The renderer walks the chat lines and tracks which "stage" each belongs to (plan, build, review, fix). Everything in a stage that's fully complete gets dimmed. The active stage and everything after it renders at full brightness.

**Stage boundaries:**

| Stage | Lines |
|---|---|
| plan | "Created plan", "Edited with..." |
| build | "Decomposed into...", subtask tree, "Built N/N subtasks" |
| review | "Review found/passed...", issue list |
| fix | "Planned fix", "Decomposed into...", fix lane blocks, "Fixed N/N subtasks", "Review fix..." |
| re-review | "Re-review passed/found..." |
| summary | "Done in...", "Agents: ..." |

Each `Message` gets a `stage: Stage` field. The renderer checks: is this line's stage < active stage? If so, dim it.

---

## Data Model

The chat reads the TaskGraph directly — no WorkflowView translation layer.

```rust
/// Pipeline stage — used for progressive dimming
#[derive(PartialOrd, Ord, PartialEq, Eq)]
enum Stage { Plan, Build, Review, Fix, ReReview, Summary }

/// A pipeline rendered as a conversation
struct Chat {
    messages: Vec<Message>,
}

/// One line (or multi-line block) in the chat
struct Message {
    stage: Stage,              // which pipeline stage this belongs to
    kind: MessageKind,
    text: String,              // "Built 8/8 subtasks"
    meta: Option<String>,      // right-aligned: "1m54" or "42s  claude"
    children: Vec<ChatChild>,  // subtasks, agent blocks, or issues
}

enum MessageKind {
    Done,       // green, past tense
    Active,     // yellow, present tense
    Pending,    // dim, future tense
    Attention,  // yellow, review found issues
    Error,      // red, failure
    Meta,       // dim, plan created/edited/summary
}

/// Children of a message — subtasks, lane blocks, or issues
enum ChatChild {
    /// A completed or pending subtask — single line with ✓/○/✗
    Subtask {
        name: String,
        status: MessageKind,     // Done/Pending/Error
        elapsed: Option<String>,
        agent: Option<String>,   // right-aligned: "claude", "cursor"
        error: Option<String>,   // shown on next line if Error
    },
    /// A single-task agent session — task line + heartbeat + footer, surface background.
    /// Used for decompose, review, plan-fix, review-fix — sessions that
    /// run a single task rather than a lane of subtasks.
    AgentBlock {
        task_name: String,           // "Decomposing plan", "Reviewing changes"
        heartbeat: Option<String>,   // latest task comment (dim italic, above footer)
        footer: BlockFooter,         // agent/model, context %, cost, elapsed
    },
    /// An active lane — subtask lines + heartbeat + footer, surface background.
    /// Contains the subtasks assigned to this lane. This is the primary
    /// mechanism for showing parallelism: multiple LaneBlocks visible
    /// = multiple lanes executing concurrently.
    LaneBlock {
        subtasks: Vec<LaneSubtask>,  // subtasks in this lane (✓/▸/○)
        heartbeat: Option<String>,   // latest comment from the active subtask
        footer: BlockFooter,         // agent/model, context %, cost, elapsed
    },
    /// A review issue
    Issue {
        number: usize,
        title: String,
        location: Option<String>,
        description: Option<String>,
    },
}

/// A subtask within a LaneBlock — distinct from ChatChild::Subtask because
/// in-lane subtasks inherit their agent from the lane's BlockFooter.
struct LaneSubtask {
    name: String,
    status: MessageKind,     // Done/Active/Pending/Error
    elapsed: Option<String>,
    error: Option<String>,   // shown on next line if Error
}

/// Footer line for active blocks — identifies who's doing the work
/// Renders as: "claude/opus-4.6 · 42% · $0.35                    32s"
struct BlockFooter {
    agent: String,           // "claude", "cursor", "codex"
    model: String,           // "opus-4.6", "sonnet-4.6", "o3-pro"
    context_pct: u8,         // context window usage (0-100)
    cost: f64,               // session cost so far in dollars
    elapsed: Option<String>, // right-aligned: "32s", "1m54"
}
```

### Block rendering

Two block types share the same visual treatment — `theme.surface` background spanning full width, **content first, footer + heartbeat last**:

**AgentBlock** — single-task sessions (decompose, review, plan-fix, review-fix):

```
   ▸ {task_name}                                                      ← surface bg, yellow ▸
   {agent}/{model} · {ctx_pct}% · ${cost}                {elapsed}    ← surface bg, dim footer
   ⎿ {heartbeat}                                                     ← surface bg, dim ⎿ (optional)
```

Task line + footer + heartbeat. The heartbeat is the agent's latest progress comment (e.g. "Scanning auth handler for null checks"), hanging off the footer with `⎿`. It's omitted if no comments have been left yet.

**LaneBlock** — multi-subtask lanes (build loop, fix loop):

```
   ✓ {subtask_1}                                      {elapsed}       ← surface bg, green ✓
   ✓ {subtask_2}                                      {elapsed}       ← surface bg, green ✓
   ▸ {subtask_3}                                                      ← surface bg, yellow ▸
   ○ {subtask_4}                                                      ← surface bg, dim ○
   {agent}/{model} · {ctx_pct}% · ${cost}                {elapsed}    ← surface bg, dim footer
   ⎿ {heartbeat}                                                     ← surface bg, dim ⎿ (optional)
```

Subtask lines + footer + heartbeat. The heartbeat shows the latest comment from the active subtask, hanging off the footer with `⎿`. No header — the background delineates the block, the footer identifies who's running it.

**Footer format:** `claude/opus-4.6 · 42% · $0.35                    32s`
- `claude/opus-4.6` — agent/model-version as one compound identifier
- `42%` — context window usage (how much runway the agent has left)
- `$0.35` — session cost so far
- `32s` — elapsed time (right-aligned)

**Both blocks** use the same background to create a unified visual language — backgrounded = alive, no background = done. When an `AgentBlock`'s task finishes, it collapses to a `✓` line with agent right-aligned. When a `LaneBlock`'s last subtask finishes, the whole block disappears and its `✓` lines join the flat done list above.

The agent identity moves from footer (active) → right-aligned metadata (done):
```
 ┃  ▸ Reviewing changes                                                    ← active: footer
 ┃  claude/opus-4.6 · 18% · $0.12                          18s

 ✓ Reviewed changes                                    18s  claude         ← done: right-aligned
```

### Theme: `surface` color

Add a `surface` field to the Theme struct — a subtle background color that's slightly elevated from the terminal default:

```rust
// In theme.rs
pub struct Theme {
    // ... existing fields ...
    pub surface: Color,   // subtle background for agent blocks
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            // ... existing ...
            surface: Color::from_u32(0x25252f),  // slightly lighter than bg (0x1a1a24)
        }
    }
    pub fn light() -> Self {
        Self {
            // ... existing ...
            surface: Color::from_u32(0xe8e8ec),  // slightly darker than bg (0xf5f5f0)
        }
    }
}
```

The surface color should be **barely visible** — just enough contrast to create a sense of depth. Think of it like a card shadow: you notice it's there without it drawing attention from the content.

### Builder

```rust
/// Build chat lines directly from the TaskGraph for a given plan
fn build_pipeline_chat(graph: &TaskGraph, plan_path: &str) -> Chat {
    // 1. Find the plan task → Message::Meta "Created plan"
    // 2. Find edit sessions → Message::Meta "Edited with {agent}"
    // 3. Find decompose task:
    //    - If active: ChatChild::AgentBlock "Decomposing plan"
    //    - If done: Message::Meta "Decomposed into N subtasks"
    // 4. Find epic subtasks + derive lanes:
    //    - If all done: Message::Done "Built N/N subtasks"
    //    - If active: ChatChild::LaneBlock per active lane (with subtasks inside),
    //      ChatChild::Subtask for unassigned pending tasks
    // 5. Find review task:
    //    - If active: ChatChild::AgentBlock "Reviewing changes"
    //    - If done + issues: Message::Attention "Review found N issues"
    //    - If done + clean: Message::Done "Review passed — approved"
    // 6. Walk review issues → ChatChild::Issue for each
    // 7. Find fix pipeline:
    //    a. Plan-fix: AgentBlock or ✓ line
    //    b. Decompose-fix: AgentBlock or ✓ line
    //    c. Fix subtasks: LaneBlock per lane (same as build step 4)
    //    d. Review-fix: AgentBlock or ✓ line
    // 8. Find re-review → AgentBlock or Message::Done "Re-review passed"
    // 9. If all done → Message::Done "Done in N iterations, Xm total"
}
```

No intermediate WorkflowView. No StageView translation. TaskGraph in, Chat out.

---

## Rendering

### Widget: `pipeline_chat.rs`

Single widget that renders a `Chat` to a Buffer. Applies progressive dimming based on stage.

```rust
impl Widget for PipelineChat<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Find the active stage (last stage with an Active/Attention line)
        let active_stage = self.chat.messages.iter()
            .filter(|l| matches!(l.kind, MessageKind::Active | MessageKind::Attention))
            .map(|l| l.stage)
            .last();

        let mut y = area.y;
        for line in &self.chat.messages {
            // Progressive dimming: if this line's stage < active stage, dim everything
            let stage_dimmed = active_stage
                .map(|active| line.stage < active)
                .unwrap_or(false);

            if stage_dimmed {
                // Override all styles to dim — green ✓ becomes dim ✓, yellow text becomes dim
                render_dimmed(line, area, buf, &mut y, self.theme);
            } else {
                // Normal rendering with kind-appropriate colors
                render_normal(line, area, buf, &mut y, self.theme);
            }
        }
    }
}
```

Layout rules:
- 1 char left padding on all lines
- Blank line between stage transitions (plan→build, build→review, etc.)
- Blank line before the first block and after the last (separates done/active/pending groups)
- Blank line between adjacent blocks
- `Subtask` children: `✓`/`✗`/`○` prefix, agent right-aligned, default bg
- `AgentBlock` children: `▸ {task}` line + footer line, **`theme.surface` background spanning full width**
- `LaneBlock` children: subtask lines (✓/▸/○) + footer line, **`theme.surface` background on all lines spanning full width**
- `Issue` children: number prefix, indented description
- Issue descriptions wrapped at width - 15 (room for right-aligned meta)
- Error text indented and red on the line after the failed subtask
- **Stage-dimmed lines**: all text rendered in `theme.dim` regardless of MessageKind
- **Stage-dimmed blocks**: background also dims — use a midpoint between `theme.surface` and default bg, or just drop the background entirely when stage-dimmed

### Width

The widget takes whatever width it's given. Works at 80 cols standalone or at ~89 cols in the two-pane layout. No hardcoded width constant.

---

## Showing Parallelism Without the DAG

The old TUI used `lane_dag.rs` to show parallelism as an abstract graph (`●━●━┬━◉━○`). The chatty view replaces this with **lane blocks** — each lane from the lane decomposition becomes a backgrounded block containing its subtasks. Parallelism is visible because you see multiple blocks simultaneously.

### What the DAG showed vs. how lane blocks handle it

| DAG capability | Lane block equivalent |
|---|---|
| Multiple lanes = parallel execution | Multiple `LaneBlock`s with surface bg visible at once |
| Dots per lane (●/◉/○/✗) | Subtask lines inside each block (✓/▸/○/✗) |
| Sequential dependencies within lane | Subtask order within a block |
| Fan-out (fork point) | New blocks appear |
| Fan-in (merge point) | Blocks collapse when their last subtask finishes |
| Lane count at a glance | Count the backgrounded blocks |
| Progress per lane | ✓/▸/○ ratio within each block |

### What we lose (intentionally)

- **Cross-lane dependency arrows**: The DAG showed which lanes blocked which. Lane blocks don't — if lane B is waiting for lane A, its pending subtasks simply show as `○`. The dependency detail lives in `aiki task show`, not the pipeline view.

### Example progression

**T=0: One lane, one agent**
```
 Decomposed into 6 subtasks                                        12s

 ┃  ▸ Explore webhook requirements
 ┃  claude/opus-4.6 · 8% · $0.04                                   8s

 ○ Create implementation plan
 ○ Implement webhook route handler
 ○ Verify Stripe signatures
 ○ Add idempotency key tracking
 ○ Write integration tests
```

**T=10: Two lanes fan out**
```
 Decomposed into 6 subtasks                                        12s

 ┃  ✓ Explore webhook requirements                             8s
 ┃  ✓ Create implementation plan                               6s
 ┃  ▸ Verify Stripe signatures
 ┃  claude/opus-4.6 · 42% · $0.35                                32s
 ┃  ⎿ Checking signature against test vectors

 ┃  ▸ Implement webhook route handler
 ┃  ○ Write integration tests
 ┃  cursor/sonnet-4.6 · 28% · $0.14                              12s
 ┃  ⎿ Setting up route at /api/webhooks

 ○ Add idempotency key tracking
```

**T=60: All done, collapsed**
```
 Built 6/6 subtasks                                                1m54
```

Two blocks with footers → two parallel lanes. Progress within each block → lane advancement. No symbols to learn, no DAG to parse.

---

## Testing

Same pattern as existing TUI tests:

```rust
#[test]
fn chat_lane_blocks_have_surface_bg() {
    let theme = Theme::dark();
    let chat = Chat { messages: vec![
        Message { kind: Active, text: "Decomposed into 6 subtasks".into(),
            children: vec![
                ChatChild::LaneBlock { agent: "claude".into(), elapsed: Some("32s".into()),
                    subtasks: vec![
                        LaneSubtask { name: "Explore".into(), status: Done, elapsed: Some("8s".into()), .. },
                        LaneSubtask { name: "Verify signatures".into(), status: Active, .. },
                    ] },
                ChatChild::LaneBlock { agent: "cursor".into(), elapsed: Some("12s".into()),
                    subtasks: vec![
                        LaneSubtask { name: "Implement handler".into(), status: Active, .. },
                        LaneSubtask { name: "Write tests".into(), status: Pending, .. },
                    ] },
                ChatChild::Subtask { name: "Add tracking".into(), status: Pending, .. },
            ], .. },
    ]};
    let buf = render_pipeline_chat(&chat, &theme, 80);
    // Lane block header should have surface background
    let lane_row = find_row_containing(&buf, "claude");
    assert_eq!(buf.cell(0, lane_row).bg, theme.surface);
    // Subtasks inside lane also have surface background
    assert_eq!(buf.cell(0, lane_row + 1).bg, theme.surface);
    // Unassigned subtask outside lane should NOT have surface background
    let pending_row = find_row_containing(&buf, "Add tracking");
    assert_eq!(buf.cell(0, pending_row).bg, Color::Reset);
}

#[test]
fn chat_agent_block_single_task() { ... }

#[test]
fn chat_fix_pipeline_multi_session() { ... }

#[test]
fn chat_completed_pipeline() { ... }

#[test]
fn chat_build_failed() { ... }
```

Plus PNG snapshots: `cargo test --test tui_snapshot_tests`

---

## Migration

This is a clean replacement, not a gradual migration:

1. Add new files: `pipeline_chat.rs`, `chat_builder.rs`, new types in `types.rs`
2. Update callers: `render_workflow()` → `render_pipeline_chat()`
3. Update `builder.rs`: `build_workflow_view()` → `build_pipeline_chat()`
4. Delete old files: `workflow.rs`, `epic_tree.rs`, `stage_list.rs`, `stage_track.rs`, `lane_dag.rs`, `issue_list.rs`
5. Delete old types: `WorkflowView`, `EpicView`, `StageView`, `SubStageView`, `SubtaskLine`, etc.
6. Update tests

The `buffer_to_ansi()` path stays identical — the chat renders to a Buffer just like the old views did. CI output works the same way.

---

## Scope Boundary

This plan covers:
- Message data model (replaces WorkflowView/EpicView/StageView)
- chat_builder (TaskGraph → Chat, replaces workflow builder)
- pipeline_chat widget (Chat → Buffer, replaces workflow/epic_tree/stage_list)
- Deletion of old view stack
- Tests

Does **not** cover:
- Two-pane layout (→ tui-two-pane-plan.md)
- Plan list / file picker (→ tui-two-pane-plan.md)
- Interactive navigation (→ tui-two-pane-plan.md)
