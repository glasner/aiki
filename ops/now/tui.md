# TUI: Kanban Board for Headless Agents

## Command

```bash
aiki <path>           # TUI scoped to specs in <path>
aiki ops/now          # your ops/now specs + related agents
aiki work/active      # someone else's convention
aiki .                # current directory
aiki                  # defaults to ops/now/ if exists, else .
```

Works with any directory structure - agnostic to naming conventions.

## Vision

A terminal UI showing both **specs** (markdown plans in a directory) and **agents** executing that work. The full picture: what's planned, what's running, what needs review.

Users kick off work with `aiki build`, `aiki review`, `aiki fix`. The TUI is the command center for watching everything in flight.

## Core Concepts

**Specs as Source**: Markdown files in the target directory represent planned work:
- Expand to see tasks spawned from the spec
- Kick off `aiki build <spec>` directly from TUI
- Track which specs have active agents

**Agents as Cards**: Each running/completed agent is a card on the board:
- Shows task name, status, agent type (build/review/fix)
- Color-coded by status (running, waiting, done, failed)
- Expandable to show comments, session output, or diff

**Columns** (status-based):

| Column | Criteria |
|--------|----------|
| Specs | `*.md` files with no `type:build` tasks |
| Pending | Tasks with `status: pending` |
| In Progress | Tasks with `status: in_progress` |
| Completed | Tasks with `status: completed` |

**Task Types** (shown as card prefix):
- `[build]` - task has `type: build` (from `aiki build`)
- `[review]` - task has `type: review` (from `aiki review`)
- `[fix]` - task has `source: task:*` (followup from review)
- No prefix for generic tasks (e.g., planning, editing)

**Spec Visibility**: A spec disappears from the Specs column when any `type:build` task exists for it. Edit tasks (interactive planning) don't affect spec visibility.

**Grouping by Spec**: Tasks in Pending/In Progress/Completed are grouped under their source spec. Expand a spec group to see individual tasks.

**Live Updates**: Board refreshes as agents complete turns, close tasks, or emit events.

## Key Features

### P0 - MVP

1. **Kanban View**
   - Grid of cards organized by status column
   - Card shows: task ID (short), name, agent type, duration
   - Visual indicator for active/stalled agents

2. **Keyboard Navigation**
   - `h/j/k/l` or arrows - move between cards
   - `Enter` - expand card to detail view
   - `q` - quit / back
   - `r` - refresh
   - `?` - help

3. **Detail Panel**
   - Shows full task info, comments, source lineage
   - Tab between: Comments | Session | Diff
   - Session view shows recent agent output (tail of session)

4. **Agent Control** (see [UX Flows](#ux-flows) for details)
   - `e` - edit (launch interactive Claude session with spec as context)
   - `b` - build (launch `aiki build` on selected spec, runs headless)
   - `s` - stop agent
   - `f` - follow (attach to live output)

### P1 - Enhanced

5. **Filtering/Search**
   - `/` - fuzzy search tasks
   - Filter by: agent type, priority, source file
   - Status filters: show/hide columns

6. **Multi-Select Operations**
   - `Space` - toggle select
   - Batch stop, batch close

7. **Split View**
   - Left: kanban board
   - Right: detail panel (always visible)
   - Responsive to terminal width

8. **Notifications**
   - Toast/status bar when agent completes
   - Sound/bell option for failures

### P2 - Advanced

9. **Session Attachment**
   - From detail view, drop into full interactive session
   - Return to TUI when done

10. **Graph View**
    - Alternative view showing task dependencies
    - Critical path highlighting

11. **Robot Mode**
    - `--json` output for scripting
    - Integration with other tools

## Tech Stack

**Rust + Ratatui**

- Same language as aiki CLI - share types/logic with existing codebase
- Ratatui is mature, well-documented
- Crossterm backend for cross-platform
- Examples: gitui, bottom

## Architecture

```
$ aiki ops/now

┌─ ops/now/ ───────────────────────────────────────────────────────────────────┐
│     Specs      │     Pending     │     In Progress     │     Completed      │
├────────────────┼─────────────────┼─────────────────────┼────────────────────┤
│                │                 │                     │                    │
│ tui.md         │ plugin phase 1  │ ▾ git-diffs.md      │ task-events        │
│ codex-events   │ plugin phase 2  │   [build] 12m ●     │   [build] ✓        │
│                │                 │   [review] 3m ●     │   [review] ✓       │
│                │ tasks-compact   │                     │                    │
│                │                 │ ▸ review-fix.md     │ lazy-load          │
│                │                 │   [build] 8m ● ◀────│── selected         │
│                │                 │                     │   [build] ✓        │
│                │                 │                     │   [review] ✓       │
│                │                 │                     │   [fix] ✓          │
│                │                 │                     │                    │
└────────────────┴─────────────────┴─────────────────────┴────────────────────┘
├──────────────────────────────────────────────────────────────────────────────┤
│ [build] review-fix.md                                                        │
│ ▶ claude-code | 8m12s | ████████░░ 80%                                       │
├──────────────────────────────────────────────────────────────────────────────┤
│ [Comments] [Session] [Diff]                                                  │
│                                                                              │
│ > Implementing fix command...                                                │
│ > Reading task comments from storage                                         │
│ > Creating followup task template                                            │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
 h/j/k/l:move  Enter:expand  e:edit  b:build  s:stop  ?:help

Legend:  ● running   ✓ done   ✗ failed   ▸ collapsed   ▾ expanded
```

## UX Flows

### `e` - Edit (Interactive Session)

When user presses `e` on a spec:

1. TUI suspends (restore terminal to normal mode)
2. Launch Claude with spec as context and initial prompt:
   ```bash
   claude @<spec-path> -p "Let's continue planning in @<spec-path>"
   ```
3. User works interactively with Claude
4. Claude uses `aiki task` per CLAUDE.md to track any work
5. On exit (user quits Claude), TUI resumes and refreshes
6. Any new tasks spawned by the session appear in appropriate columns

**Why this approach:**
- Minimal - just launches Claude with context, no special modes
- Claude follows existing CLAUDE.md instructions for task tracking
- Suspend/resume pattern is familiar (like lazygit opening $EDITOR)
- Long sessions are fine - user can quit TUI if they prefer

### `b` - Build (Headless Agent)

When user presses `b` on a spec:

1. Launch `aiki build <spec-path>` in background
2. TUI stays running (doesn't suspend)
3. New agent card appears in Running column
4. User can monitor progress, press `f` to follow output

### `f` - Follow (Attach to Output)

When user presses `f` on a running agent:

1. TUI suspends
2. Attach to agent's live output stream (tail -f style)
3. `q` or `Ctrl-C` detaches and resumes TUI

### `s` - Stop Agent

When user presses `s` on a running agent:

1. Confirm dialog: "Stop agent <name>? [y/N]"
2. If confirmed, send stop signal to agent
3. Agent card moves to appropriate column (failed/stopped)

## Data Sources

- **Specs**: Markdown files in target directory (glob `*.md`)
- **Task State**: `aiki task list --json` or direct JJ query, filtered by `--source file:<path>`
- **Agent Sessions**: Read from session storage (`~/.aiki/sessions/`)
- **Live Updates**: Watch for file changes or use event bus

## Implementation Plan

### Phase 1: Static Board
1. Create `aiki <path>` subcommand with path argument
2. Render specs column from directory listing
3. Render kanban columns from task list (filtered by source)
4. Basic hjkl navigation
5. Card selection highlighting

### Phase 2: Detail View
1. Expand/collapse card to detail panel
2. Show task comments
3. Show session output (tail)

### Phase 3: Agent Control
1. Stop running agents from TUI
2. Follow/attach to live session
3. Launch new agents

### Phase 4: Live Updates
1. Poll or watch for task changes
2. Refresh board without losing selection
3. Status bar notifications

## Open Questions

1. **Session storage format**: Is current format suitable for tailing? Need streaming?
2. **Event bus integration**: Can TUI subscribe to events directly?
3. **Multi-agent coordination**: How to show dependencies between agents?
4. **Persistence**: Should TUI state (collapsed columns, filters) persist?

## Inspiration

- [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) - kanban + graph view for issue tracking
- [lazygit](https://github.com/jesseduffield/lazygit) - keyboard-driven TUI patterns
- [gitui](https://github.com/extrawurst/gitui) - Rust TUI for git (Ratatui)
- [bottom](https://github.com/ClementTsang/bottom) - Rust system monitor (Ratatui)

## Success Criteria

- Can see specs and their execution status at a glance
- Can kick off `aiki build` on a spec directly from TUI
- Can drill into any agent's progress without leaving TUI
- Can stop misbehaving agents quickly
- Works with any directory structure (not tied to ops/now naming)
- Keyboard-only workflow (no mouse required)
- Responsive to terminal resize
- Works over SSH
