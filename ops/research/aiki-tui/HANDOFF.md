# Aiki Status Screen — Design Handoff for Implementation

## Context

This document captures the design evolution of `aiki status` — the terminal UI that displays while background tasks (build, review, fix) run. The design was iterated through 6 rounds of HTML mockups simulating ratatui terminal output. Open the HTML files in a browser to see exactly what each screen should look like.

The implementation target is **ratatui** (Rust TUI library).

---

## Design Evolution (read mockups in this order)

### 1. `arcade-status.html` — Pac-Man theme (exploratory, discarded)
Explored 80s arcade metaphors. Dots = subtasks, Pac-Man = cursor eating through work. Fun but too gimmicky.

### 2. `tecmo-status.html` — Tecmo Bowl theme (exploratory, discarded)
Football play diagram metaphor. Creative but wrong mental model for code tasks.

### 3. `tetris-status.html` — Tetris theme (exploratory, discarded)
Falling blocks = tasks landing in columns. Interesting but the game metaphor distracts from information.

### 4. `status-clean.html` — **THE FINAL CLEAN DESIGN** ⭐
Stripped arcade theming, kept the good idea: horizontal dot tracks for build→review→fix progression. This is the base design. Professional dev tool aesthetic (lazygit, btop, k9s territory). 8 scenes showing single-task detail, multi-task compressed view, event log, and key bindings.

### 5. `status-parallel.html` — Parallel work streams
Extended the clean design to show **parallel subtasks within phases** using DAG rendering with box-drawing characters. 8 scenes showing fan-out/fan-in patterns. This is the key visual innovation.

### 6. `status-scale.html` — Progressive density tiers (1→100+ agents)
Four auto-selecting density tiers based on active task count. 9 scenes. This defines how the UI scales.

### 8. `plans-integration.html` — **PLANS IN THE TUI** ⭐
Three options for integrating plan files into the pipeline model: (A) sidebar/header nav, (B) plan as "phase 0" on the track, (C) separate tabs with `[P] Plans` / `[T] Tasks` switching. Includes comparison summary with pros/cons. 10 scenes across all three options.

### 7. `status-fonts.html` — Font comparison
Same screen rendered in 9 monospace fonts. Analysis of Unicode character safety tiers. Key finding: design for JetBrains Mono (Ghostty default), implement `--ascii` fallback.

---

## Core Design Decisions

### Visual Vocabulary

```
Node states:
  ●  completed (filled circle, U+25CF)
  ◉  active/in-progress (fisheye, U+25C9) — FALLBACK: use ● with bright color
  ○  pending (empty circle, U+25CB)

Phase indicators:
  ✓  phase passed (check mark, U+2713)
  ✗  phase failed (ballot X, U+2717)

Status icons:
  ▸  in progress (right-pointing triangle, U+25B8)
  !  stuck/needs attention (ASCII)
  ◆  event marker (U+25C6)

DAG connectors (box-drawing, universally safe):
  ━  horizontal heavy (U+2501)
  ┬  heavy down and horizontal (U+252C)
  ├  heavy vertical and right (U+251C)
  ╰  light arc up and right (U+2570)
  ╯  light arc up and left (U+256F)
  │  vertical line (U+2502)
  ─  horizontal line (U+2500)

Progress bars:
  █  full block (U+2588)
  ░  light shade (U+2591)

Parallel notation (compressed view):
  [2‖]  means "2 parallel streams active"

Border chars:
  ╭╮╰╯│─  rounded corners for boxes
```

### Color Palette

```
green   #5fcc68  — build phase, completed, success
cyan    #5bb8c9  — review phase, info
yellow  #d4a840  — fix phase, active/warning, in-progress
red     #e05555  — errors, failures, stuck
magenta #c470b0  — security issues, Cursor agent
blue    #5588cc  — Devin agent, informational
orange  #cc8844  — warnings
dim     #3a3a44  — borders, inactive, labels
white   #cccccc  — task names, emphasis
```

### Phase Layout

Each task progresses through three horizontal phases:

```
build ━━━━━━━━━━ → review ━━━━━━━━━━ → fix ━━━━━━━━━━
```

Within each phase, subtasks can run in parallel (fan-out/fan-in):

```
● Analyze ━━┬━ ● Types   ━━━┬━ ● Stripe client ━━┬━ ◉ Webhooks
             ├━ ● Schema  ━━━╯                     ├━ ◉ Checkout
             ╰━ ● Config  ━━━━━━━━━━━━━━━━━━━━━━━━┬━ ○ Migrate
                                                   ╰━ ○ Tests
```

Layout algorithm: same as `git log --graph` — topological sort, parallel nodes stacked vertically at same horizontal position.

---

## Four Progressive Density Tiers

The UI auto-selects tier based on active task count. Override with `--density=full|row|group|fleet`.

### Tier 1: Full DAG (1–3 tasks)
- Complete DAG with parallel streams, subtask labels, inline issues
- You care about every subtask
- See `status-clean.html` scenes 1-5 and `status-parallel.html`

### Tier 2: Row View (4–15 tasks)
- One row per task
- Phases shown as fill bars (`████░░░░`) instead of DAGs
- Parallel work collapsed to bar fill rate
- Issue counts inline
- Key addition: `--stuck` filter

```
▸  Payment migration    cur  ████████████  ████████✗2  ██░░░░  1m02
                              [build ✓]    [review]    [fix]
```

### Tier 3: Group View (16–50 tasks)
- Grouped by status (building/reviewing/fixing/done/stuck)
- Pipeline histogram showing counts per phase
- Per-agent breakdown with mini stacked bars
- Only stuck tasks listed individually

```
PIPELINE   ██████████████ 34  ████████ 18  ██████ 11  ████████████████████ 58  ██ 4
           building       reviewing   fixing      done                stuck
```

### Tier 4: Fleet Dashboard (50+ tasks)
- Pipeline stage histogram
- Agent table with completion rates
- Throughput sparkline (commits/min over last 10 minutes)
- Only stuck/failed tasks listed
- Agent drill-down: first-pass success rate, avg fix iterations, stuck rate, common failure patterns

```
Throughput: 3.2/min    ▁▂▃▅▇█▇▅▃▂

127 tasks: 34 building, 18 reviewing, 11 fixing, 58 done, 4 stuck, 2 queued

AGENT          TASKS   DONE  1ST-PASS   AVG-ITER  STUCK
claude-code      52     31    62%        1.8       1.9%
cursor           48     22    54%        2.1       4.2%
devin            27      5    41%        2.4       7.4%
```

### Auto-selection logic
```rust
match active_task_count {
    0..=3   => Tier::Full,
    4..=15  => Tier::Row,
    16..=50 => Tier::Group,
    _       => Tier::Fleet,
}
```

Override flags:
- `--density=full|row|group|fleet` — force specific tier
- `--stuck` — filter to problems only (works at any tier)
- `--agent <name>` — filter to one agent

### Drill-down navigation
- `[Enter]` focus — zoom from fleet → agent → task → subtask DAG
- `[Esc]` back — zoom out
- Any tier can drill to Tier 1 detail for individual tasks

---

## Unicode Safety Tiers

Claude Code broke on Linux by using ⏵ (U+23F5) from the Miscellaneous Technical block. Learn from their mistake.

**Tier 1 — works everywhere (ASCII):**
`* + - = | [ ] # > .`

**Tier 2 — works in all modern coding fonts (our primary tier):**
`● ○ ✓ ✗ ─ │ ┬ ├ ╯ ╭ ╰ █ ░ ▸ ▪`

**Tier 3 — needs a good font:**
`◉ ◆ ▓ ━ ╌`

**Tier 4 — risky, avoid:**
`⏵ ⎔ ⎡`

Our design uses Tier 2–3. The `◉` (fisheye) for active nodes is the main risk — fallback to `●` with bright/bold color if it doesn't render. Box-drawing chars are universally safe.

**Ship an `--ascii` fallback mode** for degraded environments.

---

## Ratatui Implementation Notes

### Widget mapping per tier

| Tier | Primary widgets |
|------|----------------|
| Tier 1 (Full DAG) | Custom widget using `Span` sequences in `Line`. Layout: topological sort with vertical stacking for concurrent nodes. Box-drawing chars via `┬├╰━╯` |
| Tier 2 (Row) | `Table` widget with `Gauge` bars per row |
| Tier 3 (Group) | Multiple `Block` widgets with `BarChart` for histogram |
| Tier 4 (Fleet) | Dashboard layout with `Sparkline`, `Table`, `BarChart` |

### DAG rendering approach
Custom widget. Build a `Vec<Line>` where each `Line` contains `Span` sequences:
- Green spans for completed nodes
- Yellow/blinking for active
- Dim for pending
- Box-drawing chars as dim spans connecting nodes

### Multi-task log
`List` widget with task ID prefix. Parallel completions shown on one line with `│` separator and `(parallel)` suffix.

### Key bindings (updated for v3)
```
[q] quit          [Enter] drill/select    [j/k] navigate tasks
[Esc] back        [Space] toggle log      [s] sort
```
Note: v3 uses `[Enter]` as the universal drill-in key (replaces `[n]`).
`[Space]` toggles log scroll (replaces `[l]` to avoid vim conflict).

### Refresh rate
Target 100ms refresh for active tasks. Use `crossterm` events with polling.

---

## Font Landscape (for context)

Aiki doesn't choose the font — the terminal does. But we should know what our users run:

- **Ghostty** (Mitchell Hashimoto's terminal) → JetBrains Mono embedded
- **Claude Code** → inherits terminal font
- **OpenCode/Crush** → BubbleTea renderer, inherits terminal
- **Most popular fonts**: JetBrains Mono, Fira Code, Berkeley Mono ($75 premium), Monaspace (GitHub), Geist Mono (Vercel)

**Design for JetBrains Mono** as reference. Test against Fira Code and Source Code Pro.

---

## File Manifest

```
mockups/
  v3.html                 — ⭐ CURRENT implementation target (23 scenes, 7 screens)
  v2.html                 — Previous: full-screen tree DAG + stage track
  v1.html                 — Previous: sidebar + lanes
  arcade-status.html      — Pac-Man exploration (discarded)
  tecmo-status.html       — Tecmo Bowl exploration (discarded)
  tetris-status.html      — Tetris exploration (discarded)
  status-clean.html       — Clean design foundation (8 scenes)
  status-parallel.html    — Parallel work streams (8 scenes)
  status-scale.html       — Progressive density tiers (9 scenes)
  status-fonts.html       — Font comparison + Unicode analysis
  plans-integration.html  — Plans integration into pipeline (3 options)
HANDOFF.md                — This file
```

**v3.html is the current implementation target.** The implementation plan is at `ops/now/ux.md`.
The earlier mockups (v1, v2, status-*.html) are context for how we got there.
