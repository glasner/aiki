# TUI Screen States

**Date**: 2026-03-21
**Status**: Draft
**Builds on**: [chatty-iteration-2.md](chatty-iteration-2.md)

---

## Rendering Approach

- **ratatui** for buffer/widget composition (keep the rendering primitives)
- **Inline rendering** using cursor-up overwrite (no alternate screen)
- Output stays in scrollback when done (like Claude Code)
- Timers tick every second regardless of JJ read latency (cached graph)

## Visual Primitives

| Symbol | Meaning |
|--------|---------|
| `合`   | Phase header (completed/idle) |
| `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | Phase header active (braille spinner, 10 frames) |
| `⎿`   | Nested child / tree connector |
| `✔`   | Subtask done |
| `▸`   | Subtask active / running |
| `○`   | Subtask pending (in active lane, not yet started) |
| `◌`   | Subtask pending (in non-started lane) |
| `✘`   | Failed |
| `---` | Section separator |

## Color Rules

| Element | Color |
|---------|-------|
| Phase header (`合 name`) — completed | dim |
| Phase header (`⠹ name`) — active | bold, yellow spinner, fg text |
| Agent label `(claude)` | dim |
| `⎿` connector | dim |
| Done phase result | dim (entire line) |
| Active phase result | yellow |
| `✔` + done subtask | green icon, dim text |
| `▸` + active subtask | yellow icon, fg text |
| `○` pending subtask (active lane) | dim |
| `◌` pending subtask (non-started lane) | dim |
| Elapsed (right-aligned) | dim |
| Issue number | fg |
| Issue text | fg |
| `Iteration N` header | bold, fg |
| Summary per-agent line | dim |
| Summary total line | bold |

## Shared Components

These building blocks are reused across all pipeline flows.

### Phase Line

Every pipeline step renders as a phase header + child result:

```
[80 cols]
 ⠹ <phase-name> (<agent>)                    ← yellow spinner, bold+fg name, dim agent
 ⎿ <result text>                   <elapsed>  ← dim ⎿, text result, dim elapsed right-aligned
```

When active:
```
[80 cols]
 ⠹ decompose (claude)                        ← yellow spinner, bold+fg name, dim agent
 ⎿ Reading plan and creating subtasks...  32s  ← dim ⎿, yellow text, dim 32s right-aligned
```

When done (dimmed):
```
[80 cols]
 合 plan (claude)                             ← dim 合, dim name, dim agent
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md   ← dim ⎿, green ✔, dim text
```

### Session Startup Sequence

Every phase that spawns an agent goes through these steps before the first heartbeat:

```
[80 cols]
 ⠋ decompose (claude)                        ← yellow spinner, bold+fg name, dim agent
 ⎿ creating isolated workspace...             ← dim ⎿, yellow text
```
→
```
[80 cols]
 ⠙ decompose (claude)                        ← yellow spinner, bold+fg name, dim agent
 ⎿ starting session...                        ← dim ⎿, yellow text
```
→
```
[80 cols]
 ⠹ decompose (claude)                        ← yellow spinner, bold+fg name, dim agent
 ⎿ Reading plan and creating subtasks...  32s  ← dim ⎿, yellow text, dim 32s right-aligned
```

- `creating isolated workspace...` — `jj workspace add` is running
- `starting session...` — workspace ready, agent process launching
- Both are yellow (active). Elapsed timer starts when agent connects.

### Subtask Table

A bordered subtask table. Used whenever a parent task has subtasks — after decompose in build/fix flows, and in `task run` when the task has children.

```
[80 cols]
 ---                                                ← dim separator
                                                    ← blank row
     [lkji3d] Epic: Mutex for Task Writes           ← dim brackets+id, fg title
     ✔ Add get_repo_root helper to jj/mod.rs   56s  ← green ✔, dim text, dim 56s right-aligned
     ▸ Lock task writes in tasks/storage.rs    28s  ← yellow ▸, fg text, dim 28s right-aligned
     ◌ Lock conversation writes                     ← dim ◌, dim text
     ○ Delete advance_bookmark from jj/mod.rs       ← dim ○, dim text
     ✘ Build and test the mutex              1m29  ← red ✘, red text, dim 1m29 right-aligned
                                                    ← blank row
 ---                                                ← dim separator
```

- `---` separators above and below
- Header line: `[short-id]` dim, title fg
- Subtask lines: 4-space indent, status icon, name, right-aligned elapsed
- Status icons: `✔` green, `▸` yellow, `○` dim, `◌` dim, `✘` red
- Elapsed only shown for done (`✔`) and active (`▸`) and failed (`✘`) subtasks
- Long names truncate with `…` to fit terminal width

### Lane Blocks

Used inside `合 loop` to show parallel agent activity.

```
[80 cols]
 ⠹ loop                                            ← yellow spinner, bold+fg name
 ⎿ Lane 1 (claude)                                  ← dim ⎿, fg "Lane 1", dim agent
     ⎿ 1/2 subtasks completed                       ← dim ⎿, dim text
     ⎿ Writing lock function in storage.rs      28s  ← dim ⎿, yellow text, dim 28s right-aligned
                                                     ← blank row
 ⎿ Lane 2 (claude)                                  ← dim ⎿, fg "Lane 2", dim agent
     ⎿ 0/3 subtasks completed                       ← dim ⎿, dim text
     ⎿ starting session...                       3s  ← dim ⎿, yellow text, dim 3s right-aligned
```

Done lane:
```
[80 cols]
 ⎿ Lane 1 (claude)                                  ← dim ⎿, dim "Lane 1", dim agent
     ⎿ 2/2 subtasks completed                       ← dim ⎿, dim text
     ⎿ Agent shutdown.                               ← dim ⎿, dim text
```

- Each lane is a `⎿` child of `⠹ loop` (or `合 loop` when done)
- Lane children are further indented with `⎿`
- First child line is always the completion count: `x/y subtasks completed`
- Second child line is the heartbeat (active) or final status (done)
- Active lane: heartbeat text is yellow, elapsed ticks on heartbeat line
- Done lane: both lines dim
- Blank line between lanes

### Issue List

Used after review phases when issues are found.

```
[80 cols]
 ⠹ review (codex)                                  ← yellow spinner, bold+fg name, dim agent
 ⎿ Found 3 issues                                   ← dim ⎿, yellow text
                                                     ← blank row
     1. acquire_named_lock uses wrong error variant  ← fg number, fg text
     2. fs::create_dir_all error silently swallowed  ← fg number, fg text
     3. Box::leak causes unbounded memory growth     ← fg number, fg text
```

- Numbered list, 4-space indent
- Issue text is fg (user needs to read these — never dimmed)
- Blank line between `⎿ Found N issues` and the list

### Summary Line

Final line of a completed pipeline. Shows per-agent breakdown when multiple agents were used, plus a total line.

```
[80 cols]
 ---                                                ← dim separator
                                                    ← blank row
 合 build completed — mutex-for-task-writes.md      ← bold+fg 合 (static, not spinner), bold+fg text
 ⎿ Claude: 2 sessions — 35m33 — 0.72M tokens       ← dim ⎿, dim text
 ⎿ Codex: 3 sessions — 20m — 0.2M tokens           ← dim ⎿, dim text
 ⎿ Total: 5 sessions — 55m33 — 0.92M tokens        ← dim ⎿, bold text
                                                    ← blank row
 Run `aiki task diff lkji3d` to see changes.        ← dim text
```

- `---` separator before
- Phase header is bold
- Per-agent lines are dim (one per distinct agent type)
- Total line is bold
- When only one agent type was used, skip per-agent lines — just show the total
- Hint line is dim

---

## Pipeline Flows

> **Mockup format:** monospace, character-accurate, 1-char left padding, right-aligned metadata.
> Every line has a `←` style annotation suffix (never rendered) describing its color/formatting.

### Flow 1: `aiki run <id>`

Monitors an agent running a task. If the task has subtasks, shows a subtask table (same component used by build/fix flows).

**Phases:** run (+ subtask table if parent task)

#### State 1.0a: Loading

```
[80 cols]
 $ aiki run <id>                                    ← terminal prompt
                                                         ← blank row
 ⠋ task                                                  ← yellow spinner, fg name
 ⎿ Reading task graph...                                 ← dim ⎿, yellow text
```

#### State 1.0b: Resolving agent

```
[80 cols]
 ⠙ task                                                  ← yellow spinner, fg name
 ⎿ Resolving agent...                                    ← dim ⎿, yellow text
```

#### State 1.0c: Creating workspace

```
[80 cols]
 ⠹ task (claude)                                         ← yellow spinner, fg name, dim agent
 ⎿ creating isolated workspace...                        ← dim ⎿, yellow text
```

#### State 1.0d: Session starting

```
[80 cols]
 ⠸ task (claude)                                         ← yellow spinner, fg name, dim agent
 ⎿ starting session...                                   ← dim ⎿, yellow text
```

#### State 1.1: Active (leaf task)

```
[80 cols]
 ⠹ task (claude)                                         ← yellow spinner+name, dim agent
 ⎿ Reading the existing implementation...            12s  ← dim ⎿, yellow text, dim 12s right-aligned
```

#### State 1.2: Done (leaf task)

```
[80 cols]
 合 task completed — Add null check before token access          2m15  ← bold+fg text, dim 2m15 right-aligned
 ⎿ Added null check before token access in auth handler                ← dim ⎿, dim text
                                                                       ← blank row
 Run `aiki task show <id>` for details.                                ← dim hint text
```

#### State 1.3: Failed (leaf task)

```
[80 cols]
 合 task failed — Add null check before token access                    1m42  ← red+bold text, dim elapsed
 ⎿ ✘ cargo check found 3 compilation errors                                  ← dim ⎿, red ✘, red text
                                                                              ← blank row
 Run `aiki task show <id>` for details.                                       ← dim hint text
```

#### State 1.4: Detached

User presses Ctrl+C during active state:

```
[80 cols]
 ⠹ task (claude)                                         ← yellow spinner, fg name, dim agent
 ⎿ Reading the existing implementation...            12s  ← dim ⎿, yellow text, dim elapsed
 [detached]                                              ← dim text
```

#### State 1.5: Parent task — subtasks pending

When the task has subtasks, the subtask table appears:

```
[80 cols]
 ⠸ task (claude)                                                        ← yellow spinner, fg name, dim agent
 ⎿ starting session...                                                  ← dim ⎿, yellow text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [xkp29m] Fix review issues                                         ← dim brackets+id, fg title
     ○ Fix null check in auth handler                                   ← dim ○, dim text
     ○ Add missing error handling in API client                         ← dim ○, dim text
     ○ Remove unused import in utils.rs                                 ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 1.6: Parent task — subtasks in progress

```
[80 cols]
 ⠹ task (claude)                                                        ← yellow spinner, fg name, dim agent
 ⎿ 0/3 subtasks completed                                          45s  ← dim ⎿, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [xkp29m] Fix review issues                                         ← dim brackets+id, fg title
     ▸ Fix null check in auth handler                               32s  ← yellow ▸, fg text, dim elapsed
     ○ Add missing error handling in API client                         ← dim ○, dim text
     ○ Remove unused import in utils.rs                                 ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 1.7: Parent task — subtasks completing

```
[80 cols]
 ⠼ task (claude)                                                        ← yellow spinner, fg name, dim agent
 ⎿ 1/3 subtasks completed                                        1m28  ← dim ⎿, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [xkp29m] Fix review issues                                         ← dim brackets+id, fg title
     ✔ Fix null check in auth handler                               56s  ← green ✔, dim text, dim elapsed
     ▸ Add missing error handling in API client                     32s  ← yellow ▸, fg text, dim elapsed
     ○ Remove unused import in utils.rs                                 ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 1.8: Parent task — all done

```
[80 cols]
 ---                                                                    ← dim separator
                                                                        ← blank row
     [xkp29m] Fix review issues                                         ← dim brackets+id, fg title
     ✔ Fix null check in auth handler                               56s  ← green ✔, dim text, dim elapsed
     ✔ Add missing error handling in API client                   1m22  ← green ✔, dim text, dim elapsed
     ✔ Remove unused import in utils.rs                             24s  ← green ✔, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 task completed — Fix review issues                             3m42  ← bold+fg text, dim elapsed
 ⎿ 3/3 subtasks completed                                              ← dim ⎿, dim text
                                                                        ← blank row
 Run `aiki task show xkp29m` for details.                               ← dim hint text
```

#### State 1.9: Parent task — subtask failed

```
[80 cols]
 ---                                                                    ← dim separator
                                                                        ← blank row
     [xkp29m] Fix review issues                                         ← dim brackets+id, fg title
     ✔ Fix null check in auth handler                               56s  ← green ✔, dim text, dim elapsed
     ✘ Add missing error handling in API client                   1m22  ← red ✘, red text, dim elapsed
     ✔ Remove unused import in utils.rs                             24s  ← green ✔, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 task failed — Fix review issues                                2m58  ← red+bold text, dim elapsed
 ⎿ 2/3 subtasks completed, 1 failed                                    ← dim ⎿, dim text
                                                                        ← blank row
 Run `aiki task show xkp29m` for details.                               ← dim hint text
```

---

### Flow 2: `aiki build <plan>`

Build without review. Phases: plan → decompose → subtask table → loop → done.

#### State 2.0: Validating

```
[80 cols]
 $ aiki build ops/now/tasks/mutex-for-task-writes.md     ← terminal prompt
                                                         ← blank row
 ⠋ plan                                                  ← yellow spinner, fg name
 ⎿ Validating...                                         ← dim ⎿, yellow text
```

#### State 2.1: Plan loaded

```
[80 cols]
 合 plan (claude)                                        ← dim (completed phase)
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md              ← dim ⎿, green ✔, dim text
```

#### State 2.2a: Reading task graph

```
[80 cols]
 合 plan (claude)                                        ← dim (completed phase)
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md              ← dim ⎿, green ✔, dim text
                                                         ← blank row
 Initial Build                                           ← bold+fg section header
                                                         ← blank row
 ⠋ decompose                                             ← yellow spinner, fg name
 ⎿ Reading task graph...                                 ← dim ⎿, yellow text
```

#### State 2.2b: Finding epic

```
[80 cols]
 合 plan (claude)                                        ← dim (completed phase)
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md              ← dim ⎿, green ✔, dim text
                                                         ← blank row
 Initial Build                                           ← bold+fg section header
                                                         ← blank row
 ⠙ decompose                                             ← yellow spinner, fg name
 ⎿ Finding epic...                                       ← dim ⎿, yellow text
```

#### State 2.3a: Epic found, creating workspace

```
[80 cols]
 合 plan (claude)                                        ← dim (completed phase)
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md              ← dim ⎿, green ✔, dim text
                                                         ← blank row
 Initial Build                                           ← bold+fg section header
                                                         ← blank row
 ⠹ decompose (claude)                                    ← yellow spinner, fg name, dim agent
 ⎿ creating isolated workspace...                        ← dim ⎿, yellow text
                                                         ← blank row
 ---                                                     ← dim separator
                                                         ← blank row
     [lkji3d] Epic: Mutex for Task Writes                ← dim brackets+id, fg title
     ...                                                 ← dim (subtasks loading)
                                                         ← blank row
 ---                                                     ← dim separator
```

#### State 2.3b: Session starting

```
[80 cols]
 ⠸ decompose (claude)                                    ← yellow spinner, fg name, dim agent
 ⎿ starting session...                                   ← dim ⎿, yellow text
                                                         ← blank row
 ---                                                     ← dim separator
                                                         ← blank row
     [lkji3d] Epic: Mutex for Task Writes                ← dim brackets+id, fg title
     ...                                                 ← dim (subtasks loading)
                                                         ← blank row
 ---                                                     ← dim separator
```

#### State 2.4: Decompose active

```
[80 cols]
 合 plan (claude)                                                       ← dim (completed phase)
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md                            ← dim ⎿, green ✔, dim text
                                                                        ← blank row
 Initial Build                                                          ← bold+fg section header
                                                                        ← blank row
 ⠹ decompose (claude)                                                   ← yellow spinner, fg name, dim agent
 ⎿ Reading plan and creating subtasks...                            32s  ← dim ⎿, yellow text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Epic: Mutex for Task Writes                               ← dim brackets+id, fg title
     ...                                                                ← dim (subtasks loading)
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 2.4b: Decompose active, subtasks arriving

```
[80 cols]
 ⠼ decompose (claude)                                                   ← yellow spinner, fg name, dim agent
 ⎿ Reading plan and creating subtasks...                            45s  ← dim ⎿, yellow text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Epic: Mutex for Task Writes                               ← dim brackets+id, fg title
     ○ Add get_repo_root helper to jj/mod.rs                            ← dim ○, dim text
     ○ Lock task writes and replace set_tasks_bookmark in ...           ← dim ○, dim text
     ...                                                                ← dim
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 2.5: Decompose done, subtasks populated

```
[80 cols]
 合 plan (claude)                                                       ← dim (completed phase)
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md                            ← dim ⎿, green ✔, dim text
                                                                        ← blank row
 Initial Build                                                          ← bold+fg section header
                                                                        ← blank row
 合 decompose (claude)                                                  ← dim (completed phase)
 ⎿ 5 subtasks created                                            1m04  ← dim ⎿, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Epic: Mutex for Task Writes                               ← dim brackets+id, fg title
     ○ Add get_repo_root helper to jj/mod.rs                            ← dim ○, dim text
     ○ Lock task writes and replace set_tasks_bookmark in ...           ← dim ○, dim text
     ○ Lock conversation writes and replace ...                         ← dim ○, dim text
     ○ Delete advance_bookmark from jj/mod.rs and update tests          ← dim ○, dim text
     ○ Build and test the mutex implementation                          ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 2.6: Lanes assigned, agents starting

```
[80 cols]
 合 decompose (claude)                                                  ← dim (completed phase)
 ⎿ 5 subtasks created                                            1m04  ← dim ⎿, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Epic: Mutex for Task Writes                               ← dim brackets+id, fg title
     ◌ Add get_repo_root helper to jj/mod.rs                            ← dim ◌, dim text
     ◌ Lock task writes and replace set_tasks_bookmark in ...           ← dim ◌, dim text
     ◌ Lock conversation writes and replace ...                         ← dim ◌, dim text
     ◌ Delete advance_bookmark from jj/mod.rs and update tests          ← dim ◌, dim text
     ◌ Build and test the mutex implementation                          ← dim ◌, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 ⠋ loop                                                                 ← yellow spinner, bold+fg name
 ⎿ Lane 1 (claude)                                                     ← dim ⎿, fg "Lane 1", dim agent
     ⎿ 0/2 subtasks completed                                          ← dim ⎿, dim text
     ⎿ starting session...                                              ← dim ⎿, yellow text
                                                                        ← blank row
 ⎿ Lane 2 (claude)                                                     ← dim ⎿, fg "Lane 2", dim agent
     ⎿ 0/3 subtasks completed                                          ← dim ⎿, dim text
     ⎿ starting session...                                              ← dim ⎿, yellow text
```

#### State 2.7: Agents active

```
[80 cols]
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Epic: Mutex for Task Writes                               ← dim brackets+id, fg title
     ▸ Add get_repo_root helper to jj/mod.rs                       12s  ← yellow ▸, fg text, dim elapsed
     ○ Lock task writes and replace set_tasks_bookmark in ...           ← dim ○, dim text
     ◌ Lock conversation writes and replace ...                         ← dim ◌, dim text
     ◌ Delete advance_bookmark from jj/mod.rs and update tests          ← dim ◌, dim text
     ◌ Build and test the mutex implementation                          ← dim ◌, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 ⠹ loop                                                                 ← yellow spinner, bold+fg name
 ⎿ Lane 1 (claude)                                                     ← dim ⎿, fg "Lane 1", dim agent
     ⎿ 0/2 subtasks completed                                          ← dim ⎿, dim text
     ⎿ Reading jj/mod.rs to understand existing helpers            12s  ← dim ⎿, yellow text, dim elapsed
                                                                        ← blank row
 ⎿ Lane 2 (claude)                                                     ← dim ⎿, fg "Lane 2", dim agent
     ⎿ 0/3 subtasks completed                                          ← dim ⎿, dim text
     ⎿ starting session...                                          3s  ← dim ⎿, yellow text, dim elapsed
```

#### State 2.8: Mid-build

```
[80 cols]
 ---                                                              ← dim separator
                                                                  ← blank row
     [lkji3d] Epic: Mutex for Task Writes                        ← dim brackets+id, fg title
     ✔ Add get_repo_root helper to jj/mod.rs                56s  ← green ✔, dim text, dim 56s right-aligned
     ▸ Lock task writes in tasks/storage.rs                 28s  ← yellow ▸, fg text, dim 28s right-aligned
     ◌ Lock conversation writes and replace...                   ← dim ◌, dim text
     ◌ Delete advance_bookmark from jj/mod.rs                    ← dim ◌, dim text
     ◌ Build and test the mutex implementation                   ← dim ◌, dim text
                                                                  ← blank row
 ---                                                              ← dim separator
                                                                  ← blank row
 ⠹ loop                                                          ← yellow spinner+name
 ⎿ Lane 1 (claude)                                               ← dim ⎿, fg "Lane 1", dim agent
     ⎿ 1/2 subtasks completed                                    ← dim ⎿, dim text
     ⎿ Writing lock acquisition function in storage.rs      28s  ← dim ⎿, yellow text, dim 28s right-aligned
                                                                  ← blank row
 ⎿ Lane 2 (claude)                                               ← dim ⎿, fg "Lane 2", dim agent
     ⎿ 0/3 subtasks completed                                    ← dim ⎿, dim text
     ⎿ starting session...                                   3s  ← dim ⎿, yellow text, dim 3s right-aligned
```

#### State 2.9: Subtask fails

```
[80 cols]
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Epic: Mutex for Task Writes                               ← dim brackets+id, fg title
     ✔ Add get_repo_root helper to jj/mod.rs                       56s  ← green ✔, dim text, dim elapsed
     ✘ Lock task writes and replace set_tasks_bookmark in ...     1m29  ← red ✘, red text, dim elapsed
     ▸ Lock conversation writes and replace ...                    50s  ← yellow ▸, fg text, dim elapsed
     ○ Delete advance_bookmark from jj/mod.rs and update tests          ← dim ○, dim text
     ○ Build and test the mutex implementation                          ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 ⠼ loop                                                                 ← yellow spinner, bold+fg name
 ⎿ Lane 1 (claude)                                                     ← dim ⎿, fg "Lane 1", dim agent
     ⎿ 1/2 subtasks completed, 1 failed                                ← dim ⎿, dim text
     ⎿ Error: cargo check found 3 compilation errors                   ← dim ⎿, red text
                                                                        ← blank row
 ⎿ Lane 2 (claude)                                                     ← dim ⎿, fg "Lane 2", dim agent
     ⎿ 0/3 subtasks completed                                          ← dim ⎿, dim text
     ⎿ Adding lock guard to set_conversations_bookmark             50s  ← dim ⎿, yellow text, dim elapsed
```

#### State 2.10: All done

```
[80 cols]
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Epic: Mutex for Task Writes                               ← dim brackets+id, fg title
     ✔ Add get_repo_root helper to jj/mod.rs                       56s  ← green ✔, dim text, dim elapsed
     ✔ Lock task writes and replace set_tasks_bookmark in ...     1m29  ← green ✔, dim text, dim elapsed
     ✔ Lock conversation writes and replace ...                  1m15  ← green ✔, dim text, dim elapsed
     ✔ Delete advance_bookmark from jj/mod.rs and update tests   2m30  ← green ✔, dim text, dim elapsed
     ✔ Build and test the mutex implementation                  50m35  ← green ✔, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 loop                                                                ← dim (completed phase)
 ⎿ Lane 1 (claude)                                                     ← dim
     ⎿ 2/2 subtasks completed                                          ← dim
     ⎿ Agent shutdown.                                                  ← dim
                                                                        ← blank row
 ⎿ Lane 2 (claude)                                                     ← dim
     ⎿ 3/3 subtasks completed                                          ← dim
     ⎿ Agent shutdown.                                                  ← dim
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 build completed — ops/now/tasks/mutex-for-task-writes.md            ← bold+fg 合, bold+fg text
 ⎿ Claude: 2 sessions — 55m33 — 0.92M tokens                           ← dim ⎿, dim text
 ⎿ Total: 2 sessions — 55m33 — 0.92M tokens                            ← dim ⎿, bold text
                                                                        ← blank row
 Run `aiki task diff lkji3d` to see changes.                            ← dim hint text
```

Note: single agent type, so per-agent line and total are identical. Implementations MAY omit the per-agent line when there's only one agent type.

---

### Flow 3: `aiki build <plan> -f`

Full pipeline with review + fix iteration loop. Extends Flow 2 with review → (fix cycle)* → summary.

```
[80 cols]
 $ aiki build ops/now/tasks/mutex-for-task-writes.md -f  ← terminal prompt
```

Everything through the loop phase is identical to Flow 2. What follows:

#### State 3.10: Review starting

```
[80 cols]
 合 loop                                                                ← dim (completed phase)
 ⎿ Lane 1 (claude)                                                     ← dim
     ⎿ 2/2 subtasks completed                                          ← dim
     ⎿ Agent shutdown.                                                  ← dim
                                                                        ← blank row
 ⎿ Lane 2 (claude)                                                     ← dim
     ⎿ 3/3 subtasks completed                                          ← dim
     ⎿ Agent shutdown.                                                  ← dim
                                                                        ← blank row
 ⠋ review (codex)                                                      ← yellow spinner, fg name, dim agent
 ⎿ starting session...                                                 ← dim ⎿, yellow text
```

#### State 3.11: Review active

```
[80 cols]
 ⠹ review (codex)                                                      ← yellow spinner, fg name, dim agent
 ⎿ Reviewing changes in 4 files...                                22s  ← dim ⎿, yellow text, dim elapsed
```

#### State 3.12: Review approved — build done

If review finds no issues, the build completes immediately:

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 build completed — ops/now/tasks/mutex-for-task-writes.md            ← bold+fg 合, bold+fg text
 ⎿ Claude: 2 sessions — 45m33 — 0.82M tokens                           ← dim ⎿, dim text
 ⎿ Codex: 1 session — 11m22 — 0.3M tokens                              ← dim ⎿, dim text
 ⎿ Total: 3 sessions — 56m55 — 1.12M tokens                            ← dim ⎿, bold text
                                                                        ← blank row
 Run `aiki task diff lkji3d` to see changes.                            ← dim hint text
```

#### State 3.13: Review finds issues → Iteration 2

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ Found 3 issues                                                      ← dim ⎿, dim text
                                                                        ← blank row
     1. acquire_named_lock uses AikiError::WorkspaceAbsorbFailed ...    ← fg number, fg text
     2. fs::create_dir_all error is silently swallowed with ...         ← fg number, fg text
     3. Each call to acquire_named_lock leaks a Box<RwLock<File>> ...   ← fg number, fg text
                                                                        ← blank row
 Iteration 2                                                            ← bold+fg section header
                                                                        ← blank row
 ⠋ fix (claude)                                                        ← yellow spinner, fg name, dim agent
 ⎿ starting session...                                                 ← dim ⎿, yellow text
```

#### State 3.14: Fix plan written

```
[80 cols]
 Iteration 2                                                            ← bold+fg section header
                                                                        ← blank row
 合 fix (claude)                                                        ← dim (completed phase)
 ⎿ plan written to /tmp/aiki/fixes/mutex-for-task-writes.md            ← dim ⎿, dim text
```

#### State 3.15: Followup decompose done

```
[80 cols]
 Iteration 2                                                            ← bold+fg section header
                                                                        ← blank row
 合 fix (claude)                                                        ← dim (completed phase)
 ⎿ plan written to /tmp/aiki/fixes/mutex-for-task-writes.md            ← dim
                                                                        ← blank row
 合 decompose (claude)                                                  ← dim (completed phase)
 ⎿ 3 subtasks created                                                  ← dim
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Followup                                                  ← dim brackets+id, fg title
     ○ Fix error variant in acquire_named_lock                          ← dim ○, dim text
     ○ Propagate create_dir_all errors                                  ← dim ○, dim text
     ○ Replace Box::leak with scoped lock manager                       ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 3.16: Followup loop active

```
[80 cols]
 合 decompose (claude)                                                  ← dim (completed phase)
 ⎿ 3 subtasks created                                                  ← dim
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Followup                                                  ← dim brackets+id, fg title
     ▸ Fix error variant in acquire_named_lock                     18s  ← yellow ▸, fg text, dim elapsed
     ○ Propagate create_dir_all errors                                  ← dim ○, dim text
     ○ Replace Box::leak with scoped lock manager                       ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 ⠹ loop                                                                 ← yellow spinner, bold+fg name
 ⎿ Lane 1 (claude)                                                     ← dim ⎿, fg "Lane 1", dim agent
     ⎿ 0/3 subtasks completed                                          ← dim ⎿, dim text
     ⎿ Updating error variant to AikiError::LockFailed             18s  ← dim ⎿, yellow text, dim elapsed
```

#### State 3.17: Followup done, re-review approved

```
[80 cols]
 合 decompose (claude)                                                  ← dim (completed phase)
 ⎿ 3 subtasks created                                                  ← dim
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [lkji3d] Followup                                                  ← dim brackets+id, fg title
     ✔ Fix error variant in acquire_named_lock                     42s  ← green ✔, dim text, dim elapsed
     ✔ Propagate create_dir_all errors                             38s  ← green ✔, dim text, dim elapsed
     ✔ Replace Box::leak with scoped lock manager                1m15  ← green ✔, dim text, dim elapsed
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 loop                                                                ← dim (completed phase)
 ⎿ Lane 1 (claude)                                                     ← dim
     ⎿ 3/3 subtasks completed                                          ← dim
     ⎿ Agent shutdown.                                                  ← dim
                                                                        ← blank row
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ 3/3 issues resolved                                                 ← dim
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

#### State 3.18: Regression review

After fix review passes, a second review checks the original scope for regressions:

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ 3/3 issues resolved                                                 ← dim
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
                                                                        ← blank row
 合 review for regressions (codex)                                      ← dim (completed phase)
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

#### State 3.19: Build completed (after fix cycle)

```
[80 cols]
 合 review for regressions (codex)                                      ← dim (completed phase)
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 build completed — ops/now/tasks/mutex-for-task-writes.md            ← bold+fg 合, bold+fg text
 ⎿ Claude: 3 sessions — 35m29 — 1.05M tokens                           ← dim ⎿, dim text
 ⎿ Codex: 2 sessions — 10m — 0.4M tokens                               ← dim ⎿, dim text
 ⎿ Total: 5 sessions — 45m29 — 1.45M tokens                            ← dim ⎿, bold text
                                                                        ← blank row
 Run `aiki task diff lkji3d` to see changes.                            ← dim hint text
```

#### State 3.20: Regression review finds new issues → Iteration 3

If the regression review finds new issues, another fix cycle starts:

```
[80 cols]
 合 review for regressions (codex)                                      ← dim (completed phase)
 ⎿ Found 1 issue                                                       ← dim ⎿, dim text
                                                                        ← blank row
     1. New deadlock introduced in storage.rs when two tasks write ...  ← fg number, fg text
                                                                        ← blank row
 Iteration 3                                                            ← bold+fg section header
                                                                        ← blank row
 ⠋ fix (claude)                                                        ← yellow spinner, fg name, dim agent
 ⎿ starting session...                                                 ← dim ⎿, yellow text
```

The fix cycle repeats (states 3.14-3.18) until the review approves or MAX_QUALITY_ITERATIONS (10) is hit.

#### State 3.21: Max iterations reached

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ Found 2 issues (iteration 10 of 10)                                 ← dim ⎿, dim text
                                                                        ← blank row
     1. ...                                                             ← fg number, fg text
     2. ...                                                             ← fg number, fg text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 build completed — ops/now/tasks/mutex-for-task-writes.md            ← bold+fg 合, bold+fg text
 ⎿ ⚠ Max iterations reached — 2 issues remain                          ← dim ⎿, yellow ⚠, bold text
 ⎿ Claude: 8 sessions — 1h50m — 6.4M tokens                            ← dim ⎿, dim text
 ⎿ Codex: 4 sessions — 25m — 1.8M tokens                               ← dim ⎿, dim text
 ⎿ Total: 12 sessions — 2h15m — 8.2M tokens                            ← dim ⎿, bold text
                                                                        ← blank row
 Run `aiki task diff lkji3d` to see changes.                            ← dim hint text
```

---

### Flow 4: `aiki review <plan>`

Standalone plan review. Reviews the plan document itself (structure, clarity, completeness).

#### State 4.0: Loading

```
[80 cols]
 $ aiki review ops/now/tasks/mutex-for-task-writes.md    ← terminal prompt
                                                         ← blank row
 ⠋ review (codex)                                        ← yellow spinner, fg name, dim agent
 ⎿ ops/now/tasks/mutex-for-task-writes.md                ← dim ⎿, fg text
 ⎿ creating isolated workspace...                        ← dim ⎿, yellow text
```

#### State 4.1: Active

```
[80 cols]
 ⠹ review (codex)                                                      ← yellow spinner, fg name, dim agent
 ⎿ ops/now/tasks/mutex-for-task-writes.md                               ← dim ⎿, fg text
 ⎿ Reviewing plan...                                               15s  ← dim ⎿, yellow text, dim elapsed
```

#### State 4.2: Done — issues found

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ ops/now/tasks/mutex-for-task-writes.md                               ← dim ⎿, dim text
 ⎿ Found 2 issues                                                      ← dim ⎿, dim text
                                                                        ← blank row
     1. Subtask 3 depends on subtask 2 but doesn't mention it — ...     ← fg number, fg text
     2. No acceptance criteria for "build and test" — what does ...      ← fg number, fg text
```

#### State 4.3: Done — approved

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ ops/now/tasks/mutex-for-task-writes.md                               ← dim ⎿, dim text
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

---

### Flow 5: `aiki review <plan> code`

Standalone code review. Reviews the code changes produced by the plan's build.

#### State 5.0: Loading

```
[80 cols]
 $ aiki review ops/now/tasks/mutex-for-task-writes.md code  ← terminal prompt
                                                            ← blank row
 ⠋ review (codex)                                           ← yellow spinner, fg name, dim agent
 ⎿ ops/now/tasks/mutex-for-task-writes.md --code            ← dim ⎿, fg text
 ⎿ creating isolated workspace...                           ← dim ⎿, yellow text
```

#### State 5.1: Active

```
[80 cols]
 ⠹ review (codex)                                                      ← yellow spinner, fg name, dim agent
 ⎿ ops/now/tasks/mutex-for-task-writes.md --code                       ← dim ⎿, fg text
 ⎿ Reviewing diff...                                               32s  ← dim ⎿, yellow text, dim elapsed
```

#### State 5.2: Done — issues found

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ ops/now/tasks/mutex-for-task-writes.md --code                       ← dim ⎿, dim text
 ⎿ Found 3 issues                                                      ← dim ⎿, dim text
                                                                        ← blank row
     1. acquire_named_lock uses AikiError::WorkspaceAbsorbFailed ...    ← fg number, fg text
     2. fs::create_dir_all error is silently swallowed with ...         ← fg number, fg text
     3. Each call to acquire_named_lock leaks a Box<RwLock<File>> ...   ← fg number, fg text
```

#### State 5.3: Done — approved

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ ops/now/tasks/mutex-for-task-writes.md --code                       ← dim ⎿, dim text
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

---

### Flow 6: `aiki review <task-id>`

Reviews a specific task's code changes.

#### State 6.0: Loading

```
[80 cols]
 $ aiki review <task-id>                                 ← terminal prompt
                                                         ← blank row
 ⠋ review (codex)                                        ← yellow spinner, fg name, dim agent
 ⎿ [lkji3d] Add get_repo_root helper to jj/mod.rs       ← dim ⎿, dim brackets+id, fg text
 ⎿ creating isolated workspace...                        ← dim ⎿, yellow text
```

#### State 6.1: Active

```
[80 cols]
 ⠹ review (codex)                                                      ← yellow spinner, fg name, dim agent
 ⎿ [lkji3d] Add get_repo_root helper to jj/mod.rs                     ← dim ⎿, dim brackets+id, fg text
 ⎿ Reviewing changes...                                           18s  ← dim ⎿, yellow text, dim elapsed
```

#### State 6.2: Done — issues found

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ [lkji3d] Add get_repo_root helper to jj/mod.rs                     ← dim ⎿, dim text
 ⎿ Found 1 issue                                                       ← dim ⎿, dim text
                                                                        ← blank row
     1. get_repo_root shells out to `jj root` on every call — ...      ← fg number, fg text
```

#### State 6.3: Done — approved

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ [lkji3d] Add get_repo_root helper to jj/mod.rs                     ← dim ⎿, dim text
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

---

### Flow 7: `aiki review` (session scope)

Reviews all closed tasks in the current session.

#### State 7.0: Loading

```
[80 cols]
 $ aiki review                                           ← terminal prompt
                                                         ← blank row
 ⠋ review (codex)                                        ← yellow spinner, fg name, dim agent
 ⎿ 4 completed tasks                                     ← dim ⎿, fg text
 ⎿ creating isolated workspace...                        ← dim ⎿, yellow text
```

#### State 7.1: Active

```
[80 cols]
 ⠹ review (codex)                                                      ← yellow spinner, fg name, dim agent
 ⎿ 4 completed tasks                                                   ← dim ⎿, fg text
 ⎿ Reviewing changes...                                           45s  ← dim ⎿, yellow text, dim elapsed
```

#### State 7.2: Done — issues found

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ 4 completed tasks                                                   ← dim ⎿, dim text
 ⎿ Found 5 issues across 2 tasks                                       ← dim ⎿, dim text
                                                                        ← blank row
     1. [Add get_repo_root helper] get_repo_root shells out to ...     ← fg number, fg text
     2. [Add get_repo_root helper] Missing error context on ...        ← fg number, fg text
     3. [Lock task writes] acquire_named_lock uses wrong error ...     ← fg number, fg text
     4. [Lock task writes] fs::create_dir_all error silently ...       ← fg number, fg text
     5. [Lock task writes] Box::leak causes unbounded memory growth     ← fg number, fg text
```

#### State 7.3: Done — approved

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ 4 completed tasks                                                   ← dim ⎿, dim text
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

---

### Flow 8: `aiki fix <review-id>`

Standalone fix pipeline. Takes a review task ID, creates fix-plan → decomposes → loops → re-reviews.

#### State 8.0a: Loading

```
[80 cols]
 $ aiki fix <review-id>                                  ← terminal prompt
                                                         ← blank row
 ⠋ fix                                                   ← yellow spinner, fg name
 ⎿ Reading review task...                                ← dim ⎿, yellow text
```

#### State 8.0b: Checking issues

```
[80 cols]
 ⠙ fix                                                   ← yellow spinner, fg name
 ⎿ Checking for actionable issues...                     ← dim ⎿, yellow text
```

#### State 8.0c: Starting

```
[80 cols]
 ⠹ fix (claude)                                          ← yellow spinner, fg name, dim agent
 ⎿ starting session...                                   ← dim ⎿, yellow text
```

#### State 8.1: Fix plan active

```
[80 cols]
 ⠼ fix (claude)                                                        ← yellow spinner, fg name, dim agent
 ⎿ Writing fix plan for 3 issues...                                 8s  ← dim ⎿, yellow text, dim elapsed
```

#### State 8.2: Fix plan done

```
[80 cols]
 合 fix (claude)                                                        ← dim (completed phase)
 ⎿ plan written to /tmp/aiki/fixes/mutex-for-task-writes.md            ← dim ⎿, dim text
```

#### State 8.3: Decompose done, followup table

```
[80 cols]
 合 fix (claude)                                                        ← dim (completed phase)
 ⎿ plan written to /tmp/aiki/fixes/mutex-for-task-writes.md            ← dim
                                                                        ← blank row
 合 decompose (claude)                                                  ← dim (completed phase)
 ⎿ 3 subtasks created                                                  ← dim
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
     [xkp29m] Followup                                                  ← dim brackets+id, fg title
     ○ Fix error variant in acquire_named_lock                          ← dim ○, dim text
     ○ Propagate create_dir_all errors                                  ← dim ○, dim text
     ○ Replace Box::leak with scoped lock manager                       ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
```

#### State 8.4: Loop active

```
[80 cols]
 ---                                                                    ← dim separator
                                                                        ← blank row
     [xkp29m] Followup                                                  ← dim brackets+id, fg title
     ▸ Fix error variant in acquire_named_lock                     18s  ← yellow ▸, fg text, dim elapsed
     ○ Propagate create_dir_all errors                                  ← dim ○, dim text
     ○ Replace Box::leak with scoped lock manager                       ← dim ○, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 ⠹ loop                                                                 ← yellow spinner, bold+fg name
 ⎿ Lane 1 (claude)                                                     ← dim ⎿, fg "Lane 1", dim agent
     ⎿ 0/3 subtasks completed                                          ← dim ⎿, dim text
     ⎿ Updating error variant to AikiError::LockFailed            18s  ← dim ⎿, yellow text, dim elapsed
```

#### State 8.5: Loop done, review of fixes

```
[80 cols]
 合 loop                                                                ← dim (completed phase)
 ⎿ Lane 1 (claude)                                                     ← dim
     ⎿ 3/3 subtasks completed                                          ← dim
     ⎿ Agent shutdown.                                                  ← dim
                                                                        ← blank row
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ 3/3 issues resolved                                                 ← dim
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

#### State 8.6: Regression review

```
[80 cols]
 合 review (codex)                                                      ← dim (completed phase)
 ⎿ 3/3 issues resolved                                                 ← dim
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
                                                                        ← blank row
 合 review for regressions (codex)                                      ← dim (completed phase)
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
```

#### State 8.7: Fix completed

```
[80 cols]
 合 review for regressions (codex)                                      ← dim (completed phase)
 ⎿ ✔ approved                                                          ← dim ⎿, green ✔, dim text
                                                                        ← blank row
 ---                                                                    ← dim separator
                                                                        ← blank row
 合 fix completed — 3/3 issues resolved                                ← bold+fg 合, bold+fg text
 ⎿ Claude: 3 sessions — 8m42 — 0.45M tokens                            ← dim ⎿, dim text
 ⎿ Total: 3 sessions — 8m42 — 0.45M tokens                             ← dim ⎿, bold text
                                                                        ← blank row
 Run `aiki task diff xkp29m` to see changes.                            ← dim hint text
```

#### State 8.8: No actionable issues

If the review had no issues to fix:

```
[80 cols]
 合 fix (claude)                                                        ← dim (completed phase)
 ⎿ ✔ approved — no actionable issues                                   ← dim ⎿, green ✔, dim text
```

---

## Full Final State: `aiki build <plan> -f` (happy path with one fix cycle)

This is what scrollback looks like when the build is completely done:

```
[80 cols]
 合 plan (claude)                                                      ← dim (completed phase)
 ⎿ ✔ ops/now/tasks/mutex-for-task-writes.md                           ← dim ⎿, green ✔, dim text
                                                                       ← blank row
 Initial Build                                                         ← bold+fg section header
                                                                       ← blank row
 合 decompose (claude)                                                 ← dim (completed phase)
 ⎿ 5 subtasks created                                           1m04  ← dim ⎿, dim text, dim elapsed
                                                                       ← blank row
 ---                                                                   ← dim separator
                                                                       ← blank row
     [lkji3d] Epic: Mutex for Task Writes                              ← dim brackets+id, fg title
     ✔ Add get_repo_root helper to jj/mod.rs                     56s  ← green ✔, dim text, dim elapsed
     ✔ Lock task writes in tasks/storage.rs                    1m29  ← green ✔, dim text, dim elapsed
     ✔ Lock conversation writes and replace...                 1m15  ← green ✔, dim text, dim elapsed
     ✔ Delete advance_bookmark from jj/mod.rs                  2m30  ← green ✔, dim text, dim elapsed
     ✔ Build and test the mutex implementation                50m35  ← green ✔, dim text, dim elapsed
                                                                       ← blank row
 ---                                                                   ← dim separator
                                                                       ← blank row
 合 loop                                                               ← dim (completed phase)
 ⎿ Lane 1 (claude)                                                    ← dim
     ⎿ 2/2 subtasks completed                                         ← dim
     ⎿ Agent shutdown.                                                 ← dim
                                                                       ← blank row
 ⎿ Lane 2 (claude)                                                    ← dim
     ⎿ 3/3 subtasks completed                                         ← dim
     ⎿ Agent shutdown.                                                 ← dim
                                                                       ← blank row
 合 review (codex)                                                     ← dim (completed phase)
 ⎿ Found 3 issues                                                     ← dim ⎿, dim text
                                                                       ← blank row
     1. acquire_named_lock uses wrong error variant                    ← fg number, fg text (never dimmed)
     2. fs::create_dir_all error silently swallowed                    ← fg number, fg text (never dimmed)
     3. Box::leak causes unbounded memory growth                       ← fg number, fg text (never dimmed)
                                                                       ← blank row
 Iteration 2                                                           ← bold+fg section header
                                                                       ← blank row
 合 fix (claude)                                                       ← dim (completed phase)
 ⎿ plan written to /tmp/aiki/fixes/mutex-for-task-writes.md           ← dim
                                                                       ← blank row
 合 decompose (claude)                                                 ← dim (completed phase)
 ⎿ 3 subtasks created                                                 ← dim
                                                                       ← blank row
 ---                                                                   ← dim separator
                                                                       ← blank row
     [lkji3d] Followup                                                 ← dim brackets+id, fg title
     ✔ Fix error variant in acquire_named_lock                   42s  ← green ✔, dim text, dim elapsed
     ✔ Propagate create_dir_all errors                           38s  ← green ✔, dim text, dim elapsed
     ✔ Replace Box::leak with scoped lock manager              1m15  ← green ✔, dim text, dim elapsed
                                                                       ← blank row
 ---                                                                   ← dim separator
                                                                       ← blank row
 合 loop                                                               ← dim (completed phase)
 ⎿ Lane 1 (claude)                                                    ← dim
     ⎿ 3/3 subtasks completed                                         ← dim
     ⎿ Agent shutdown.                                                 ← dim
                                                                       ← blank row
 合 review (codex)                                                     ← dim (completed phase)
 ⎿ 3/3 issues resolved                                                ← dim
 ⎿ ✔ approved                                                         ← dim ⎿, green ✔, dim text
                                                                       ← blank row
 合 review for regressions (codex)                                     ← dim (completed phase)
 ⎿ No issues found                                                    ← dim
 ⎿ ✔ approved                                                         ← dim ⎿, green ✔, dim text
                                                                       ← blank row
 ---                                                                   ← dim separator
                                                                       ← blank row
 合 build completed — mutex-for-task-writes.md                         ← bold+fg (summary, not dimmed)
 ⎿ Claude: 3 sessions — 35m29 — 1.05M tokens                          ← dim ⎿, dim text
 ⎿ Codex: 2 sessions — 10m — 0.4M tokens                              ← dim ⎿, dim text
 ⎿ Total: 5 sessions — 45m29 — 1.45M tokens                           ← dim ⎿, bold text
                                                                       ← blank row
 Run `aiki task diff lkji3d` to see changes.                           ← dim hint text
```

---

## Progressive Dimming

As phases complete, their content dims to keep focus on what's active.

**Rule:** When a phase transitions from active to done:
1. The phase header spinner (`⠋`) stops and becomes static `合`, text goes dim
2. All child lines (`⎿ ...`) go dim
3. Subtask table: `✔` subtasks go dim; `▸` stays yellow; `○`/`◌` stay dim

**What stays bright:**
- The currently active phase header
- Active heartbeat lines
- The `▸` active subtask in the subtask table
- Issue text (user needs to read these)
- The final summary line

## Inline Rendering Behavior

1. **First render:** output appears at current cursor position, scrolling terminal down
2. **Subsequent renders:** cursor moves up N lines (height of last render), clears and re-renders
3. **Growth:** as new phases appear, the rendered area grows. Old content scrolls up naturally
4. **Done:** rendering stops. Output is in scrollback. No cleanup needed
5. **Ctrl+C:** stops rendering, prints `[detached]` on next line. Output persists in scrollback

## Open Questions

1. **"Initial Build" label** — in the mockup, "Initial Build" appears between plan and decompose. Is this always present? What triggers it vs not?
2. **Lane completion counts** — `2/3 subtasks completed` when the lane had 3 tasks but only 2 finished. Does the 3rd get reassigned? What's the failure mode?
3. **Subtask table width** — subtask names can be long. Truncate with `…`? Or wrap?
4. **Multiple iterations** — `Iteration 2`, `Iteration 3`, etc. **Resolved:** Max is `MAX_QUALITY_ITERATIONS = 10` (defined in `build.rs` command logic). When `iteration == MAX_QUALITY_ITERATIONS` and review still has issues, the summary shows `⚠ Max iterations reached — N issues remain` (see step-2-new-screens.md, section 2.2). The TUI doesn't enforce the limit — it just renders whatever iterations exist. The command logic stops spawning new fix tasks at the max.
5. **Detach and reattach** — **Resolved:** Yes, `aiki build --attach <epic-id>` re-enters the inline renderer. Because the view is graph-driven (renders whatever's in the current TaskGraph), it naturally handles cold-start reattachment — "I'm seeing this graph for the first time but the build is 70% done" just works. The view function renders completed phases as dimmed, active phases with spinners, and pending phases as not-yet-rendered. Add a test case: construct a graph that's 70% done, verify `build::view()` produces correct output with dimmed completed phases and active current phase.
6. **Review phase header variants** — are `review plan`, `review code`, `review task`, `review session` the right labels? Or should it be `review (codex)` with the scope in the result line?
7. **Session review issue grouping** — session reviews span multiple tasks. Should issues be grouped by task (with `[task name]` prefix) as shown in Flow 7?
8. **Fix with `--once`** — skips the post-fix review loop. Summary should say "fix completed (no review)" or similar?
