# Plan: Remove magic in-progress defaults from task subcommands

## Problem

Several `aiki task` subcommands default to the current in-progress task when no ID is given. This "magic" causes confusion — agents comment on, show, or modify the wrong task because they forgot which task is in-progress. The implicit behavior is surprising and error-prone.

Additionally, `comment` is a flat subcommand (`aiki task comment [--id ID] TEXT`) rather than a subcommand group. This is inconsistent with `aiki review issue {add, list}` and makes it ambiguous whether `comment` means "add a comment" or "manage comments."

## Scope

1. Restructure `comment` into a subcommand group: `aiki task comment add` / `aiki task comment list` (matching `aiki review issue {add, list}` pattern).
2. Remove the magic in-progress default from: **`comment add`**, **`show`**, **`set`**, **`unset`**.
3. Keep magic for **`stop`** and **`close`** (state-transition commands where "stop/close what I'm working on" is the natural use case).

## Changes

### 1. `comment` — flat command → subcommand group with `add` and `list`

**CLI definition** (~line 652):

```rust
// Before
Comment {
    /// Task ID to comment on (defaults to current in-progress task)
    #[arg(long)]
    id: Option<String>,
    /// Comment text (required)
    text: String,
    /// Add structured data to the comment. Can be specified multiple times.
    #[arg(long, value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
    data: Vec<String>,
},

// After
/// Manage task comments
Comment {
    #[command(subcommand)]
    command: TaskCommentSubcommands,
},
```

**New enum** (add near `TaskCommands`):

```rust
/// Subcommands for managing task comments
#[derive(Subcommand)]
pub enum TaskCommentSubcommands {
    /// Add a comment to a task
    Add {
        /// Task ID to comment on
        id: String,
        /// Comment text
        text: String,
        /// Add structured data to the comment. Can be specified multiple times.
        #[arg(long, value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
        data: Vec<String>,
    },
    /// List comments on a task
    List {
        /// Task ID to list comments for
        id: String,
    },
}
```

**Dispatch** (~line 1161):

```rust
// Before
TaskCommands::Comment { id, text, data } => run_comment(&cwd, id, text, data),

// After
TaskCommands::Comment { command } => match command {
    TaskCommentSubcommands::Add { id, text, data } => run_comment_add(&cwd, &id, text, data),
    TaskCommentSubcommands::List { id } => run_comment_list(&cwd, &id),
},
```

**`run_comment`** → rename to **`run_comment_add`** (~line 5152): Change signature from `id: Option<String>` to `id: &str`. Delete the `in_progress` lookup and fallback branch. Just validate with `find_task_in_graph` and call `comment_on_task`.

**New `run_comment_list`**: Read events, materialize graph, find task, print its comments. Simple list output matching `aiki review issue list` style.

### 2. `show` — remove magic

**CLI definition** (~line 574):

```rust
// Before
Show {
    /// Task ID to show (defaults to current in-progress task)
    id: Option<String>,
    ...
},

// After
Show {
    /// Task ID to show
    id: Option<String>,
    ...
},
```

Keep `Option<String>` since `aiki task show` with no args should print a helpful "no task ID provided" error (not a clap usage dump). But remove the in-progress fallback.

**`run_show`** (~line 3797): Remove the `in_progress` lookup. When `id` is `None`, emit an error like "No task ID provided. Usage: aiki task show <task-id>" instead of silently picking the in-progress task.

### 3. `set` — remove magic

**CLI definition** (~line 596):

```rust
// Before
Set {
    /// Task ID (defaults to current in-progress task)
    id: Option<String>,
    ...
},

// After
Set {
    /// Task ID
    id: Option<String>,
    ...
},
```

Keep `Option<String>` for the same reason as `show`.

**`run_set`** (~line 4838): Remove the `in_progress` lookup. When `id` is `None`, emit error "No task ID provided. Usage: aiki task set <task-id> [OPTIONS]".

### 4. `unset` — remove magic

**CLI definition** (~line 634):

```rust
// Before
Unset {
    /// Task ID (defaults to current in-progress task)
    id: Option<String>,
    ...
},

// After
Unset {
    /// Task ID
    id: Option<String>,
    ...
},
```

**`run_unset`** (~line 5052): Same pattern — remove `in_progress` lookup, emit error when `id` is `None`.

### 5. `cli/src/commands/agents_template.rs` — template docs

Update all references:
- `aiki task comment --id <task-id> "..."` → `aiki task comment add <task-id> "..."`
- `aiki task comment <task-id> "..."` → `aiki task comment add <task-id> "..."`
- Remove any mention of "defaults to current in-progress task" for comment/show/set/unset
- Bump `AIKI_BLOCK_VERSION` from `"1.15"` to `"1.16"`

### 6. `AGENTS.md` — user-facing docs

Mirror the same changes as the template:
- Update comment syntax examples throughout: `aiki task comment` → `aiki task comment add`
- Bump version tag from `1.15` to `1.16`

Note: `CLAUDE.md` is a symlink — it gets updated via `agents_template.rs`, not directly.

## Files touched

| File | What changes |
|------|-------------|
| `cli/src/commands/task.rs` | `Comment` → subcommand group, new `TaskCommentSubcommands` enum, `run_comment` → `run_comment_add`, new `run_comment_list`, `run_show`, `run_set`, `run_unset` |
| `cli/src/commands/agents_template.rs` | Doc examples, version bump to 1.16 |
| `AGENTS.md` | Doc examples, version bump to 1.16 |

## Backwards compatibility

Not needed. This is a clean break — no aliases, no hidden fallbacks for the old `aiki task comment` form.

## What does NOT change

- `comment_on_task()` — shared implementation (already takes `&str`)
- `aiki review issue add` — uses `comment_on_task` directly, unaffected
- `stop` / `close` — keep magic in-progress default (natural UX for state transitions)
- `--data` flags — stay as-is on `comment add`
- `aiki task close --summary` — unrelated

## Testing

- `cargo build` — verify it compiles
- `aiki task comment` with no args → clap help showing `add` and `list` subcommands
- `aiki task comment add <valid-id> "test"` → works
- `aiki task comment list <valid-id>` → shows comments
- `aiki task show` with no args → "No task ID provided" error
- `aiki task set` with no args → "No task ID provided" error
- `aiki task unset` with no args → "No task ID provided" error
- `aiki task stop` with no args → still stops in-progress task (unchanged)
- `aiki task close` with no args → still closes in-progress task (unchanged)
