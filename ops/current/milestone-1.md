# Milestone 1: Core Extensions (Phase 8 - The Aiki Way)

**Timeline:** 2-3 weeks  
**Goal:** Add event types and capabilities needed for all aiki/default patterns

---

## Overview

Milestone 1 extends Aiki's flow system (built in Phase 5) with new event types and capabilities that enable the four key patterns in aiki/default:
1. **PrePrompt event** - Inject context before agent sees prompt
2. **PostResponse event** - Validate after agent responds
3. **Flow composition** - Reuse flows via `includes:` directive
4. **Session state** - Track data across multiple events
5. **Doc management** - Create/update/query structured docs

**Why this matters:** These primitives unlock Milestone 2-6 features. Without them, we can't inject skills, cache architecture docs, run builds automatically, or manage tasks.

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

This milestone delivers six core capabilities. Each has its own detailed documentation:

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
- File path detection (reads file contents automatically)
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

**Summary:** Fire after agent completes response, enabling validation and automatic iteration.

**Key capabilities:**
- Run builds/lints automatically after AI edits
- Use `autoreply:` action to send follow-up requests to agent
- Smart stuck detection (content-based check IDs)
- Automatic scope reduction when stuck
- Max iteration limits prevent infinite loops

**Example:**
```yaml
PostResponse:
  - let: error_count = self.count_build_errors
  - if: $error_count > 0 && $event.loop_count < 3
    then:
      autoreply: "Build failed with $error_count errors. Please fix."
```

**Timeline:** Week 1-2

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

### 1.4. Session State Persistence
📄 **Detailed doc:** *(to be created)*

**Summary:** Store data across multiple events within a session.

**Key capabilities:**
- Track edited files and affected repos
- Store intermediate computation results
- Built-in helper functions (`get_edited_files`, `get_affected_repos`, etc.)
- Automatic cleanup on session end

**Example:**
```yaml
PostResponse:
  - let: repos = self.get_affected_repos
  - shell: |
      for repo in $repos; do
        cd $repo && npm run build
      done
```

**Storage:** `.aiki/.session-state/` (ephemeral, not committed)

**Timeline:** Week 2

---

### 1.5. Doc Management Action Type
📄 **Detailed doc:** *(to be created)*

**Summary:** Create, update, and query structured documentation.

**Key capabilities:**
- Create/update/append to markdown docs
- Operations: `create`, `update`, `append`, `query`
- Automatic directory creation
- Path validation for security

**Example:**
```yaml
PostResponse:
  - doc_management:
      operation: create
      path: .aiki/tasks/my-feature/plan.md
      content: |
        # Feature Plan
        Implement user authentication
```

**Timeline:** Week 3

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
├── flows/
│   ├── engine.rs               # Event dispatch, action execution
│   ├── parser.rs               # Flow YAML parsing
│   ├── loader.rs               # Flow loading and composition
│   ├── resolver.rs             # Flow path resolution
│   ├── session_state.rs        # Session state management
│   ├── handlers/
│   │   ├── pre_prompt.rs       # PrePrompt handler
│   │   ├── post_response.rs    # PostResponse handler
│   │   └── mod.rs
│   ├── actions/
│   │   ├── prepend.rs          # Prepend action (PrePrompt)
│   │   ├── append.rs           # Append action (PrePrompt)
│   │   ├── respond.rs          # Respond action (PostResponse)
│   │   ├── doc_management.rs   # Doc management action
│   │   ├── flow.rs             # Flow composition action
│   │   └── mod.rs
│   └── functions/              # Built-in helper functions
│       ├── get_edited_files.rs
│       ├── get_affected_repos.rs
│       ├── determine_repo_for_file.rs
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
- Flow parser (includes, flow action, doc_management)
- Session state persistence
- Helper function logic

**Integration tests:**
- PrePrompt → Agent → PostResponse lifecycle
- Flow composition (includes + flow action)
- Session state across multiple events
- Doc management operations

**Manual testing:**
- Real Claude Code session with PrePrompt injection
- Real build failure detection in PostResponse
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

### Week 2: PostResponse & Flow Composition

**Day 1-2: PostResponse Event (Milestone 1.2)**
- Define `PostResponseEvent` and `PostResponseResult` structs
- Implement `respond:` action
- Add event dispatch in vendors (Stop hook for Claude Code/Cursor)
- Track `files_edited` during response
- Track `loop_count` and enforce max 5 iterations
- Implement handler in flow engine
- Return followup_message to vendors
- Unit tests for respond action and loop limits

**Day 5: Integration Testing**
- End-to-end test: PrePrompt → Agent → PostResponse
- Test respond action with real agent (verify it continues working)
- Test loop_count increments correctly
- Test max iterations enforced
- Verify event data is correct
- Test across all three integrations

---

### Week 2: Flow Composition & Session State

**Day 1-2: Flow Composition**
- Parse `includes:` directive
- Implement flow loader with includes support
- Implement `flow:` action type
- Resolve flow paths (aiki/*, vendor/*, local)
- Detect circular dependencies
- Unit tests

**Day 3-4: Session State**
- Implement session state manager
- Create built-in helper functions:
  - `get_edited_files`
  - `get_affected_repos`
  - `determine_repo_for_file`
  - `count_build_errors`
- Session lifecycle (init, cleanup)
- Unit tests

**Day 5: Integration Testing**
- Test flow composition with multiple includes
- Test session state across multiple events
- Manual testing with real workflows

---

### Week 3: Doc Management & Polish

**Day 1-2: Doc Management**
- Implement `doc_management` action
- Operations: create, update, append, query
- Path validation and security
- Atomic writes
- Unit tests

**Day 3: Integration Testing**
- End-to-end workflow tests
- Test all features together:
  - PrePrompt injects skills
  - PostResponse runs build
  - Session state tracks files
  - Doc management updates tasks
- Performance testing (event dispatch overhead)

**Day 4: Documentation**
- API documentation for new events
- Flow composition guide
- Session state guide
- Built-in function reference
- Example flows

**Day 5: Code Review & Cleanup**
- Code review with team
- Address feedback
- Final testing
- Merge to main

---

## Success Criteria

### Functional Requirements
- ✅ PrePrompt event fires before agent sees prompt
- ✅ PostResponse event fires after agent responds
- ✅ Flow composition works (includes + flow action)
- ✅ Session state persists across events
- ✅ Doc management operations work (create, update, append, query)
- ✅ Built-in helper functions work correctly
- ✅ All integrations supported (Claude Code, Cursor, ACP)

### Non-Functional Requirements
- ✅ Event dispatch overhead < 50ms
- ✅ Session state operations < 10ms
- ✅ Flow composition resolves in < 100ms
- ✅ No memory leaks in long-running sessions
- ✅ All tests pass (unit + integration)
- ✅ Code coverage > 80%

### Documentation Requirements
- ✅ Event API documentation complete
- ✅ Flow composition guide complete
- ✅ Session state guide complete
- ✅ Built-in function reference complete
- ✅ At least 3 example flows demonstrating new features

---

## Dependencies

**Depends on:**
- Phase 5 (Internal Flow Engine) - Core flow system must be complete

**Enables:**
- Milestone 2 (Auto Architecture Docs) - Needs PrePrompt for injection, doc_management for cache
- Milestone 3 (Skills Auto-Activation) - Needs PrePrompt for injection
- Milestone 4 (Multi-Stage Pipeline) - Needs PostResponse for builds, session state for tracking
- Milestone 5 (Dev Docs System) - Needs doc_management for task docs
- Milestone 6 (Process Management) - Needs session state for log tracking

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

### Risk 3: Session state cleanup failures leave stale files
**Mitigation:**
- Add SessionEnd hook to all integrations
- Implement cleanup retry logic
- Add TTL-based cleanup (delete files > 24h old)

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
