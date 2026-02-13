# Better Include for Plugins

**Date**: 2026-02-12
**Status**: Draft
**Priority**: P2

**Related Documents**:
- [Default Hooks](default-hooks.md) - Hookfile scaffolding and `aiki/default` plugin
- [Review Loop](review-loop.md) - First composable plugin (after-position)
- [Plugin Directory](../next/plugin-directory.md) - Future plugin directory structure
- [Flow Composition](../done/milestone-1.3-flow-composition.md) - Current before/after architecture

---

## Problem

The current `before:`/`after:` composition model has a limitation: a plugin referenced from a hookfile runs **entirely** in one position. If `hooks.yml` says `after: aiki/default`, then everything inside `aiki/default` (its before hooks, own handlers, and after hooks) all execute after the user's handlers. There's no way for a single plugin to place some logic *before* and some logic *after* the user's handlers.

This matters for `aiki/default` — the opinionated "Aiki Way" workflow bundle:

- **Before user hooks**: Context injection (`aiki/context-inject`) should run *before* the user's `turn.started` handlers so the agent has context when user handlers fire.
- **After user hooks**: Review loop (`aiki/review-loop`) should run *after* the user's `turn.completed` handlers so reviews happen after all user-defined post-turn logic.

Today, the only solution is to split `aiki/default` into two separate files — `aiki/default-before` and `aiki/default-after` — and ask users to reference both:

```yaml
# hooks.yml (ugly workaround)
before:
  - aiki/default-before    # Context injection
after:
  - aiki/default-after     # Review loop
```

This defeats the purpose of a single opinionated bundle. Users shouldn't need to know or care about the internal structure of `aiki/default`.

---

## Summary

Two changes that work together:

1. **`include:` directive** — At the top level, `include:` merges a plugin's `before:`/`after:`/handlers into the hookfile's own. Inside `before:`/`after:` blocks, `include:` runs plugins for all events in that phase. One keyword, context determines the semantics.

2. **`hook:` action + inline event handlers in `before:`/`after:`** — `before:` and `after:` are always mappings containing event-specific handlers. The `hook:` action invokes another plugin's handler for just the current event. This lets a plugin be fully self-contained without needing separate sub-plugin files for each piece of logic.

Together, `aiki/default` becomes a single self-contained file:

```yaml
# aiki/default.yml — the entire "Aiki Way" in one file
name: aiki/default

before:
  turn.started:
    - context: "Aiki project context injection"

after:
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"
```

```yaml
# .aiki/hooks.yml — one line
include:
  - aiki/default
```

No sub-plugins. No split files. The plugin declares what runs before and after the user's handlers.

---

## Current Behavior

For reference, here's how the existing composition model works:

### Execution Order (Today)

When the engine processes an event for a hookfile:

```
1. before[0]  → (load plugin, run its before → own → after)
2. before[1]  → (load plugin, run its before → own → after)
3. This hookfile's own handlers
4. after[0]   → (load plugin, run its before → own → after)
5. after[1]   → (load plugin, run its before → own → after)
```

Each plugin in `before:` or `after:` is loaded and composed recursively. But the entire plugin — including anything it composes internally — runs in one position.

### The Gap

If `aiki/default` has:

```yaml
before:
  include:
    - aiki/context-inject
after:
  include:
    - aiki/review-loop
```

And `hooks.yml` says `after: { include: [aiki/default] }`, the execution is:

```
1. hooks.yml's own handlers
2. aiki/context-inject           ← Runs AFTER user handlers (wrong!)
3. aiki/default's own handlers
4. aiki/review-loop              ← Runs after user handlers (correct)
```

`aiki/context-inject` is trapped inside `aiki/default`'s after-position execution. It can never run before the user's handlers.

---

## Change 1: Inline Event Handlers in `before:`/`after:` + `hook:` Action

### The Sub-Plugin Problem

Without inline handlers, every piece of before/after logic requires a separate plugin file:

```yaml
# aiki/default.yml — delegates everything to sub-plugins
before:
  include:
    - aiki/context-inject     # Separate file just for context injection
after:
  include:
    - aiki/review-loop        # Separate file just for review triggering
```

This pushes the "split into two files" problem down one level. The author of `aiki/default` needs three files to express what should be one self-contained plugin. Worse, as `aiki/default` grows (lint gates, build checks, etc.), every new behavior needs yet another sub-plugin file.

### Solution: `before:`/`after:` as Composition Blocks

`before:` and `after:` are always **mappings** (never lists) containing two kinds of keys:

- **`include:`** — a list of plugins to run for all events in this phase
- **`<event>:`** — inline handler list for that specific event (with `hook:` actions for per-event plugin calls)

```yaml
# aiki/default.yml — fully self-contained
name: aiki/default

before:
  turn.started:
    - context: "Aiki project context"
  session.started:
    - log: "Aiki Way enabled"

after:
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"
```

The `before:` block's handlers run before the includer's handlers. The `after:` block's handlers run after. No sub-plugin files needed.

### The `hook:` Action

`hook:` is an action (like `context:`, `shell:`, `if:`) that invokes another plugin's handler for the current event. It works anywhere in a handler list — in before/after blocks or in own handlers:

```yaml
# In before block — run a plugin's handler for a specific event
before:
  turn.started:
    - hook: myorg/pre-check         # Run pre-check's turn.started handler
    - context: "Extra context"      # Then inline handler

# In own handlers — interleave plugin calls with inline handlers
turn.started:
  - hook: aiki/context-inject       # Run its turn.started handler
  - context: "My custom context"    # Then my own

# In after block
after:
  turn.completed:
    - hook: aiki/review-loop        # Run review-loop's turn.completed handler
```

`hook:` is fine-grained: it runs a single plugin's handler for just the current event. This contrasts with `include:` inside before/after, which runs plugins for all events in that phase.

### Syntax

`before:` and `after:` are always mappings with optional keys:

**Inline handlers only:**
```yaml
before:
  turn.started:
    - context: "Injected before user handlers"
  session.started:
    - log: "Before session"
```

**Include only (plugins for all events):**
```yaml
before:
  include:
    - myorg/pre-check
    - myorg/lint-gate
```

**Mixed (include + inline handlers):**
```yaml
before:
  include:
    - myorg/pre-check
  turn.started:
    - hook: myorg/special-check
    - context: "Extra context"
```

### Execution Model

For a given event, the composer walks three phase lists. Each list may contain multiple **blocks** (from includes + the hookfile's own). Each block is atomic — its includes run, then its inline handlers, before moving to the next block.

```
1. Walk before blocks in order. For each block:
   a. Run include plugins (compose recursively)
   b. Run inline handlers for event
      (composer handles hook: actions; self.* set to block's source hook)
2. Walk own handler segments in order:
   For each segment, set self.* to segment's source hook, then execute statements
3. Walk after blocks in order. For each block:
   a. Run include plugins (compose recursively)
   b. Run inline handlers for event
      (composer handles hook: actions; self.* set to block's source hook)
```

---

## Change 2: `include:` Directive

### Semantics

`include:` has two uses depending on context:

**Top-level `include:`** — structural composition. `include: foo` means: **prepend foo's composition blocks and handler segments before mine, preserving provenance boundaries.**

Concretely, when a hookfile includes a plugin:

1. The plugin's `before:` blocks are prepended to the hookfile's before block list (as separate blocks, not merged)
2. The plugin's `after:` blocks are prepended to the hookfile's after block list (as separate blocks, not merged)
3. The plugin's own handler segments are prepended to the hookfile's handler segment list (as a separate segment, not concatenated)
4. The plugin's `include:` list is expanded recursively (transitive)

```
Effective hookfile after include expansion:

  before   = [...include.before_blocks, self.before_block]   ← blocks sequenced
  handlers = [...include.handler_segments, self.handlers]    ← segments sequenced
  after    = [...include.after_blocks, self.after_block]     ← blocks sequenced
```

Each block and segment retains its source hook identity for correct `self.*` resolution. Blocks are not merged — they execute in sequence, each as an atomic unit (includes first, then inline handlers).

Includes are the "inherited" base configuration. Explicits customize.

**`include:` inside `before:`/`after:`** — run these plugins for all events in this phase. The plugins are loaded and composed recursively (their before → own → after all execute in the enclosing phase).

The context disambiguates: top-level `include:` composes structure, nested `include:` inside a composition block runs plugins in that phase.

### Full Example

```yaml
# aiki/default.yml — self-contained plugin
name: aiki/default
description: "The opinionated Aiki Way"

before:
  turn.started:
    - context: "Aiki project context"

after:
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"

session.started:
  - log: "Aiki Way enabled"
```

```yaml
# .aiki/hooks.yml
include:
  - aiki/default

before:
  include:
    - myorg/pre-check
  turn.started:
    - hook: myorg/special-check

turn.started:
  - context: "My custom context"
```

**Effective hookfile after include expansion (block lists, not merged YAML):**

```
before_blocks = [
  # Block 0 — from include (aiki/default's before block):
  { source: "aiki/default", include: [], turn.started: [context: "Aiki project context"] },
  # Block 1 — hookfile's own before block:
  { source: "hooks", include: [myorg/pre-check], turn.started: [hook: myorg/special-check] },
]

handler_segments = [
  # Segment 0 — from include (aiki/default's own handlers):
  { source: "aiki/default", session.started: [log: "Aiki Way enabled"] },
  # Segment 1 — hookfile's own handlers:
  { source: "hooks", turn.started: [context: "My custom context"] },
]

after_blocks = [
  # Block 0 — from include (aiki/default's after block):
  { source: "aiki/default", turn.completed: [if: $event.turn.tasks.completed, then: [autoreply: "aiki review..."]] },
  # Block 1 — hookfile's own after block (empty):
]
```

**Execution order for `turn.started`:**

```
1. context: "Aiki project context"  (before_blocks[0] inline — from aiki/default)
2. myorg/pre-check                  (before_blocks[1] include — from hooks)
3. hook: myorg/special-check        (before_blocks[1] inline — from hooks)
4. context: "My custom context"     (handler_segments[1] — from hooks)
5. (after has no turn.started)
```

**Execution order for `turn.completed`:**

```
1. (before blocks have no turn.completed)
2. (no handler segments for turn.completed)
3. autoreply: "aiki review..."      (after_blocks[0] inline — from aiki/default)
```

Context injection runs before the user's handlers. Review loop runs after. One `include:` reference, one plugin file.

---

## Detailed Semantics

### Full Execution Order

For a given event, the composer processes three phase lists in order. Each list may contain multiple blocks/segments (from includes + the hookfile's own). Each block is atomic:

```
1. Walk before blocks [include_1.before, include_2.before, ..., self.before]:
   For each block:
     a. Run block.include plugins (compose recursively)
     b. Run block's inline handlers for event
        (composer intercepts hook: actions; self.* set to block's source hook)
2. Walk own handler segments [include_1.handlers, include_2.handlers, ..., self.handlers]:
   For each segment:
     a. Set self.* to segment's source hook
     b. Execute statements (engine handles actions, composer intercepts hook:)
3. Walk after blocks [include_1.after, include_2.after, ..., self.after]:
   For each block:
     a. Run block.include plugins (compose recursively)
     b. Run block's inline handlers for event
        (composer intercepts hook: actions; self.* set to block's source hook)
```

With top-level `include:` expansion, the included plugin's blocks and segments are prepended to the hookfile's lists as separate entries (not merged into the hookfile's own block).

### Ordering Rules

| Source | Before list | Handler list | After list |
|--------|------------|-------------|-----------|
| `include:` (first) | Block prepended | Segment prepended | Block prepended |
| `include:` (second) | Block after first | Segment after first | Block after first |
| Explicit `before:` | Last block | — | — |
| Own handlers | — | Last segment | — |
| Explicit `after:` | — | — | Last block |

**Mnemonic**: Includes set the base. Explicits customize. Blocks never merge across provenance boundaries.

### Multiple Includes

```yaml
include:
  - aiki/default      # before has turn.started context, after has turn.completed review
  - myorg/standard    # before has session.started logging, after has turn.completed notify
```

Expansion (includes stack in declaration order — blocks prepended, not merged):

```
before_blocks = [
  # aiki/default's before block (first include)
  { source: "aiki/default", turn.started: [context: "Aiki project context"] },
  # myorg/standard's before block (second include)
  { source: "myorg/standard", session.started: [log: "Standard logging enabled"] },
  # hookfile's own before block (if any)
]

after_blocks = [
  # aiki/default's after block (first include)
  { source: "aiki/default", turn.completed: [if: ..., then: [autoreply: "aiki review..."]] },
  # myorg/standard's after block (second include)
  { source: "myorg/standard", turn.completed: [shell: "notify-send 'Turn completed'"] },
  # hookfile's own after block (if any)
]
```

When both includes contribute handlers for the same event in the same phase, they execute in include order — each in its own block with its own `self.*` context.

### Transitive Includes

Plugins can include other plugins:

```yaml
# aiki/default.yml
include:
  - aiki/base

before:
  turn.started:
    - context: "Aiki context"

after:
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"
```

```yaml
# aiki/base.yml
before:
  session.started:
    - log: "Session tracking enabled"
```

**Effective aiki/default after expansion:**

```yaml
before:
  # From aiki/base (transitive):
  session.started:
    - log: "Session tracking enabled"
  # Own:
  turn.started:
    - context: "Aiki context"

after:
  # Own:
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"
```

### Cycle Detection

Include expansion and `hook:` invocation both use the existing cycle detection in `HookComposer`'s call stack. Circular includes or `hook:` references produce `AikiError::CircularHookDependency`:

```yaml
# INVALID: circular include
# aiki/a.yml → include: aiki/b
# aiki/b.yml → include: aiki/a
# Error: Circular hook dependency detected: aiki/a → aiki/b → aiki/a
```

### Interaction Between All Directives

`include:`, `before:`, and `after:` can all coexist:

```yaml
include:
  - aiki/default           # Expand into before/handlers/after

before:
  include:
    - myorg/pre-check          # Plugin for all events in before phase
  turn.started:
    - hook: myorg/special-check    # Per-event plugin call
    - context: "Pre-turn"          # Inline before handler

after:
  turn.completed:
    - log: "Post-turn"         # Inline after handler

turn.started:
  - context: "My stuff"       # Own handler
```

### Handler Merging (Include)

When an included plugin has own handlers for the same event as the includer:

```yaml
# aiki/default.yml
session.started:
  - log: "Default loaded"

# .aiki/hooks.yml
include:
  - aiki/default

session.started:
  - log: "User session started"
```

Both handler lists are concatenated. The included plugin's handlers run first:

```
session.started:
  1. log: "Default loaded"         (from include)
  2. log: "User session started"   (own)
```

### Before/After Block Ordering

Composition blocks from different sources are **never merged**. They remain as separate entries in the before/after block list. Each block executes atomically (its includes, then its inline handlers) before the next block begins:

```
# After expanding two includes, before block list is:
before_blocks = [
  CompositionBlock { source: "first-include", turn.started: [context: "From first include"] },
  CompositionBlock { source: "second-include", turn.started: [context: "From second include"] },
  CompositionBlock { source: "self", ... },   # hookfile's own before block
]
```

The composer walks the list in order. For `turn.started`, block 0 runs its includes then `context: "From first include"`, then block 1 runs its includes then `context: "From second include"`, etc. Each block's inline handlers have `self.*` set to the block's source hook.

### `hook:` Action Semantics

The `hook:` action invokes a plugin's handler for the current event only:

```yaml
turn.started:
  - hook: aiki/context-inject    # Runs aiki/context-inject's turn.started handlers
  - context: "My stuff"          # Runs after
```

If `aiki/context-inject` has no `turn.started` handler, the `hook:` action is a no-op.

`hook:` is handled by the **composer** (not the engine), because it requires the composer's `HookLoader` and call stack for cycle detection. When the composer encounters a `hook:` statement while walking inline handlers:

1. Load the referenced plugin via `HookLoader`
2. Push the plugin onto the composer's call stack (cycle detection)
3. Save current `state.hook_name` and variables, clear variables, set `hook_name` to the target plugin's identity
4. Get the plugin's own handlers for the current event type
5. Execute those handlers via the **composer** (not the engine directly) so that any nested `hook:` actions inside the target plugin are intercepted and handled correctly. No before/after — just the own handlers.
6. Restore caller's `state.hook_name` and variables (**unconditionally**, even on error — see Error Safety below)
7. Pop from call stack

The plugin's `before:`/`after:` blocks are **not** executed — only its own handlers for the current event.

**State isolation policy:** `hook:` isolates some `AikiState` fields and intentionally shares others. The full policy:

| Field | Behavior | Rationale |
|-------|----------|-----------|
| `let_vars` | **Isolated** — cleared before target, restored after | Variables are scoping artifacts. Leaking them would break caller's variable resolution. |
| `variable_metadata` | **Isolated** — cleared with `let_vars` via `clear_variables()` | Metadata follows its variables. |
| `hook_name` | **Isolated** — saved/restored around invocation | `self.*` must resolve to the target plugin during execution, then revert. |
| `context_assembler` | **Shared** — target's `context:` actions contribute to the caller's assembled prompt | This is the point: `hook: aiki/context-inject` exists to add context to the caller's prompt/autoreply. Isolating it would make `hook:` useless for context injection. |
| `failures` | **Shared** — target's failures accumulate into the caller's failure list | A failure inside a `hook:` target is a real failure of the overall execution. Isolating would silently swallow errors. |
| `pending_session_ends` | **Shared** — target can register PIDs for deferred termination | These are global side-effects executed once after all hooks complete. Isolating would lose termination requests. |
| `event` | **Shared** (immutable) — the triggering event is read-only, never modified | No isolation needed. |

**Nested `hook:` actions:** Because `hook:` executes the target plugin's handlers through `execute_statements_with_hooks` (not `HookEngine` directly), any `hook:` actions inside the target plugin's handlers are intercepted and handled correctly. This enables arbitrary nesting: plugin A can `hook:` plugin B, which can `hook:` plugin C, with cycle detection at every level.

**Error safety:** State cleanup (restore variables, restore `hook_name`, pop call stack) is **unconditional** — it runs even when the target plugin's handlers return an error. This prevents leaked call stack entries, stale `state.hook_name`, and variable drift after failures. The same unconditional cleanup applies to `execute_composition_block`'s `hook_name` save/restore.

---

## What `include:` Is NOT

- **Not `before:`** — `before:` scopes execution to the before phase. Include at the top level expands the plugin's composition into your own.
- **Not `after:`** — Same distinction. `after:` scopes to the after phase.
- **Not handler overriding** — Include merges handlers, it doesn't replace them. There's no way for an included plugin to suppress the includer's handlers.
- **Not middleware** — There's no "next()" or "yield" concept. Include is structural composition — the plugin's blocks and handler segments are prepended to the hookfile's lists at load time, preserving provenance boundaries for correct `self.*` resolution.

---

## Implementation

### Phase 1: Type Changes — Composition Blocks, Handler Segments, `hook:` Action

Change `before:` and `after:` from `Vec<String>` to `Vec<CompositionBlock>`. Add `HandlerSegment` for own handlers with provenance. Add `hook:` as a new `HookStatement` variant.

**File:** `cli/src/flows/types.rs`

```rust
/// A composition block used in before/after positions.
/// Always a mapping with optional `include:` (plugins for all events)
/// and event-specific inline handler lists.
/// Each block retains its source hook identity for self.* resolution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompositionBlock {
    /// The hook identity this block came from (for self.* resolution in inline handlers).
    /// Set during include expansion; None for the hookfile's own block (resolved at execution time).
    #[serde(skip)]
    pub source_hook: Option<String>,

    /// Plugin references to run for all events in this phase
    #[serde(default)]
    pub include: Vec<String>,

    // Event handlers
    #[serde(rename = "session.started", default)]
    pub session_started: Vec<HookStatement>,
    #[serde(rename = "session.resumed", default)]
    pub session_resumed: Vec<HookStatement>,
    // ... all other event handler fields ...
    #[serde(rename = "turn.started", default)]
    pub turn_started: Vec<HookStatement>,
    #[serde(rename = "turn.completed", default)]
    pub turn_completed: Vec<HookStatement>,
    // ... etc
}

/// A segment of own handlers tagged with their source hook identity.
/// Preserves self.* context when handlers from different plugins are
/// sequenced together via top-level include expansion.
///
/// Stores the full Hook so that the correct event's handlers can be
/// selected at execution time (via event_type.get_statements()).
/// This avoids the need for a get_all_statements() method that would
/// conflate "all events" with "current event."
#[derive(Debug, Clone)]
pub struct HandlerSegment {
    /// The hook identity for self.* resolution
    pub source_hook: String,
    /// The included hook (handlers selected per-event at execution time)
    pub hook: Hook,
}

/// Add to the HookStatement enum:
pub enum HookStatement {
    // ...existing variants...

    /// Invoke another plugin's handler for the current event.
    /// Handled by the composer (not the engine) because it requires
    /// the composer's HookLoader and call stack for cycle detection.
    Hook { hook: String },
}

pub struct Hook {
    pub name: String,
    // ...existing fields...

    /// Plugins to include (expand their blocks/segments into this hook's lists)
    #[serde(default)]
    pub include: Vec<String>,

    /// Composition block list: blocks run before this hook's own handlers.
    /// Vec because include expansion prepends blocks without merging.
    #[serde(default, deserialize_with = "deserialize_single_as_vec")]
    pub before: Vec<CompositionBlock>,

    /// Composition block list: blocks run after this hook's own handlers.
    /// Vec because include expansion prepends blocks without merging.
    #[serde(default, deserialize_with = "deserialize_single_as_vec")]
    pub after: Vec<CompositionBlock>,

    // ...event handlers (deserialized normally, wrapped into HandlerSegment at load time)...
}
```

**Deserialization note:** YAML hookfiles write `before:` as a single mapping (one `CompositionBlock`). The custom deserializer wraps it into a `Vec<CompositionBlock>` with one entry. Include expansion prepends additional blocks to the vec.

> **Note:** `CompositionBlock` shares the same event handler fields as `Hook`. Consider extracting a shared `EventHandlers` trait or struct to avoid duplication. Alternatively, `CompositionBlock` could embed a `Hook` with metadata fields ignored.

### Phase 2: `hook:` Action Handling in HookComposer

`hook:` is a **composer-level concern**, not an engine concern. The engine remains stateless (no loader, no event_type, no call stack). The composer intercepts `Hook` statements while walking inline handlers and own handler segments.

**File:** `cli/src/flows/composer.rs`

Add a method that walks a statement list, delegating normal statements to the engine and handling `hook:` itself:

```rust
/// Execute statements, intercepting hook: actions.
/// Normal statements go to HookEngine. Hook statements are handled here
/// because they require the loader, call stack, and event_type context.
fn execute_statements_with_hooks(
    &mut self,
    statements: &[HookStatement],
    event_type: EventType,
    state: &mut AikiState,
) -> Result<HookOutcome> {
    for statement in statements {
        let result = match statement {
            HookStatement::Hook { hook: plugin_path } => {
                self.execute_hook_action(plugin_path, event_type, state)?
            }
            other => {
                // Delegate to engine (single statement)
                HookEngine::execute_statements(std::slice::from_ref(other), state)?
            }
        };
        match result {
            HookOutcome::Success => {}
            HookOutcome::FailedContinue => { /* track, continue */ }
            HookOutcome::FailedStop | HookOutcome::FailedBlock => return Ok(result),
        }
    }
    Ok(HookOutcome::Success)
}

/// Execute a hook: action — surgical invocation of a plugin's own handlers.
/// Variable-isolated: the target plugin gets a clean variable scope,
/// consistent with how composed flow boundaries clear variables.
fn execute_hook_action(
    &mut self,
    plugin_path: &str,
    event_type: EventType,
    state: &mut AikiState,
) -> Result<HookOutcome> {
    // 1. Load plugin (HookLoader resolves path)
    let (plugin, canonical_path) = self.loader.load(plugin_path)?;

    // 2. Cycle detection (composer's call stack)
    if self.call_stack.contains(&canonical_path) {
        return Err(AikiError::CircularHookDependency { ... });
    }
    self.call_stack.push(canonical_path.clone());

    // 3. Isolation: save/clear scoped fields, leave shared fields untouched.
    //    Isolated: let_vars, variable_metadata (via clear_variables), hook_name.
    //    Shared:   context_assembler, failures, pending_session_ends, event.
    //    See "State isolation policy" table in the design doc for rationale.
    let saved_hook_name = state.hook_name.take();
    let saved_variables = state.save_variables();
    state.clear_variables();
    state.hook_name = Some(Self::extract_flow_identifier(&canonical_path));

    // 4. Get plugin's own handlers for current event (no before/after).
    //    Use execute_statements_with_hooks (not HookEngine directly) so that
    //    any hook: actions inside the target plugin's handlers are intercepted.
    let statements = event_type.get_statements(&plugin);
    let result = if !statements.is_empty() {
        self.execute_statements_with_hooks(statements, event_type, state)
    } else {
        Ok(HookOutcome::Success)
    };

    // 5. Restore caller's context and variables (unconditionally — even on error).
    //    This prevents leaked call_stack entries and stale state after failures.
    state.restore_variables(saved_variables);
    state.hook_name = saved_hook_name;
    self.call_stack.pop();

    result
}
```

### Phase 3: Composition Block List Execution in HookComposer

Update `HookComposer::execute_composed_flow()` to walk `Vec<CompositionBlock>` and `Vec<HandlerSegment>`:

**File:** `cli/src/flows/composer.rs`

```rust
fn execute_composed_flow(
    &mut self,
    hook: &Hook,
    canonical_path: &Path,
    event_type: EventType,
    state: &mut AikiState,
) -> Result<HookOutcome> {
    // 1. Walk before blocks in order
    for block in &hook.before {
        let outcome = self.execute_composition_block(block, canonical_path, event_type, state)?;
        if outcome.should_stop() { return Ok(outcome); }
    }

    // 2. Walk own handler segments in order
    for segment in &hook.handler_segments {
        // Select handlers for the current event only (not all events)
        let statements = event_type.get_statements(&segment.hook);
        if statements.is_empty() { continue; }

        state.clear_variables();
        state.hook_name = Some(segment.source_hook.clone());
        let outcome = self.execute_statements_with_hooks(
            statements, event_type, state
        )?;
        if outcome.should_stop() { return Ok(outcome); }
    }

    // 3. Walk after blocks in order
    for block in &hook.after {
        let outcome = self.execute_composition_block(block, canonical_path, event_type, state)?;
        if outcome.should_stop() { return Ok(outcome); }
    }

    Ok(HookOutcome::Success)
}

/// Execute a single composition block: includes first, then inline handlers.
fn execute_composition_block(
    &mut self,
    block: &CompositionBlock,
    canonical_path: &Path,
    event_type: EventType,
    state: &mut AikiState,
) -> Result<HookOutcome> {
    // 1. Run included plugins for all events (compose recursively)
    for plugin_path in &block.include {
        let outcome = self.compose_hook(plugin_path, event_type, state)?;
        if outcome.should_stop() { return Ok(outcome); }
    }

    // 2. Run inline handlers for this event type
    let statements = event_type.get_statements(block);
    if !statements.is_empty() {
        // Variable isolation: clear variables before inline handlers, matching
        // the clear_variables() done for handler segments. Without this, state
        // from earlier blocks (or include plugins above) leaks into inline handlers.
        state.clear_variables();

        // Set self.* context for inline handlers (save/restore unconditionally)
        let saved_hook_name = state.hook_name.take();
        state.hook_name = block.source_hook.clone()
            .or_else(|| Some(Self::extract_flow_identifier(canonical_path)));

        let result = self.execute_statements_with_hooks(statements, event_type, state);

        // Restore unconditionally — even on error — to prevent stale hook_name.
        state.hook_name = saved_hook_name;

        let outcome = result?;
        if outcome.should_stop() { return Ok(outcome); }
    }

    Ok(HookOutcome::Success)
}
```

The execution order for a hook becomes:

```
1. Walk before blocks (each: include plugins → inline handlers for event)
2. Walk own handler segments (each: set self.* → execute statements)
3. Walk after blocks (each: include plugins → inline handlers for event)
```

### Phase 4: Include Expansion

Add include expansion to `HookComposer::compose_hook()`. Expansion **prepends** blocks and segments — it never merges or flattens.

**File:** `cli/src/flows/composer.rs`

When a hook has a non-empty top-level `include:` list:

1. For each included plugin (in **reverse** order, so the first-declared include ends up first in the list after prepending):
   a. Load the included plugin via `HookLoader`
   b. Push onto call stack (cycle detection via `HookComposer`, not `HookLoader`)
   c. Recursively expand any nested includes
   d. Prepend the included plugin's `before:` blocks to the current hook's `before:` list
   e. Prepend the included plugin's `after:` blocks to the current hook's `after:` list
   f. Prepend the included plugin's own handlers as a `HandlerSegment` to the current hook's segment list
   g. Pop from call stack
2. Execute the expanded hook normally

**Ordering invariant:** Reverse iteration ensures that `include: [a, b]` produces `[a.blocks, b.blocks, self.blocks]` — matching declaration order.

**Prepending blocks (no merging):**

```rust
/// Expand top-level includes into the hook's block/segment lists.
/// Processes includes in REVERSE order so that prepending preserves
/// declaration order: include: [a, b] → [a.blocks, b.blocks, self.blocks].
fn expand_includes(
    hook: &mut Hook,
    includes: &[(Hook, PathBuf)],  // already loaded in declaration order
) {
    for (included, included_canonical_path) in includes.iter().rev() {
        Self::prepend_included(hook, included, included_canonical_path);
    }
}

fn prepend_included(
    hook: &mut Hook,
    included: &Hook,
    included_canonical_path: &Path,
) {
    let source_hook = Self::extract_flow_identifier(included_canonical_path);

    // Prepend before blocks (tag each with source)
    let mut included_before = included.before.clone();
    for block in &mut included_before {
        block.source_hook.get_or_insert_with(|| source_hook.clone());
    }
    hook.before.splice(0..0, included_before);

    // Prepend after blocks (tag each with source)
    let mut included_after = included.after.clone();
    for block in &mut included_after {
        block.source_hook.get_or_insert_with(|| source_hook.clone());
    }
    hook.after.splice(0..0, included_after);

    // Prepend own handlers as a HandlerSegment.
    // Clone the included hook's per-event handler maps as-is;
    // the segment is filtered to the current event at execution time.
    hook.handler_segments.insert(0, HandlerSegment {
        source_hook: source_hook.clone(),
        hook: included.clone(),
    });
}
```

### Phase 5: Update Default Hookfile Template

**File:** `cli/src/commands/init.rs` (or wherever the hookfile template is embedded)

Change the scaffolded `hooks.yml` from:

```yaml
after:
  - aiki/default
```

To:

```yaml
include:
  - aiki/default
```

### Phase 6: Tests

Add tests in `cli/src/flows/composer.rs`:

**Composition Block tests:**
1. **Include only** — `before: { include: [a, b] }` runs plugins in order for all events
2. **Inline only** — `before: { turn.started: [...] }` runs inline handlers for that event
3. **Mixed** — `before: { include: [a], turn.started: [{ hook: b }] }` runs include plugins first, then inline handlers
4. **Empty block** — No-op

**`hook:` action tests (in composer):**
5. **Basic hook:** — Invokes plugin's handler for current event
6. **hook: with no matching handler** — Plugin has no handler for event, no-op
7. **hook: does not run before/after** — Only the plugin's own handlers execute
8. **hook: interleaved** — `hook:` and inline actions execute in declaration order
9. **hook: in own handlers** — Works outside composition blocks too
10. **hook: cycle detection** — Circular `hook:` references produce `CircularHookDependency`
11. **hook: self.* context** — `self.*` resolves against the target plugin during `hook:` execution, not the caller

**Include tests:**
12. **Basic include** — Include a plugin with inline before/after, verify execution order
13. **Multiple includes** — First include's blocks come first
14. **Transitive includes** — Plugin includes another plugin, verify full expansion
15. **Include + explicit before/after** — Include contributions come first
16. **Handler segments** — Same event in includer and included, verify each runs with correct `self.*` context
17. **Circular include** — Verify `CircularHookDependency` error
18. **Include of plugin with no include field** — Backwards compatibility

**`self.*` context tests:**
19. **Inline handlers in before block** — `self.*` resolves to the block's source hook, not the hookfile
20. **Included handler segments** — Each segment's `self.*` resolves to the included plugin, not the hookfile
21. **hook: context switch** — `self.*` is saved/restored around `hook:` invocation
22. **Multiple includes with self.*** — Each include's handlers resolve `self.*` independently

**Variable isolation tests:**
23. **hook: variable isolation** — Variables set by the target plugin do not leak back to the caller
24. **hook: caller variables restored** — Caller's variables are fully restored after `hook:` returns
25. **handler segment variable isolation** — Each segment starts with a clean variable scope (via `clear_variables()`)

---

## Migration

### Existing `before:`/`after:` List Form

The old list form for `before:`/`after:` is removed with no compatibility shim:

```yaml
# Old (no longer supported — will fail to parse)
before:
  - aiki/context-inject
  - myorg/pre-check

# New
before:
  include:
    - aiki/context-inject
    - myorg/pre-check
```

This is a clean break. Existing hookfiles using `before: [plugin-list]` must migrate to `before: { include: [plugin-list] }`. The migration is mechanical — wrap the list in an `include:` key. No compatibility parsing or deprecation warnings; the old form produces a deserialization error with a clear message.

### Existing `after: aiki/default` Pattern

The default hookfile template in `default-hooks.md` uses:

```yaml
after:
  - aiki/default
```

This migrates to:

```yaml
include:
  - aiki/default
```

Using top-level `include:` instead of `after:` unlocks the before/after splitting within `aiki/default`.

### `aiki/default` Plugin Update

Once `include:` is implemented, `aiki/default` can change from:

```yaml
# Old: everything runs in one position
after:
  include:
    - aiki/review-loop
```

To:

```yaml
# New: before and after relative to the includer
before:
  turn.started:
    - context: "Aiki project context"

after:
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"
```

Users with `include: aiki/default` get both behaviors automatically.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Composition block format | Always a mapping (never a list) | One parsing path, simpler types. `include:` key handles the "plugins for all events" case that the old list form covered. |
| Inline handlers | `before:`/`after:` contain event-keyed handler lists | Avoids sub-plugin proliferation. Plugin authors can express before/after logic in one file. |
| Top-level keyword | `include:` | Well-understood from YAML config ecosystems (Docker Compose, GitHub Actions). Clear meaning: "merge this into my config." Structural expansion semantics match prior art. |
| Nested `include:` in before/after | Same keyword, different context | Top-level `include:` does structural expansion. Nested `include:` inside a composition block runs plugins in that phase. The context (top-level vs inside before/after) disambiguates naturally: "include this plugin" vs "in the before phase, include these plugins." |
| Plugin invocation | `hook:` action | Domain-native keyword — these are hooks. Works as an action alongside `context:`, `shell:`, `if:`. Leaves `call:` free for future use (HTTP webhooks, reusable macros). Chosen over `use:` (too generic), `call:` (future collision risk), `run:` (conflicts with shell execution). |
| `hook:` scope | Current event only, no before/after | Surgical invocation: "run this plugin's handler for this event." Running the plugin's full composition would be confusing and redundant with `include:`. |
| `hook:` ownership | Composer, not engine | `hook:` requires the loader, call stack (cycle detection), and event_type — all composer state. The engine remains stateless. |
| Block merging | Never — blocks stay separate | Flattening destroys provenance boundaries needed for `self.*` resolution. `Vec<CompositionBlock>` preserves ordering guarantees across include expansion. |
| Handler segments | Tagged with source hook | Own handlers from includes become `HandlerSegment { source_hook, statements }`. Each segment sets `self.*` to its source hook before execution. |
| Include expansion | Prepend blocks/segments, don't merge | Included plugin's before blocks prepend to the before list, after blocks prepend to the after list, own handlers become a HandlerSegment prepended to the handler list. Natural inheritance model. |
| Own handlers | Included as segments | A plugin can contribute both composition (before/after) and direct handlers. No need for sub-plugins. |
| Transitive | Yes | Plugins should be able to compose other plugins. Keeps the model uniform. |
| Backwards compat | Clean break, no compat shim | Existing `before: [list]` must migrate to `before: { include: [list] }`. Mechanical migration. No compatibility parsing or deprecation warnings — old form produces a clear deserialization error. |

### Why Not Require Sub-Plugins?

The alternative to inline handlers is requiring separate plugin files for every piece of before/after logic:

```
aiki/default.yml          → composition manifest only
aiki/context-inject.yml   → just the turn.started context handler
aiki/review-loop.yml      → just the turn.completed review handler
```

This works but has real costs:
- **File proliferation**: Every new behavior in `aiki/default` needs a new file
- **Indirection**: Reading `aiki/default` doesn't show you what it does — you have to open N sub-files
- **Naming pressure**: Each piece of logic needs a globally unique plugin name
- **Barrier to contribution**: Adding one handler to a plugin means creating a new file, naming it, and referencing it

Inline handlers eliminate this for simple cases. Sub-plugins remain available for complex, reusable logic that benefits from its own file.

---

## Open Questions

1. **Should `include:` be available in all hookfiles, or only the root `hooks.yml`?** If a `before:` plugin uses `include:`, the include expansion happens inside that plugin's scope. This works naturally but might be surprising. Could restrict to root-level only for simplicity.

2. **Should there be an `exclude:` to remove specific plugins from an include?** E.g., "include aiki/default but without the review loop." Useful for power users but adds complexity. Could be deferred.
   ```yaml
   include:
     - aiki/default:
         without:
           - aiki/review-loop   # Don't include the review loop
   ```

3. **Should `CompositionBlock` share event handler fields with `Hook` via a trait?** Both structs have identical event handler fields (plus `CompositionBlock` adds `source_hook`). A shared `EventHandlers` trait or embedded struct would reduce duplication. Trade-off: adds abstraction complexity vs. maintaining two copies of the same fields.
