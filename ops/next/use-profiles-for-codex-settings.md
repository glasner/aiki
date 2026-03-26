# Use Profiles For Codex Settings

## Goal

Move Aiki-managed Codex settings out of the top-level `~/.codex/config.toml`
namespace and into a dedicated profile while preserving the current behavior of
`aiki init` and `aiki doctor`.

## Why

The current Codex integration updates multiple top-level settings:

- `[otel]`
- `[hooks]`
- `[sandbox_workspace_write]`
- removal of legacy `notify`

That works, but it mixes Aiki-managed defaults directly into the user's primary
Codex configuration. A dedicated profile would make Aiki's footprint clearer
and easier to reason about.

## Constraints

- Codex profiles appear to be override layers, not standalone config files.
- The public config reference documents `profiles.<name>.*` and `profile`, but
  does not document profile inheritance or composition.
- Any profile-based design must still ensure Codex native hooks can write to
  Aiki's global state directory via `sandbox_workspace_write.writable_roots`.
- We should avoid clobbering an existing user-selected default profile without
  an explicit choice.

## Proposed Design

1. Define a managed profile name, likely `aiki`.
2. Move all Aiki Codex settings into `[profiles.aiki.*]`.
3. Teach `aiki init` to:
   - create/update the `profiles.aiki` block idempotently
   - only set top-level `profile = "aiki"` when safe
   - otherwise warn when another default profile is already active
4. Teach `aiki doctor` to:
   - validate the `profiles.aiki` block
   - distinguish between "profile exists" and "profile is selected"
   - offer `--fix` behavior that preserves existing user intent where possible
5. Update the Codex runtime if needed so Aiki-spawned Codex sessions can opt
   into the managed profile explicitly instead of relying only on the user's
   default profile.

## Open Questions

- Should `aiki init` ever overwrite an existing top-level `profile` value?
- Should Aiki-spawned Codex sessions pass `--profile aiki` explicitly?
- If a user already has a custom profile, should Aiki merge top-level settings
  as a fallback or require manual opt-in to the managed profile?
- Do profile-scoped arrays like `writable_roots` replace or merge with top-level
  values in all cases? This should be verified empirically before migration.

## Migration Plan

1. Add a small compatibility layer that can read both top-level-managed and
   profile-managed Codex configurations.
2. Update `doctor` first so it can detect both layouts without false alarms.
3. Add profile-aware install/update logic in `aiki init`.
4. Add targeted tests for:
   - existing top-level config
   - fresh profile-only config
   - profile-selected vs profile-defined-only states
5. Once stable, decide whether to migrate existing top-level installs
   automatically or leave them as supported legacy state.
