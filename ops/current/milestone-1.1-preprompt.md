# Milestone 1.1: PrePrompt Event

This document outlines the implementation plan for the PrePrompt event system (Milestone 1.1).

See [milestone-1.md](./milestone-1.md) for the full Milestone 1 overview.

---

## Overview

The PrePrompt event fires before the agent sees the user's prompt, allowing flows to inject context, skills, architecture docs, and task information.

**Key Decision:** Use the shared MessageBuilder parser for consistent syntax across all events.

**Syntax:** See [Shared Syntax Pattern](./milestone-1.md#shared-syntax-pattern) in milestone-1.md for the `prompt:` action syntax (short form and explicit form).

---

## Core Features

### 1. Prompt Modification

The `prompt:` action modifies the user's prompt before the agent sees it:

```yaml
PrePrompt:
  # Short form - defaults to append
  prompt: "Remember to follow our architecture patterns."

  # Explicit form - with prepend
  prompt:
    prepend:
      - .aiki/arch/backend.md
      - .aiki/skills/testing.md
    append: "Run tests when done."
```

**How it works:**
1. User submits prompt
2. PrePrompt event fires with original prompt
3. Flow executes `prompt:` actions via MessageBuilder
4. Prepended content added before original prompt
5. Appended content added after original prompt
6. Agent receives modified prompt

**Final prompt structure:**
```
[prepended content 1]
[prepended content 2]
...
[original user prompt]
...
[appended content 1]
[appended content 2]
```

### 2. Multiple Prompt Actions

Can use multiple `prompt:` actions in the same flow:

```yaml
PrePrompt:
  - prompt:
      prepend: .aiki/arch/backend.md
  
  - let: task_context = self.load_task_context
  - if: $task_context != ""
    then:
      prompt:
        prepend: $task_context
  
  - prompt: "Remember to write tests."
```

**Accumulation order:**
- Prepends accumulate in order (first action → first in output)
- Appends accumulate in order (first action → first in output)
- All prepends before original prompt, all appends after

---

## Event Structure

```rust
pub struct PrePromptEvent {
    pub original_prompt: String,         // Immutable original prompt
    pub prompt: RefCell<String>,         // Mutable, modified by MessageBuilder
    pub session_id: Option<String>,      // Current session
    pub timestamp: DateTime<Utc>,
    pub working_directory: PathBuf,      // Current working directory
    pub recent_files: Vec<PathBuf>,      // Files edited recently (optional)
}
```

**Event lifecycle:**
1. User submits prompt
2. `PrePromptEvent` created with original prompt
3. Flow engine executes PrePrompt flow
4. `prompt:` actions modify `event.prompt` via RefCell
5. Modified prompt sent to agent

---

## Use Cases

### Use Case 1: Auto-Inject Architecture Docs

```yaml
PrePrompt:
  prompt:
    prepend:
      - .aiki/arch/structure/backend/index.md
      - .aiki/arch/patterns/error-handling.md
```

**Result:** Agent sees architecture context before every request

### Use Case 2: Skill Activation

```yaml
PrePrompt:
  # Detect if user is asking about testing
  - if: $event.original_prompt contains "test"
    then:
      prompt:
        prepend: .aiki/skills/testing-best-practices.md

  # Detect if user is asking about database
  - if: $event.original_prompt contains "database"
    then:
      prompt:
        prepend: .aiki/skills/database-patterns.md
```

**Result:** Relevant skills auto-injected based on prompt content

### Use Case 3: Task Context Loading

```yaml
PrePrompt:
  - let: current_task = self.load_current_task
  - if: $current_task != ""
    then:
      prompt:
        prepend: |
          # Current Task
          $current_task
          
          Remember to update the task doc when done.
```

**Result:** Active task context always visible to agent

### Use Case 4: Reminders

```yaml
PrePrompt:
  prompt:
    append: |
      
      ---
      
      Important reminders:
      - Run `npm test` before committing
      - Update CHANGELOG.md for user-facing changes
      - Follow our commit message convention
```

**Result:** Important reminders appended to every prompt

---

## Implementation Tasks

### Core Engine

- [ ] Add `PrePromptEvent` struct to `cli/src/events.rs`
- [ ] Add `prompt:` action to flow DSL
- [ ] Implement `cli/src/flows/actions/prompt.rs` using MessageBuilder
- [ ] Add PrePrompt handler: `cli/src/flows/handlers/pre_prompt.rs`
- [ ] Hook into vendor prompt submission lifecycle

### Vendor Integration

- [ ] `cli/src/vendors/claude_code.rs` - Hook before prompt sent
- [ ] `cli/src/vendors/cursor.rs` - Hook before prompt sent  
- [ ] `cli/src/vendors/acp.rs` - Hook before prompt sent
- [ ] Ensure modified prompt sent to agent
- [ ] Preserve session ID across event

### Testing

- [ ] Unit tests: MessageBuilder with `prompt:` action
- [ ] Unit tests: Inline content preservation
- [ ] Unit tests: Multiple prepend items accumulate correctly
- [ ] Unit tests: Multiple append items accumulate correctly
- [ ] Integration tests: PrePrompt flow execution
- [ ] Integration tests: Multiple `prompt:` actions
- [ ] E2E tests: Real agent receives modified prompt
- [ ] E2E tests: File injection works end-to-end

### Documentation

- [ ] Tutorial: "Injecting Context with PrePrompt"
- [ ] Cookbook: Common patterns (skills, architecture, tasks)
- [ ] Reference: PrePrompt DSL syntax
- [ ] Examples: Real-world PrePrompt flows

---

## Success Criteria

✅ PrePrompt fires before agent sees prompt (all vendor integrations)  
✅ Short form `prompt: "string"` defaults to append  
✅ Explicit form `prompt: { prepend: [...], append: [...] }` works  
✅ MessageBuilder correctly parses both forms  
✅ Inline content is preserved as-is  
✅ Multiple prepend items accumulate in correct order  
✅ Multiple append items accumulate in correct order  
✅ Multiple `prompt:` actions can be used in same flow  
✅ Modified prompt is sent to agent  
✅ Session ID is captured and available  

---

## Example Output

Given this flow:

```yaml
PrePrompt:
  prompt:
    prepend:
      - .aiki/arch/backend.md
      - |
        # Current Task
        Implementing user authentication
    append: "Remember to write tests."
```

And user prompt: `"Add login endpoint"`

**Agent sees:**

```markdown
# Backend Architecture

[contents of .aiki/arch/backend.md]

---

# Current Task
Implementing user authentication

---

Add login endpoint

---

Remember to write tests.
```

---

## Technical Components

### File Structure

```
cli/src/
├── events.rs                          # PrePromptEvent struct
├── flows/
│   ├── actions/
│   │   ├── message_builder.rs         # Shared parser (see milestone-1.md)
│   │   └── prompt.rs                  # Prompt action using MessageBuilder
│   ├── handlers/
│   │   └── pre_prompt.rs              # PrePrompt handler
│   └── engine.rs                      # Flow execution
└── vendors/
    ├── claude_code.rs                 # Claude Code integration
    ├── cursor.rs                      # Cursor integration
    └── acp.rs                         # ACP integration
```

### Key Functions

```rust
// cli/src/flows/actions/prompt.rs
pub fn execute_prompt(value: &Value, event: &PrePromptEvent) -> Result<()> {
    let builder = MessageBuilder::parse(value)?;
    
    for prepend in &builder.prepends {
        event.prompt.borrow_mut().insert_str(0, prepend);
    }
    
    for append in &builder.appends {
        event.prompt.borrow_mut().push_str(append);
    }
    
    Ok(())
}
```

---

## Expected Timeline

**Week 1**

- Days 1-2: Event structure, MessageBuilder integration
- Days 3-4: Vendor hooks, flow execution
- Day 5: Testing and documentation

---

## Future Enhancements

### 1. Smart Skill Detection

Automatically detect which skills to inject based on prompt analysis:

```yaml
PrePrompt:
  # System automatically detects relevant skills
  # No manual if/then needed
  auto_inject_skills: true
```

### 2. Template Variables

Support variables in prepended content:

```yaml
PrePrompt:
  prompt:
    prepend: |
      # Project: $project.name
      # Current branch: $git.branch
```

### 3. Conditional File Injection

Only inject file if it exists:

```yaml
PrePrompt:
  prompt:
    prepend_if_exists:
      - .aiki/tasks/current.md
      - .aiki/context/session.md
```

---

## References

- [milestone-1.md](./milestone-1.md) - Milestone 1 overview and shared syntax
- [response-strategy-comparison.md](./response-strategy-comparison.md) - Related PostResponse design
- [ROADMAP.md](../ROADMAP.md) - Strategic context
