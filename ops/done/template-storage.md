# Template Storage & Lifecycle

**Date**: 2026-03-06
**Status**: Ready
**Purpose**: Define how built-in templates are installed, upgraded, and protected from accidental overwrites.

**Related Documents**:
- [Current built-in templates](../../.aiki/templates/aiki/) - Will move to `.aiki/templates/` root
- [aiki/default plugin](../../cli/src/flows/plugins/aiki_default.yaml) - Plugin that will own these templates
- [Template Marketplace (future)](../future/templates/template-marketplace.md) - External template distribution
- [init.rs](../../cli/src/commands/init.rs) - Current init flow

---

## Executive Summary

Built-in templates are part of the **`aiki/default` plugin** — the same plugin that already ships hooks. Templates are embedded in the CLI binary and installed to `.aiki/templates/` (root, no namespace prefix) so users reference them as just `plan`, `review/task`, etc. A **manifest** at `.aiki/.manifest.json` tracks checksums per plugin so the system can detect user modifications and skip those files during upgrades. The entire upgrade process is **invisible** — it runs during the auto-init on every session start.

The `aiki/default` plugin gets **special handling**: its templates install to the templates root rather than a namespaced subdirectory. Other plugins install to `.aiki/templates/{ns}/{plugin}/` and use three-part refs (`ns/plugin/template`).

---

## Scope

**In scope (implementation):** `aiki/default` plugin templates — the built-in templates that ship with the CLI binary.

**In scope (design):** The manifest and sync mechanism are plugin-aware from day one, so any plugin can ship managed templates using the same infrastructure.

**Out of scope:** Plugin template sync for non-builtin plugins (uses the same manifest, but the plugin system provides the source templates). User-created templates not tracked in the manifest are never touched.

---

## How It Works

### Concepts

| Term | Definition |
|------|-----------|
| **Source template** | The canonical version embedded in the CLI binary (for `aiki/default`) or in a plugin directory |
| **Installed template** | The copy on disk at `.aiki/templates/{path}.md` |
| **Manifest** | `.aiki/.manifest.json` — tracks what was installed, by which plugin, and its checksum |
| **Clean** | Installed template matches its manifest checksum (user hasn't modified it) |
| **Dirty** | Installed template differs from its manifest checksum (user has modified it) |

### Template installation paths

| Plugin | Install path | Reference syntax | Example |
|--------|-------------|-----------------|---------|
| `aiki/default` (special) | `.aiki/templates/{name}.md` | `{name}` | `plan`, `review/task` |
| Other plugins | `.aiki/templates/{ns}/{plugin}/{name}.md` | `{ns}/{plugin}/{name}` | `intel/toolkit/new-target` |

The `aiki/default` plugin installs to the templates root — no namespace directory. This is special-cased because these are the foundational templates that every project uses. Users just say `plan`, not `aiki/default/plan`.

### Manifest File

`.aiki/.manifest.json` — a shared repo manifest. The `templates` key tracks managed templates per plugin. Other top-level keys will be added later for non-template state.

```json
{
  "schema": 1,
  "templates": {
    "aiki/default": {
      "source_version": "0.4.2",
      "install_root": ".",
      "files": {
        "plan.md": {
          "checksum": "sha256:a1b2c3d4...",
          "version": "1.0.0",
          "installed_at": "2026-03-06T12:00:00Z"
        },
        "review/task.md": {
          "checksum": "sha256:e5f6g7h8...",
          "version": "3.0.0",
          "installed_at": "2026-03-06T12:00:00Z"
        }
      }
    },
    "intel/toolkit": {
      "source_version": "1.2.0",
      "install_root": "intel/toolkit",
      "files": {
        "new-target.md": {
          "checksum": "sha256:x9y8z7w6...",
          "version": "1.0.0",
          "installed_at": "2026-03-06T12:00:00Z"
        }
      }
    }
  }
}
```

**Top-level fields:**
- `schema` — manifest format version (for future migration)
- `templates` — managed templates keyed by plugin ref (`namespace/name`)

**Per-plugin fields:**
- `source_version` — version of the plugin/CLI that installed them
- `install_root` — relative path under `.aiki/templates/` where files are written. `"."` for `aiki/default` (root), `"intel/toolkit"` for other plugins
- `files` — keyed by relative path within the install root

**Per-file fields:**
- `checksum` — SHA-256 of the file content as written to disk
- `version` — template's frontmatter version (for display)
- `installed_at` — ISO 8601 timestamp

**Rules:**
- Any plugin in the manifest is "managed" — sync upgrades clean templates, skips dirty ones
- Templates from plugins **not tracked** in the manifest are user-owned — never touched by any sync operation
- All `install_root` values and template relative paths must be validated to not contain `..` segments or resolve outside `.aiki/templates/`. Sync must reject and skip any plugin whose paths fail validation.
- The manifest should be `.gitignore`'d — it's local installation state

### Why templates live on disk

Templates are written to `.aiki/templates/` so users can:
- **Read them** to understand what a template does before running it
- **Copy them** as a starting point for customization
- **Modify them** directly (the system detects this and stops upgrading that file)

The binary is the source of truth; the disk copy is a readable cache that stays current automatically.

---

## Lifecycle

### Install (first `aiki init`)

```
For each aiki/default template embedded in the binary:
  1. Compute SHA-256 of the source content
  2. If file doesn't exist on disk → write it, add to manifest
  3. If file exists on disk AND no manifest entry → first-time adoption:
     - Hash the on-disk file
     - Compare on-disk hash to the source hash
     - If match → add to manifest with the source checksum (clean state —
       on-disk content matches manifest, future syncs will upgrade normally)
     - If differ → add to manifest with the SOURCE checksum, NOT the
       on-disk checksum (effectively dirty — on-disk content differs from
       manifest checksum, so future syncs will skip this file)

     **Trade-off:** Recording the source checksum when content differs is
     conservative-correct: it protects user edits from being overwritten.
     The downside is that outdated-but-unmodified templates (e.g., user
     never edited the file, but it's from an older CLI version) also get
     marked dirty and won't auto-upgrade. Users in this situation can
     reset to the latest version — see "User wants to reset a modified
     template" below.

  4. If file exists AND manifest entry exists → handled by sync (below)
```

**First-time adoption** handles migration from current state (templates in `.aiki/templates/aiki/`, no manifest) to the new layout (templates in `.aiki/templates/` root). On first init after this feature ships:
1. Move existing files from `.aiki/templates/aiki/` to `.aiki/templates/`. If destination already exists, hash both files: if identical, skip the move; if different, preserve the legacy file as `{name}.backup-legacy` and warn in non-quiet mode
2. Create manifest entries for **all embedded templates** at their destination paths — whether moved or already present — using the same adoption logic as Install step 3
3. Clean up empty `.aiki/templates/aiki/` directory

### Sync (every session start, invisible)

Runs during `aiki init --quiet` which fires on every session start. **No output in quiet mode** — completely invisible to users. **Exception:** unsupported manifest schema emits a warning to stderr even in quiet mode, since silencing it would leave users with permanently stale templates and no signal.

**Precondition:** Migration from the old layout (`.aiki/templates/aiki/`) must have completed before sync runs. See the Migration section for ordering details.

```
1. Load manifest from .aiki/.manifest.json (or create empty one)
2. For plugin "aiki/default" (install_root: "."):
   a. Get or create plugin entry in manifest
   b. For each embedded template:
      i.   Compute SHA-256 of embedded source
      ii.  Look up manifest entry for this file
      iii. If no entry AND file does not exist on disk → fresh install (write file, add to manifest)
      iv.  If no entry AND file exists on disk → adoption (compare on-disk hash to source;
           if match → add to manifest as clean; if differ → add with source checksum, effectively dirty)
      v.   If entry exists AND file does not exist on disk → re-install
           (write file from source, update manifest checksum)
      vi.  If source checksum == manifest checksum → up to date, skip
      vii. If source checksum != manifest checksum (new version):
           - Hash the on-disk file
           - If on-disk hash == manifest checksum → CLEAN → silently overwrite,
             update manifest
           - If on-disk hash != manifest checksum → DIRTY → silently skip
   c. Prune removed templates: for each file in this plugin's manifest `files`
      that is NOT in the current embedded source set:
      - Remove the manifest entry (leave on-disk file untouched — it becomes user-owned)
      (See "Template removed from CLI" in Edge Cases for the rationale.)
3. Save manifest (atomic write: temp file + rename to prevent corruption from concurrent writes)
```

The same loop works for other plugins — just change the source (plugin directory instead of embedded binary) and the `install_root`.

**Performance:** ~12 templates, each 2-10KB. The work is: 1 manifest read, ~12 SHA-256 hashes (microseconds), 0-2 file writes on version bumps. Well under 1ms total.

### User modifies a template

1. User edits `.aiki/templates/plan.md`
2. On next session start, sync detects on-disk hash != manifest checksum
3. Sync silently skips that template — user's changes are preserved forever
4. Future CLI versions with updated `plan.md` will also skip it

### User wants to reset a modified template

1. Delete the file: `rm .aiki/templates/plan.md`
2. Next session start (or `aiki init`), sync sees "no file on disk" → writes fresh copy from binary

No special command needed.

### Adoption trade-off: protecting edits vs. auto-upgrading stale files

When a file exists on disk without a manifest entry (first-time adoption), the system cannot distinguish between "user intentionally edited this template" and "this template is simply from an older CLI version and was never touched." Both cases show a checksum mismatch against the current source.

The system records the **source** checksum in both cases, which means on-disk content differs from the manifest and future syncs will skip the file. This is conservative-correct: it guarantees user edits are never lost. The cost is that stale-but-unmodified templates won't auto-upgrade until the user resets them (delete the file and re-run `aiki init`).

This is the right default because silently overwriting a user-edited template is destructive and hard to recover from, while a stale template is merely inconvenient and easy to fix.

This same conservative behavior applies to **corrupt-manifest recovery** — when the manifest is corrupt or unparseable, the system re-runs first-time adoption for all embedded templates, which means stale-but-unmodified files will again be marked dirty. See the "Manifest corrupt or unparseable" row in the Edge Cases table.

### Template removed from CLI in future version

If a template exists in the manifest but is no longer embedded in the binary:
- Leave the on-disk file untouched
- Remove the manifest entry
- The template continues to work as a user-owned file

This is implemented by the prune step (step 2.c) in the sync pseudocode above.

---

## User Experience

### On `aiki init` (new repo, non-quiet)

```
$ aiki init
Initializing Aiki in: /path/to/repo
✓ Initialized JJ repository
✓ Configured Git hooks
✓ Installed 12 built-in templates
✓ Repository initialized successfully!
```

### On session start (auto-init, quiet) — the normal case

Nothing. Templates silently stay current. User never sees upgrade output.

### On `aiki init` (re-init, non-quiet, templates changed)

```
$ aiki init
Repository already initialized at /path/to/repo
✓ Updated 3 built-in templates
⚠ 1 template modified locally (skipped): fix.md
```

### Template references (before → after)

| Before | After |
|--------|-------|
| `aiki/plan` | `plan` |
| `aiki/review/task` | `review/task` |
| `aiki/fix` | `fix` |
| `aiki/loop` | `loop` |

### Doctor integration

```
$ aiki doctor
...
Templates (aiki/default):
  ✓ 11 templates up to date
  ⚠ 1 template modified locally: fix.md
    (delete and re-init to reset)
```

---

## Migration

### Moving from `.aiki/templates/aiki/` to `.aiki/templates/`

**Ordering:** Migration runs as the first step of init, before sync. This ensures user-modified templates in the old location are moved to the new location before sync runs. If sync ran first, it would write embedded templates to the new paths; migration's collision handling would preserve differing legacy files as backups, but the destination would contain the embedded version rather than the user's migrated content, which is incorrect.

On first init after this change ships:

1. Detect old layout: `.aiki/templates/aiki/` exists, no manifest
2. For each file in `.aiki/templates/aiki/`:
   - Move to `.aiki/templates/` (same relative path minus the `aiki/` prefix)
   - If destination already exists, hash both files (legacy source and existing destination):
     - If identical → skip the move (no data loss, files are the same)
     - If different → preserve the legacy file as `{name}.backup-legacy` alongside the destination (e.g., `plan.md.backup-legacy`) and log a warning in non-quiet mode so the user knows to reconcile
   - In either collision case, the file at the destination path will be adopted in step 4.
3. Remove empty `.aiki/templates/aiki/` directory
4. Create manifest entries for all embedded source templates (not just
   moved files). For each embedded template, check the file at its
   destination path in `.aiki/templates/` — whether it arrived there
   via step 2 (moved) or was already present (skipped) — and apply
   the same adoption logic as Install step 3:
   - Compute the SHA-256 of the on-disk content
   - Compare to the embedded source hash
   - If match → record the source checksum (clean state — future syncs
     will upgrade this file normally)
   - If differ → record the source checksum (dirty state — on-disk
     content != manifest checksum, future syncs will skip this file,
     preserving the user's modifications)
5. Update all template references in code: `aiki/plan` → `plan`, etc.

### Backward compatibility

During the transition period, the template resolver should check both locations:
1. `.aiki/templates/{name}.md` (new)
2. `.aiki/templates/aiki/{name}.md` (legacy fallback)

This ensures existing repos work before running `aiki init` with the new CLI.

**Sunset condition:** The legacy fallback is only active when no manifest file
(`.aiki/.manifest.json`) exists. Once the manifest is created by `aiki init`,
the fallback is disabled — the manifest proves migration has completed.

> **Note:** If the legacy directory `.aiki/templates/aiki/` still exists after
> migration, `aiki doctor` should report it as cleanup-eligible so the user can
> remove the stale directory.

#### Ref normalization

Existing automation and scripts may reference templates using the old `aiki/plan`
ref syntax. After migration, these refs would fail because the resolver expects
`plan`, not `aiki/plan`.

To handle this, the template resolver normalizes legacy refs by stripping the
`aiki/` prefix — but **only** when the remaining path resolves to a known
`aiki/default` plugin template name. This is a resolver-level concern (applied
at lookup time), not a storage-level transformation.

**Normalization rules:**

1. The ref starts with `aiki/`
2. After stripping the prefix, the remaining path matches a template name
   registered by the `aiki/default` plugin (e.g., `plan`, `review/task`, `fix`)

Both conditions must hold. If either fails, the ref passes through unchanged.

**Examples — normalized** (legacy `aiki/default` refs):

- `aiki/plan` → `plan`
- `aiki/review/task` → `review/task`
- `aiki/fix` → `fix`

**Examples — NOT normalized** (pass through unchanged):

- `aiki/toolkit/template` — multi-segment ref under an `aiki` plugin namespace;
  `toolkit/template` is not a known `aiki/default` template name
- `aiki/unknown` — `unknown` is not a registered `aiki/default` template
- `intel/toolkit/new-target` — different plugin namespace entirely

**Deprecation warning:** When normalization is applied, emit a deprecation
warning in non-quiet mode:

```
warning: Template ref 'aiki/plan' uses deprecated syntax. Use 'plan' instead.
```

In quiet mode (session auto-init), the warning is suppressed.

**Sunset timeline:** Remove ref normalization in the next major version after
the one that introduces it. For example, if normalization ships in 0.5.x,
remove it in 1.0.0. This gives users at least one full major version cycle to
update their scripts.

---

## Implementation Plan

### Phase 1: Embed templates in binary

- Add `include_dir` crate to `cli/Cargo.toml`
- Use `include_dir!("../../.aiki/templates/aiki")` to embed the current template directory (this path targets the pre-migration layout; Phase 4 updates it to `../../.aiki/templates` after the move)
- Add module `cli/src/tasks/templates/builtin.rs` exposing the embedded content
- Helper: `fn default_plugin_templates() -> Vec<(&str, &[u8])>` returning `(relative_path, content)` pairs
- Unit test: verify all expected templates are present

### Phase 2: Manifest system

- Define in `cli/src/tasks/templates/manifest.rs`:
  - `RepoManifest`: `schema: u32`, `templates: HashMap<String, PluginTemplateEntry>`
  - `PluginTemplateEntry`: `source_version: String`, `install_root: String`, `files: HashMap<String, FileEntry>`
  - `FileEntry`: `checksum: String`, `version: Option<String>`, `installed_at: String`
- Manifest lives at `.aiki/.manifest.json`
- `install_root` is `"."` for `aiki/default`, `"{ns}/{plugin}"` for others
- Implement `load(repo_root)`, `save(repo_root)` (writes to a temporary file in the same directory, then renames to `.manifest.json`), `checksum(content) -> String`
- Auto-add `.aiki/.manifest.json` to `.gitignore` during init:
  - Check whether `.aiki/.manifest.json` is already covered by an existing `.gitignore` rule (e.g., `.aiki/` or `.aiki/.manifest.json`)
  - If not covered, append `.aiki/.manifest.json` to the repo-root `.gitignore` (create the file if it doesn't exist)
  - In quiet mode, do this silently. In non-quiet mode, include it in the init summary output (e.g., `✓ Added .aiki/.manifest.json to .gitignore`)
- Unit tests for manifest CRUD, checksum comparison, corrupt manifest handling

### Phase 3: Sync flow

- Add generic `sync_plugin_templates(manifest, plugin_ref, install_root, source_templates, templates_dir) -> SyncReport`
  - First step: validate that `install_root` joined with each template's relative path resolves within `templates_dir`. If any path escapes, skip the entire plugin and log a warning.
  - Works for any plugin — `aiki/default` with embedded source, others with plugin directory source
- `SyncReport`: `installed: usize`, `upgraded: usize`, `skipped_dirty: Vec<String>`
- Add `sync_default_templates(repo_root, quiet)` that calls generic sync for `aiki/default`
- Wire into `run()` in `init.rs` — after `.aiki` directory creation and after migration (Phase 4), before plugin install
- In quiet mode: no output. In non-quiet mode: summary line(s)

### Phase 4: Migration + resolver update

- Move templates from `.aiki/templates/aiki/` to `.aiki/templates/` in the repo
- Update `include_dir!` path in `builtin.rs` from `../../.aiki/templates/aiki` to `../../.aiki/templates` to match the new layout
- Update template resolver to look in `.aiki/templates/{name}.md` (no namespace prefix)
- Add ref normalization: strip `aiki/` prefix from refs before resolution (e.g., `aiki/plan` → `plan`), emit deprecation warning in non-quiet mode
- Add legacy fallback: check `.aiki/templates/aiki/{name}.md` if new path not found
- Update all internal template references (`aiki/plan` → `plan`, etc.)
- Add migration logic to init: move files from old layout on first run. **Migration must run before sync (Phase 3)** in the init flow — see Migration section for rationale

### Phase 5: Doctor integration

- Add template health check to `aiki doctor`:
  - Unsupported manifest schema → **prominently report** with upgrade instructions (e.g., `✗ Manifest schema version 2 is not supported by this CLI (supports version 1). Upgrade your CLI to manage templates.`)
  - Missing manifest → "Templates not managed yet, run `aiki init`"
  - Missing on-disk files:
    - **Report mode** (`aiki doctor`): `⚠ Template '{name}' is in manifest but missing from disk. Run 'aiki doctor --fix' to reinstall.`
    - **Fix mode** (`aiki doctor --fix`): Re-install from embedded source, update manifest checksums, report: `✓ Reinstalled template '{name}'`
    - **Quiet mode**: Suppress warning text in report mode; in fix mode, still reinstall but suppress output
  - Dirty templates → informational warning with reset instructions
- `aiki doctor --fix` runs sync (but skips sync when schema is unsupported — cannot safely modify a manifest it doesn't understand)

---

## Edge Cases & Error Handling

| Scenario | Behavior |
|----------|----------|
| Template deleted from disk, still in manifest | `aiki doctor`: report as warning. `aiki doctor --fix`: re-install from embedded source, update manifest checksums. Next sync also re-installs automatically. |
| Manifest corrupt or unparseable | Log warning, re-run first-time adoption for all embedded source templates (the binary's embedded set defines the recovery scope, not the corrupt manifest). On-disk files matching an embedded template are adopted (compared to source; matching = clean, differing = dirty/protected from overwrites). On-disk files not corresponding to any embedded template are ignored as user-owned. Note: this recovery uses the same adoption logic as first-time install — stale-but-unmodified templates will be marked dirty and stop auto-updating. See "Adoption trade-off" section. `aiki doctor` should report templates that may be stale after recovery. |
| No manifest file at all | First-time adoption for all existing files |
| New template added in CLI update | Fresh install (write to disk, add to manifest) |
| Template removed from CLI | Leave on disk, remove from manifest |
| `.aiki/templates/` doesn't exist | Create it during sync |
| User adds custom files to templates root | Ignored by manifest (only tracks known plugin-managed files) |
| Plugin path escapes `.aiki/templates/` | Skip entire plugin sync, log warning. Manifest entry not created/updated. |
| Manifest `schema` field higher than supported | Skip all template sync operations (both read and write) to avoid corrupting data in a format it doesn't understand. Emit a warning to stderr **even in quiet mode** (e.g., `warning: .aiki/.manifest.json uses schema version 2, but this CLI only supports version 1. Template sync skipped — upgrade your CLI.`). `aiki doctor` should detect and prominently report this condition. |
| `.gitignore` already covers `.aiki/.manifest.json` | No-op — skip silently (do not duplicate the rule) |
| Old layout (`.aiki/templates/aiki/`) still present | Migration moves files to root on first init. If a destination file already exists and differs from the legacy file, the legacy file is preserved as `{name}.backup-legacy` and a warning is logged in non-quiet mode. |
| Name collision between `aiki/default` template and user file | Manifest entry exists → managed. No entry → user-owned, never touched |
| Automation uses old `aiki/plan` ref syntax | Ref is normalized (`aiki/plan` → `plan`), deprecation warning logged in non-quiet mode. Sunset: removed in next major version. |
| Sync runs before migration | Must not happen. If sync ran first, it would fresh-install embedded templates to the new location. Migration's collision handling (hash-and-compare with backup) would preserve differing legacy files, but ordering still matters for correctness: the destination file should be the user's migrated content, not a freshly-installed embedded template. Prevented by init ordering: migration always runs before sync. |

---

## Design Decisions

### Why `aiki/default` installs to templates root?

These are the foundational templates every project uses. Forcing users to type `aiki/default/plan` instead of just `plan` adds noise with no benefit. The `aiki/default` plugin is special — it's the only plugin whose templates don't get namespaced. This is analogous to how `aiki/default` hooks are already the "assumed default" in `.aiki/hooks.yml`.

### Why SHA-256 over frontmatter version alone?

The `version` field in frontmatter is human-maintained and could be forgotten or wrong. SHA-256 of the full file content is mechanical — if a single character changes, the checksum detects it. We store both: version for human context in doctor output, checksum for correctness.

### Why .gitignore the manifest?

It's local installation state. Each developer's machine tracks its own installed state independently. The templates themselves can be in git; the manifest tracks "what did the CLI write here" which varies by CLI version per machine. To prevent accidental commits, `aiki init` auto-adds `.aiki/.manifest.json` to `.gitignore` if it's not already covered by an existing rule — this is enforced automatically rather than relying on documentation guidance.

### Why not three-way merge for dirty templates?

Three-way merge is complex and error-prone for markdown. The simpler model: if clean, overwrite silently. If dirty, skip silently. Users who want to customize can modify in place (it'll stay dirty) or copy to a user-owned location. To reset: just delete the file.

### Why embed in binary vs. fetch from remote?

- No network dependency
- Version-locked to the CLI release (consistent behavior)
- Same pattern as `aiki/default` hooks (already embedded via `include_str!`)
- Template marketplace (future) handles "fetch from remote"

### Why atomic write + last-writer-wins for the manifest?

**For single-plugin sync (current scope):** Sync is idempotent — concurrent writers produce equivalent results (same embedded templates, same on-disk state). Atomic write (temp file + rename) prevents partial or corrupt JSON if two sessions sync simultaneously. No locking is needed because the worst case is redundant work, not data loss.

**For multi-plugin sync (future):** Last-writer-wins is NOT safe when multiple plugins sync concurrently, because each plugin mutates different sections of the manifest. Two concurrent read-merge-write cycles can cause lost updates (plugin A's writes are overwritten by plugin B's stale read). File locking must be introduced before multi-plugin sync is supported.

> **TODO (multi-plugin):** Before adding multi-plugin sync, implement advisory file locking:
> - Acquire an advisory lock on `.aiki/.manifest.lock` sidecar before the read-merge-write cycle
> - Hold the lock for the entire read → merge → write-to-temp → rename sequence
> - Continue using atomic write (temp file + rename) for crash safety
> - This ensures both atomicity (no partial writes) and isolation (no lost updates between plugins)

### Why sync on session start vs. lazy on template load?

Sync-on-init is simpler: one place in the code, runs once, updates everything. Lazy-on-load would spread the logic across template resolution and add writes during read operations. The performance cost of sync-on-init is negligible (<1ms for ~12 small files).

---

## Open Questions

_(None currently — to be surfaced during review)_

---
