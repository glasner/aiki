# Spec File Frontmatter

**Date**: 2026-02-14
**Status**: Draft
**Purpose**: Add YAML frontmatter to spec/plan files to mark draft status.

---

## Executive Summary

Spec files (the markdown documents authored via `aiki spec` / `aiki plan`) currently have no metadata to indicate whether they're still being authored or ready for implementation.

This spec adds YAML frontmatter to spec files with a single field: `draft` (boolean flag).

The link between a spec and its implementing task comes from the TaskGraph (via task edges), not from frontmatter.

---

## Frontmatter Format

```yaml
---
draft: true
---
```

| Field | Type | Description |
|-------|------|-------------|
| `draft` | boolean | If true, spec is still being authored and not ready for implementation |

### Draft Field

The `draft` field is a simple boolean:
- `draft: true` - Spec is still being authored, not ready for implementation
- `draft: false` (or omitted) - Spec is complete and ready to build

**When to use:**
- Set `draft: true` when creating a spec that's not yet complete
- Remove `draft: true` (or set to `false`) when the spec is ready for implementation
- Specs without `draft` field are assumed to be ready (not drafts)

---

## State Transitions by Command

### `aiki spec <path>` — File Creation

When `aiki spec` creates a new spec file, write frontmatter with `draft: true`:

**Current behavior** (`spec.rs:454`):
```rust
fs::write(&spec_path, "").map_err(...)?;
```

**New behavior**:
```rust
let frontmatter = "---\ndraft: true\n---\n\n";
fs::write(&spec_path, frontmatter).map_err(...)?;
```

### `aiki spec` — Session Completion

When the spec session finishes successfully, remove the `draft` field (or set to `false`).

**Where**: After the Claude session exits with success in `run_spec()` (around spec.rs:500-520).

**How**: Read file, remove `draft` field from frontmatter, write file.

```rust
// After successful spec session:
let (mut frontmatter, body) = read_frontmatter(&spec_path)?;
frontmatter.remove("draft");  // Spec is now ready
write_frontmatter(&spec_path, &frontmatter, &body)?;
```

---

## Frontmatter Read/Write Utility

**New file**: `cli/src/frontmatter.rs`

All frontmatter updates are done internally by `spec.rs` — no new CLI commands. This module provides the shared read/write functions.

```rust
/// Parsed frontmatter as key-value pairs
pub type Frontmatter = BTreeMap<String, serde_yaml::Value>;

/// Read frontmatter from a markdown file. Returns (frontmatter, body).
/// Returns empty map if no frontmatter block exists.
pub fn read_frontmatter(path: &Path) -> Result<(Frontmatter, String)>;

/// Write frontmatter + body to a markdown file.
/// If frontmatter is empty, writes body only (no `---` block).
pub fn write_frontmatter(path: &Path, fm: &Frontmatter, body: &str) -> Result<()>;

/// Update a single field in a file's frontmatter, preserving body.
/// Creates frontmatter block if none exists.
pub fn set_frontmatter_field(path: &Path, key: &str, value: serde_yaml::Value) -> Result<()>;

/// Remove a field from a file's frontmatter, preserving body.
pub fn remove_frontmatter_field(path: &Path, key: &str) -> Result<()>;
```

**Key behavior for `write_frontmatter`**:
- If file has existing `---` block: replace it, keep body
- If file has no `---` block: prepend frontmatter + blank line before body
- Preserve all non-frontmatter content exactly as-is

---

## Existing Infrastructure

The codebase already has what we need:

| Component | Location | Reusable? |
|-----------|----------|-----------|
| YAML frontmatter parser | `cli/src/tasks/templates/parser.rs:111-146` | Pattern yes, code partially (typed to `TemplateFrontmatter`) |
| `serde_yaml` dependency | `cli/Cargo.toml:26` | Yes, no new deps |
| `serde` with derive | `cli/Cargo.toml:24` | Yes |

The existing `extract_yaml_frontmatter<T>()` in `parser.rs` parses frontmatter into a typed struct. The new utility should use `BTreeMap<String, serde_yaml::Value>` for flexibility.

---

## Implementation Plan

### Phase 1: Frontmatter Utility

1. Add `cli/src/frontmatter.rs` with `read_frontmatter()`, `write_frontmatter()`, `set_frontmatter_field()`, `remove_frontmatter_field()`
2. Unit tests for: no frontmatter, existing frontmatter, empty file, frontmatter-only file

### Phase 2: Spec Integration

3. Update `spec.rs` file creation to write `draft: true` frontmatter
4. Update `spec.rs` session completion to remove `draft` field

### Phase 3: Validation

5. `cargo test` — all unit tests pass
6. Manual smoke test: `aiki spec` — verify frontmatter at each step

---

## Example Lifecycle

```bash
$ aiki spec dark-mode.md "Add dark mode toggle"
# File created: ops/now/dark-mode.md
```

```yaml
---
draft: true
---

```

```bash
# Claude session runs, fills in spec content...
# Session completes successfully
```

No frontmatter (or empty frontmatter can be omitted):

```markdown
# Dark Mode Toggle
...spec content...
```

---

## Open Questions

1. **Manual files**: Users might create spec files by hand (no `aiki spec`). Should there be a way to manually mark as draft?

2. **Draft filtering**: Should `aiki spec list` or similar commands filter out drafts by default? Or show them with a `[draft]` indicator?

3. **Abandoned drafts**: How to handle old draft specs that were never completed? Archive command?
