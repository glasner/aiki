# Remote Plugin Installation

## Summary

Make plugins installable from GitHub repositories. Plugins are fetched as shallow clones into `~/.aiki/plugins/` and resolved at runtime alongside project-level templates.

## Plugin Reference Format

Plugin identity follows `owner/repo`, mapping directly to GitHub:

```
aiki plugin install owner/repo
```

Only GitHub is supported. Git operations use HTTPS URLs: `https://github.com/{owner}/{repo}.git`. References that look like explicit hosts (first segment contains a dot) are rejected with an error: `"Only GitHub plugins are supported"`.

## Namespace Alias

`aiki` is a **reserved namespace** that maps to the `glasner` GitHub owner. This is hardcoded and will never be configurable.

```
aiki plugin install aiki/way
# Fetches: https://github.com/glasner/way.git
# Stored:  ~/.aiki/plugins/aiki/way/
```

For all other namespaces, owner = namespace:

```
aiki plugin install somecorp/security
# Fetches: https://github.com/somecorp/security.git
# Stored:  ~/.aiki/plugins/somecorp/security/
```

## Repo Structure

A repo **is** a plugin. The repo root contains `hooks.yaml` and/or `templates/` directly:

```
# github.com/glasner/way repo contents:
├── hooks.yaml
└── templates/
    ├── fix.md
    └── review.md
```

One repo = one plugin. No monorepos, no subdirectory extraction.

## Plugin Dependencies

Dependencies are **auto-derived** — no manifest file needed. The same three-part reference scanning used for project-level derivation (`ns/plugin/template`) is applied to a plugin's own contents. Any three-part reference found in a plugin's `hooks.yaml` or markdown files under `templates/` (recursively) implies a dependency on `ns/plugin`.

### How it works

A plugin's `hooks.yaml` and templates may reference other plugin templates:

```yaml
# hooks.yaml in aiki/way
review:
  template: aiki/core/review-base    # ← implies dependency on aiki/core
```

```markdown
<!-- templates/fix.md in aiki/way -->
{{> aiki/core/preamble}}             <!-- ← implies dependency on aiki/core -->
```

Scanning extracts `aiki/core` as a dependency. Self-references (references to the plugin's own namespace/plugin) are ignored.

### Transitive resolution

Dependencies are resolved recursively. If `aiki/way` references `aiki/core`, and `aiki/core` references `aiki/base`, installing `aiki/way` installs all three.

**Cycle detection:** Track visited plugins during resolution. If a plugin appears twice in the walk, skip it (no error).

**Diamond dependencies:** If `A → C` and `B → C`, `C` is installed once. The set of plugins to install is deduplicated before any cloning happens.

### Shared scanning function

The same `derive_plugin_refs(dir)` function works on both project `.aiki/` dirs and plugin dirs — it scans YAML files and all markdown files under `templates/` (recursively) for three-part references and returns unique `namespace/plugin` pairs. This means zero new file formats and zero extra work for plugin authors.

## Storage

Plugins are installed **user-level** at `~/.aiki/plugins/`. They are shallow clones (`git clone --depth 1`).

**Storage keys on `namespace/plugin` only** — host is not part of the path. This is safe because only `github.com` is supported.

```
~/.aiki/
└── plugins/
    ├── aiki/
    │   ├── core/             # cloned from https://github.com/glasner/core.git (dep of way)
    │   │   ├── .git/
    │   │   └── templates/
    │   │       └── base.md
    │   └── way/              # cloned from https://github.com/glasner/way.git
    │       ├── .git/
    │       ├── hooks.yaml    # references aiki/core/review-base → dep on aiki/core
    │       └── templates/
    │           ├── fix.md
    │           └── review.md
    └── somecorp/
        └── security/         # cloned from https://github.com/somecorp/security.git
            ├── .git/
            ├── hooks.yaml
            └── templates/
                └── audit.md
```

## Template Resolution

Resolution checks project-level first, then user-level plugins. The reference format determines where to look.

### Resolution Order

```
1. .aiki/templates/{reference}.md        (project-local override/custom)
2. ~/.aiki/plugins/{ns}/{plugin}/templates/{template}.md   (installed plugin)
```

### Reference Examples

```
# Project-local templates (any path depth)
"custom"                → .aiki/templates/custom.md
"my/custom"             → .aiki/templates/my/custom.md
"deploy/checklist"      → .aiki/templates/deploy/checklist.md

# Plugin templates (three-part: namespace/plugin/resource)
"aiki/way/review"       → ~/.aiki/plugins/aiki/way/templates/review.md
"somecorp/security/audit" → ~/.aiki/plugins/somecorp/security/templates/audit.md

# Project override of a plugin template
"aiki/way/review"       → .aiki/templates/aiki/way/review.md  (wins over plugin)
```

The reference is the path relative to `.aiki/templates/`, minus `.md`. The `aiki` namespace is reserved (see Namespace Alias). Project templates always win.

## Commands

### `aiki plugin install [reference]`

With argument — install a specific plugin and its dependencies:

```bash
aiki plugin install aiki/way
# 1. git clone --depth 1 https://github.com/glasner/way.git ~/.aiki/plugins/aiki/way
# 2. Scan hooks.yaml + templates/**/*.md for three-part references
# 3. Install any referenced plugins not already present (recursively)
```

Output:

```
Installed: aiki/way
Installed (dependency): aiki/core
```

Already-installed plugins (directory exists with `.git/`) are skipped silently. Partial directories (no `.git/`) are removed and re-cloned.

Without argument (outside a repo) — exit with error: `"Not in an aiki project. Use 'aiki plugin install <reference>' to install a specific plugin."`.

Without argument (inside a repo) — derive required plugins from project references and install all missing, including their dependencies:

```bash
aiki plugin install
# Scans .aiki/ for template references
# Derives unique namespace/plugin pairs from three-part refs (ns/plugin/template)
# Installs any that are missing from ~/.aiki/plugins/
# Recursively installs deps of each installed plugin
```

This same derivation logic is shared by `aiki init` and `aiki doctor`.

### `aiki plugin update [reference]`

With argument — update a specific plugin and reconcile its dependencies. If the plugin is not installed, exit with error: `"Plugin {ref} is not installed"`.


```bash
aiki plugin update aiki/way
# 1. git -C ~/.aiki/plugins/aiki/way pull
# 2. Re-scan for references (may have changed after pull)
# 3. Install any newly-referenced plugins not already present (recursively)
# 4. Update existing deps
```

References that disappear after an update don't trigger auto-deletion — the formerly-referenced plugin may be used by others. Use `aiki plugin remove` to clean up.

Without argument — update all installed plugins and reconcile dependencies:

```bash
aiki plugin update
# 1. git pull each installed plugin
# 2. Re-scan all plugins for references
# 3. Install any newly-required deps
# 4. Update existing deps
```

No auto-update. Updates are always explicit.

### `aiki plugin list`

Inside a repo — shows project context:

```
Plugins:
  aiki/way            installed
    └ aiki/core       installed (dependency)
  somecorp/security   installed
  cooldev/linter      not installed    ← aiki plugin install cooldev/linter

Overrides:
  aiki/way/review → .aiki/templates/aiki/way/review.md
```

Outside a repo — lists all installed:

```
Installed (~/.aiki/plugins/):
  aiki/way
    └ aiki/core       (dependency)
  somecorp/security
```

### `aiki plugin remove <reference>`

If the plugin is not installed, exit with error: `"Plugin {ref} is not installed"`.

Removes the plugin and any of its dependencies that are no longer needed:

```bash
aiki plugin remove aiki/way
# 1. Scan aiki/way for references → [aiki/core, somecorp/utils]
# 2. rm -rf ~/.aiki/plugins/aiki/way
# 3. For each dep:
#    - Scan all remaining installed plugins for references to it
#    - Also check project .aiki/ references
#    - If nothing else references it → remove it too (recursively)
#    - If still referenced → keep it
```

Output:

```
Removed: aiki/way
Removed (unused dependency): somecorp/utils
Kept (still needed): aiki/core ← used by cooldev/linter
```

**Reverse dependency check** scans two sources:
1. All remaining installed plugins' `hooks.yaml` and `templates/**/*.md`
2. `.aiki/` project references (if inside a repo) — project-level usage

A dependency is only removed if it appears in neither.

## Error Handling

**No rollback.** Install and update operations are not transactional. If a dependency fails to clone or pull, already-installed plugins remain on disk. The command reports which plugins succeeded and which failed, then exits with a non-zero status:

```
Installed: aiki/way
Error: failed to clone aiki/core — repository not found
```

Re-running the same command retries failed plugins (they aren't installed yet, so they won't be skipped). This is the remediation path — no manual cleanup needed.

**Integrity check.** A plugin directory is considered installed only if it contains a `.git/` directory. Directories without `.git/` (e.g. from a previously interrupted clone) are treated as not installed — install will remove the partial directory and re-clone.

## Deriving Required Plugins

The core derivation function scans project `.aiki/` files for template references, extracts unique `namespace/plugin` pairs from three-part refs (`ns/plugin/template`), and checks installation status.

References are found in:
- Hook YAML files (template references like `review: aiki/way/review`)
- Project configuration that names templates

Three shared consumers:
1. `aiki plugin install` (no args) — derive + install missing (with deps)
2. `aiki init` — scaffold project + derive + install missing (with deps)
3. `aiki doctor` — derive + report status (including missing deps)

```rust
// Core scanning — works on both project .aiki/ dirs and plugin dirs
fn derive_plugin_refs(dir: &Path) -> Vec<PluginRef>

fn check_installed(plugins: &[PluginRef]) -> Vec<(PluginRef, InstallStatus)>
fn install_missing(plugins: &[PluginRef]) -> Result<()>

// Dependency resolution (uses derive_plugin_refs internally)
fn resolve_deps_recursive(root: &PluginRef, installed: &HashSet<PluginRef>) -> Vec<PluginRef>
fn reverse_deps(plugin: &PluginRef, plugins_dir: &Path) -> Vec<PluginRef>
```

## Integration with Plugin Directory Design

This document extends [plugin-directory.md](plugin-directory.md). Key changes to that design:

| Aspect | plugin-directory.md | This doc |
|--------|-------------------|----------|
| Plugin storage | `.aiki/plugins/` (project-level) | `~/.aiki/plugins/` (user-level) |
| Project templates | Inside plugin dirs | `.aiki/templates/` (flat override layer) |
| Resolution | Plugin dirs only | Project templates → user-level plugins |
| Installation | Manual | `aiki plugin install` (fetch from GitHub) |

## Deferred

- **Versioning/pinning** — No tags, branches, or lockfiles. Always fetches default branch. Add when the first user complains.
- **Non-GitHub hosts** — Only GitHub is supported. If added later, storage layout must include host in path to prevent collisions.
- **Full plugin metadata** — No `plugin.yaml` or manifest. Dependencies are auto-derived from content references. Identity comes from directory convention. A manifest can be introduced later if explicit metadata is needed.
- **Plugin search/discovery** — No registry. Users share plugin references directly.
- **Dependency version constraints** — Deps are unversioned. All plugins track their default branch. Pinning a dep to a tag/branch is deferred.
