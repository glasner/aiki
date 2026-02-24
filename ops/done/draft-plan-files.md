# Draft Plan Files

**Date**: 2026-02-14
**Status**: Draft
**Purpose**: Add YAML frontmatter to plan files to mark draft status.

---

## Executive Summary

Plan files (the markdown documents authored via `aiki plan`) currently have no metadata to indicate whether they're still being authored or ready for implementation.

This plan adds YAML frontmatter to plan files with a single field: `draft` (boolean flag).

The link between a plan and its implementing task comes from the TaskGraph (via task edges), not from frontmatter.

---

## Frontmatter Format

```yaml
---
draft: true
---
```

| Field | Type | Description |
|-------|------|-------------|
| `draft` | boolean | If true, plan is still being authored and not ready for implementation |

### Draft Field

The `draft` field is a simple boolean:
- `draft: true` - Plan is still being authored, not ready for implementation
- `draft: false` (or omitted) - Plan is complete and ready to build

**When to use:**
- Set `draft: true` when creating a plan that's not yet complete
- Remove `draft: true` (or set to `false`) when the plan is ready for implementation
- Plans without `draft` field are assumed to be ready (not drafts)

---

## State Transitions by Command

### `aiki plan <path>` — File Creation

When `aiki plan` creates a new plan file, write frontmatter with `draft: true`:

**Current behavior** (`plan.rs:454`):
```rust
fs::write(&plan_path, "").map_err(...)?;
```

**New behavior**:
```rust
let frontmatter = "---\ndraft: true\n---\n\n";
fs::write(&plan_path, frontmatter).map_err(...)?;
```

### `aiki plan` — Session Completion

The plan template includes a final "Confirm completion" subtask that asks the user if the plan is ready.

**When to remove `draft`:**
- Only when the user explicitly confirms the plan is complete
- The final subtask in the template handles this by asking the user
- If the user wants to continue editing later, the `draft: true` remains

**Implementation:**
- The template's final subtask instructs the agent to remove `draft` when the user confirms completion
- No automatic removal in `run_plan()` — the agent handles it during the session
- This allows users to save progress and return to continue editing later

**Agent workflow (from template):**
```
1. Ask: "Is this plan complete and ready for implementation?"
2. If user says yes:
   - Remove `draft` field from frontmatter
   - Close subtask
3. If user wants to continue:
   - Leave `draft: true` in place
   - Close subtask as wont_do
```

---

## Frontmatter Read/Write Utility

**New file**: `cli/src/utils/frontmatter.rs`

All frontmatter updates are done internally by `plan.rs` — no new CLI commands. This module provides the shared read/write functions.

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
| YAML frontmatter parser | `cli/src/tasks/templates/parser.rs:111-146` | Yes - should be moved to `cli/src/utils/frontmatter.rs` |
| `serde_yaml` dependency | `cli/Cargo.toml:26` | Yes, no new deps |
| `serde` with derive | `cli/Cargo.toml:24` | Yes |

### Refactoring Strategy

The existing `extract_yaml_frontmatter<T>()` in `parser.rs` should be:
1. **Moved** to `cli/src/utils/frontmatter.rs` as a shared utility
2. **Kept generic** to work with any typed struct
3. **Extended** with additional functions for generic key-value frontmatter (using `BTreeMap<String, serde_yaml::Value>`)

This allows:
- Task templates to continue using typed `TemplateFrontmatter`
- Plan files to use typed `PlanFrontmatter` 
- Generic read/write operations for any frontmatter

## PlanGraph Integration

The `draft` field is read by the PlanGraph to determine plan lifecycle status.

### PlanMetadata Structure

The `PlanMetadata` struct in `cli/src/plans/parser.rs` already includes the `draft` field:

```rust
pub struct PlanMetadata {
    /// Title extracted from first H1 heading
    pub title: Option<String>,
    /// First paragraph after the H1 heading
    pub description: Option<String>,
    /// Whether the plan is marked as a draft in frontmatter
    pub draft: bool,
}
```

### Status Derivation

The PlanGraph derives the overall plan status based on both the `draft` field and implementing tasks:

| Frontmatter | Epic Status | Derived Status |
|-------------|-------------|----------------|
| `draft: true` | (any) | `Draft` |
| `draft: false` or omitted | No epic | `Draft` |
| `draft: false` or omitted | Epic open (not started) | `Planned` |
| `draft: false` or omitted | Epic in_progress | `Implementing` |
| `draft: false` or omitted | Epic closed | `Implemented` |

**Key behavior:**
- When `draft: true` is present, the plan is always shown as `Draft` regardless of epic status
- This allows incrementally authoring a plan while tasks are being created
- Once `draft` is removed, the status is derived from the implementing epic task

### Parser Updates

The `parse_plan_metadata()` function in `cli/src/plans/parser.rs` should:
1. Parse YAML frontmatter using existing `extract_yaml_frontmatter()` infrastructure
2. Read the `draft` field (default to `false` if not present)
3. Extract title and description as it currently does

**Implementation:**
```rust
pub fn parse_spec_metadata(path: &Path) -> SpecMetadata {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return SpecMetadata::default(),
    };

    // Parse frontmatter
    let draft = extract_yaml_frontmatter::<PlanFrontmatter>(&content)
        .map(|fm| fm.draft)
        .unwrap_or(false);

    // Extract title and description from markdown body
    // ... existing title/description parsing logic ...

    SpecMetadata {
        title,
        description,
        draft,
    }
}
```

Where `PlanFrontmatter` is a simple struct:
```rust
#[derive(Debug, Deserialize)]
struct PlanFrontmatter {
    #[serde(default)]
    draft: bool,
}
```

## Implementation Plan

### Phase 1: Frontmatter Utility

1. Add `cli/src/utils/frontmatter.rs` with `read_frontmatter()`, `write_frontmatter()`, `set_frontmatter_field()`, `remove_frontmatter_field()`
2. Unit tests for: no frontmatter, existing frontmatter, empty file, frontmatter-only file

### Phase 2: Plan Integration

3. Update `plan.rs` file creation to write `draft: true` frontmatter
4. Update `plan.rs` session completion to remove `draft` field

### Phase 3: Validation

5. `cargo test` — all unit tests pass
6. Manual smoke test: `aiki plan` — verify frontmatter at each step

---

## Example Lifecycle

```bash
$ aiki plan dark-mode.md "Add dark mode toggle"
# File created: ops/now/dark-mode.md
```

```yaml
---
draft: true
---

```

```bash
# Claude session runs, fills in plan content...
# Session completes successfully
```

No frontmatter (or empty frontmatter can be omitted):

```markdown
# Dark Mode Toggle
...plan content...
```

---

## Open Questions

1. **Manual files**: Users might create plan files by hand (no `aiki plan`). Should there be a way to manually mark as draft?

2. **Draft filtering**: Should `aiki plan list` or similar commands filter out drafts by default? Or show them with a `[draft]` indicator?

3. **Abandoned drafts**: How to handle old draft plans that were never completed? Archive command?
