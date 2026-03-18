# User Settings (`~/.aiki/config.yaml`)

**Status:** Future — Foundation for multiple planned features
**Priority:** Medium (blocks several downstream plans)

---

## Summary

Introduce a unified `~/.aiki/config.yaml` file for user-level settings that multiple features can build on. This avoids each feature independently inventing its own config file.

## Motivation

Several planned features assume a user-level config file exists, but each names it differently or defines its own structure:

- **[better-agent-detection](../now/better-agent-detection.md)** — wants `agents` section for reviewer preferences
- **[state-trust-boundaries](state-trust-boundaries.md)** — wants `workspaces` sections for isolation boundaries
- **[smart-type-detection](tasks/smart-type-detection.md)** — wants project-level `.aiki/config.yaml` for task type patterns
- **[AIKI_TWIN](AIKI_TWIN.md)** — wants twin permissions config

Building the settings foundation once means these features get config "for free" instead of each reinventing it.

## Design

### Why YAML

Aiki already uses YAML for hooks (`.aiki/hooks.yml`), flows, and plugin definitions. Using the same format for config keeps one serialization format across the project and avoids mixing TOML and YAML mental models for users.

### File Location

| Level | Path | Purpose |
|-------|------|---------|
| User | `~/.aiki/config.yaml` | Global defaults and preferences |
| Project | `.aiki/config.yaml` | Per-project overrides |

Resolution: project overrides user, same as hooks and task templates.

Respects `AIKI_HOME` override (see `global.rs`).

### Schema (Starter)

```yaml
# ~/.aiki/config.yaml

agents:
  preferred_reviewer: codex       # which agent to prefer for reviews
  allow_self_review: true          # allow same agent to review its own work

workspaces:
  - name: personal
    paths: ["/Users/me/personal/**"]
    home: ~/.aiki/workspaces/personal

tasks:
  # Future: smart-type-detection patterns, default priority, etc.

twin:
  # Future: AIKI_TWIN permissions and personality config
```

Each feature owns its own top-level key. The settings module just handles loading, merging, and access.

### Layered Merge Strategy

1. Load `~/.aiki/config.yaml` (user defaults)
2. Load `.aiki/config.yaml` (project overrides) if present
3. Merge: project values override user values at the key level
4. Apply hardcoded defaults for anything not specified

### Implementation

```rust
// cli/src/settings.rs

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub agents: AgentSettings,
    // Future sections added here as features land
}

#[derive(Debug, Deserialize, Default)]
pub struct AgentSettings {
    pub preferred_reviewer: Option<String>,
    #[serde(default = "default_true")]
    pub allow_self_review: bool,
}

fn default_true() -> bool { true }

impl Settings {
    /// Load settings with project → user → defaults layering.
    pub fn load(project_root: Option<&Path>) -> Self {
        // 1. User config
        // 2. Project config (if present)
        // 3. Merge + defaults
        todo!()
    }
}
```

### Access Pattern

Settings should be loaded once per command invocation and threaded through as needed — not stored in a global/static. This keeps testability simple.

## Scope

### In Scope (This Plan)

- `Settings` struct with serde deserialization
- `Settings::load()` with user → project layering
- `agents` section (first consumer: better-agent-detection)
- Tests for loading, merging, missing files, malformed YAML

### Out of Scope (Downstream Features)

- `workspaces` sections — see [state-trust-boundaries](state-trust-boundaries.md)
- `tasks` sections — see [smart-type-detection](tasks/smart-type-detection.md)
- `twin` sections — see [AIKI_TWIN](AIKI_TWIN.md)
- XDG path resolution — see [xdg-support](xdg-support.md)
- `aiki config` CLI commands (get/set/list)
- Schema validation or migration tooling

## Open Questions

1. **Deep merge or shallow?** Start with shallow (key-level) merge. Deep merge can be added if a feature needs it.
2. **Accept `.yml` too?** For consistency with hooks (which accept both `.yml` and `.yaml`), could look for both. Start with `.yaml` only and add `.yml` fallback if users request it.

## Dependencies

- `global.rs` already provides `global_aiki_dir()` — use it for user config path
- `serde_yaml` crate already in `Cargo.toml` — no new dependency needed
