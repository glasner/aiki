# AikiState Data Flow Diagram

## Overview: Event → State → Actions → JJ

```
┌─────────────────────────────────────────────────────────────────────┐
│ 1. EDITOR EVENT (e.g., file saved)                                 │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 2. VENDOR HOOK (Claude Code, Cursor, etc.)                         │
│    - Detects event                                                  │
│    - Calls: aiki hooks handle --agent claude-code                   │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 3. EVENT BUS (src/events.rs)                                       │
│    Creates AikiEvent:                                               │
│    ┌───────────────────────────────────────┐                       │
│    │ AikiEvent {                           │                       │
│    │   event_type: PostChange              │ ◄── Immutable Input   │
│    │   agent: ClaudeCode                   │                       │
│    │   session_id: "session-123"           │                       │
│    │   cwd: PathBuf("/project")            │                       │
│    │   timestamp: DateTime                 │                       │
│    │   metadata: {                         │                       │
│    │     "tool_name": "Edit",              │                       │
│    │     "file_path": "src/main.rs"        │                       │
│    │   }                                   │                       │
│    │ }                                     │                       │
│    └───────────────────────────────────────┘                       │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 4. HANDLER (src/handlers.rs)                                       │
│    handle_post_change(event: AikiEvent)                            │
│                                                                     │
│    // Validate event has required fields                           │
│    if event.session_id.is_none() { error }                         │
│    if event.metadata.get("tool_name").is_none() { error }          │
│                                                                     │
│    // Create execution state from event                            │
│    let mut state = AikiState::new(event);  ◄────┐                  │
│    state.flow_name = Some("aiki/core");         │                  │
└─────────────────────────────────────────────────┼──────────────────┘
                                 │                │
                                 ▼                │
┌────────────────────────────────────────────────┼──────────────────┐
│ 5. AIKISTATE (src/flows/types.rs)              │                  │
│    Mutable execution state:                    │                  │
│    ┌────────────────────────────────────────┐  │                  │
│    │ AikiState {                            │  │                  │
│    │   // IMMUTABLE: Original trigger       │  │                  │
│    │   event: AikiEvent { ... } ────────────┼──┘                  │
│    │                                        │                      │
│    │   // MUTABLE: Computed during execution│                     │
│    │   let_vars: {                          │ ◄── Grows during    │
│    │     // Initially empty                 │     execution       │
│    │   }                                    │                      │
│    │                                        │                      │
│    │   variable_metadata: {                 │ ◄── Action results  │
│    │     // Stores ActionResult per var    │                      │
│    │   }                                    │                      │
│    │                                        │                      │
│    │   flow_name: "aiki/core"               │ ◄── Context info    │
│    │ }                                      │                      │
│    │                                        │                      │
│    │ Helper methods:                        │                      │
│    │   cwd() -> &Path                       │ ◄── Accesses event  │
│    │   agent_type() -> AgentType            │     fields          │
│    └────────────────────────────────────────┘                      │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 6. FLOW EXECUTOR (src/flows/executor.rs)                           │
│    FlowExecutor::execute_actions(&actions, &mut state)             │
│                                                                     │
│    For each action:                                                 │
│      ┌─────────────────────────────────────────────┐               │
│      │ create_resolver(state) -> VariableResolver  │               │
│      │   ↓                                         │               │
│      │   Stringify from state.event:               │               │
│      │   • state.event.agent -> "claude-code"      │               │
│      │   • state.event.session_id -> session str   │               │
│      │   • state.event.metadata -> event.* vars    │               │
│      │   • state.let_vars -> user variables        │               │
│      │   • state.cwd() -> $cwd                     │               │
│      │   • std::env::vars() -> $HOME, $PATH, etc   │               │
│      └─────────────────────────────────────────────┘               │
│                                                                     │
│    Execute action with resolved variables                          │
│    Store result in state.let_vars & state.variable_metadata        │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│ 7. FLOW EXECUTION (flows/core.yaml)                                │
│                                                                     │
│    PostChange:                                                      │
│      - let: description = self.build_description                   │
│        │                                                            │
│        ├──► Calls build_description(aiki: &AikiState)              │
│        │    • Accesses aiki.agent_type()                           │
│        │    • Accesses aiki.event.session_id                       │
│        │    • Accesses aiki.event.metadata["tool_name"]            │
│        │    • Returns provenance description                       │
│        │                                                            │
│        └──► Stores in state.let_vars["description"]                │
│             Stores ActionResult in state.variable_metadata          │
│                                                                     │
│      - jj: describe -m "$description"                              │
│        │                                                            │
│        ├──► Variable resolved from state.let_vars                  │
│        │                                                            │
│        └──► Executes JJ command ──────────────────────────────────┼───┐
└─────────────────────────────────────────────────────────────────────┘   │
                                                                           │
                                                                           ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 8. JJ REPOSITORY (Jujutsu)                                              │
│    Change description updated with provenance:                          │
│    ┌──────────────────────────────────────────────────────────────┐    │
│    │ [aiki]                                                        │    │
│    │ agent=claude-code                                             │    │
│    │ session=session-123                                           │    │
│    │ tool=Edit                                                     │    │
│    │ confidence=High                                               │    │
│    │ method=Hook                                                   │    │
│    │ [/aiki]                                                       │    │
│    └──────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
```

## Key Data Transformations

### 1. Event Creation (Immutable)
```rust
// Editor event → AikiEvent
AikiEvent::new(PostChange, ClaudeCode, "/project")
    .with_session_id("session-123")
    .with_metadata("tool_name", "Edit")
    .with_metadata("file_path", "src/main.rs")
```

### 2. State Creation (Mutable Container)
```rust
// AikiEvent → AikiState
let mut state = AikiState::new(event);
state.flow_name = Some("aiki/core");

// State now contains:
// - state.event (immutable reference to original event)
// - state.let_vars (empty, will grow)
// - state.variable_metadata (empty, will grow)
```

### 3. Variable Resolution (Stringification)
```rust
// AikiState → VariableResolver (for $variable interpolation)
let resolver = create_resolver(state);

// Maps to strings:
state.event.agent           → $event.agent = "claude-code"
state.event.session_id      → $event.session_id = "session-123"
state.event.metadata["..."] → $event.tool_name = "Edit"
state.let_vars["..."]       → $description = "..."
state.cwd()                 → $cwd = "/project"
std::env::vars()            → $HOME, $PATH, etc.
```

### 4. Action Execution (State Mutation)
```rust
// Execute: let description = self.build_description
let result = build_description(aiki: &state);

// Store result in state:
state.let_vars["description"] = result.stdout;
state.variable_metadata["description"] = result;

// Now available for next action:
// $description → resolved from state.let_vars
```

### 5. Provenance Writing (Final Output)
```rust
// Execute: jj describe -m "$description"
// $description resolved from state.let_vars

// JJ change updated with metadata
```

## Data Flow Characteristics

### Single Source of Truth
- **AikiEvent** is created once, never modified
- Embedded in **AikiState.event** field
- All event data accessed through `state.event.*`
- No duplication or syncing needed

### State Accumulation
- **let_vars** grows as actions execute
- **variable_metadata** tracks action results
- Each action can reference previous results
- State flows through entire action chain

### Clean Separation
```
Immutable Input:  AikiEvent (what happened)
       ↓
Mutable State:    AikiState (execution context)
       ↓
Computed Output:  let_vars (results)
       ↓
Side Effects:     JJ commands (provenance storage)
```

### Type Safety
- Structured `AgentType` enum (not strings)
- Compile-time field access
- Helper methods for common patterns
- Clear ownership: Event → State

## Variable Scoping

### Event Variables (from trigger)
- Accessed via: `$event.agent`, `$event.session_id`, `$event.file_path`
- Source: `state.event.metadata`
- Immutable (from original event)

### Let Variables (computed)
- Accessed via: `$description`, `$my_var` (no event. prefix)
- Source: `state.let_vars`
- Mutable (grows during execution)

### System Variables
- Accessed via: `$cwd`, `$HOME`, `$PATH`
- Source: `state.cwd()` or `std::env::vars()`
- Dynamically fetched

## Benefits of Current Design

1. **Single Source of Truth**: Event data lives in one place
2. **Type Safety**: Structured types instead of string hashmaps
3. **Clear Flow**: Event → State → Actions → Output
4. **Immutability**: Original event never changes
5. **Traceability**: Can always see original trigger via `state.event`
6. **Testability**: Easy to construct test events with builder pattern
7. **Performance**: No unnecessary copying, minimal allocations
