---
status: draft
---

# Plugin Directory Structure

## Summary

Unify hooks and templates under a plugin-based directory structure. Plugins are organized under namespaces, with each plugin containing a single `hooks.yaml` file and a `templates/` folder.

## Final Structure

```
.aiki/
└── plugins/
    └── {namespace}/
        └── {plugin}/
            ├── hooks.yaml           # all hooks for this plugin (single file, optional)
            └── templates/           # template files (optional)
                ├── fix.md
                └── review.md
```

No metadata file, no versioning. Just hooks and templates.

### Concrete Example

```
.aiki/
└── plugins/
    ├── aiki/                        # namespace (official)
    │   ├── way/                     # plugin: "the aiki way"
    │   │   ├── hooks.yaml
    │   │   └── templates/
    │   │       ├── fix.md           # aiki/way/fix
    │   │       └── review.md        # aiki/way/review
    │   │
    │   └── advanced/                # plugin: advanced workflows
    │       ├── hooks.yaml
    │       └── templates/
    │           └── deep-review.md   # aiki/advanced/deep-review
    │
    ├── eslint/                      # namespace (community)
    │   └── standard/                # plugin
    │       ├── hooks.yaml
    │       └── templates/
    │           └── lint-report.md   # eslint/standard/lint-report
    │
    └── mycompany/                   # namespace (org-specific)
        ├── core/                    # plugin
        │   ├── hooks.yaml
        │   └── templates/
        │       └── pr-template.md   # mycompany/core/pr-template
        └── security/                # plugin
            ├── hooks.yaml
            └── templates/
                └── audit.md         # mycompany/security/audit
```

## Reference Syntax

Three-part references: `{namespace}/{plugin}/{resource}`

```
aiki/way/review          → plugins/aiki/way/templates/review.md
aiki/way/fix             → plugins/aiki/way/templates/fix.md
aiki/advanced/deep-review → plugins/aiki/advanced/templates/deep-review.md
eslint/standard/lint-report → plugins/eslint/standard/templates/lint-report.md
```

## Plugin Anatomy

A plugin is a directory containing:

| File/Folder | Required | Purpose |
|-------------|----------|---------|
| `hooks.yaml` | No | Hook definitions for this plugin |
| `templates/` | No | Template markdown files |

At least one of `hooks.yaml` or `templates/` should exist.

### hooks.yaml

Single file containing all hook definitions for the plugin:

```yaml
# plugins/aiki/way/hooks.yaml
on:
  session.started:
    - run: echo "Way plugin loaded"

  turn.completed:
    - review: aiki/way/review
      when: "{{ changes.files | length > 0 }}"
```

## Migration from Current Structure

### Current
```
.aiki/
├── templates/
│   └── aiki/
│       ├── fix.md
│       └── review.md
└── hooks/
    └── aiki/
        └── core/
            └── hooks.yaml
```

### After Migration
```
.aiki/
└── plugins/
    └── aiki/
        └── way/                    # or "core" - pick a plugin name
            ├── hooks.yaml          # moved from hooks/aiki/core/hooks.yaml
            └── templates/
                ├── fix.md          # moved from templates/aiki/fix.md
                └── review.md       # moved from templates/aiki/review.md
```

### Breaking Change

References change from two-part to three-part:
- `aiki/review` → `aiki/way/review`
- `aiki/fix` → `aiki/way/fix`

## Files to Update

### Template Resolution

**File:** `cli/src/tasks/templates/resolver.rs`

Change path resolution:
```rust
// Old: "aiki/review" → .aiki/templates/aiki/review.md
// New: "aiki/way/review" → .aiki/plugins/aiki/way/templates/review.md

fn resolve_template_path(name: &str, plugins_dir: &Path) -> Result<PathBuf> {
    let parts: Vec<&str> = name.splitn(3, '/').collect();
    if parts.len() != 3 {
        return Err(AikiError::InvalidTemplateName { ... });
    }
    let (namespace, plugin, template) = (parts[0], parts[1], parts[2]);

    let full_path = plugins_dir
        .join(namespace)
        .join(plugin)
        .join("templates")
        .join(format!("{}.md", template));

    // ...
}
```

### Hook Resolution

**File:** `cli/src/flows/hook_resolver.rs`

Change path resolution:
```rust
// Old: "aiki/session-start" → .aiki/hooks/aiki/session-start.yml
// New: "aiki/way/session-start" → .aiki/plugins/aiki/way/hooks.yaml (then find handler)

// Actually, hooks.yaml is loaded per-plugin, not per-hook
// The hook reference might just be plugin-level: "aiki/way"
```

**Open question:** How do hook references work now?
- Option A: Reference the plugin, load its `hooks.yaml`: `plugins: [aiki/way, eslint/standard]`
- Option B: Keep hook references but resolve to `hooks.yaml`: `aiki/way/turn-complete` → load `hooks.yaml`, find `turn.completed` handler

### Init Command

**File:** `cli/src/commands/init.rs`

```rust
// Create default plugin structure
fs::create_dir_all(aiki_dir.join("plugins/aiki/way/templates"))?;
```

### Hook Loading

**File:** `cli/src/flows/loader.rs` (or similar)

Need to:
1. Discover all plugins (scan `plugins/{namespace}/{plugin}/`)
2. Load each plugin's `hooks.yaml`
3. Register handlers by event type

## User-Level Plugins

```
~/.aiki/
└── plugins/
    └── personal/
        └── daily/
            ├── hooks.yaml
            └── templates/
                └── standup.md      # personal/daily/standup
```

**Resolution priority:**
1. Project: `.aiki/plugins/{ns}/{plugin}/...`
2. User: `~/.aiki/plugins/{ns}/{plugin}/...`

## Implementation Tasks

1. [ ] Define plugin discovery/loading logic
2. [ ] Update `resolver.rs` - three-part template references
3. [ ] Update `hook_resolver.rs` - plugin-based hook loading
4. [ ] Update hook engine to load `hooks.yaml` per plugin
5. [ ] Update `init.rs` - create `plugins/aiki/way/` structure
6. [ ] Migrate existing templates and hooks
7. [ ] Update all existing references (two-part → three-part)
8. [ ] Update tests
9. [ ] Update documentation

## Open Questions

1. **Default plugin name?** What should `aiki/???/review` be? Options:
   - `aiki/way/review` - "the aiki way"
   - `aiki/core/review` - core functionality

2. **Hook activation:** How do users enable/disable plugins?
   - All discovered plugins active by default?
   - Per-project config to disable specific plugins?

## Deferred

- **Versioning** - Add later if plugin ecosystem develops
- **Plugin metadata** - `plugin.yaml` for name, description, author
- **Plugin installation** - `aiki plugin add eslint/standard`
