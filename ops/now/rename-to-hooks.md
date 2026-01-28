# Rename "Flows" to "Hooks"

## Context

With the introduction of task templates that model "workflows," the term "flows" creates confusion. The current "flows" feature is actually **event-driven automation** - it hooks into lifecycle events and executes commands automatically.

**Current terminology issues:**
- `.aiki/flows/` contains event-driven automation (hooks)
- Task templates model "workflows" 
- `aiki hooks` command installs agent integrations (Git hooks, agent lifecycle hooks)
- The term "flow" is vague and conflicts with "workflow"

## Proposed Solution

Rename "flows" to "hooks" and remove the `aiki hooks install` command (merge into `aiki init`/`aiki doctor`).

### New Structure

```
.aiki/
├── hooks/              # Event-driven automation (renamed from flows/)
│   ├── aiki/
│   │   └── core.yaml   # Core hookfile (renamed from flow.yaml)
│   └── myorg/
│       └── custom.yaml # Custom hookfile
└── templates/          # Task blueprints
    ├── aiki/
    │   └── review.md   # Review template
    └── myorg/
        └── custom.md   # Custom template
```

### Command Changes

| Current | New | Purpose |
|---------|-----|---------|
| `aiki hooks install` | Merged into `aiki init` | Install agent integrations during init |
| `aiki hooks handle` | `aiki hooks stdin` | Stdin integration: Claude Code, Cursor (JSON via stdin) |
| `aiki acp` | `aiki hooks acp` | ACP integration: proxy ACP protocol agents |
| `aiki otel-receive` | `aiki hooks otel` | OTel integration: Codex (HTTP/OTLP via stdin) |
| `.aiki/flows/` | `.aiki/hooks/` | Event-driven automation directory |
| "flow" | "hook" | Terminology in docs/code |
| "flow file" | "hookfile" | YAML file containing hooks |

**Integration points** (hidden commands):
- `aiki hooks stdin` - Claude Code, Cursor hooks (reads JSON from stdin)
- `aiki hooks acp` - ACP protocol agents (stdio proxy)
- `aiki hooks otel` - Codex OTel receiver (reads HTTP/OTLP from stdin)

**Future**: `aiki hooks list/add/remove` for managing user-defined hooks in `.aiki/hooks/`

## Benefits

1. **Accuracy**: Hooks = event-triggered automation (industry standard)
2. **Clarity**: "Hooks" vs "Templates" is unambiguous
3. **Industry alignment**: Everyone understands "hooks" (Git hooks, webhooks, React hooks)
4. **Future-proofing**: Prevents confusion with "workflow" terminology, and reserves `aiki hooks` namespace for managing user-defined hooks
5. **Consistency**: Aligns with actual behavior (hooks into lifecycle events)
6. **Simpler UX**: One command (`aiki init`) instead of two (`aiki hooks install` + `aiki init`)

## Implementation Plan

### Phase 1: Internal Rename (Codebase)

**1. Remove/Merge CLI Commands**

Files to modify:
- `cli/src/main.rs`
  - Remove `Hooks` command enum with `Install` and `Handle` subcommands
  - Remove `Acp` top-level command
  - Remove `OtelReceive` top-level command
  - Add new hidden `Hooks` command with `Stdin`, `Acp`, and `Otel` subcommands:
    ```rust
    #[command(hide = true)]
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    }
    
    enum HooksCommands {
        /// Stdin integration point (Claude Code, Cursor - reads JSON from stdin)
        #[command(hide = true)]
        Stdin {
            #[arg(long)]
            agent: String,
            #[arg(long)]
            event: String,
            #[arg(trailing_var_arg = true)]
            payload: Vec<String>,
        },
        /// ACP integration point (proxy for ACP protocol agents)
        #[command(hide = true)]
        Acp {
            #[arg(long)]
            agent: String,
            #[arg(short, long)]
            bin: Option<String>,
            #[arg(last = true)]
            agent_args: Vec<String>,
        },
        /// OTel integration point (Codex - reads HTTP/OTLP from stdin)
        #[command(hide = true)]
        Otel {
            #[arg(long, default_value = "codex")]
            agent: String,
        },
    }
    ```
    
    **Note**: All three commands now use `--agent` flag for consistency:
    - `stdin`: requires `--agent` and `--event`
    - `acp`: requires `--agent`, optional `--bin`
    - `otel`: `--agent` defaults to "codex"

- `cli/src/commands/init.rs`
  - Merge `run_install()` logic from `hooks.rs` into init
  - Check if global agent integrations are installed
  - Install them if needed (Git hooks, Claude Code hooks, Cursor hooks, Codex config, OTel receiver)
  - Make it idempotent (don't reinstall if already present)

- `cli/src/commands/doctor.rs`
  - Already checks agent integration status
  - With `--fix` flag, can repair/reinstall agent integrations
  - Update checks to look for `aiki hooks stdin` in installed hook commands

- `cli/src/commands/hooks.rs`
  - Remove `run_install()` function
  - Rename `run_handle()` → `run_stdin()` 
  - Update comments to clarify this is the stdin integration point

- `cli/src/commands/acp.rs`
  - Change function signature from `run(agent_type: String, ...)` to `run(agent: String, ...)`
  - Update argument name: `agent_type` → `agent` throughout the function
  - Keep file separate but called from hooks command
  - Update comments to clarify this is the ACP integration point

- `cli/src/commands/otel_receive.rs`
  - Change function signature from `run()` to `run(agent: String)`
  - Add `agent` parameter (will default to "codex" in CLI arg parsing)
  - Keep file separate but called from hooks command
  - Update comments to clarify this is the OTel integration point
  - Note: The agent parameter allows future extensibility for non-Codex OTel sources

- `cli/src/config.rs`
  - Update hook installation to use `aiki hooks stdin` instead of `aiki hooks handle`
  - Update OTel receiver installation to use `aiki hooks otel` instead of `aiki otel-receive`
  - Update ACP invocations to use `aiki hooks acp --agent` instead of `aiki acp`
  - Update patterns like:
    - `aiki hooks handle --agent claude-code --event SessionStart`
    - → `aiki hooks stdin --agent claude-code --event SessionStart`
    - `aiki acp claude-code`
    - → `aiki hooks acp --agent claude-code`
    - `aiki otel-receive`
    - → `aiki hooks otel --agent codex` (or just `aiki hooks otel` using default)

**2. Rename Flow Types to Hook Types**

Files to modify:
- `cli/src/flows/mod.rs`
  - Rename types: `Flow` → `Hook`, `FlowEngine` → `HookEngine`, etc.
  - Update exports

- `cli/src/flows/composer.rs`
  - Rename `FlowComposer` → `HookComposer`
  - Update method names and docs

- `cli/src/flows/engine.rs`
  - Rename `FlowEngine` → `HookEngine`
  - Update internal terminology

- `cli/src/flows/core/functions.rs`
  - Update comments referring to "flows"

- `cli/src/flows/state.rs`
  - Rename `FlowState` → `HookState` (if exists)
  - Update docs

**3. Update Event System**

Files to check:
- `cli/src/event_bus.rs`
  - Update comments about "flows"
  
- `cli/src/events/*.rs`
  - Update docs mentioning "flows"

**4. Update Configuration**

Files to modify:
- `cli/src/config.rs`
  - Update any flow-related config loading to use `.aiki/hooks/`

**5. Rename Core Flow File**

- `cli/src/flows/core/flow.yaml` → `cli/src/flows/core/hook.yaml`
  - Or rename to `core.yaml` (since directory is now `hooks/`)

**6. Update Embedded Resources**

- Update any embedded YAML that references "flows"

### Phase 2: Update Tests

Files to modify:
- `cli/tests/*.rs` - Update test expectations for new naming

### Phase 3: Documentation

**1. Update User Documentation**

Files to modify:
- `README.md` - Update all references to "flows" → "hooks"
- `CLAUDE.md` - Update if it mentions flows
- `AGENTS.md` - Update if it mentions flows

**2. Update Ops Documentation**

Files to check/update:
- `ops/now/*.md` - Update current plans
- `ops/done/*.md` - Historical docs (optional, for context)
- `ops/ROADMAP.md` - Update terminology

**3. Create Migration Guide**

Create `ops/MIGRATION.md` or add section to docs:
- Explain the rename
- Show before/after examples
- Migration steps for users

### Phase 4: Optional Cleanup (Future)

**1. Rename Module Directory**
- `cli/src/flows/` → `cli/src/hooks/` (if desired)
- This is low priority and optional

## Migration Timeline

### Immediate (Phase 1)
- Remove `aiki hooks install` command
- Merge agent integration installation into `aiki init`
- Rename `aiki hooks handle` → `aiki hooks stdin`
- Move `aiki acp` → `aiki hooks acp`
- Move `aiki otel-receive` → `aiki hooks otel`
- Update all hook installation code to use `aiki hooks stdin`
- Update OTel receiver to use `aiki hooks otel`
- Update all documentation to use `aiki hooks acp`
- Rename types: `Flow*` → `Hook*`
- Update core YAML file name

### Short-term (Phase 2 - Same Release)
- Update all tests
- Update all docs

### Optional (Future)
- Consider renaming `cli/src/flows/` module directory to `cli/src/hooks/`

## Risks & Mitigations

### Risk: Breaking existing flow files
**Mitigation**: Users will need to manually move `.aiki/flows/` → `.aiki/hooks/` (simple directory rename)

### Risk: Breaking existing agent hook installations and ACP/OTel usage
**Mitigation**: 
- Users run `aiki init` or `aiki doctor --fix` to reinstall with new `aiki hooks stdin` command
- Old hooks using `aiki hooks handle` will fail, but error message can direct users to reinstall
- Users running `aiki acp` will get "command not found", update docs to use `aiki hooks acp`
- OTel receiver systemd service will break, needs reinstall with `aiki init` or `aiki doctor --fix`

### Risk: Incomplete rename leaving mixed terminology
**Mitigation**: Comprehensive grep for "flow" and "handle" in codebase, systematic replacement

## Testing Strategy

1. **Unit Tests**: Update existing flow tests to use hook terminology
2. **Integration Tests**: Test loading hooks from `.aiki/hooks/`
3. **Manual Testing**: 
   - Run `aiki init` and verify agent integrations are installed
   - Run `aiki init` again and verify it's idempotent (doesn't reinstall)
   - Run `aiki doctor` and verify it detects agent integration status
   - Run `aiki doctor --fix` and verify it repairs broken integrations
   - Load hooks from `.aiki/hooks/`
   - Verify all hook types work correctly
   - Verify `aiki hooks stdin` works (called by Claude Code/Cursor hooks)
   - Verify `aiki hooks acp` works (ACP proxy functionality)
   - Verify `aiki hooks otel` works (OTel receiver functionality)
   - Verify installed hooks use `aiki hooks stdin` instead of `aiki hooks handle`
   - Verify OTel receiver uses `aiki hooks otel` instead of `aiki otel-receive`

## Success Criteria

- [ ] `aiki hooks install` command removed
- [ ] `aiki init` installs global agent integrations if not present
- [ ] `aiki doctor --fix` can repair agent integrations
- [ ] `aiki hooks handle` renamed to `aiki hooks stdin`
- [ ] `aiki acp` moved to `aiki hooks acp`
- [ ] `aiki otel-receive` moved to `aiki hooks otel`
- [ ] All installed hooks updated to call `aiki hooks stdin`
- [ ] OTel receiver systemd service updated to call `aiki hooks otel`
- [ ] Hooks load from `.aiki/hooks/` directory
- [ ] All documentation uses "hooks" terminology
- [ ] All code uses `Hook*` types instead of `Flow*`
- [ ] Clear distinction between:
  - **Hooks** (event-driven automation in `.aiki/hooks/`)
  - **Templates** (task blueprints in `.aiki/templates/`)
  - **Agent integrations** (installed by `aiki init`, managed by `aiki doctor`)
  - **Integration points** (hidden):
    - `aiki hooks stdin` - Claude Code, Cursor (JSON via stdin)
    - `aiki hooks acp` - ACP protocol agents (stdio proxy)
    - `aiki hooks otel` - Codex (HTTP/OTLP via stdin)
  - **Future**: `aiki hooks list/add/remove` for managing user hooks

## Examples

### Before
```bash
# Install agent integrations globally
aiki hooks install

# Initialize a repository
aiki init

# Start ACP proxy
aiki acp claude-code

# OTel receiver
aiki otel-receive

# Directory structure
.aiki/flows/aiki/core.yaml

# Documentation
"Flows are event-driven automation..."
```

### After
```bash
# Initialize a repository (installs agent integrations automatically)
aiki init

# Repair agent integrations if needed
aiki doctor --fix

# Start ACP proxy (now under hooks namespace)
aiki hooks acp claude-code

# Directory structure
.aiki/hooks/aiki/core.yaml

# Documentation
"Hooks are event-driven automation..."

# Integration points (hidden, called by installed hooks/services)
aiki hooks stdin --agent claude-code --event SessionStart  # Claude Code, Cursor
aiki hooks acp --agent claude-code                          # ACP agents
aiki hooks otel --agent codex                               # Codex OTel receiver

# Future: manage user-defined hooks
aiki hooks list
aiki hooks add myorg/custom
```

## Architecture Decisions

### Why use `aiki hooks stdin/acp/otel`?

**Decision**: Three integration points under `aiki hooks` namespace

**Rationale**:
1. **Parallel structure**: All three are integration points for firing hooks
   - `stdin` - Claude Code, Cursor (reads JSON from stdin)
   - `acp` - ACP protocol agents (stdio proxy)
   - `otel` - Codex (reads HTTP/OTLP from stdin)
2. **Clear purpose**: Names describe the integration method (how data arrives)
3. **Future-compatible**: Clean namespace for user commands:
   - `aiki hooks stdin/acp/otel` - integration points (hidden)
   - `aiki hooks list/add/remove` - user-facing commands
4. **Accurate**: All three exist purely to fire aiki hooks, so they belong in the hooks namespace
5. **Unified architecture**: All hook-related functionality under one namespace
6. **Worth the migration**: Since we're already updating hook installations, consolidating under `aiki hooks` is minimal additional work
7. **Descriptive**: `stdin` is more accurate than `cli` - it's about how the command receives input (stdin vs ACP protocol vs HTTP on stdin)

### Why merge into `aiki init` instead of keeping separate?

**Decision**: Remove `aiki hooks install` and merge into `aiki init`

**Rationale**:
1. Simpler user experience - one command instead of two
2. `aiki init` is already the entry point for setting up a repository
3. Agent integrations are required for tracking to work, so they should be automatic
4. `aiki doctor --fix` can handle repairs if needed
5. Frees up `aiki hooks` namespace for future user-defined hook management

### Future: `aiki hooks` namespace

**Reserved for**: Managing user-defined hooks in `.aiki/hooks/`

**Possible commands**:
- `aiki hooks list` - Show installed hooks
- `aiki hooks add <namespace/name>` - Install a hook from registry
- `aiki hooks remove <namespace/name>` - Remove a hook
- `aiki hooks enable/disable <name>` - Toggle hooks

**Integration points** (already implemented, hidden):
- `aiki hooks stdin` - Claude Code, Cursor (JSON via stdin)
- `aiki hooks acp` - ACP protocol proxy for ACP-based agents
- `aiki hooks otel` - Codex OTel receiver (HTTP/OTLP via stdin)

## Signature Changes Summary

### Command Signature Changes

| Old Command | New Command | Key Changes |
|-------------|-------------|-------------|
| `aiki hooks handle --agent X --event Y` | `aiki hooks stdin --agent X --event Y` | Command rename only, signature unchanged |
| `aiki acp <agent>` | `aiki hooks acp --agent <agent>` | Positional arg → `--agent` flag |
| `aiki otel-receive` | `aiki hooks otel --agent codex` | Added `--agent` flag (defaults to "codex") |

### Code Changes Required

**1. CLI Argument Parsing (`cli/src/main.rs`)**
```rust
// OLD
Acp {
    agent_type: String,  // positional
    ...
}
Otel,  // no args

// NEW
Acp {
    #[arg(long)]
    agent: String,  // named flag
    ...
}
Otel {
    #[arg(long, default_value = "codex")]
    agent: String,  // named flag with default
}
```

**2. Function Signatures**

`cli/src/commands/acp.rs`:
```rust
// OLD
pub fn run(agent_type: String, bin: Option<String>, agent_args: Vec<String>) -> Result<()>

// NEW
pub fn run(agent: String, bin: Option<String>, agent_args: Vec<String>) -> Result<()>
// Also update all uses of `agent_type` variable inside function → `agent`
```

`cli/src/commands/otel_receive.rs`:
```rust
// OLD
pub fn run() -> Result<()>

// NEW
pub fn run(agent: String) -> Result<()>
// The `agent` parameter enables future extensibility (other OTel sources)
// For now, it will always be "codex" from the CLI default
```

**3. Command Dispatch (`cli/src/main.rs`)**
```rust
// OLD
Commands::Acp { agent_type, bin, agent_args } => {
    commands::acp::run(agent_type, bin, agent_args)
}
Commands::OtelReceive => {
    commands::otel_receive::run()
}

// NEW
Commands::Hooks { command } => match command {
    HooksCommands::Acp { agent, bin, agent_args } => {
        commands::acp::run(agent, bin, agent_args)
    }
    HooksCommands::Otel { agent } => {
        commands::otel_receive::run(agent)
    }
    ...
}
```

**4. Installation Code (`cli/src/config.rs`)**

Search for all command generation strings and update:
- `aiki acp` → `aiki hooks acp --agent`
- `aiki otel-receive` → `aiki hooks otel` (or `--agent codex`)

**Specific locations to update:**

a) **Launchd plist template** (`generate_launchd_plist()` function):
```xml
<!-- OLD -->
<string>otel-receive</string>

<!-- NEW -->
<string>hooks</string>
<string>otel</string>
<string>--agent</string>
<string>codex</string>
```

b) **Systemd service template** (`generate_systemd_service()` function):
```ini
# OLD
ExecStart={} otel-receive

# NEW
ExecStart={} hooks otel --agent codex
```

c) **File paths** (stay the same for backwards compatibility):
- `com.aiki.otel-receive.plist` ✓ (keep name)
- `aiki-otel-receive.socket` ✓ (keep name)
- `aiki-otel-receive@.service` ✓ (keep name)
- `/tmp/aiki-otel-receive.err` ✓ (keep log path)

d) **Function names** (internal, can stay the same):
- `install_otel_receiver()` ✓
- `restart_otel_receiver()` ✓
- `is_otel_receiver_installed()` ✓

e) **Other locations to check:**
- Git hook installation templates
- Agent config file generation
- Help text and comments

**5. Documentation Updates**

Files to search and update:
- `README.md` - Update command examples
- `AGENTS.md` - If it shows command examples
- Any config examples in `cli/src/config.rs` comments
- Help text in command definitions

### Testing Checklist

After changes, verify:
- [ ] `aiki hooks stdin --agent claude-code --event SessionStart` works
- [ ] `aiki hooks acp --agent claude-code` works (new signature)
- [ ] `aiki hooks otel` works (uses default agent)
- [ ] `aiki hooks otel --agent codex` works (explicit agent)
- [ ] `aiki init` generates correct hook commands
- [ ] `aiki doctor` validates correct hook commands
- [ ] **OTel receiver systemd service** uses correct command:
  - [ ] Linux: Check `~/.config/systemd/user/aiki-otel-receive@.service`
  - [ ] Verify `ExecStart` line: `aiki hooks otel --agent codex`
  - [ ] Test: `systemctl --user restart aiki-otel-receive.socket`
- [ ] **OTel receiver launchd service** uses correct command:
  - [ ] macOS: Check `~/Library/LaunchAgents/com.aiki.otel-receive.plist`
  - [ ] Verify `ProgramArguments`: `hooks`, `otel`, `--agent`, `codex`
  - [ ] Test: `launchctl unload/load ~/Library/LaunchAgents/com.aiki.otel-receive.plist`
- [ ] **Codex integration** works end-to-end:
  - [ ] Codex sends OTel to port 19876
  - [ ] Aiki receives and processes the data
  - [ ] Hooks fire correctly from OTel events
- [ ] Old commands fail with helpful error (removed, suggest new command)

## Open Questions

1. Should we rename `cli/src/flows/` module directory to `cli/src/hooks/`?
   - **Recommendation**: Not immediately - do this after migration period

2. Should the core file be `core.yaml` or `hook.yaml` inside `.aiki/hooks/aiki/`?
   - **Recommendation**: `core.yaml` (shorter, directory already says "hooks")

3. Should `aiki init` be verbose about installing agent integrations?
   - **Recommendation**: Yes, print "Installing agent integrations..." so users know what's happening

4. Should we keep `acp.rs` and `otel_receive.rs` as separate files or merge into `hooks.rs`?
   - **Recommendation**: Keep separate initially, just change the command paths:
     - `cli/src/commands/hooks.rs` - `run_stdin()` (was `run_handle()`)
     - `cli/src/commands/acp.rs` - `run()` called as `run_acp()` from hooks command
     - `cli/src/commands/otel_receive.rs` - `run()` called as `run_otel()` from hooks command
   - Can consolidate later if desired, but keeping separate maintains clarity

## Related Work

- See `ops/done/rename-actions-to-commands.md` for previous terminology cleanup
- Task templates implementation (already uses clear "template" terminology)
- Event system documentation

## Next Steps

1. Review this plan with team
2. Decide on migration timeline
3. Create tasks for each phase
4. Begin Phase 1 implementation
