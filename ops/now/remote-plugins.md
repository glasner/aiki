# Remote Plugin Installation

## Summary

Make plugins installable from GitHub repositories. Plugins are fetched as shallow clones into `~/.aiki/plugins/` and resolved at runtime alongside project-level templates.

## Plugin Reference Format

Plugin identity follows `owner/repo`, mapping directly to GitHub:

```
aiki plugin install owner/repo
```

Internally, references carry a host: `github.com/owner/repo`. The host is hidden from users — `github.com` is the implicit default. Git operations use HTTPS URLs: `https://github.com/{owner}/{repo}.git`. An explicit host is supported for future extensibility:

```
aiki plugin install gitlab.com/myorg/security   # explicit host (future)
```

**Host detection:** If the first segment contains a dot, it's treated as a host. Otherwise, `github.com` is implied.

**Host restriction (current):** Only `github.com` is supported. Explicit non-GitHub hosts (e.g. `gitlab.com/myorg/security`) are rejected with an error: `"Non-GitHub hosts are not yet supported"`. This avoids storage collisions — see Storage section for details.

## Namespace Alias

The `aiki` namespace maps to the `glasner` GitHub owner. This is hardcoded and will never be configurable.

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

A repo **is** a plugin. The repo root contains `hooks.yaml` and/or `templates/` directly, plus an optional `deps` file:

```
# github.com/glasner/way repo contents:
├── deps                ← optional, declares plugin dependencies
├── hooks.yaml
└── templates/
    ├── fix.md
    └── review.md
```

One repo = one plugin. No monorepos, no subdirectory extraction.

## Plugin Dependencies

Plugins can declare dependencies on other plugins via a `deps` file in the repo root.

### `deps` file format

One plugin reference per line. Blank lines and `#` comments allowed:

```
# deps
aiki/core
somecorp/utils
```

References follow the same format as `aiki plugin install` arguments (including explicit hosts).

No `deps` file = no dependencies. Most plugins won't have one.

### Transitive resolution

Dependencies are resolved recursively. If `aiki/way` depends on `aiki/core`, and `aiki/core` depends on `aiki/base`, installing `aiki/way` installs all three.

**Cycle detection:** Track visited plugins during resolution. If a plugin appears twice in the walk, skip it (no error).

**Diamond dependencies:** If `A → C` and `B → C`, `C` is installed once. The set of plugins to install is deduplicated before any cloning happens.

## Storage

Plugins are installed **user-level** at `~/.aiki/plugins/`. They are shallow clones (`git clone --depth 1`).

**Storage keys on `namespace/plugin` only** — host is not part of the path. This is safe because only `github.com` is currently accepted. When multi-host support is added, the layout must change to `~/.aiki/plugins/{host}/{ns}/{repo}/` to avoid collisions (e.g. `github.com/acme/tool` vs `gitlab.com/acme/tool`).

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
    │       ├── deps          # contains: aiki/core
    │       ├── hooks.yaml
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

The reference is the path relative to `.aiki/templates/`, minus `.md`. No special namespaces, no reserved words. Project templates always win.

## Commands

### `aiki plugin install [reference]`

With argument — install a specific plugin and its dependencies:

```bash
aiki plugin install aiki/way
# 1. git clone --depth 1 https://github.com/glasner/way.git ~/.aiki/plugins/aiki/way
# 2. Read ~/.aiki/plugins/aiki/way/deps
# 3. Install any listed plugins not already present (recursively)
```

Output:

```
Installed: aiki/way
Installed (dependency): aiki/core
```

Already-installed plugins are skipped silently.

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

With argument — update a specific plugin and reconcile its dependencies:

```bash
aiki plugin update aiki/way
# 1. git -C ~/.aiki/plugins/aiki/way pull
# 2. Re-read deps (may have changed after pull)
# 3. Install any new deps not already present (recursively)
# 4. Update existing deps
```

Deps that were removed from the `deps` file are **not** auto-deleted — they may be used by other plugins. Use `aiki plugin remove` to clean up.

Without argument — update all installed plugins and reconcile dependencies:

```bash
aiki plugin update
# 1. git pull each installed plugin
# 2. Re-read all deps files
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

Removes the plugin and any of its dependencies that are no longer needed:

```bash
aiki plugin remove aiki/way
# 1. Read aiki/way/deps → [aiki/core]
# 2. rm -rf ~/.aiki/plugins/aiki/way
# 3. For each dep (aiki/core):
#    - Scan all remaining installed plugins' deps files
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
1. `~/.aiki/plugins/*/*/deps` — other installed plugins
2. `.aiki/` project references (if inside a repo) — project-level usage

A dependency is only removed if it appears in neither.

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
fn derive_required_plugins(aiki_dir: &Path) -> Vec<PluginRef>
fn check_installed(plugins: &[PluginRef]) -> Vec<(PluginRef, InstallStatus)>
fn install_missing(plugins: &[PluginRef]) -> Result<()>

// Dependency resolution
fn parse_deps(plugin_dir: &Path) -> Vec<PluginRef>
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
- **Non-GitHub hosts** — Host detection parses dots in the first segment, but non-GitHub hosts are rejected at install time. When added, storage layout must include host in path to prevent collisions.
- **Full plugin metadata** — No `plugin.yaml`. The `deps` file handles dependencies; identity comes from directory convention. A full manifest can be introduced later if more metadata is needed.
- **Plugin search/discovery** — No registry. Users share plugin references directly.
- **Dependency version constraints** — Deps are unversioned. All plugins track their default branch. Pinning a dep to a tag/branch is deferred.
