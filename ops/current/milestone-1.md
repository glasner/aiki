# Milestone 1: Core Extensions (Phase 8 - The Aiki Way)

**Timeline:** 4 weeks  
**Goal:** Add event types and capabilities needed for all aiki/default patterns

---

## Overview

Milestone 1 extends Aiki's flow system (built in Phase 5) with new event types and capabilities that enable the three key patterns in aiki/default:
1. **PrePrompt event** - Inject context before agent sees prompt
2. **PostResponse event & Task System** - Validate after agent responds with structured task management
3. **Flow composition** - Reuse flows via `before:` and `after:` directives

**Why this matters:** These primitives unlock Milestone 2-5 features. Without them, we can't inject skills, cache architecture docs, run builds automatically, or manage tasks.

---

## Shared Syntax Pattern

All Milestone 1 events use a consistent syntax pattern for modifying text (prompts, autoreplies, commit messages):

| Event | Action | Short Form (default: append) | Explicit Form |
|-------|--------|------------------------------|---------------|
| **PrePrompt** | `prompt:` | `prompt: "string"` | `prompt: { prepend: [...], append: [...] }` |
| **PostResponse** | `autoreply:` | `autoreply: "string"` | `autoreply: { prepend: "...", append: "..." }` |
| **PrepareCommitMessage** | `commit_message:` | `commit_message: "string"` | `commit_message: { append: "..." }` |

**Why this pattern:**
- ✅ **Consistent across all events** - Same mental model everywhere
- ✅ **Terser for common case** - 90% of usage is just append
- ✅ **Explicit when needed** - Use object form for prepend or multiple items
- ✅ **Readable and natural** - Clear what's being modified

**Implementation:**
All three actions use a shared `MessageBuilder` parser (`cli/src/flows/actions/message_builder.rs`) to ensure consistent parsing behavior.

**Examples:**

```yaml
# PrePrompt - short form
PrePrompt:
  prompt: "Remember to follow our architecture patterns"

# PrePrompt - explicit form
PrePrompt:
  prompt:
    prepend:
      - .aiki/arch/backend.md
      - "# Current Task\nImplementing auth"
    append: "Run tests when done"

# PostResponse - short form
PostResponse:
  - if: $errors > 0
    then:
      autoreply: "Fix the errors above"

# PrepareCommitMessage - short form
PrepareCommitMessage:
  commit_message: "Co-authored-by: AI Agent <ai@example.com>"
```

---

## What Gets Built

This milestone delivers four core capabilities. Each has its own detailed documentation:

### 1.0. MessageChunk & MessageAssembler Shared Syntax
📄 **Detailed doc:** [milestone-1.0-message-assembler.md](./milestone-1.0-message-assembler.md)

**Summary:** Shared data structure and assembler for consistent syntax across all message-building events.

**Key capabilities:**
- Parse short form (`action: "string"`) and explicit form (`action: { prepend: [...], append: [...] }`)
- MessageChunk data structure for prepend/append fields parsed from YAML
- MessageAssembler stateful builder that collects chunks and assembles final message
- Events own MessageAssembler instances (e.g., `prompt_assembler`, `body_assembler`, `trailers_assembler`)
- Generate content-based check IDs for stuck detection
- Ensure consistent behavior across PrePrompt, PostResponse, and PrepareCommitMessage

**Why this comes first:** All three events (PrePrompt, PostResponse, PrepareCommitMessage) need MessageChunk/MessageAssembler. This is foundational infrastructure that must be completed before implementing any event handlers.

**Timeline:** Week 1 (Days 1-3)

---

### 1.1. PrePrompt Event
📄 **Detailed doc:** [milestone-1.1-preprompt.md](./milestone-1.1-preprompt.md)

**Summary:** Fire before agent sees user prompt, allowing context injection.

**Key capabilities:**
- Inject architecture docs, skills, and task context
- Use `prompt:` action with MessageBuilder (short/explicit forms)
- Prepend and append to user's original prompt

**Example:**
```yaml
PrePrompt:
  prompt:
    prepend: .aiki/arch/backend.md
    append: "Run tests when done"
```

**Timeline:** Week 1

---

### 1.2. PostResponse Event
📄 **Detailed doc:** [milestone-1.2-post-response.md](./milestone-1.2-post-response.md)

**Summary:** Fire after agent completes response, enabling validation and autoreplies.

**Key capabilities:**
- Validate agent output (errors, warnings, tests)
- Send autoreplies based on validation results
- Use `autoreply:` action with MessageAssembler (short/explicit forms)
- Helper functions for error detection (`self.count_typescript_errors`, etc.)

**Example:**
```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors
  - if: $ts_errors > 0
    then:
      autoreply: "Fix the TypeScript errors above before continuing."
  
  - let: test_results = self.run_tests
  - if: $test_results.failed > 0
    then:
      autoreply: "Tests failed. Run `npm test` to see details."
```

**Timeline:** Week 2

---

### 1.3. Flow Composition
📄 **Detailed doc:** [milestone-1.3-flow-composition.md](./milestone-1.3-flow-composition.md)

**Summary:** Allow flows to include and reuse other flows with explicit ordering control.

**Key capabilities:**
- Include flows that run before this flow via `before:` directive
- Include flows that run after this flow via `after:` directive
- Invoke flows inline with `flow:` action (runs at specific point)
- Flow resolution (aiki/*, vendor/*, local paths)
- Circular dependency detection
- Atomic flow execution (each flow runs its own before/after internally)

**Example:**
```yaml
name: "My Workflow"

before:
  - aiki/quick-lint        # Runs before this flow
  - aiki/security-scan

after:
  - aiki/cleanup           # Runs after this flow

PostResponse:
  - if: $errors > 0
    then:
      flow: aiki/detailed-lint  # Invoke inline (runs NOW)
```

**Timeline:** Week 3

---

### 1.4. Task System (Optional)
📄 **Detailed doc:** [milestone-1.4-task-system.md](./milestone-1.4-task-system.md)

**Summary:** Event-sourced task management for structured workflows. Alternative to text autoreplies.

**Key capabilities:**
- Event-sourced task system stored on JJ `aiki/tasks` branch
- Create/query/start/close tasks via PostResponse flows
- CLI commands: `aiki task ready`, `aiki task create`, `aiki task start`, `aiki task close`
- Task state reconstruction from immutable event log
- Agent workflow: query tasks → start task → make changes → PostToolUse auto-closes
- Attempt-based stuck detection (3+ failed attempts)
- Content-addressed task IDs prevent duplicates

**Example:**
```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors
  - for: error in $ts_errors
    then:
      task.create:
        objective: "Fix: $error.message"
        type: error
        file: $error.file
        line: $error.line
```

**Timeline:** 2-3 weeks (Phase 1), optional - evaluate after completing 1.0-1.3

---

## Technical Architecture

### Error Handling Strategy

**Core Principle: Graceful Degradation**

Flow errors should never break the user's workflow. If a PrePrompt or PostResponse flow fails, the system falls back to default behavior.

#### PrePrompt Error Handling

```
User submits prompt
       ↓
PrePromptEvent fires
       ↓
   Flow executes PrePrompt actions
       ↓
   ❌ Error occurs (file not found, parse error, etc.)
       ↓
   Log error + show warning to user
       ↓
   Use original prompt (unmodified)
       ↓
Agent receives original prompt  ← Graceful degradation
```

**Rationale:** 
- User's intent (the prompt) is preserved
- Agent can still respond, even without injected context
- User sees warning about what failed
- Non-blocking: doesn't prevent work

**Example error output:**
```
⚠️ PrePrompt flow failed: File not found: .aiki/arch/backend.md
Continuing with original prompt...

[Agent receives original prompt and responds normally]
```

#### PostResponse Error Handling

```
Agent generates response
       ↓
PostResponseEvent fires
       ↓
   Flow executes PostResponse actions
       ↓
   ❌ Error occurs (helper function crash, invalid syntax, etc.)
       ↓
   Log error + show warning to user
       ↓
   Skip autoreply
       ↓
Response shown to user (without autoreply)  ← Graceful degradation
```

**Rationale:**
- Agent's response is preserved and shown to user
- Validation failure doesn't block delivery of response
- User sees warning about what failed
- Non-blocking: user can continue working

**Example error output:**
```
[Agent's response shown normally]

⚠️ PostResponse flow failed: count_typescript_errors crashed
No autoreply generated.
```

#### Configuration (Future Enhancement)

For users who want stricter error handling:

```yaml
# .aiki/config.toml
[flows]
error_handling = "strict"  # Options: "graceful" (default), "strict"
```

**Strict mode behavior:**
- PrePrompt error → Show error, don't invoke agent
- PostResponse error → Show error, don't show agent response

**Use case:** Teams that require validation gates (e.g., all tests must pass before agent can respond).

**Not in Milestone 1:** Keep it simple with graceful degradation only. Add strict mode later if users request it.

### Event Flow Diagram

```
User submits prompt
       ↓
PrePromptEvent fires (with error handling)
       ↓
   Flow executes PrePrompt actions
   (inject skills, load cache, etc.)
   [On error: log, warn, use original prompt]
       ↓
Agent receives prompt (augmented or original)
       ↓
Agent generates response
       ↓
PostResponseEvent fires (with error handling)
       ↓
   Flow executes PostResponse actions
   (run builds, detect patterns, etc.)
   [On error: log, warn, skip autoreply]
       ↓
Response shown to user (with or without autoreply)
```

### Module Structure

```
cli/src/
├── events.rs                    # Event type definitions
├── tasks/                       # Task system (optional, Milestone 1.4)
│   ├── manager.rs              # TaskManager with JJ operations
│   ├── types.rs                # Task, TaskEvent, TaskDefinition
│   └── cli.rs                  # CLI command handlers
├── flows/
│   ├── engine.rs               # Event dispatch, action execution
│   ├── parser.rs               # Flow YAML parsing
│   ├── loader.rs               # Flow loading and composition (Milestone 1.3)
│   ├── resolver.rs             # Flow path resolution (Milestone 1.3)
│   ├── handlers/
│   │   ├── pre_prompt.rs       # PrePrompt handler (Milestone 1.1)
│   │   ├── post_response.rs    # PostResponse handler (Milestone 1.2)
│   │   └── mod.rs
│   ├── actions/
│   │   ├── messages.rs         # MessageChunk + MessageAssembler (Milestone 1.0)
│   │   ├── task.rs             # Task actions (optional, Milestone 1.4)
│   │   ├── flow.rs             # Flow composition action (Milestone 1.3)
│   │   └── mod.rs
│   └── functions/              # Built-in helper functions (Milestone 1.2)
│       ├── count_typescript_errors.rs
│       ├── count_build_errors.rs
│       └── mod.rs
└── vendors/
    ├── claude_code.rs          # Hook PrePrompt/PostResponse
    ├── cursor.rs               # Hook PrePrompt/PostResponse
    └── acp.rs                  # Hook PrePrompt/PostResponse
```

### Testing Strategy

**Unit tests:**
- Event struct serialization/deserialization
- MessageChunk/MessageAssembler (Milestone 1.0)
- Flow parser (includes, flow action for Milestone 1.3)
- Helper function logic (error counting for Milestone 1.2)
- Task event serialization/deserialization (optional, Milestone 1.4)
- Task state reconstruction from events (optional, Milestone 1.4)

**Integration tests:**
- PrePrompt → Agent → PostResponse lifecycle
- Flow composition (includes + flow action) (Milestone 1.3)
- Task creation and querying (optional, Milestone 1.4)
- PostToolUse auto-closing tasks (optional, Milestone 1.4)

**Manual testing:**
- Real Claude Code session with PrePrompt injection
- Real build failure detection triggering PostResponse autoreplies
- Agent querying tasks with `aiki task ready --json` (optional, Milestone 1.4)
- Flow composition with aiki/core + custom flow (Milestone 1.3)

---

## Implementation Plan

### Week 1: MessageChunk/MessageAssembler & PrePrompt

**Day 1-3: MessageChunk/MessageAssembler (Milestone 1.0)**
- Phase 0: Extract actions from types.rs to actions.rs
- Create `cli/src/flows/messages.rs`
- Implement `StringOrArray`, `MessageChunk`, and `MessageAssembler`
- Implement `check_id()` method using `DefaultHasher`
- Write comprehensive unit tests
- **Deliverable:** Shared infrastructure ready for all events

**Day 4-5: PrePrompt Event (Milestone 1.1)**
- Define `PrePromptEvent` struct (owns `MessageAssembler`)
- Add event dispatch in vendors (Claude Code, Cursor, ACP)
- Implement handler in flow engine (uses MessageChunk/MessageAssembler)
- Unit tests

---

### Week 2: PostResponse Event

**Day 1-3: PostResponse Event (Milestone 1.2)**
- Define `PostResponseEvent` struct (owns `MessageAssembler`)
- Add event dispatch in vendors
- Implement handler in flow engine
- Implement helper functions (`count_typescript_errors`, `count_rust_errors`, etc.)
- Unit tests

**Day 4-5: Integration Testing**
- Test PrePrompt → Agent → PostResponse lifecycle
- Test error detection and autoreplies
- E2E tests with real agent

---

### Week 3: Flow Composition (Milestone 1.3)

**Day 1-2: Flow Composition**
- Parse `before:` and `after:` directives
- Implement flow loader with before/after support
- Implement `flow:` action type (inline invocation)
- Resolve flow paths (aiki/*, vendor/*, local)
- Detect circular dependencies
- Implement atomic flow execution (each flow runs its own before/after)
- Unit tests

**Day 3-5: Integration Testing**
- Test flow composition with multiple before/after flows
- Test atomic execution and nested flows
- Manual testing with real workflows
- Integration tests across milestones 1.0-1.3

---

### Week 4: Integration & Polish

**Day 1: Integration Testing**
- End-to-end workflow tests
- Test core features together:
  - PrePrompt injects skills
  - PostResponse detects errors and sends autoreplies
  - Flow composition with includes
- Performance testing (event dispatch overhead)

**Day 2-3: Documentation**
- API documentation for new events (PrePrompt, PostResponse)
- Flow composition guide
- Built-in helper function reference (error counting, etc.)
- Example flows for common patterns

**Day 4-5: Code Review & Cleanup**
- Code review with team
- Address feedback
- Final testing
- Merge to main

---

## Success Criteria

### Functional Requirements (Core Milestones)
- ✅ PrePrompt event fires before agent sees prompt (Milestone 1.1)
- ✅ PostResponse event fires after agent responds (Milestone 1.2)
- ✅ MessageChunk/MessageAssembler provides consistent syntax (Milestone 1.0)
- ✅ Helper functions work correctly (error counting, test parsing) (Milestone 1.2)
- ✅ Flow composition works (before/after + flow action) (Milestone 1.3)
- ✅ All integrations supported (Claude Code, Cursor, ACP)

### Functional Requirements (Optional: Task System - Milestone 1.4)
- ✅ Task system creates/queries/closes tasks via PostResponse
- ✅ Task events stored on JJ `aiki/tasks` branch
- ✅ Task state reconstruction from event log works correctly
- ✅ CLI commands work: `aiki task ready`, `aiki task create`, `aiki task start`, `aiki task close`
- ✅ Attempt-based stuck detection works (3+ failed attempts)

### Non-Functional Requirements
- ✅ Event dispatch overhead < 50ms
- ✅ Flow composition resolves in < 100ms
- ✅ No memory leaks in long-running sessions
- ✅ All tests pass (unit + integration)
- ✅ Code coverage > 80%
- ✅ Task query operations < 100ms (optional, if Milestone 1.4 implemented)

### Documentation Requirements
- ✅ Event API documentation complete (PrePrompt, PostResponse)
- ✅ MessageChunk/MessageAssembler documentation complete
- ✅ Flow composition guide complete
- ✅ Built-in helper function reference complete
- ✅ At least 3 example flows demonstrating new features
- ✅ Task system guide complete (optional, if Milestone 1.4 implemented)

---

## Dependencies

**Depends on:**
- Phase 5 (Internal Flow Engine) - Core flow system must be complete

**Enables:**
- Milestone 2 (Auto Architecture Docs) - Needs PrePrompt for injection (Milestone 1.1)
- Milestone 3 (Skills Auto-Activation) - Needs PrePrompt for injection (Milestone 1.1)
- Milestone 4 (Multi-Stage Pipeline) - Needs PostResponse for build validation (Milestone 1.2)
- Future milestones - Task system available if needed (Milestone 1.4, optional)

---

## Risks & Mitigations

### Risk 1: Event dispatch overhead affects performance
**Mitigation:** 
- Profile event dispatch early (Week 1)
- Optimize hot paths if needed
- Set performance budget: < 50ms overhead

### Risk 2: Flow composition circular dependencies hard to detect
**Mitigation:**
- Track call stack during execution
- Error immediately on duplicate flow name
- Add unit tests for common circular patterns

### Risk 3: Helper functions (error counting) are slow or unreliable
**Mitigation:**
- Cache parser output where possible
- Use streaming parsers for large output
- Add timeout limits on external commands
- Provide clear error messages when parsing fails

### Risk 4: Task event log grows too large (if Milestone 1.4 implemented)
**Mitigation:**
- Phase 1 targets <100 tasks (acceptable performance)
- Document performance characteristics clearly
- Phase 2 adds SQLite cache for scale (future work)
- Set performance budget: <100ms for task queries

---

## Next Steps After Completion

Once core milestones (1.0, 1.1, 1.2, 1.3) are complete:
1. Demo new capabilities to team
2. Gather feedback on whether task system (1.4) is needed
3. Begin Milestone 2 (Auto Architecture Documentation) using PrePrompt
4. Begin Milestone 3 (Skills Auto-Activation) using PrePrompt
5. Consider implementing Milestone 1.4 (Task System) if multi-error workflows prove unwieldy

---

## Related Documentation

- `ops/the-aiki-way.md` - Overall aiki/default vision
- `ops/ROADMAP.md` - Phase 8 overview
- `ops/phase-5.md` - Flow system foundation
- [milestone-1.0-message-assembler.md](./milestone-1.0-message-assembler.md) - MessageChunk/MessageAssembler infrastructure
- [milestone-1.1-preprompt.md](./milestone-1.1-preprompt.md) - PrePrompt event
- [milestone-1.2-post-response.md](./milestone-1.2-post-response.md) - PostResponse event
- [milestone-1.3-flow-composition.md](./milestone-1.3-flow-composition.md) - Flow composition
- [milestone-1.4-task-system.md](./milestone-1.4-task-system.md) - Task system (optional)
- `cli/src/flows/README.md` - Flow system architecture (to be created)
