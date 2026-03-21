# Plan: `aiki support` Command

## Overview

Add an `aiki support` command that uses the task template system to interactively guide users through filing good GitHub issues — the same way `aiki plan` guides users through writing good plans.

## Design

### UX

```
aiki support issue "crashes on close"              # bug by default, spawns Claude
aiki support issue "custom priorities" --feature   # feature request
aiki support issue                                 # prompts for description
```

The command:
1. Loads the `support/issue` template (built-in)
2. Creates a task from it (with subtasks that guide the conversation)
3. Spawns Claude interactively to walk the user through filing a good issue
4. Claude collects context, helps articulate the problem, gathers diagnostics, and files via `gh`

### Why template, not a hardcoded command

- **Better issues** — Claude asks clarifying questions, helps reproduce, suggests titles
- **Consistent with aiki patterns** — same flow as `aiki plan`
- **Customizable** — users can override `.aiki/tasks/support/issue.md` to change the flow
- **Less CLI code** — the template carries the logic, the command is thin

### Command shape

```
aiki support issue [description] [--bug|--feature]
```

- `--bug` is the default (full diagnostic collection)
- `--feature` for feature requests (minimal context)
- Mutually exclusive flags enforced by clap
- No `--dry-run` needed — Claude shows preview and confirms before filing

## Template: `support/issue.md`

```markdown
---
version: 1.0.0
type: support
assignee: claude-code
interactive: true
---

# Support: File {{data.issue_type}} on glasner/aiki

Help the user file a high-quality GitHub issue.

# Instructions

You are helping the user file a **{{data.issue_type}}** on the `glasner/aiki` repository.

**User's description:** {{data.description}}
**Issue type:** {{data.issue_type}}

Work through the subtasks in order. The goal is a well-written GitHub issue
that gives maintainers everything they need to act on it.

{% if data.issue_type == "bug" %}
For bugs, collect full diagnostics. Help the user describe what happened,
what they expected, and how to reproduce it.
{% else %}
For feature requests, focus on the problem being solved and the desired
behavior. Keep diagnostics minimal.
{% endif %}

## Subtasks

## Clarify the issue

1. Read the user's description
2. Ask clarifying questions:
{% if data.issue_type == "bug" %}
   - What were you doing when this happened?
   - What did you expect to happen?
   - What actually happened?
   - Can you reproduce it consistently?
   - Any error messages or output?
{% else %}
   - What problem does this solve?
   - What's your current workaround (if any)?
   - What would the ideal behavior look like?
{% endif %}
3. If the user already provided enough detail, skip redundant questions
4. Close subtask when you have enough clarity to write the issue

## Collect diagnostics

{% if data.issue_type == "bug" %}
Collect a diagnostic snapshot by running these commands and capturing output.
Each section is independent — if one fails, continue with the rest.

1. **Environment:**
   - Run `aiki --version` for aiki version
   - Check OS/arch via platform info
   - Run `git --version` and `jj --version`
   - Check shell from `$SHELL`

2. **Repository state** (if in a repo):
   - Run `jj status` for working copy state
   - Check for `.aiki/` directory
   - Run `aiki task list --status in-progress` for active tasks

3. **Configuration:**
   - Check for `.aiki/hooks/` and `.aiki/instructions.md`
   - List installed plugins if any

Format all diagnostics as a markdown section for the issue body.
{% else %}
Collect minimal context:
- Run `aiki --version`
- Note OS/arch

This is a feature request — full diagnostics aren't needed.
{% endif %}

Close subtask when diagnostics are collected (or skipped for features).

## Draft the issue

1. Compose the GitHub issue:
   - **Title:** Clear, specific summary (from user's description + clarification)
   - **Body:** Use the appropriate template:

{% if data.issue_type == "bug" %}
   ```
   ## Bug Report

   ### Description
   <clear description of the bug>

   ### Steps to Reproduce
   1. ...
   2. ...

   ### Expected Behavior
   <what should happen>

   ### Actual Behavior
   <what actually happens>

   ### Diagnostics
   <collected diagnostic snapshot>
   ```
{% else %}
   ```
   ## Feature Request

   ### Problem
   <what problem this solves>

   ### Proposed Solution
   <desired behavior>

   ### Alternatives Considered
   <any workarounds or alternative approaches>

   ### Context
   - aiki: <version>
   - OS: <os/arch>
   ```
{% endif %}

2. Show the user the full draft
3. Ask: "Does this look right? Any changes before I file it?"
4. Iterate until the user approves
5. Close subtask when draft is approved

## File the issue

1. Verify `gh` CLI is available (check with `which gh`)
   - If not installed, tell the user to install from https://cli.github.com/ and run `gh auth login`
   - As a fallback, print the formatted issue to stdout so they can file manually
2. Run:
   ```bash
   gh issue create --repo glasner/aiki \
     --title "<title>" \
     --label "{{data.label}}" \
     --body "<body>"
   ```
3. Print the issue URL on success
4. Close subtask (and parent task) when done
```

## Implementation

### Files to create/modify

1. **`src/tasks/templates/core/support/issue.md`** (new) — the template above

2. **`src/commands/support.rs`** (new) — thin command, modeled on `plan.rs`:
   - Parse `--bug`/`--feature` flags
   - Set `data.issue_type` = "bug" or "feature"
   - Set `data.label` = "bug" or "enhancement"
   - Set `data.description` from positional arg (or prompt)
   - Load template, create task, spawn Claude interactively
   - Handle exit codes same as plan.rs

3. **`src/commands/mod.rs`** — add `pub mod support;`

4. **`src/main.rs`** — add `Support` variant with subcommand dispatch

### Command implementation (thin wrapper)

The command follows plan.rs almost exactly:

```rust
pub fn run(description: Option<String>, is_feature: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let issue_type = if is_feature { "feature" } else { "bug" };
    let label = if is_feature { "enhancement" } else { "bug" };

    // Prompt for description if not provided
    let description = match description {
        Some(d) => d,
        None => {
            // prompt_multiline_input (reuse from plan.rs or extract)
        }
    };

    // Load template, create task
    let mut variables = VariableContext::new();
    variables.set_data("issue_type", issue_type);
    variables.set_data("label", label);
    variables.set_data("description", &description);

    // create_support_task() → same pattern as create_plan_task()
    let task_id = create_support_task(&cwd, &variables, timestamp)?;

    // Start task, spawn Claude interactively
    start_task_core(&cwd, &[task_id.clone()])?;
    spawn_claude(&cwd, &task_id, &description)?;

    Ok(())
}
```

### What to reuse from plan.rs

- `prompt_multiline_input` — extract to shared util or duplicate (it's small)
- Task creation pattern: load template → create variable context → create tasks → generate IDs → write events
- Claude spawn pattern: `Command::new("claude").env("AIKI_TASK", &task_id).arg(&prompt).status()`
- Exit code handling (0, 130, 143)

### CLI definition

```rust
/// Get support — file bugs, request features
Support {
    #[command(subcommand)]
    command: SupportCommands,
},

#[derive(Subcommand)]
pub enum SupportCommands {
    /// File a bug report or feature request as a GitHub issue
    Issue {
        /// Description of the issue
        description: Vec<String>,

        /// File as a feature request instead of a bug report
        #[arg(long)]
        feature: bool,

        /// File as a bug report (default)
        #[arg(long, conflicts_with = "feature")]
        bug: bool,

        /// Override which agent to use
        #[arg(long)]
        agent: Option<String>,

        /// Output format
        #[arg(long)]
        output: Option<OutputFormat>,
    },
}
```

### Error handling

- `gh` not installed → Claude tells user to install (handled in template instructions, not Rust code)
- Not in a repo → diagnostics show `<not available>`, command still works
- Each diagnostic step is independent (template instructs Claude to continue on failure)

### Testing

- Unit test: slug generation and flag parsing
- Template test: verify conditionals render correctly for bug vs feature
- Integration: template loads and creates valid task structure

## Not in scope

- `aiki support chat` — future
- `aiki support docs` — future
- Separate `--title` flag
- Anonymization/redaction
- Issue templates on GitHub repo side
