# Milestone 1: Core Extensions (Phase 8 - The Aiki Way)

**Timeline:** 4 weeks  
**Goal:** Add event types and capabilities needed for all aiki/default patterns

---

## Overview

Milestone 1 extends Aiki's flow system (built in Phase 5) with new event types and capabilities that enable the three key patterns in aiki/default:
1. **PrePrompt event** - Inject context before agent sees prompt
2. **PostResponse event & Task System** - Validate after agent responds with structured task management
3. **Flow composition** - Reuse flows via `includes:` directive

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

### 1.0. MessageBuilder Shared Syntax
📄 **Detailed doc:** [milestone-1.0-message-builder.md](./milestone-1.0-message-builder.md)

**Summary:** Shared parser infrastructure for consistent syntax across all message-building events.

**Key capabilities:**
- Parse short form (`action: "string"`) and explicit form (`action: { prepend: [...], append: [...] }`)
- Detect file paths in strings and convert to absolute paths
- Generate content-based check IDs for stuck detection
- Ensure consistent behavior across PrePrompt, PostResponse, and PrepareCommitMessage

**Why this comes first:** All three events (PrePrompt, PostResponse, PrepareCommitMessage) need MessageBuilder. This is foundational infrastructure that must be completed before implementing any event handlers.

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

### 1.2. PostResponse Event & Task System
📄 **Detailed doc:** [milestone-1.2-post-response-and-tasks.md](./milestone-1.2-post-response-and-tasks.md)

**Summary:** Fire after agent completes response, enabling validation and structured task management.

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
        evidence:
          - source: typescript
            message: $error.message
            code: $error.code
  
  - if: self.ready_tasks | length > 0
    then:
      autoreply: "Run `aiki task ready --json` to see what needs fixing"
```

**Timeline:** 2-3 weeks (Phase 1 of phased implementation)

---

### 1.3. Flow Composition
📄 **Detailed doc:** [milestone-1.3-flow-composition.md](./milestone-1.3-flow-composition.md)

**Summary:** Allow flows to include and reuse other flows.

**Key capabilities:**
- Include other flows via `includes:` directive
- Invoke flows inline with `flow:` action
- Flow resolution (aiki/*, vendor/*, local paths)
- Circular dependency detection

**Example:**
```yaml
name: "My Workflow"
includes:
  - aiki/quick-lint
  - aiki/build-check

PostResponse:
  - flow: aiki/quick-lint  # Invoke inline
```

**Timeline:** Week 2

---

## Technical Architecture

### Event Flow Diagram

```
User submits prompt
       ↓
PrePromptEvent fires
       ↓
   Flow executes PrePrompt actions
   (inject skills, load cache, etc.)
       ↓
Agent receives augmented prompt
       ↓
Agent generates response
       ↓
PostResponseEvent fires
       ↓
   Flow executes PostResponse actions
   (run builds, detect patterns, etc.)
       ↓
Response shown to user
```

### Module Structure

```
cli/src/
├── events.rs                    # Event type definitions
├── tasks/                       # Task system (event-sourced)
│   ├── manager.rs              # TaskManager with JJ operations
│   ├── types.rs                # Task, TaskEvent, TaskDefinition
│   └── cli.rs                  # CLI command handlers
├── flows/
│   ├── engine.rs               # Event dispatch, action execution
│   ├── parser.rs               # Flow YAML parsing
│   ├── loader.rs               # Flow loading and composition
│   ├── resolver.rs             # Flow path resolution
│   ├── handlers/
│   │   ├── pre_prompt.rs       # PrePrompt handler
│   │   ├── post_response.rs    # PostResponse handler
│   │   └── mod.rs
│   ├── actions/
│   │   ├── message_builder.rs  # Shared MessageBuilder parser
│   │   ├── task.rs             # Task actions (create, start, close)
│   │   ├── flow.rs             # Flow composition action
│   │   └── mod.rs
│   └── functions/              # Built-in helper functions
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
- Task event serialization/deserialization
- Task state reconstruction from events
- Flow parser (includes, flow action)
- MessageBuilder parser (short/explicit forms)
- Helper function logic

**Integration tests:**
- PrePrompt → Agent → PostResponse lifecycle
- Flow composition (includes + flow action)
- Task creation and querying
- PostToolUse auto-closing tasks

**Manual testing:**
- Real Claude Code session with PrePrompt injection
- Real build failure detection creating tasks in PostResponse
- Agent querying tasks with `aiki task ready --json`
- Flow composition with aiki/core + custom flow

---

## Implementation Plan

### Week 1: MessageBuilder & Event Types

**Day 1-3: MessageBuilder (Milestone 1.0)**
- Create `cli/src/flows/actions/message_builder.rs`
- Implement `MessageBuilder` enum (Simple and Explicit variants)
- Implement `check_id()` method using SHA-256
- Implement `apply()` method
- Implement `validate()` method
- Write comprehensive unit tests
- **Deliverable:** Shared parser ready for use by all events

**Day 4-5: PrePrompt Event (Milestone 1.1)**
- Define `PrePromptEvent` struct
- Add event dispatch in vendors (Claude Code, Cursor, ACP)
- Implement handler in flow engine (uses MessageBuilder)
- Unit tests

---

### Week 2-3: PostResponse & Task System (Milestone 1.2)

**Week 2: Core Task System**
- Define `TaskEvent`, `TaskDefinition`, `Task` structs
- Implement `TaskManager` with JJ operations (event-sourced)
- Implement `aiki/tasks` branch management
- Implement event append and reconstruction
- CLI commands: `aiki task ready`, `aiki task create`, `aiki task start`, `aiki task close`
- Unit tests for task operations

**Week 3: Flow Integration & Testing**
- Implement `task:` flow actions (create, start, close, fail)
- Add `self.ready_tasks` and `self.current_task` to flow context
- Implement PostToolUse auto-closing
- Implement attempt-based stuck detection
- Integration tests: task creation from errors, auto-closing
- E2E tests with real agent workflow

---

### Week 3: Flow Composition

**Day 1-2: Flow Composition (Milestone 1.3)**
- Parse `includes:` directive
- Implement flow loader with includes support
- Implement `flow:` action type
- Resolve flow paths (aiki/*, vendor/*, local)
- Detect circular dependencies
- Unit tests

**Day 3-5: Integration Testing**
- Test flow composition with multiple includes
- Manual testing with real workflows
- Integration tests across all milestone 1 features

---

### Week 4: Integration & Polish

**Day 1: Integration Testing**
- End-to-end workflow tests
- Test all features together:
  - PrePrompt injects skills
  - PostResponse creates tasks from errors
  - Task system queries and auto-closes
  - Flow composition with includes
- Performance testing (event dispatch overhead, task queries)

**Day 2-3: Documentation**
- API documentation for new events
- Task system guide (event-sourced architecture)
- Flow composition guide
- Built-in function reference
- Example flows

**Day 4-5: Code Review & Cleanup**
- Code review with team
- Address feedback
- Final testing
- Merge to main

---

## Success Criteria

### Functional Requirements
- ✅ PrePrompt event fires before agent sees prompt
- ✅ PostResponse event fires after agent responds
- ✅ Task system creates/queries/closes tasks via PostResponse
- ✅ Task events stored on JJ `aiki/tasks` branch
- ✅ Task state reconstruction from event log works correctly
- ✅ CLI commands work: `aiki task ready`, `aiki task create`, `aiki task start`, `aiki task close`
- ✅ Attempt-based stuck detection works (3+ failed attempts)
- ✅ Flow composition works (includes + flow action)
- ✅ All integrations supported (Claude Code, Cursor, ACP)

### Non-Functional Requirements
- ✅ Event dispatch overhead < 50ms
- ✅ Task query operations < 100ms (for <100 tasks)
- ✅ Flow composition resolves in < 100ms
- ✅ No memory leaks in long-running sessions
- ✅ All tests pass (unit + integration)
- ✅ Code coverage > 80%

### Documentation Requirements
- ✅ Event API documentation complete
- ✅ Task system guide complete (event-sourced architecture)
- ✅ Flow composition guide complete
- ✅ Built-in function reference complete
- ✅ At least 3 example flows demonstrating new features

---

## Dependencies

**Depends on:**
- Phase 5 (Internal Flow Engine) - Core flow system must be complete

**Enables:**
- Phase 9 (Doc Management) - Flow action infrastructure ready
- Milestone 2 (Auto Architecture Docs) - Needs PrePrompt for injection
- Milestone 3 (Skills Auto-Activation) - Needs PrePrompt for injection
- Milestone 4 (Multi-Stage Pipeline) - Needs PostResponse for builds, task system for tracking
- Milestone 5 (Process Management) - Needs task system for process tracking

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

### Risk 3: Task event log grows too large and queries become slow
**Mitigation:**
- Phase 1 targets <100 tasks (acceptable performance)
- Phase 2 adds SQLite cache for scale
- Monitor query performance in production
- Set performance budget: <100ms for task queries

### Risk 4: Doc management security vulnerabilities (path traversal)
**Mitigation:**
- Validate all paths are within `.aiki/` directory
- Reject paths with `..` components
- Add security-focused unit tests

---

## Next Steps After Completion

Once Milestone 1 is complete:
1. Demo new capabilities to team
2. Begin Milestone 2 (Auto Architecture Documentation)
3. Write blog post: "Building Context-Aware AI Workflows"
4. Gather user feedback on new event types

---

## Related Documentation

- `ops/the-aiki-way.md` - Overall aiki/default vision
- `ops/ROADMAP.md` - Phase 8 overview
- `ops/phase-5.md` - Flow system foundation
- `cli/src/flows/README.md` - Flow system architecture (to be created)
