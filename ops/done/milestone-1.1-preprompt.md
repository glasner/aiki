# Milestone 1.1: PrePrompt Event

This document outlines the implementation plan for the PrePrompt event system (Milestone 1.1).

See [milestone-1.md](./milestone-1.md) for the full Milestone 1 overview.

---

## Overview

The PrePrompt event fires before the agent sees the user's prompt, allowing flows to inject context, skills, architecture docs, and task information.

**Key Decision:** Use the shared MessageChunk/MessageAssembler infrastructure for consistent syntax across all events.

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
3. Flow parser creates MessageChunk from YAML `prompt:` action
4. PrePrompt event handler adds chunk to its `prompt_assembler`
5. Handler calls `prompt_assembler.build()` to assemble final prompt
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
    pub original_prompt: String,              // Immutable original prompt
    pub prompt_assembler: MessageAssembler,   // Owns MessageAssembler for building final prompt
    pub session_id: Option<String>,           // Current session
    pub timestamp: DateTime<Utc>,
    pub working_directory: PathBuf,           // Current working directory
    pub recent_files: Vec<PathBuf>,           // Files edited recently (optional)
}

impl PrePromptEvent {
    pub fn new(prompt: String) -> Self {
        Self {
            original_prompt: prompt.clone(),
            prompt_assembler: MessageAssembler::new(Some(prompt), "\n\n"),
            session_id: None,
            timestamp: Utc::now(),
            working_directory: std::env::current_dir().unwrap(),
            recent_files: Vec::new(),
        }
    }
    
    pub fn apply_prompt_action(&mut self, chunk: MessageChunk) {
        self.prompt_assembler.add_chunk(chunk);
    }
    
    pub fn build_prompt(&self) -> String {
        self.prompt_assembler.build()
    }
}
```

**Event lifecycle:**
1. User submits prompt
2. `PrePromptEvent` created with original prompt and `MessageAssembler`
3. Flow engine executes PrePrompt flow
4. `prompt:` actions parsed as MessageChunk, added to `prompt_assembler`
5. Handler calls `build_prompt()` to assemble final message
6. Modified prompt sent to agent

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

**Pre-requisite:** Complete Milestone 1.0 Phase 0 (extract actions from types.rs)

### Core Engine

- [ ] Add `PrePromptEvent` struct to `cli/src/events.rs` (owns `MessageAssembler`)
- [ ] Add `prompt:` action to flow DSL (parses as `MessageChunk`)
- [ ] Implement `cli/src/flows/actions/prompt.rs` that creates MessageChunk from YAML
- [ ] Add PrePrompt handler: `cli/src/flows/handlers/pre_prompt.rs` (calls `event.apply_prompt_action(chunk)`)
- [ ] Implement error handling (graceful degradation: use original prompt on error)
- [ ] Hook into vendor prompt submission lifecycle

### Vendor Integration

- [ ] `cli/src/vendors/claude_code.rs` - Hook before prompt sent
- [ ] `cli/src/vendors/cursor.rs` - Hook before prompt sent  
- [ ] `cli/src/vendors/acp.rs` - Hook before prompt sent
- [ ] Ensure modified prompt sent to agent
- [ ] Preserve session ID across event

### Testing

- [ ] Unit tests: MessageChunk parsing from YAML `prompt:` action
- [ ] Unit tests: PrePromptEvent owns MessageAssembler correctly
- [ ] Unit tests: Multiple prepend chunks accumulate correctly
- [ ] Unit tests: Multiple append chunks accumulate correctly
- [ ] Unit tests: Error handling (file not found, parse errors)
- [ ] Integration tests: PrePrompt flow execution
- [ ] Integration tests: Multiple `prompt:` actions
- [ ] Integration tests: Flow error triggers graceful degradation
- [ ] E2E tests: Real agent receives modified prompt
- [ ] E2E tests: Path expansion works end-to-end
- [ ] E2E tests: Flow error shows warning but doesn't block agent

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
✅ MessageChunk correctly parses both forms from YAML  
✅ PrePromptEvent owns MessageAssembler instance  
✅ Multiple prepend chunks accumulate in correct order  
✅ Multiple append chunks accumulate in correct order  
✅ Multiple `prompt:` actions can be used in same flow  
✅ Modified prompt is sent to agent  
✅ Session ID is captured and available  
✅ Flow errors trigger graceful degradation (original prompt used)  
✅ User sees warning when flow fails, but workflow continues  

---

## Error Handling

**Strategy: Graceful Degradation**

If a PrePrompt flow fails, the system falls back to the original prompt. The agent workflow continues without interruption.

### Common Error Scenarios

#### 1. File Not Found

```yaml
PrePrompt:
  prompt:
    prepend: .aiki/arch/missing-file.md
```

**User sees:**
```
⚠️ PrePrompt flow failed: File not found: .aiki/arch/missing-file.md
Continuing with original prompt...

[Agent receives and responds to original prompt normally]
```

#### 2. Invalid YAML Syntax

```yaml
PrePrompt:
  prompt:
    prepend:
      - "Text"
      - 123  # Invalid: should be string
```

**User sees:**
```
⚠️ PrePrompt flow failed: Invalid YAML: expected string, got number
Continuing with original prompt...

[Agent receives and responds to original prompt normally]
```

#### 3. MessageChunk Validation Error

```yaml
PrePrompt:
  prompt: {}  # Invalid: empty object
```

**User sees:**
```
⚠️ PrePrompt flow failed: MessageChunk must have at least prepend or append
Continuing with original prompt...

[Agent receives and responds to original prompt normally]
```

### Implementation Pattern

```rust
impl PrePromptHandler {
    pub fn execute(&self, event: &mut PrePromptEvent, flow: &Flow) -> Result<()> {
        match self.execute_flow(event, flow) {
            Ok(()) => {
                // Flow succeeded - use modified prompt
                Ok(())
            }
            Err(e) => {
                // Flow failed - log error, show warning, use original prompt
                eprintln!("⚠️ PrePrompt flow failed: {}", e);
                eprintln!("Continuing with original prompt...\n");
                
                // Reset assembler to use original prompt
                event.prompt_assembler = MessageAssembler::new(
                    Some(event.original_prompt.clone()),
                    "\n\n"
                );
                
                // Don't propagate error - graceful degradation
                Ok(())
            }
        }
    }
}
```

**Key points:**
- Error is logged and shown to user
- Original prompt is preserved and used
- Agent invocation proceeds normally
- User can continue working without interruption

### Future: Strict Mode (Not in Milestone 1.1)

For teams that want validation gates:

```yaml
# .aiki/config.toml
[flows]
error_handling = "strict"
```

**Strict mode behavior:**
- PrePrompt error → Don't invoke agent, show error to user
- User must fix flow before agent can respond

**Use case:** Teams that require certain context (e.g., security guidelines) to always be present.

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
/Users/myuser/project/arch/backend.md

---

# Current Task
Implementing user authentication

---

Add login endpoint

---

Remember to write tests.
```

**Note:** Paths like `.aiki/arch/backend.md` are expanded to absolute paths (e.g., `/Users/myuser/project/arch/backend.md`) but NOT automatically loaded. The path string is passed as-is. File loading (if needed) happens at the vendor integration layer.

---

## Technical Components

### File Structure

```
cli/src/
├── events.rs                          # PrePromptEvent struct (owns MessageAssembler)
├── flows/
│   ├── messages.rs                    # MessageChunk + MessageAssembler (see milestone-1.0)
│   ├── actions/
│   │   └── prompt.rs                  # Parses YAML to MessageChunk
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
use crate::flows::MessageChunk;

pub fn execute_prompt(value: &Value, event: &mut PrePromptEvent) -> Result<()> {
    // Parse YAML into MessageChunk
    let chunk: MessageChunk = serde_yaml::from_value(value.clone())?;
    
    // Add chunk to event's assembler
    event.apply_prompt_action(chunk);
    
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
- [milestone-1.2-post-response.md](./milestone-1.2-post-response.md) - Related PostResponse design
- [ROADMAP.md](../ROADMAP.md) - Strategic context
