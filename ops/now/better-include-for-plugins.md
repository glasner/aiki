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

For a given event, the engine runs:

```
1. before.include plugins (in order)   ← plugins for all events in before phase
2. before's inline handlers for event  ← event-specific handlers (including hook: actions)
3. Own handlers for event              ← the hookfile's own event handlers
4. after.include plugins (in order)    ← plugins for all events in after phase
5. after's inline handlers for event   ← event-specific handlers (including hook: actions)
```

---

## Change 2: `include:` Directive

### Semantics

`include:` has two uses depending on context:

**Top-level `include:`** — structural expansion. `include: foo` means: **take foo's composition configuration and merge it into mine.**

Concretely, when a hookfile includes a plugin:

1. The plugin's `before:` block is prepended to the hookfile's `before:` block
2. The plugin's `after:` block is prepended to the hookfile's `after:` block
3. The plugin's own event handlers are prepended to the hookfile's handlers (plugin's run first)
4. The plugin's `include:` list is expanded recursively (transitive)

```
Effective hookfile after include expansion:

  before   = [include.before, self.before]       ← blocks merged
  handlers = [include.handlers, self.handlers]   ← lists concatenated
  after    = [include.after, self.after]          ← blocks merged
```

Includes are the "inherited" base configuration. Explicit `before:`/`after:` are customizations on top.

**`include:` inside `before:`/`after:`** — run these plugins for all events in this phase. The plugins are loaded and composed recursively (their before → own → after all execute in the enclosing phase).

The context disambiguates: top-level `include:` expands structure, nested `include:` inside a composition block runs plugins in that phase.

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

**Effective hookfile after include expansion:**

```yaml
before:
  # From include (aiki/default's before block):
  turn.started:
    - context: "Aiki project context"
  # User's explicit:
  include:
    - myorg/pre-check
  turn.started:
    - hook: myorg/special-check

session.started:
  # From include (aiki/default's own handlers, run first):
  - log: "Aiki Way enabled"

turn.started:
  # User's own handler:
  - context: "My custom context"

after:
  # From include (aiki/default's after block):
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"
```

**Execution order for `turn.started`:**

```
1. context: "Aiki project context"  (include's before — inline handler)
2. myorg/pre-check                  (before's include — plugin, all events)
3. hook: myorg/special-check        (before's inline — hook action)
4. context: "My custom context"     (own handler)
5. (after has no turn.started)
```

**Execution order for `turn.completed`:**

```
1. (before has no turn.completed)
2. (no own handlers)
3. autoreply: "aiki review..."      (include's after — inline handler)
```

Context injection runs before the user's handlers. Review loop runs after. One `include:` reference, one plugin file.

---

## Detailed Semantics

### Full Execution Order

For a given event, the engine processes phases in this order:

```
1. before.include plugins (in order)   ← plugins for all events
2. before's inline handlers for event  ← event-specific handlers (including hook: actions)
3. Own handlers for event              ← the hookfile's event handlers
4. after.include plugins (in order)    ← plugins for all events
5. after's inline handlers for event   ← event-specific handlers (including hook: actions)
```

With top-level `include:` expansion, the included plugin's blocks are prepended to the corresponding blocks of the includer.

### Ordering Rules

| Source | Before position | Handler position | After position |
|--------|----------------|-----------------|---------------|
| `include:` (first) | Prepended to before | Prepended to handlers | Prepended to after |
| `include:` (second) | After first include | After first include | After first include |
| Explicit `before:` | After all includes | — | — |
| Own handlers | — | After all includes | — |
| Explicit `after:` | — | — | After all includes |

**Mnemonic**: Includes set the base. Explicits customize.

### Multiple Includes

```yaml
include:
  - aiki/default      # before has turn.started context, after has turn.completed review
  - myorg/standard    # before has session.started logging, after has turn.completed notify
```

Expansion (includes stack in declaration order):

```yaml
before:
  # aiki/default's before block first
  turn.started:
    - context: "Aiki project context"
  # then myorg/standard's before block
  session.started:
    - log: "Standard logging enabled"

after:
  # aiki/default's after block first
  turn.completed:
    - if: $event.turn.tasks.completed
      then:
        - autoreply: "aiki review --fix --start"
  # then myorg/standard's after block
  turn.completed:
    - shell: "notify-send 'Turn completed'"
```

When both includes contribute handlers for the same event in the same phase, they concatenate in include order.

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

Include expansion uses the existing cycle detection in `HookComposer`. Circular includes produce `AikiError::CircularHookDependency`:

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

### Handler Merging (Before/After Inline)

When merging composition blocks (e.g., two includes both contribute `before:` blocks with handlers for the same event), handlers for the same event concatenate:

```yaml
# After expanding two includes, before block has:
before:
  turn.started:
    - context: "From first include"      # Runs first
  turn.started:
    - context: "From second include"     # Runs second
```

In practice, the engine collects all before-phase handlers for the current event and runs them in order.

### `hook:` Action Semantics

The `hook:` action invokes a plugin's handler for the current event only:

```yaml
turn.started:
  - hook: aiki/context-inject    # Runs aiki/context-inject's turn.started handlers
  - context: "My stuff"          # Runs after
```

If `aiki/context-inject` has no `turn.started` handler, the `hook:` action is a no-op.

`hook:` loads the referenced plugin via `HookLoader` (cycle detection applies), finds the plugin's handlers for the current event type, and executes them inline. The plugin's `before:`/`after:` blocks are **not** executed — only its handlers for the current event.

This makes `hook:` a surgical invocation: "run this plugin's handler for this event, right here."

---

## What `include:` Is NOT

- **Not `before:`** — `before:` scopes execution to the before phase. Include at the top level expands the plugin's composition into your own.
- **Not `after:`** — Same distinction. `after:` scopes to the after phase.
- **Not handler overriding** — Include merges handlers, it doesn't replace them. There's no way for an included plugin to suppress the includer's handlers.
- **Not middleware** — There's no "next()" or "yield" concept. Include is pure expansion — the plugin's structure is flattened into the hookfile at load time.

---

## Implementation

### Phase 1: Type Changes — Composition Blocks + `hook:` Action

Change `before:` and `after:` from `Vec<String>` to a composition block struct. Add `hook:` as a new `HookStatement` variant.

**File:** `cli/src/flows/types.rs`

```rust
/// A composition block used in before/after positions.
/// Always a mapping with optional `include:` (plugins for all events)
/// and event-specific inline handler lists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompositionBlock {
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

/// Add to the HookStatement enum:
pub enum HookStatement {
    // ...existing variants...

    /// Invoke another plugin's handler for the current event
    Hook { hook: String },
}

pub struct Hook {
    pub name: String,
    // ...existing fields...

    /// Plugins to include (expand their before/after/handlers into this hook)
    #[serde(default)]
    pub include: Vec<String>,

    /// Composition block: runs before this hook's own handlers
    #[serde(default)]
    pub before: Option<CompositionBlock>,

    /// Composition block: runs after this hook's own handlers
    #[serde(default)]
    pub after: Option<CompositionBlock>,

    // ...event handlers...
}
```

No `#[serde(untagged)]` enum needed — `before:`/`after:` always deserialize as a `CompositionBlock` struct.

> **Note:** `CompositionBlock` shares the same event handler fields as `Hook`. Consider extracting a shared `EventHandlers` trait or struct to avoid duplication. Alternatively, `CompositionBlock` could embed a `Hook` with metadata fields ignored.

### Phase 2: `hook:` Action Execution in HookEngine

Add `hook:` handling to `HookEngine::execute_statement()`.

**File:** `cli/src/flows/engine.rs`

When the engine encounters a `Hook { hook: plugin_path }` statement:

1. Load the referenced plugin via `HookLoader` (cycle detection applies)
2. Get the plugin's handlers for the current event type
3. Execute those handlers inline (no before/after — just the handlers)
4. Return the outcome

```rust
HookStatement::Hook { hook: plugin_path } => {
    let plugin = self.loader.load(plugin_path)?;
    let statements = event_type.get_statements(&plugin);
    if !statements.is_empty() {
        let outcome = self.execute_statements(statements, state)?;
        if outcome.should_stop() { return Ok(outcome); }
    }
    Ok(HookOutcome::Success)
}
```

### Phase 3: Composition Block Execution in HookComposer

Update `HookComposer::execute_composed_flow()` to handle `CompositionBlock`:

**File:** `cli/src/flows/composer.rs`

When executing a hook's before/after:

1. Run `include:` plugins — load each plugin, compose recursively (existing behavior).
2. Run inline handlers — extract the inline handlers for the current event type, execute them via `HookEngine` (which handles `hook:` actions).

```rust
fn execute_composition_block(
    &mut self,
    block: &CompositionBlock,
    event_type: EventType,
    state: &mut AikiState,
) -> Result<HookOutcome> {
    // 1. Run included plugins for all events
    for plugin_path in &block.include {
        let outcome = self.compose_hook(plugin_path, event_type, state)?;
        if outcome.should_stop() { return Ok(outcome); }
    }
    // 2. Run inline handlers for this event type (hook: actions handled by engine)
    let statements = event_type.get_statements(block);
    if !statements.is_empty() {
        let engine = HookEngine::new();
        let outcome = engine.execute(statements, state)?;
        if outcome.should_stop() { return Ok(outcome); }
    }
    Ok(HookOutcome::Success)
}
```

The execution order for a hook becomes:

```
1. Execute before block (include plugins, then inline handlers for event)
2. Execute own handlers for event
3. Execute after block (include plugins, then inline handlers for event)
```

### Phase 4: Include Expansion

Add include expansion to `HookComposer::compose_hook()`.

**File:** `cli/src/flows/composer.rs`

When a hook has a non-empty top-level `include:` list:

1. For each included plugin (in order):
   a. Load the included plugin via `HookLoader` (cycle detection applies)
   b. Recursively expand any nested includes
   c. Merge the included plugin's `before:` block into the current hook's `before:` block
   d. Merge the included plugin's `after:` block into the current hook's `after:` block
   e. Prepend the included plugin's own event handlers to the current hook's handlers
2. Execute the expanded hook normally

**Merging composition blocks:**

```rust
/// Merge two CompositionBlocks. `source` is prepended to `target`.
fn merge_blocks(target: &mut Option<CompositionBlock>, source: Option<CompositionBlock>) {
    match (source, target.as_mut()) {
        (None, _) => {} // Nothing to merge
        (Some(src), None) => *target = Some(src),
        (Some(src), Some(tgt)) => {
            // Prepend source's include list
            let mut merged_include = src.include;
            merged_include.extend(tgt.include.drain(..));
            tgt.include = merged_include;
            // Prepend source's event handlers for each event type
            tgt.prepend_handlers_from(&src);
        }
    }
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

Add tests in `cli/src/flows/composer.rs` and `cli/src/flows/engine.rs`:

**Composition Block tests:**
1. **Include only** — `before: { include: [a, b] }` runs plugins in order for all events
2. **Inline only** — `before: { turn.started: [...] }` runs inline handlers for that event
3. **Mixed** — `before: { include: [a], turn.started: [{ hook: b }] }` runs include plugins first, then inline handlers
4. **Empty block** — No-op

**`hook:` action tests:**
5. **Basic hook:** — Invokes plugin's handler for current event
6. **hook: with no matching handler** — Plugin has no handler for event, no-op
7. **hook: does not run before/after** — Only the plugin's own handlers execute
8. **hook: interleaved** — `hook:` and inline actions execute in declaration order
9. **hook: in own handlers** — Works outside composition blocks too

**Include tests:**
10. **Basic include** — Include a plugin with inline before/after, verify execution order
11. **Multiple includes** — First include's blocks come first
12. **Transitive includes** — Plugin includes another plugin, verify full expansion
13. **Include + explicit before/after** — Include contributions come first
14. **Handler merging** — Same event in includer and included, verify concat order
15. **Circular include** — Verify `CircularHookDependency` error
16. **Include of plugin with no include field** — Backwards compatibility

---

## Migration

### Existing `before:`/`after:` List Form

The old list form for `before:`/`after:` is removed:

```yaml
# Old (no longer supported)
before:
  - aiki/context-inject
  - myorg/pre-check

# New
before:
  include:
    - aiki/context-inject
    - myorg/pre-check
```

This is a breaking change. Existing hookfiles using `before: [plugin-list]` must migrate to `before: { include: [plugin-list] }`.

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
| Merge order | Includes prepend | Includes set the base/inherited configuration. Explicit before/after are customizations on top. Natural inheritance model. |
| Handler merge | Concatenate (include first) | Consistent with how before-position plugins run before your handlers. Included handlers are "inherited" behavior. |
| Own handlers | Included in merge | A plugin can contribute both composition (before/after) and direct handlers. No need for sub-plugins. |
| Transitive | Yes | Plugins should be able to compose other plugins. Keeps the model uniform. |
| Backwards compat | Breaking change for list form | Existing `before: [list]` must migrate to `before: { include: [list] }`. Acceptable trade-off for a simpler, uniform model. |

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

3. **Should `CompositionBlock` share event handler fields with `Hook` via a trait?** Both structs have identical event handler fields. A shared `EventHandlers` trait or embedded struct would reduce duplication. Trade-off: adds abstraction complexity vs. maintaining two copies of the same fields.
