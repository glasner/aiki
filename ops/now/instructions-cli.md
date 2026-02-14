# Set task instructions via CLI

## Goal

Make it possible to set `task.instructions` via the CLI when creating tasks manually (not just from templates). Then update `plan.md` to use this to set instructions on plan tasks and subtasks.

## Background

Currently, `task.instructions` is only populated by the template-based creation path (`create_from_template`). Manual `aiki task add` always sets `instructions: None`. The plan template (`aiki/plan.md`) instructs agents to create tasks via `aiki task add`, which means those tasks never get instructions — the agent's intent for each subtask is only captured in the task name, not in a structured instructions field.

## Requirements

1. `aiki task update <id> --instructions` sets instructions on an existing task, reading content from stdin
2. The plan template uses this to attach context to each subtask after creation

## Design

Add `--instructions` to the existing `task update` command. When this flag is present, read instructions from stdin:

```bash
TASK_ID=$(aiki task add "Fix auth" --parent $PLAN_ID)
aiki task update $TASK_ID --instructions <<'MD'
1. Check token validation in `auth_handler.rs`
2. Handle the null `claims` field — it can be None
3. Write tests covering both paths

Acceptance criteria: auth tests pass, no panics on expired tokens.
MD
```

Why stdin:
- Instructions are always multi-line markdown content
- Heredoc at top level is clean — no quoting, no escaping
- Quotes, backticks, special characters all just work
- Agents can produce heredocs naturally

## Changes

### 1. Add `--instructions` flag to `Update` variant

**File:** `cli/src/commands/task.rs`

Add to the `Update` variant of `TaskCommands`:
```rust
/// Set instructions (reads content from stdin)
#[arg(long)]
instructions: bool,
```

It's a bool flag (not a value) because the content comes from stdin.

### 2. Add `instructions` field to `TaskEvent::Updated`

**File:** `cli/src/tasks/types.rs`

Add to the `Updated` variant:
```rust
Updated {
    task_id: String,
    name: Option<String>,
    priority: Option<TaskPriority>,
    assignee: Option<Option<String>>,
    data: Option<HashMap<String, String>>,
    instructions: Option<String>,  // NEW
    timestamp: DateTime<Utc>,
},
```

### 3. Read stdin in `run_update`

**File:** `cli/src/commands/task.rs`

When `instructions` flag is true:
- Read all of stdin (`std::io::read_to_string(std::io::stdin())`)
- Trim trailing whitespace
- Pass as `instructions: Some(content)` in the `TaskEvent::Updated`

Update the "nothing to update" check to also consider `instructions`.

### 4. Handle in materialization

**File:** `cli/src/tasks/graph.rs`

When applying `Updated` events, if `instructions` is `Some(text)`:
```rust
task.instructions = Some(text.clone());
```

### 5. Wire through storage

**File:** `cli/src/tasks/storage.rs`

Add serialization/deserialization of the `instructions` field in `Updated` events. Same pattern as `Created` — use `add_metadata_escaped` for writing, percent-decode on read.

### 6. Update dispatch

**File:** `cli/src/commands/task.rs`

Update the match arm for `TaskCommands::Update` to destructure and forward the new `instructions` field.

### 7. Update `plan.md` template

**File:** `.aiki/templates/aiki/plan.md`

Set instructions on the plan task and each subtask after creation:

```bash
# Plan task
PLAN_ID=$(aiki task add "Plan: <spec title>" \
  --source task:{{parent.id}} \
  --source file:{{data.spec}} \
  --data spec={{data.spec}})
aiki task update $PLAN_ID --instructions <<'MD'
Implementation plan for <spec title>.
See spec: {{data.spec}}
MD

# Each subtask
TASK_ID=$(aiki task add "<step description>" --parent $PLAN_ID)
aiki task update $TASK_ID --instructions <<'MD'
<detailed instructions for this step — enough context for an
executing agent to complete the step without re-reading the spec>
MD
```

## Testing

- Unit tests: write `Updated` event with instructions, materialize, verify `task.instructions` is set
- Unit tests: verify multi-line content with special characters round-trips through storage
- Manual: create task, update with instructions, `aiki task show <id> --with-instructions` should display them

## Non-goals

- No new subcommand (reuse existing `task update`)
- No new event variant (extend existing `TaskEvent::Updated`)
- No changes to template-based creation (already supports instructions via template body)
- No changes to `task show` display (already renders instructions when present)
