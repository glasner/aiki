# Default Hooks: Init-Time Hook Scaffolding

**Date**: 2026-02-11
**Status**: Draft
**Priority**: P2

**Related Documents**:
- [The Aiki Way](the-aiki-way.md) - Vision for opinionated workflow automation
- [Review Loop](review-loop.md) - Review-fix cycle plugin (first composable plugin)
- [Built-In Plugin Resolution](built-in-plugin-resolution.md) - Embedded plugin fallback (prerequisite)
- [Turn Event Payloads](turn-event-payloads.md) - Task context in turn.completed events (prerequisite)
- [Rename to Hooks](../done/rename-to-hooks.md) - Hooks terminology and architecture
- [Workflow Hook Commands](workflow-hook-commands.md) - Flow action integration (spec/plan/build)

---

## Problem

Today, `aiki init` sets up the JJ repository and installs agent integrations (Claude Code hooks, Cursor hooks, Codex OTel receiver), but it doesn't create a user-editable hookfile. The only hooks that run are the embedded `aiki/core` hooks compiled into the binary.

This means:

1. **Users can't see what hooks are running.** The core hooks are invisible — embedded in the binary with no file on disk to inspect.
2. **There's no composition point.** Without a user-owned hookfile, there's nowhere to add `after: aiki/review-loop` or other plugins.
3. **"The Aiki Way" has no delivery mechanism.** The opinionated workflow patterns (review loops, lint gates, etc.) need a hookfile to compose into.
4. **New users don't know what's possible.** Without a scaffolded hookfile showing the extension points, discoverability is poor.

---

## Summary

When `aiki init` runs, it creates a **hookfile** at `.aiki/hooks.yml` that:

1. **Composes the opinionated "Aiki Way" workflow** from built-in plugins via `after:` directives
2. **Serves as the user's customization point** — they edit this file to add/remove plugins, add project-specific hooks, or override behavior
3. **Is fully commented** with explanations of each plugin and event, so users understand what's happening and how to customize

The core hooks (`aiki/core`) remain embedded in the binary and always run first. The hookfile runs after core, adding the opinionated automation layer.

---

## How It Works

### Two-Layer Architecture

```
Request comes in
       │
       ▼
  ┌─────────────────────────────────┐
  │  aiki/core (embedded, always)   │  Layer 1: Provenance tracking
  │  - Session init                 │  - Always runs
  │  - Change tracking              │  - Not user-editable
  │  - Provenance metadata          │  - Compiled into binary
  │  - Co-author generation         │
  └─────────────────┬───────────────┘
                    │
                    ▼
  ┌─────────────────────────────────┐
  │  .aiki/hooks.yml                │  Layer 2: Workflow automation
  │  - Review loop                  │  - Created by aiki init
  │  - Lint gates                   │  - User-editable
  │  - Build checks                 │  - Composable plugins
  │  - Custom project hooks         │
  └─────────────────────────────────┘
```

**Layer 1 (core)** handles the fundamentals: provenance tracking, change attribution, co-author generation, session lifecycle. These are non-negotiable and always run.

**Layer 2 (hooks.yml)** handles workflow automation: review loops, quality gates, context injection. These are opinionated defaults that users can customize.

### Execution Model

The engine loads `aiki/core` first (embedded), then loads `.aiki/hooks.yml` (if it exists). For each event, it runs core's handlers, then the hookfile's handlers (including any plugins composed via `after:`).

If `.aiki/hooks.yml` doesn't exist, only core runs — backwards compatible with existing repos.

---

## User Experience

### `aiki init` (New Repository)

```
$ aiki init
Initialized Aiki repository
Created .aiki/hooks.yml with default workflow automation
```

### `aiki init` (Existing Repository, No Hookfile)

```
$ aiki init
Aiki repository already initialized
Created .aiki/hooks.yml with default workflow automation
```

### `aiki init` (Existing Repository, Hookfile Exists)

```
$ aiki init
Aiki repository already initialized
.aiki/hooks.yml already exists (skipping)
```

Init never overwrites an existing hookfile — the user's customizations are sacred.

### Editing the Hookfile

Users open `.aiki/hooks.yml` and edit directly:

```yaml
# .aiki/hooks.yml

# before:
#   - your/plugin  # Runs before aiki/default

after:
  - aiki/default  # The opinionated Aiki Way - updated automatically with new releases

# Add your own project-specific hooks below:
# shell.permission_asked:
#   - if: $event.command | contains("rm -rf")
#     then:
#       - block: "Refusing rm -rf"
```

---

## The Hookfile

### Initial Content (Created by `aiki init`)

```yaml
# Aiki Hooks
#
# This file configures agent hooks for your project.
#
# Learn more:
#   aiki hooks --help
#   https://aiki.sh/help/hooks

# before:
#   - your/plugin  # Runs before aiki/default

after:
  - aiki/default  # The opinionated Aiki Way (auto-updates with new releases)

# ============================================================================
# Custom Hooks
# ============================================================================
# Add your own event handlers below. Each event fires at a specific point
# in the agent lifecycle. Uncomment and modify to customize.
#
# --- Session Lifecycle ---
#
# session.started:
#   # Fires when a new agent session begins (after aiki/core initializes)
#   # Use for: injecting project context, setting up session state
#   - context: "Remember to run tests before committing"
#
# session.resumed:
#   # Fires when an existing session is resumed (not a fresh start)
#   # Use for: re-injecting context that may have been lost to compaction
#
# session.ended:
#   # Fires when an agent session ends
#   # Use for: cleanup, notifications, session summaries
#
# --- Turn Lifecycle ---
#
# turn.started:
#   # Fires before each agent turn (user prompt or autoreply)
#   # Use for: injecting per-turn context, rate limiting
#   # Note: survives context compaction (re-injected every turn)
#
# turn.completed:
#   # Fires after the agent finishes responding
#   # Use for: post-turn validation, autoreplies, review triggers
#   # Supports: autoreply: (send a follow-up message to the agent)
#
# --- File Operations ---
#
# change.permission_asked:
#   # Fires before a file write, delete, or move (gateable)
#   # Use for: blocking writes to protected files, requiring approval
#   # - if: $event.file_paths | contains(".env")
#   #   then:
#   #     - block: "Cannot modify .env files"
#
# change.completed:
#   # Fires after a file mutation completes
#   # Use for: post-change validation, lint checks
#
# read.permission_asked:
#   # Fires before a file read (gateable)
#   # Use for: blocking reads of sensitive files (secrets, credentials)
#
# --- Shell Commands ---
#
# shell.permission_asked:
#   # Fires before a shell command executes (gateable)
#   # Use for: blocking dangerous commands, requiring review before push
#   # - if: $event.command | contains("git push")
#   #   then:
#   #     - block: "Run tests before pushing"
#
# shell.completed:
#   # Fires after a shell command completes
#   # Use for: logging, post-command validation
#
# --- Task Lifecycle ---
#
# task.started:
#   # Fires when a task transitions to in_progress
#   # Use for: notifications, task setup
#
# task.closed:
#   # Fires when a task is closed
#   # Use for: notifications, triggering follow-up work
#
# --- Other Events ---
#
# commit.message_started:
#   # Fires during Git's prepare-commit-msg hook
#   # Use for: adding trailers, enforcing commit message format
#
# mcp.permission_asked:
#   # Fires before an MCP tool call (gateable)
#   # Use for: rate limiting, blocking expensive operations
#
# web.permission_asked:
#   # Fires before a web fetch (gateable)
#   # Use for: blocking external requests, domain allowlisting
```

### Plugin Files

Each plugin referenced by `after:` is a separate YAML file shipped with aiki. These are **built-in plugins** — they ship inside the binary (like core) but are referenced by name from the hookfile.

#### `aiki/default` (Wrapper Plugin)

The default plugin wraps the opinionated "Aiki Way" workflow automation. It composes other plugins and can be updated behind the scenes with new Aiki releases without requiring users to modify their hookfile.

**Benefits**:
- Users on default settings get automatic workflow improvements with updates
- Users who want explicit control can replace `aiki/default` with individual plugins
- Users can override by creating `.aiki/hooks/aiki/default.yml`

```yaml
# Built-in: aiki/default
description: "The opinionated Aiki Way - workflow automation that updates automatically"

after:
  - aiki/review-loop
  # Future plugins added here in new releases:
  # - aiki/context-inject
  # - aiki/build-check
```

#### `aiki/review-loop` (Review Plugin)

Already designed in `ops/now/review-loop.md`. Triggers `aiki review --fix --start` after agent turns that complete original work tasks.

**Prerequisite**: Requires task context in `turn.completed` payload - see [Turn Event Payloads](turn-event-payloads.md).

```yaml
# Built-in: aiki/review-loop
name: aiki/review-loop
description: "Review and fix code after each agent turn"
version: "1"

turn.completed:
  - if: $event.tasks.closed | any(.task_type == null)
    then:
      - autoreply: |
          Original work tasks closed: $event.tasks.closed | filter(.task_type == null) | map(.description)

          Review your work and fix any issues:

          aiki review --fix --start
```

---

## Plugin Resolution

Plugins are resolved in this order:

1. **Project `.aiki/hooks/{namespace}/{name}.yml`** — user overrides
2. **User `~/.aiki/hooks/{namespace}/{name}.yml`** — user-global overrides
3. **Built-in (embedded in binary)** — shipped with aiki

This means:

- `after: aiki/review-loop` resolves to the built-in plugin by default
- Users can override by creating `.aiki/hooks/aiki/review-loop.yml` in their project
- The built-in serves as fallback when no user file exists

See [Built-In Plugin Resolution](built-in-plugin-resolution.md) for implementation details.

---

## Default Hookfile Loading (Already Implemented)

The two-layer execution model already exists in `execute_hook` (`cli/src/events/prelude.rs`):

1. Runs bundled `aiki/core` statements first
2. Short-circuits on block/stop
3. Loads user's hookfile via `HookComposer::compose_hook_from_path()`
4. Combines results (user failure takes precedence)

```
Engine execution (existing code):
  1. aiki/core handlers for event
  2. hooks.yml's before: plugins for event
  3. hooks.yml's own handlers for event
  4. hooks.yml's after: plugins for event
```

### Known Bug: Path Resolution

The current code resolves the hookfile path using `state.cwd()`:
```rust
let default_flow_path = state.cwd().join(".aiki/hooks/default.yml");
```
This fails when commands run from subdirectories. Fix: use `loader.project_root()` instead:
```rust
let default_flow_path = loader.project_root().join(".aiki/hooks.yml");
```

### Relationship to Core

The hookfile does **not** use `before: aiki/core` — core is handled specially by the engine and always runs first. The hookfile's `before:` and `after:` compose additional plugins on top.

---

## Prerequisites

### Built-In Plugin Resolution

The default hookfile references `aiki/default` and `aiki/review-loop` plugins, which need to be embedded in the binary (not files on disk). This requires adding a built-in plugin fallback to the hook loader.

**Status**: Designed in [Built-In Plugin Resolution](built-in-plugin-resolution.md)

**Must be implemented before**: Phase 1 (Default Hookfile Scaffolding) creates hookfile that references `aiki/default`

### Turn Event Payloads

The `aiki/review-loop` plugin requires task context in `turn.completed` events to trigger reviews only for original work tasks (not review/fix tasks). This requires adding a `tasks` field to `AikiTurnCompletedPayload`.

**Status**: Designed in [Turn Event Payloads](turn-event-payloads.md)

**Must be implemented before**: Shipping `aiki/review-loop` plugin

---

## Implementation Plan

### Phase 1: Default Hookfile Scaffolding

**What**: `aiki init` creates `.aiki/hooks.yml` with the review-loop plugin.

1. Create `ensure_hooks_yml()` helper (like existing `ensure_agents_md()`) — idempotent, never overwrites
2. Call it **before** the early return on line 46-54 of `init.rs` (currently returns early when hooks are already configured, which would skip hookfile creation on existing repos)
3. Also call it in the normal init path further down
4. Embed hookfile template content in binary
5. Print confirmation showing which plugins are included

**Files to modify**:
- `cli/src/commands/init.rs` — add `ensure_hooks_yml()`, call before early return and in normal flow
- `cli/src/flows/` — embed hookfile template

### Phase 2: Path Resolution Fix

**What**: Fix hookfile path resolution to use project root instead of cwd.

1. Change `state.cwd().join(...)` to `loader.project_root().join(".aiki/hooks.yml")` in `execute_hook`

**Files to modify**:
- `cli/src/events/prelude.rs` — fix path resolution (one-line change)

### Phase 3: Documentation

- Update CLAUDE.md with hooks info
- Add examples to `aiki init` help text

---

## Future Plugins

The hookfile is designed to grow as more plugins are built. Each plugin is independently useful and composable:

| Plugin | Event | Purpose | Status |
|--------|-------|---------|--------|
| `aiki/default` | N/A (wrapper) | Opinionated workflow automation | Designed |
| `aiki/review-loop` | `turn.completed` | Review and fix after each turn | Designed (`ops/now/review-loop.md`) |
| `aiki/lint-gate` | `shell.permission_asked` | Block git push if lint fails | Future |
| `aiki/build-check` | `turn.completed` | Run build after changes | Future |
| `aiki/test-gate` | `shell.permission_asked` | Block git push if tests fail | Future |
| `aiki/context-inject` | `turn.started` | Inject project context per-turn | Future |

**Default users** get automatic updates via `aiki/default`:
```yaml
after:
  - aiki/default  # Automatically includes review-loop and future plugins
```

**Power users** can opt for explicit control:
```yaml
after:
  - aiki/review-loop
  - aiki/lint-gate
  - aiki/build-check
```

---

## Doctor Validation

`aiki doctor` validates the hookfile with these checks:

| Check | Severity | Message |
|-------|----------|---------|
| File exists | Info | "No hookfile found. Run `aiki init` to create one." |
| YAML syntax valid | Error | ".aiki/hooks.yml has invalid YAML: {parse error}" |
| Referenced plugins exist | Warning | "Plugin 'aiki/lint-gate' not found (referenced in after:)" |
| Unknown event types | Warning | "Unknown event 'session.starting' in .aiki/hooks.yml (did you mean 'session.started'?)" |
| Core not in before/after | Info | "No need to reference aiki/core — it always runs automatically" |

`aiki doctor --fix` can:
- Recreate a missing `hooks.yml` (same as init behavior)
- Remove references to non-existent plugins (with confirmation)

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `hooks.yml` has syntax error | Log warning, skip user hooks, core still runs |
| Plugin in `after:` not found | Log warning, skip that plugin, continue with others |
| Plugin execution fails | Follow plugin's `on_failure:` directive, or skip and continue |
| `aiki init` can't write `.aiki/hooks.yml` | Error: "Failed to create hookfile" |
| `.aiki/hooks.yml` exists at init time | Skip creation, print "already exists" |

---

## Version Control

`.aiki/hooks.yml` **should be committed** to the repository. It represents the team's shared workflow configuration — all team members should run the same hooks. This is analogous to `.eslintrc` or `.prettierrc`.

When a user customizes the hookfile, those changes are committed and shared with the team. For personal-only overrides, users can create hookfiles in `~/.aiki/hooks/` (user-global, not committed).

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Hookfile content | List all events with docs | Each event is listed as a commented-out section with explanation of when it fires and what it's useful for. Maximizes discoverability. |
| Default plugin wrapper | `aiki/default` instead of direct plugin list | Users on default settings get automatic workflow improvements with updates. Power users can opt out and use explicit plugin list. |
| Review loop default | Enabled via `aiki/default` | Opinionated out of the box — users remove `aiki/default` if unwanted or replace with explicit plugins. |
| Doctor validation | Full validation | `aiki doctor` checks YAML syntax, verifies referenced plugins exist, and warns about unknown event types. |
| Plugin versioning | Silent update | Built-in fallback always uses latest version. Users who haven't overridden get updates automatically. Simple and low-maintenance. |

## Open Questions

1. **Should there be an `aiki hooks reset` command to regenerate the hookfile?** Useful if users break their hookfile and want to start fresh, but risky if they've made intentional customizations. Could prompt for confirmation or create a backup.
