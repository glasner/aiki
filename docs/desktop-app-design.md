# Aiki Desktop App Design

## Inspiration

[Superset.sh](https://superset.sh/) provides a desktop workspace for running parallel AI coding agents in isolated git worktrees. It wraps terminals, diffs, file browsers, and agent monitoring into a single Electron app. We want that ergonomic — a unified desktop surface for orchestrating AI work — but shaped around Aiki's opinionated SDLC workflow rather than being a generic terminal multiplexer.

## What Makes Aiki Different from Superset

Superset is agent-agnostic: it gives you terminals and worktrees, and you bring your own workflow. Aiki already *is* the workflow — plan, build, review, fix — with task graphs, provenance, session isolation, and JJ-backed change tracking. The desktop app should surface Aiki's native concepts as first-class UI elements, not just wrap terminals.

| Superset | Aiki Desktop |
|----------|-------------|
| Generic terminal multiplexer for agents | Workflow-native UI for plan/build/review/fix |
| Git worktrees for isolation | JJ workspaces for isolation (already built) |
| You manage branches manually | Task graph manages the DAG automatically |
| No built-in review/fix loop | Review/fix loop is a core primitive |
| Agent-agnostic (bring your own) | Agent-aware (claude-code, cursor, codex, ACP) |
| No provenance tracking | Full provenance per change via JJ metadata |

## Design Principles

1. **Workflow-first, not terminal-first.** The primary view is the task graph and SDLC pipeline, not a grid of terminals. Terminals exist to serve tasks.
2. **Surface what Aiki already knows.** Aiki tracks task status, provenance, agent assignments, dependencies, reviews, and diffs. The desktop app renders this data — it doesn't recreate it.
3. **One pane of glass.** See all in-flight work across all agents in one place. No switching between terminal tabs to figure out what's happening.
4. **Keyboard-driven with visual affordances.** Power users navigate by keyboard; the UI provides at-a-glance status for everyone.
5. **Rust-native where possible.** Prefer Tauri over Electron to stay in the Rust ecosystem and keep the binary small.

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Desktop framework | **Tauri v2** | Rust backend, small binary, native webview. Aligns with Aiki's Rust codebase. |
| Frontend | **React + TypeScript** | Broad ecosystem, fast iteration for UI. |
| Styling | **Tailwind CSS v4** | Utility-first, themeable, fast. |
| State management | **Zustand** | Lightweight, works well with external event sources. |
| Terminal emulation | **xterm.js** | Standard for web-based terminals; embed agent sessions. |
| IPC | **Tauri commands + events** | Type-safe Rust ↔ JS bridge via Tauri's invoke/emit system. |
| Data source | **Aiki CLI / lib** | The Tauri backend calls into `aiki` library crate directly (or shells out to `aiki` CLI). Task graph, provenance, and session data come from JJ via the existing Rust code. |

### Why Tauri over Electron

- Aiki is a Rust project. Tauri lets the backend be Rust — we can call `aiki` library functions directly without IPC serialization overhead.
- Binary size: ~10MB vs ~150MB+ for Electron.
- Memory: native webview vs bundled Chromium.
- Security: Rust backend with capability-based permissions.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Tauri Shell (native window)             │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                  React Frontend                       │  │
│  │                                                       │  │
│  │  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌───────────┐  │  │
│  │  │ Sidebar │ │ Pipeline │ │ Detail │ │ Terminal  │  │  │
│  │  │  Panel  │ │   View   │ │  Panel │ │   Panel   │  │  │
│  │  └─────────┘ └──────────┘ └────────┘ └───────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
│                           │ Tauri IPC                       │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                  Rust Backend                         │  │
│  │                                                       │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐  │  │
│  │  │TaskGraph │ │ Session  │ │Provenance│ │  JJ    │  │  │
│  │  │ Monitor  │ │ Manager  │ │ Reader   │ │ Bridge │  │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────┘  │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Backend (Rust / Tauri)

The Tauri backend wraps Aiki's existing library crate and exposes commands to the frontend:

- **TaskGraph Monitor** — Polls or watches JJ for changes, materializes the task graph, and pushes updates to the frontend via Tauri events.
- **Session Manager** — Lists active agent sessions, spawns new sessions (via `aiki run`), and monitors session health (PID liveness).
- **Provenance Reader** — Reads `[aiki]` metadata blocks from JJ change descriptions.
- **JJ Bridge** — Thin wrapper around `jj-lib` for diffs, log, status, workspace operations.
- **Command Runner** — Executes `aiki build`, `aiki review`, `aiki fix`, etc. as child processes, streaming stdout/stderr to the frontend.

### Frontend (React)

The frontend consumes Tauri events and commands to render the UI. It never talks to JJ directly.

## Screen Layout

### Main Layout: Three-Column + Bottom Panel

```
┌──────────┬──────────────────────────┬──────────────────┐
│          │                          │                  │
│ Sidebar  │      Main View           │   Detail Panel   │
│          │                          │                  │
│ Projects │  (Pipeline / Graph /     │  (Task detail /  │
│ Tasks    │   Board / Terminal)      │   Diff / Review  │
│ Sessions │                          │   / Provenance)  │
│ History  │                          │                  │
│          │                          │                  │
├──────────┴──────────────────────────┴──────────────────┤
│                   Terminal Drawer                       │
│  [agent-1: claude]  [agent-2: codex]  [agent-3: ...]  │
└────────────────────────────────────────────────────────┘
```

## Screens & Views

### 1. Dashboard (Home)

The landing screen. Shows a high-level overview of all work in the current repository.

**Content:**
- **Active pipelines** — Any running `build`, `review`, or `fix` pipelines with live progress (mirrors the existing TUI build/review/fix screens).
- **Task summary** — Counts by status: in-progress, blocked, completed, needs-review.
- **Recent activity** — Timeline of recent task state changes and agent actions.
- **Agent status** — Which agents are currently active, what they're working on.

**Interactions:**
- Click a pipeline → opens Pipeline View for that build/review/fix.
- Click a task → opens Task Detail.
- Click an agent → opens its terminal session.

### 2. Pipeline View

Live visualization of an `aiki build`, `aiki review`, or `aiki fix` run. This is the desktop equivalent of Aiki's existing TUI screens (`build.rs`, `review.rs`, `fix.rs`).

**Content:**
- **Phase progress bar** — plan → decompose → loop (with sub-phase detail).
- **Subtask lanes** — Visual representation of parallel execution lanes, showing which tasks are running concurrently and their dependencies.
- **Per-subtask status** — Agent assignment, elapsed time, status (queued/running/done/failed).
- **Review loop indicator** — For fix pipelines: current iteration number, pass/fail status of each review cycle.

**Layout:**
```
 ops/now/user-auth.md
 [luppzupt] Add user authentication
 ▸ build  2/4  1m32s
    ✓ plan         8s
    ✓ decompose   14s
    ▸ loop  2/4  1m10s                    ●━━●━◉━━○
     ⎿ ✓ Create auth middleware     claude    32s
     ⎿ ✓ Add session storage        codex     28s
     ⎿ ▸ Wire up login endpoint     claude    10s
     ⎿ ○ Add rate limiting          (queued)
```

This reuses the rendering logic from the existing TUI — same data model, different renderer.

### 3. Task Graph View

Interactive DAG visualization of the full task graph.

**Content:**
- **Nodes** = tasks, colored by status (in-progress, blocked, done, failed).
- **Edges** = dependency links (`depends-on`, `needs-context`, `blocks`, `fixes`, etc.).
- **Clusters** = epics group their subtasks visually.
- **Filters** — By status, agent, epic, time range.

**Interactions:**
- Click a node → opens Task Detail in the right panel.
- Hover → shows task summary tooltip.
- Zoom/pan for large graphs.
- Filter controls to focus on specific epics or statuses.

### 4. Board View (Kanban)

Optional kanban-style view for teams that prefer column-based tracking.

**Columns:** Planned → In Progress → In Review → Fix → Done

Each card shows: task title, agent, elapsed time, dependency count.

### 5. Task Detail Panel (Right Sidebar)

Shows everything about a single task when selected from any view.

**Sections:**
- **Header** — Change ID, status badge, agent assignment, elapsed time.
- **Description** — Task description from the JJ change.
- **Provenance** — Full `[aiki]` metadata: agent, session, tool, confidence.
- **Dependencies** — Links to parent, siblings, blockers.
- **Diff** — Inline diff viewer showing what changed (rendered from `jj diff`).
- **Review** — If reviewed: review status, issues list, fix iterations.
- **History** — Change evolution log (from `jj evolog`).

### 6. Terminal Panel (Bottom Drawer)

Tabbed terminal emulator for live agent sessions. Similar to Superset's terminal management but integrated with Aiki's session system.

**Features:**
- **One tab per active agent session.** Tabs auto-created when `aiki run` spawns an agent.
- **Session metadata in tab header** — Agent type icon, task ID, elapsed time.
- **Input/output streaming** — Live view of agent work via PTY.
- **Quick actions** — Stop agent, restart, open in external editor.
- **Resizable** — Drag to resize the drawer, or collapse to a thin status bar.

### 7. Session Manager

Dedicated view for managing all agent sessions.

**Content:**
- **Active sessions table** — Session ID, agent type, task, PID, uptime, status.
- **Session history** — Past sessions with start/end times and outcomes.
- **Spawn controls** — Start a new agent session with agent type and task selection.

### 8. Plan Editor

For `aiki plan` — a split-pane editor for interactive plan authoring.

**Layout:**
- **Left:** Markdown editor for the plan file.
- **Right:** Live preview + AI chat panel for plan refinement.
- **Bottom:** Terminal for the planning agent session.

### 9. Review View

For `aiki review` results — structured display of review outcomes.

**Content:**
- **Review summary** — Pass/fail, issue count, criteria scores.
- **Issues list** — Each issue with severity, description, file location.
- **Diff context** — Click an issue to see the relevant code with the issue highlighted.
- **Fix action** — One-click "Fix this review" button that launches `aiki fix`.

### 10. Settings

- **Agent configuration** — Default agent types, API keys, model preferences.
- **Theme** — Dark/light mode (leverage Aiki's existing theme system from `tui/theme.rs`).
- **Keyboard shortcuts** — Customizable bindings.
- **Flow hooks** — Visual editor for `.aiki/hooks.yml`.

## Data Flow

```
JJ Repository
     │
     ▼ (jj-lib / jj CLI)
Aiki Rust Library
     │
     ▼ (Tauri commands + events)
React Frontend
     │
     ▼ (renders)
User
```

### Event-Driven Updates

The backend watches for JJ repository changes (via filesystem watcher on `.jj/`) and pushes `TaskGraphUpdated` events to the frontend. The frontend re-renders affected views. This mirrors the existing TUI's worker thread pattern but uses Tauri's event system instead of `mpsc` channels.

```
Backend                              Frontend
  │                                     │
  │  [fs watch: .jj/ changed]          │
  │  → materialize_graph()              │
  │  → emit("task-graph-updated", graph)│
  │  ─────────────────────────────────► │
  │                                     │  → zustand store update
  │                                     │  → React re-render
```

### Tauri Commands (Rust → JS bridge)

```rust
// Examples of Tauri commands the backend would expose:

#[tauri::command]
fn get_task_graph() -> Result<TaskGraphDto, String>

#[tauri::command]
fn get_task_detail(change_id: String) -> Result<TaskDetailDto, String>

#[tauri::command]
fn get_task_diff(change_id: String) -> Result<String, String>

#[tauri::command]
fn list_sessions() -> Result<Vec<SessionDto>, String>

#[tauri::command]
fn spawn_agent(agent_type: String, task_id: String) -> Result<SessionDto, String>

#[tauri::command]
fn run_build(plan_path: String) -> Result<PipelineDto, String>

#[tauri::command]
fn run_review(task_id: String) -> Result<PipelineDto, String>

#[tauri::command]
fn run_fix(review_id: String) -> Result<PipelineDto, String>

#[tauri::command]
fn get_provenance(change_id: String) -> Result<ProvenanceDto, String>
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `⌘1` | Dashboard |
| `⌘2` | Task Graph |
| `⌘3` | Board View |
| `⌘B` | New Build |
| `⌘R` | Review selected task |
| `⌘F` | Fix selected review |
| `⌘T` | Toggle terminal drawer |
| `⌘K` | Command palette (fuzzy search for any task, command, or view) |
| `j/k` | Navigate task list |
| `Enter` | Open task detail |
| `Esc` | Close detail panel / back |
| `⌘N` | New plan |
| `Tab` | Cycle terminal tabs |

## Theming

Extend Aiki's existing `tui/theme.rs` color palette to CSS custom properties:

```css
:root {
  --aiki-bg: #1a1b26;
  --aiki-fg: #c0caf5;
  --aiki-accent: #7aa2f7;
  --aiki-success: #9ece6a;
  --aiki-warning: #e0af68;
  --aiki-error: #f7768e;
  --aiki-muted: #565f89;
}
```

Dark mode is default. Light mode derived from the existing `theme.rs` light palette.

## Monorepo Structure

```
aiki/
├── src/                    # Existing Rust CLI + library
├── desktop/                # New: Tauri desktop app
│   ├── src-tauri/          # Rust backend (Tauri commands, event handlers)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── commands/   # Tauri command handlers
│   │   │   ├── events/     # Event watchers and emitters
│   │   │   └── bridge/     # Aiki lib integration layer
│   │   ├── Cargo.toml      # Depends on aiki (path = "../../")
│   │   └── tauri.conf.json
│   ├── src/                # React frontend
│   │   ├── App.tsx
│   │   ├── components/
│   │   │   ├── sidebar/
│   │   │   ├── pipeline/
│   │   │   ├── task-graph/
│   │   │   ├── task-detail/
│   │   │   ├── terminal/
│   │   │   ├── board/
│   │   │   ├── review/
│   │   │   └── plan-editor/
│   │   ├── stores/         # Zustand stores
│   │   ├── hooks/          # Tauri event listeners
│   │   └── lib/            # Shared utilities
│   ├── package.json
│   ├── tailwind.config.ts
│   └── vite.config.ts
├── docs/
├── tests/
└── Cargo.toml              # Workspace: [members] includes desktop/src-tauri
```

## Migration Path from TUI

The existing TUI (`src/tui/`) remains as-is for terminal users. The desktop app is additive.

**Shared code:**
- `TaskGraph` materialization logic (already in `src/tasks/graph.rs`)
- Provenance parsing (already in `src/`)
- Session management (already in `src/session/`)
- All SDLC commands (already in `src/commands/`)

The Tauri backend imports the `aiki` library crate and calls the same functions. No duplication of core logic.

## Implementation Phases

### Phase 1: Foundation
- Tauri project scaffolding in `desktop/`.
- Rust backend: TaskGraph monitor, basic Tauri commands (`get_task_graph`, `get_task_detail`, `get_task_diff`).
- React frontend: Sidebar, Dashboard, Task Detail panel.
- Event-driven graph updates via filesystem watcher.

### Phase 2: Pipeline & Terminal
- Pipeline View (port TUI build/review/fix screens to React).
- Terminal panel with xterm.js (embed agent sessions).
- Session management (list, spawn, stop agents).
- Command runner for `aiki build/review/fix` with live output streaming.

### Phase 3: Graph & Review
- Interactive task graph visualization (DAG with zoom/pan/filter).
- Review View with issue list and diff context.
- Board View (kanban).
- One-click fix from review.

### Phase 4: Editing & Polish
- Plan Editor with markdown editing and AI chat.
- Settings panel.
- Keyboard shortcut system and command palette.
- Theming (dark/light, custom themes).
- Auto-update and distribution (`.dmg`, `.AppImage`, `.msi`).

## Open Questions

1. **Graph visualization library** — D3.js? React Flow? ELK.js for layout? Need something that handles DAGs well with auto-layout.
2. **Terminal multiplexing** — Should we manage PTYs directly in Rust (via `portable-pty`) or delegate to the system shell?
3. **Multi-repo support** — Should the app support multiple repositories simultaneously, or one repo per window (like VS Code)?
4. **Collaboration features** — Real-time sharing of task graph state across team members? Or local-only for v1?
5. **Plugin system** — Should the desktop app support plugins/extensions, or keep it focused on core Aiki workflow?
