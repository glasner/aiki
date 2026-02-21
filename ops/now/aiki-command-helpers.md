# Aiki Command Helpers for Templates

**Date**: 2026-02-21
**Status**: Planning
**Purpose**: Add template helpers to reduce repetitive bash code blocks in task templates

**Related Documents**:
- `.aiki/templates/aiki/*.md` - Current template files with repetitive patterns
- `cli/src/tasks/templates/` - Template parsing and resolution code

---

## Executive Summary

Task templates currently contain repetitive bash code blocks for aiki commands. Each command requires wrapping in ` ```bash ... ``` ` fencing, adding visual noise and making templates harder to maintain. This proposal adds template helpers like `{% task.start {{data.plan}} %}` that auto-expand to properly formatted bash code blocks, reducing template verbosity by ~30%.

---

## User Experience

### Current Syntax (Verbose)

Templates require explicit bash code fencing for every command:

```markdown
Start the plan:

```bash
aiki task start {{data.plan}}

```

Execute each subtask:

```bash
aiki task run {{data.plan}} --next-subtask
```
```

### New Syntax (Concise)

With helpers, the same template becomes much cleaner:

```markdown
Start the plan:

{% task.start {{data.plan}} %}

Execute each subtask:

{% task.run {{data.plan}} --next-subtask %}
```

### Supported Commands

**Single-line commands:**


All `aiki task` subcommands get helpers:

```markdown
{% task.add "Description" --subtask-of {{parent}} --source file:{{spec}} %}
{% task.start {{task_id}} %}
{% task.run {{task_id}} --next-subtask %}
{% task.stop {{task_id}} --reason "Blocked on X" %}
{% task.close {{task_id}} --summary "Done" %}
{% task.show {{task_id}} --with-source %}
{% task.comment {{task_id}} "Progress update" %}
```

**Multiline commands** (with `{% endtask %}`):
```markdown
{% task.set $PLAN_ID --instructions %}
Implementation plan for <spec title>.
See spec: {{data.spec}}
{% endtask %}
```

Expands to:

```bash
aiki task set $PLAN_ID --instructions <<'MD'
Implementation plan for <spec title>.
See spec: {{data.spec}}
MD
```
```

### Variable Capture Pattern

For commands that output values (like `--output id`), support capture syntax:

```markdown
{% task.add "Plan title" --source file:{{data.spec}} --output id | capture PLAN_ID %}
```

Expands to:

```bash
PLAN_ID=$(aiki task add "Plan title" --source file:{{data.spec}} --output id)
```

---

## How It Works

### Template Processing Pipeline

1. **Parse template** - Extract frontmatter and body (existing)
2. **Process command helpers** - NEW STEP
   - Find `{% task.COMMAND args %}` directives and `{% task.COMMAND args %}...{% endtask %}` blocks
   - Parse command name and arguments
   - Expand to bash code block format (with heredoc for multiline blocks)
   - Handle variable capture with `| capture VAR`
3. **Process conditionals** - `{% if %}`, `{% for %}` (existing)
4. **Substitute variables** - `{{data.key}}` (existing)
5. **Return expanded template**

### Helper Syntax Grammar

```
helper := "{% task." COMMAND ARGS ( "|" "capture" VAR )? "%}"

COMMAND := "add" | "start" | "run" | "stop" | "close" | "show" | "comment" | ...

ARGS := any text (may contain {{variables}})

VAR := shell variable name
```

### Expansion Rules

**Simple command:**
```
{% task.start {{data.plan}} %}
```
↓
```markdown
```bash
aiki task start {{data.plan}}

```
```

**With capture:**
```
{% task.add "Title" --output id | capture VAR %}
```
↓
```bash
VAR=$(aiki task add "Title" --output id)
```

**Multiline block:**
```
{% task.set $PLAN_ID --instructions %}
Implementation plan for <spec title>.
See spec: {{data.spec}}
{% endtask %}
```
↓
```bash
aiki task set $PLAN_ID --instructions <<'MD'
Implementation plan for <spec title>.
See spec: {{data.spec}}
MD
```

---


---

## Implementation Plan

### Phase 1: Core Helper Processing

**Goal**: Add helper expansion to template processing pipeline

**Tasks**:
1. Create `cli/src/tasks/templates/helpers.rs`
   - Define `CommandHelper` struct (command name, args, capture var)
   - Implement parser for `{% task.COMMAND args %}` syntax
   - Implement expansion to bash code blocks
   - Handle `| capture VAR` pattern

2. Integrate into template resolver
   - Add `process_helpers()` step before variable substitution
   - Update `resolver.rs` to call helper processor
   - Ensure helpers expand before `{{variables}}` are substituted

3. Add tests
   - Unit tests for helper parsing
   - Unit tests for expansion logic
   - Integration tests with actual templates

### Phase 2: Update Existing Templates

**Goal**: Migrate all templates to use new helper syntax

**Tasks**:
1. Update `.aiki/templates/aiki/build.md`
2. Update `.aiki/templates/aiki/plan.md`
3. Update `.aiki/templates/aiki/fix.md`
4. Update `.aiki/templates/aiki/review.md`
5. Update `.aiki/templates/aiki/spec.md`
6. Update any subtemplates in `explore/`, `fix/`, `review/` directories

### Phase 3: Documentation

**Goal**: Document the new syntax for template authors

**Tasks**:
1. Update template system docs in `cli/src/tasks/templates/mod.rs`
2. Add helper syntax examples to module documentation
3. Update any external documentation (if exists)

---

## Technical Details

### Helper Processing Implementation

```rust
// cli/src/tasks/templates/helpers.rs

pub struct CommandHelper {
    pub command: String,           // e.g., "start", "add", "close"
    pub args: String,              // Raw argument string (may contain {{vars}})
    pub capture_var: Option<String>, // Variable name for `| capture VAR`
}

/// Find all {% task.COMMAND args %} directives in content
pub fn find_helpers(content: &str) -> Vec<(usize, usize, CommandHelper)> {
    // Returns (start_pos, end_pos, helper)
}

/// Expand helper to bash code block
pub fn expand_helper(helper: &CommandHelper) -> String {
    if let Some(var) = &helper.capture_var {
        format!("```bash\n{}=$(aiki task {} {})\n```", var, helper.command, helper.args)
    } else {
        format!("```bash\naiki task {} {}\n\n```", helper.command, helper.args)
    }
}

/// Process all helpers in template content
pub fn process_helpers(content: &str) -> Result<String> {
    let mut result = content.to_string();
    let helpers = find_helpers(&result);
    
    // Replace in reverse order to maintain positions
    for (start, end, helper) in helpers.into_iter().rev() {
        let expanded = expand_helper(&helper);
        result.replace_range(start..end, &expanded);
    }
    
    Ok(result)
}
```


### Integration Point

```rust
// cli/src/tasks/templates/resolver.rs

pub fn load_template(name: &str) -> Result<TaskTemplate> {
    let content = load_template_file(name)?;
    
    // Process helpers BEFORE variable substitution
    let content = helpers::process_helpers(&content)?;
    
    // Existing processing continues
    let template = parse_template(&content, name)?;
    // ...
}
```

---

## Error Handling

**Invalid helper syntax:**
```
Error: Invalid helper syntax at line 15: {% task.invalid %}
Expected: {% task.COMMAND args %}
```

**Unknown command:**
```
Error: Unknown task command 'invalid' at line 15
Supported commands: add, start, run, stop, close, show, comment
```

**Malformed capture:**
```
Error: Invalid capture syntax at line 15: {% task.add "X" | capture %}
Expected: {% task.add "X" --output id | capture VAR %}
```

---

## Open Questions

1. **Should we support other aiki commands?** (e.g., `{% review.issue.add %}`, `{% explore %}`)
   - Start with `task.*` only, expand later if needed

2. **Should helpers support multi-line arguments?**
   - No - keep it simple. Complex commands can still use manual bash blocks

3. **How should we handle escaping in arguments?**
   - Let bash handle it - helpers just wrap the args as-is

4. **Should the blank line after commands be configurable?**
   - No - standardize on blank line for consistency

---

## Success Criteria

- [ ] All existing templates work with new helper syntax
- [ ] Templates are ~30% shorter and easier to read
- [ ] No change to template output or behavior (transparent to users)
- [ ] Helper processing adds <10ms to template load time
- [ ] Tests cover parsing edge cases and error conditions
