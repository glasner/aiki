# ReviewScope Refactor

**Date**: 2026-02-09
**Status**: Idea
**Priority**: P2
**Depends on**: `ops/done/fix-original-task.md`

**Related Documents**:
- [Review and Fix Non-Task Targets](review-and-fix-files.md) - Depends on this refactor

---

## Problem

Review scope metadata is currently passed as loose strings (`scope_id`, `scope_name`) through builtins, and fix routing parses `task:`/`file:` prefixes from task sources. This creates several issues:

1. `create_review_task_from_template` hardcodes `task:{scope_id}` as the source for all non-session reviews (`resolver.rs:1092-1093`), which would produce `task:ops/now/feature.md` for file reviews — misclassified as a task review by fix.
2. Fix routes by parsing source prefixes (`fix.rs:103,108`), coupling routing logic to a string format.
3. The `--implementation` review mode has no way to flow through to fix — both spec and implementation reviews would produce identical `file:` sources.

## Solution

Introduce a typed `ReviewScope` struct that serializes to/from task `data` fields. Review builds it, fix deserializes it. No prefix parsing, no lost metadata.

---

## ReviewScope Struct

```rust
/// What is being reviewed and how
pub struct ReviewScope {
    pub kind: ReviewScopeKind,
    pub id: String,            // task ID or file path
    pub task_ids: Vec<String>, // session reviews only (empty otherwise)
}

pub enum ReviewScopeKind {
    Task,
    Spec,
    Implementation,
    Session,
}

impl ReviewScope {
    /// Get display name (computed from kind and id)
    pub fn name(&self) -> String {
        match self.kind {
            ReviewScopeKind::Task => format!("Task ({})", &self.id),
            ReviewScopeKind::Spec => {
                let filename = Path::new(&self.id)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.id);
                format!("Spec ({})", filename)
            }
            ReviewScopeKind::Implementation => {
                let filename = Path::new(&self.id)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.id);
                format!("Implementation ({})", filename)
            }
            ReviewScopeKind::Session => format!("Session"),
        }
    }

    /// Serialize to task data HashMap for persistence
    pub fn to_data(&self) -> HashMap<String, String> {
        let mut data = HashMap::new();
        data.insert("scope.type".into(), self.kind.as_str().into());
        data.insert("scope.id".into(), self.id.clone());
        data.insert("scope.name".into(), self.name());
        if !self.task_ids.is_empty() {
            data.insert("scope.task_ids".into(), self.task_ids.join(","));
        }
        data
    }

    /// Deserialize from task data HashMap
    pub fn from_data(data: &HashMap<String, String>) -> Result<Self> {
        let kind = data.get("scope.type")
            .ok_or_else(|| AikiError::InvalidArgument("Missing scope.type in review task data".into()))?;
        Ok(Self {
            kind: ReviewScopeKind::from_str(kind)?,
            id: data.get("scope.id").cloned().unwrap_or_default(),
            task_ids: data.get("scope.task_ids")
                .map(|s| s.split(',').map(String::from).collect())
                .unwrap_or_default(),
        })
    }
}
```

### Data Flow

```
review.rs                          task data                        fix.rs
─────────                          ─────────                        ──────
detect_target()
    │
    ▼
ReviewScope {
  kind: Implementation,            scope.type = "implementation"
  id: "ops/now/feature.md",   ──►  scope.id = "ops/now/feature.md" ──►  ReviewScope::from_data()
}                                  scope.name = scope.name()             │
    │                              (computed)                            ▼
    ▼                                                               match scope.kind {
scope.to_data()                                                       Task => subtask on original
    │                                                                 Spec | Implementation => standalone fix
    ▼                                                                 Session => session fix
create_review_task(data: HashMap)                                   }
```

### Template Access

Templates access scope fields via `{{data.scope.type}}`, `{{data.scope.id}}`, `{{data.scope.name}}`:

```markdown
# Review: {{data.scope.name}}
{% subtask aiki/review/{{data.scope.type}} %}
```

### Sources Stay for Lineage

Sources (`task:`, `file:`, `prompt:`) remain purely for lineage/provenance — tracking where a task came from, not what it's about. Fix no longer reads sources for routing.

---

## Code Changes

### 1. Add ReviewScope struct

**File**: `cli/src/commands/review.rs` (or new `cli/src/review_scope.rs` if shared)

- `ReviewScope` struct with `to_data()` / `from_data()`
- `ReviewScopeKind` enum with `as_str()` / `from_str()`
- Unit tests for serialization round-trip

### 2. Update review command

**File**: `cli/src/commands/review.rs`

| Current | Change |
|---------|--------|
| `scope_id: String` field on `ReviewParams` (line 113) | Replace with `scope: ReviewScope` |
| Builds `scope_name`/`scope_id` strings (line 131) | Build `ReviewScope` from target detection (only `kind`, `id`, `task_ids`) |
| Passes `scope_id` to `create_review_task_from_template` (line 188) | Pass `scope.to_data()` as data HashMap (includes computed `scope.name()`) |

### 3. Update template resolver

**File**: `cli/src/tasks/templates/resolver.rs`

| Current | Change |
|---------|--------|
| `create_review_task_from_template` takes `scope_name: &str, scope_id: &str` (lines 1079-1080) | Accept `data: HashMap<String, String>` parameter |
| Sets `scope`, `scope.name`, `scope.id` as builtins (lines 1087-1089) | Pass data through to task creation (data fields, not builtins) |
| Hardcodes `task:{scope_id}` source (lines 1092-1093) | Remove. Caller provides sources separately if needed for lineage. |

### 4. Update fix routing

**File**: `cli/src/commands/fix.rs`

| Current | Change |
|---------|--------|
| `get_review_target` parses `task:`/`file:` from `review_task.sources` (lines 100-113) | Use `ReviewScope::from_data(&review_task.data)` and match on `scope.kind` |
| `ReviewTarget` enum used for routing | Can reuse or replace with `ReviewScopeKind` |

### 5. Update comments/docs

**File**: `cli/src/commands/task.rs`

| Current | Change |
|---------|--------|
| `TemplateTaskParams.builtins` docs reference scope (lines 3761, 4156) | Update comments to reflect scope now lives in data |

### 6. Update tests

**Files**: `templates/resolver.rs`, `templates/variables.rs`, `templates/types.rs`

| Current | Change |
|---------|--------|
| Tests use `set_data("scope", "@")` | Update to use `scope.type`/`scope.id`/`scope.name` key pattern |

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Missing `scope.type` in review task data | `ReviewScope::from_data` returns error: `"Missing scope.type in review task data"` |
| Unknown `scope.type` value | `ReviewScopeKind::from_str` returns error: `"Unknown review scope type: '{value}'"` |
| Review task created before this refactor (no data fields) | Fix falls back to error — user must re-run review |

---

## Testing

- `ReviewScope::to_data()` → `ReviewScope::from_data()` round-trip for all four kinds
- `ReviewScopeKind::as_str()` / `from_str()` for all variants
- `from_data` with missing `scope.type` returns error
- `from_data` with unknown `scope.type` returns error
- Integration: review creates task with data fields, fix reads them back
