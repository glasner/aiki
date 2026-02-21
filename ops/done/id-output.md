# `--output` Flag for `build show` and `plan show`

**Date**: 2026-02-20
**Status**: Draft
**Purpose**: Add `--output=id` to `aiki build show` and `aiki plan show` so scripts can extract the task ID for a given spec without parsing human-readable output.

---

## Executive Summary

`aiki build show <spec>` and `aiki plan show <spec>` display detailed human-readable output (plan name, ID, status, subtasks, builds). Scripts that need to look up the task ID associated with a spec file have no clean way to extract it -- they'd need to parse markdown tables from stderr.

This spec adds `--output <format>` / `-o <format>` as a per-subcommand flag on `build show` and `plan show` (following kubectl's pattern). When `--output id` is specified, the command outputs the bare 32-character task ID to stdout instead of the human-readable display. The flag is designed as an extensible enum so future formats (e.g., `json`) can be added.

---

## User Experience

### Look up plan ID for a spec

```bash
# Get the plan task ID
PLAN_ID=$(aiki plan show ops/now/feature.md --output id)
echo $PLAN_ID  # abcdefghijklmnopqrstuvwxyzabcdef
```

### Look up build ID for a spec

```bash
# Get the most recent build orchestrator task ID
BUILD_ID=$(aiki build show ops/now/feature.md --output id)
echo $BUILD_ID  # mvslrspmoynoxyyywqyutmovxpvztkls
```

### Chaining with other commands

```bash
# Look up the plan and review it
PLAN=$(aiki plan show ops/now/feature.md -o id)
aiki review $PLAN

# Look up the build and check its task details
BUILD=$(aiki build show ops/now/feature.md -o id)
aiki task show $BUILD
```

### Short form

```bash
PLAN=$(aiki plan show ops/now/feature.md -o id)
```

### Multiple build orchestrator tasks

`aiki build show` may find multiple build orchestrator tasks for a spec (e.g., if the spec was built multiple times). With `--output id`, each orchestrator ID is emitted on its own line:

```bash
aiki build show ops/now/feature.md --output id
# outputs (one per line, most recent first):
# mvslrspmoynoxyyywqyutmovxpvztkls
# xtuttnyvykpulsxzqnznsxylrzkkqssy
```

### Unsupported commands

The flag is per-subcommand (not global). Commands that don't define it get a parse error:

```
$ aiki task list --output id
error: unexpected argument '--output' found
```

---

## How It Works

### Per-subcommand flag (kubectl pattern)

The `--output` flag is added directly to the `build show` and `plan show` subcommand arg structs, not as a global flag. This means:
- Only commands that explicitly opt in have the flag
- Unsupported commands reject it at parse time (clap error)
- No silent-ignore ambiguity
- New commands opt in by adding the flag to their arg struct

```rust
#[derive(Clone, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Bare task ID (full 32-char), one per line
    Id,
}
```

On each supporting subcommand:

```rust
#[arg(long, short = 'o', value_name = "FORMAT")]
output: Option<OutputFormat>,
```

Today only `Id` is supported. Future formats (`Json`, `Yaml`, etc.) are added to the `OutputFormat` enum and become available on all commands that use it.

### Behavior when `--output id` is set

1. **Write bare task ID to stdout** -- full 32-char ID, one per line
2. **Suppress human-readable output** -- no `eprintln!` status display
3. **Exit code unchanged** -- errors still produce non-zero exit, error messages to stderr

### `plan show` implementation

```rust
fn run_show(cwd: &Path, arg: &str, output_format: Option<OutputFormat>) -> Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);

    let plan = if is_task_id(arg) || is_task_id_prefix(arg) {
        find_task(&graph.tasks, arg)?
    } else {
        find_plan_for_spec(&graph, arg).ok_or_else(|| {
            AikiError::InvalidArgument(format!("No plan found for spec: {}", arg))
        })?
    };

    match output_format {
        Some(OutputFormat::Id) => {
            println!("{}", plan.id);
        }
        None => {
            let subtasks = get_subtasks(&graph, &plan.id);
            output_plan_show(plan, &subtasks)?;
        }
    }

    Ok(())
}
```

### `build show` implementation

`build show --output id` emits the build orchestrator task ID(s), not the plan ID. The plan ID is available via `plan show --output id`.

```rust
fn run_show(cwd: &Path, spec_path: &str, output_format: Option<OutputFormat>) -> Result<()> {
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);

    let plan = find_plan_for_spec(&graph, spec_path).ok_or_else(|| {
        AikiError::InvalidArgument(format!("No plan found for spec: {}", spec_path))
    })?;

    match output_format {
        Some(OutputFormat::Id) => {
            // Emit build orchestrator task IDs (not the plan ID)
            let build_tasks: Vec<&Task> = graph
                .tasks
                .values()
                .filter(|t| {
                    t.task_type.as_deref() == Some("orchestrator")
                        && t.data.get("spec").map(|s| s.as_str()) == Some(spec_path)
                })
                .collect();

            if build_tasks.is_empty() {
                // No builds yet -- emit the plan ID as fallback
                println!("{}", plan.id);
            } else {
                for build in &build_tasks {
                    println!("{}", build.id);
                }
            }
        }
        None => {
            let subtasks = get_subtasks(&graph, &plan.id);
            let build_tasks = /* existing orchestrator query */;
            output_build_show(plan, &subtasks, &build_tasks)?;
        }
    }

    Ok(())
}
```

---

## Use Cases

### 1. Script needs to review a spec's plan

```bash
# Without --output id: need to parse markdown
aiki plan show ops/now/feature.md 2>&1 | grep "ID:" | awk '{print $NF}'

# With --output id: clean
PLAN=$(aiki plan show ops/now/feature.md -o id)
aiki review $PLAN
```

### 2. CI/CD checks build status

```bash
BUILD=$(aiki build show ops/now/feature.md -o id)
STATUS=$(aiki task show $BUILD | grep "Status:" | awk '{print $2}')
if [ "$STATUS" = "closed" ]; then
    echo "Build complete"
fi
```

### 3. Hook script looks up related tasks

```bash
# In a post-spec hook
PLAN=$(aiki plan show "$SPEC_PATH" -o id 2>/dev/null)
if [ -n "$PLAN" ]; then
    echo "Plan already exists: $PLAN"
fi
```

### 4. Extending to other commands later

When other commands need `--output id`, they just add the flag to their arg struct:

```rust
// In any new subcommand's args
#[arg(long, short = 'o', value_name = "FORMAT")]
output: Option<OutputFormat>,
```

The `OutputFormat` enum is shared, so all commands get the same format options.

---

## Implementation Plan

1. **Add `OutputFormat` enum** -- in `cli/src/commands/mod.rs` or a shared types module. Derive `clap::ValueEnum`.
2. **Add `--output` / `-o` to `plan show` args** -- thread to `run_show`, emit `plan.id` when `--output id`
3. **Add `--output` / `-o` to `build show` args** -- thread to `run_show`, emit build orchestrator ID(s) when `--output id`
4. **Add tests** -- verify `--output id` produces bare ID on stdout, verify default behavior unchanged, verify multiple builds emit one ID per line
5. **Future**: extend to other commands as needed (add the flag to their arg struct)

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `--output id` + no plan found | Non-zero exit, error to stderr, no stdout |
| `--output id` + invalid spec path | Non-zero exit, error to stderr, no stdout |
| `--output unknown_format` | Clap error: `Invalid value 'unknown_format' for '--output'. Possible values: id` |
| `--output id` on unsupported command | Clap error: `unexpected argument '--output'` |
| `build show -o id` + no builds (only plan) | Emit plan ID as fallback |

---

## Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Flag scope | Per-subcommand (kubectl pattern) | Unsupported commands get parse errors, no ambiguity |
| `build show -o id` output | Build orchestrator ID(s) | Plan ID available via `plan show -o id`. Build-specific IDs are what `build show` uniquely provides |
| ID length | Full 32-char | Unambiguous, suitable for scripting |
| Flag syntax | `--output <format>` / `-o <format>` | Extensible enum, familiar from kubectl |
