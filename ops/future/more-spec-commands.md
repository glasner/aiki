# More Spec Commands

Additional spec commands to add after Phase 0 SpecGraph is implemented.

## Commands

### `aiki spec list`

List all specs with their status.

```bash
# List all specs
aiki spec list

# List only unimplemented specs
aiki spec list --no-plan

# List specs by status
aiki spec list --status implementing
```

**Implementation:**

```rust
// cli/src/commands/spec.rs

pub fn run(args: SpecArgs) -> Result<()> {
    let cwd = env::current_dir()?;
    let task_graph = TaskGraph::load(&cwd)?;
    let spec_graph = SpecGraph::build(&cwd, &task_graph)?;

    match args.subcommand {
        SpecSubcommand::List(list_args) => {
            run_list(&spec_graph, &task_graph, &list_args)
        }
        SpecSubcommand::Show(show_args) => {
            run_show(&spec_graph, &task_graph, &show_args)
        }
    }
}

fn run_list(
    spec_graph: &SpecGraph,
    task_graph: &TaskGraph,
    args: &ListArgs,
) -> Result<()> {
    let specs: Vec<&Spec> = if args.no_plan {
        spec_graph.unimplemented()
    } else {
        spec_graph.specs.values().collect()
    };

    for spec in specs {
        let task_count = spec_graph.implementing_tasks(&spec.path, task_graph).len();
        println!("{:?} {} ({} tasks)", spec.status, spec.title, task_count);
    }

    Ok(())
}
```

### `aiki spec show <path>`

Show details about a specific spec.

```bash
aiki spec show ops/now/feature.md
```

**Output:**
```
Title: Feature Name
Status: Implementing
Path: file:ops/now/feature.md

Description:
First paragraph of the spec...

Implementing Tasks:
- abc123 (Plan: Implement feature)
  - 5 subtasks (3 open, 2 closed)
```

### `aiki spec graph`

Visualize spec dependencies (future - requires spec relationships).

```bash
aiki spec graph
```

**Output:**
```
ops/now/foundation.md [Implemented]
  └─> ops/now/feature-a.md [Implementing] (refines)
      └─> ops/now/feature-b.md [Draft] (depends-on)
```

## Future Enhancements

- `--status` filter for list command
- `--ready` flag to show specs ready for implementation
- Spec relationship commands (`aiki spec link`, `aiki spec unlink`)
- Validation of circular dependencies
- Export to various formats (JSON, DOT graph)
