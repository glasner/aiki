# Plan: Reorganize task template directories (`aiki/core` and `.aiki/tasks`)

## Problem / investigation summary
The directory name `.aiki/templates` is misleading for the task-oriented workflow. Additionally, built-in system templates are stored in `aiki/default` which should be reserved for user-customizable defaults.

This creates three risks:
1. User confusion about where task artifacts vs task templates live.
2. Built-in templates mixed with user customizations in `aiki/default`.
3. Directory naming doesn't align with task-oriented product language.

## Goal
Separate built-in templates from user customizations through directory structure:
- **System templates:** `aiki/core/templates/**/*.md` (new location for built-in templates)
- **User templates:** `.aiki/tasks/**/*.md` (user-defined templates in repo)

Templates and rendered outputs both use `.md` extension - location distinguishes them:
- Templates live in `aiki/core/templates/` or `.aiki/tasks/`
- Rendered task docs live in `.aiki/sessions/` or task storage

Make the canonical user-facing directory reflect product language:
- `.aiki/tasks` (only supported path going forward)

## Scope
Two coordinated changes:
1. **System template relocation:** Move built-in templates from `aiki/default` to `aiki/core`.
2. **User path migration:** `.aiki/templates` becomes `.aiki/tasks` across code, defaults, docs, tests.

**HARD SWITCH:** No backward compatibility. Old paths will not be supported.

## Proposed behavior

### New behavior (only)
- Built-in template discovery: `aiki/core/templates/**/*.md`.
- User template discovery: `.aiki/tasks/**/*.md`.
- Rendered outputs also use `.md` (distinguished by location).
- CLI/docs/help reference only `.aiki/tasks`.
- Old paths (`.aiki/templates`, `aiki/default/templates`) are no longer recognized.

## Implementation plan

### Phase 1 — Inventory + change map
Identify all references to:
- `.aiki/templates`
- `aiki/default` template storage
- template discovery globbing
- docs/examples/tests with old paths

Deliverable: short impact table (component → file(s) → change type).

### Phase 2 — Canonical constants and helpers
Centralize canonical values in shared helpers/constants:
- `AikiCoreTemplatesDir = "aiki/core/templates"`
- `AikiUserTasksDir = ".aiki/tasks"`

Replace scattered string literals and ad-hoc checks.

### Phase 3 — System template migration
Move built-in templates from `aiki/default` to `aiki/core`:
1. Move all templates from `aiki/default/templates/` to `aiki/core/templates/`.
2. Update any hardcoded references in code.

### Phase 4 — Loader/render pipeline update
Update template loader to:
1. Load built-in templates from `aiki/core/templates/**/*.md` ONLY.
2. Load user templates from `.aiki/tasks/**/*.md` ONLY.
3. Remove all legacy path fallback logic.
4. Preserve deterministic ordering.

### Phase 5 — Init creates new structure
Update `aiki init` to:
1. Create `.aiki/tasks/` directory (not `.aiki/templates/`).
2. Bootstrap example templates in `.aiki/tasks/` if desired.
3. No migration logic needed (hard switch).

### Phase 6 — CLI UX update
- Update help text and command output examples to canonical forms only.
- Remove all references to legacy paths from user-facing output.

### Phase 7 — Docs and examples update
Update docs/examples/scripts:
- `.aiki/templates/...` → `.aiki/tasks/...`
- `aiki/default/templates/...` → `aiki/core/templates/...`
- Remove all references to legacy paths.
- Add upgrade note: "If upgrading from pre-1.x, move `.aiki/templates/` to `.aiki/tasks/` manually."

### Phase 8 — Test coverage
Add tests for:
1. Fresh repo uses only `.aiki/tasks/*.md` for user templates.
2. Built-in templates load from `aiki/core/templates/*.md`.
3. Old paths are not recognized (no fallback).
4. `aiki init` creates `.aiki/tasks/` directory.

## Acceptance criteria
1. Built-in templates stored in `aiki/core/templates/**/*.md`.
2. User template location is `.aiki/tasks/**/*.md`.
3. No legacy path support (hard switch).
4. Docs/examples only show canonical format.
5. Tests cover new structure only.
6. `aiki init` creates `.aiki/tasks/` directory.

## Risks / caveats
- **Breaking change:** Existing repos with `.aiki/templates/` will break immediately.
- Users must manually migrate by renaming `.aiki/templates/` to `.aiki/tasks/`.
- External scripts hardcoding legacy paths will fail.
- This should be part of a major version bump (e.g., 1.0 or 2.0).

## User migration path
Users upgrading from older versions must:
```bash
# Manual migration (one-time)
mv .aiki/templates .aiki/tasks
```

This can be documented in release notes and upgrade guides.

## Immediate next steps
1. Build the inventory/change map and identify exact files.
2. Move built-in templates from `aiki/default` to `aiki/core`.
3. Implement constants and update loader (remove fallback logic).
4. Update `aiki init` to create `.aiki/tasks/`.
5. Update docs/examples in same PR.
6. Add tests for new structure only.

## Suggested rollout
1. Implement all changes in a single PR.
2. Release as part of a major version bump.
3. Include clear upgrade instructions in release notes.
4. No deprecation window needed (hard switch).
