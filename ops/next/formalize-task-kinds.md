---
draft: false
---

# Formalize Task Kinds

**Date**: 2026-03-04
**Status**: Approved
**Purpose**: Centralize task kind definitions so kind-specific behavior (linkage, lifecycle, validation) lives in one place instead of scattered across commands.

---

## Executive Summary

Today, task kinds are free-form strings assigned in template frontmatter. Kind-specific behavior — like "orchestrator tasks cascade-close descendants" or "review tasks have issues" — is implemented ad-hoc across `fix.rs`, `build.rs`, `review.rs`, `manager.rs`, etc. There's no registry, no validation, and no way to ask "what kinds exist and what do they do?"

This plan introduces a **task kind registry** that centralizes kind definitions, their required links, lifecycle behavior, and validation rules.

---

## Problem

1. **Scattered kind checks**: `is_review_task()` in fix.rs, `is_orchestrator()` in types.rs, type string comparisons in build.rs, review.rs, tui/builder.rs
2. **No validation**: Any string is accepted as `task_type` — typos silently create unrecognized kinds
3. **Implicit link patterns**: Each command independently creates its links (e.g., build creates `orchestrates`, decompose creates `decomposes-plan`). The "correct" set of links for a kind isn't defined anywhere
4. **No discoverability**: Can't ask the system "what kinds exist?"
5. **Inconsistent kind detection**: `is_review_task()` checks *both* task_type and template prefix as a workaround

---

## Proposed Kinds

| Kind | Created By | Key Links | Lifecycle Behavior |
|------|-----------|-----------|-------------------|
| `plan` | `aiki plan` | `adds-plan → file` | Interactive; has subtasks for drafting phases |
| `decompose` | `aiki decompose` | `decomposes-plan → file`, subtask-of epic | Creates subtasks under target epic |
| `review` | `aiki review` | `validates → target`, `sourced-from → target` | Has issue comments; consumed by `aiki fix` |
| `fix` | `aiki fix` | `fixes → review`, `remediates → issues` | Creates plan + decompose + loop + review cycle |
| `orchestrator` | `aiki loop`, `aiki build` | `orchestrates → epic` | Cascade-close descendants on stop/fail; uses lanes |
| `epic` | `aiki epic add` | `implements-plan → file` | Parent of work subtasks; target of decompose/orchestrator |
| `resolve` | `aiki resolve` | `sourced-from → conflict:change_id` | Merge conflict resolution |
| `explore` | explore subtasks | `sourced-from → scope` | Read-only investigation |

Kinds without special lifecycle behavior (explore, resolve) would still benefit from validation and discoverability.

---

## How It Works

### Kind Registry (new: `tasks/kind_registry.rs`)

A Rust enum with associated data, providing compile-time exhaustiveness checking:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskKind {
    Plan,
    Decompose,
    Review,
    Fix,
    Orchestrator,
    Epic,
    Explore,
}

impl TaskKind {
    /// Parse from string. Returns Err for unknown kinds.
    pub fn from_str(s: &str) -> Result<Self, AikiError> { ... }

    /// The string representation stored in task events.
    pub fn as_str(&self) -> &'static str { ... }

    /// Human description of what this kind does.
    pub fn description(&self) -> &'static str { ... }

    /// Required outbound link kinds when creating a task of this kind.
    /// e.g., Review requires "validates", Orchestrator requires "orchestrates".
    pub fn required_links(&self) -> &'static [&'static str] { ... }

    /// Optional but expected link kinds.
    pub fn expected_links(&self) -> &'static [&'static str] { ... }

    /// Whether stopping/failing a task of this kind cascade-closes descendants.
    pub fn cascade_close(&self) -> bool {
        matches!(self, Self::Orchestrator)
    }

    /// Whether this kind tracks issue comments (consumed by fix).
    pub fn has_issues(&self) -> bool {
        matches!(self, Self::Review)
    }

    /// Whether this kind uses the lane system for parallel execution.
    pub fn uses_lanes(&self) -> bool {
        matches!(self, Self::Orchestrator)
    }

    /// All known kinds (for iteration/discovery).
    pub fn all() -> &'static [TaskKind] { ... }
}
```

### Centralized Kind Methods on Task

Replace scattered checks with enum-backed methods:

```rust
impl Task {
    /// Returns the parsed TaskKind, if this task has a known kind.
    pub fn parsed_kind(&self) -> Option<TaskKind> {
        self.task_type.as_deref().and_then(|s| TaskKind::from_str(s).ok())
    }

    pub fn is_orchestrator(&self) -> bool {
        self.parsed_kind().map_or(false, |k| k.cascade_close())
    }

    pub fn has_issues(&self) -> bool {
        self.parsed_kind().map_or(false, |k| k.has_issues())
    }

    pub fn is_kind(&self, kind: TaskKind) -> bool {
        self.parsed_kind() == Some(kind)
    }
}
```

> **Note:** The existing `task_type` field in task events and frontmatter is read by `parsed_kind()` as-is. Renaming the stored field to `task_kind` is a separate migration tracked outside this plan.

### Validation at Creation

When `create_tasks_from_template()` resolves a kind from template frontmatter:
1. **Error on unknown kinds** — strict validation. If it's not in the enum, it's a bug. This keeps the kind system tight and prevents silent typos.
2. Required links are validated (present or will be added by the calling command).

### Link Centralization

Today, each command independently adds links after task creation. The proposal:
- Kind definitions declare their **expected links** (documentation + validation)
- Commands still create links, but the enum provides the canonical reference
- Future: helper `create_kinded_task()` that creates task + required links atomically

This is deliberately **not** auto-creating links from the registry. Commands know the concrete target IDs; the enum just documents and validates what links a kind should have.

---

## Migration Path

### Phase 1: Registry + Validation (non-breaking)
1. Create `tasks/kind_registry.rs` with `TaskKind` enum and all methods
2. Add `parsed_kind()`, `has_issues()`, `is_kind()` methods to `Task`
3. Replace `is_review_task()` and `is_orchestrator()` with enum-backed methods
4. Error on unknown kinds during task creation (strict validation)
5. Remove template-prefix fallback in `is_review_task()` (all review tasks should have explicit kind)

### Phase 2: Link Documentation + CLI Discoverability
1. Add `required_links` / `expected_links` to kind definitions
2. `aiki task kind list` — show all known kinds and their properties
3. `aiki task kind show <name>` — show kind details, expected links, behavior

### Phase 3: Atomic Task Creation Helper
1. Add `create_kinded_task()` helper in `tasks/mod.rs`
2. Helper creates task + required links atomically (one function call)
3. Refactor commands to use helper instead of manual create + link pattern

---

## Decisions

1. **Unknown kinds → error.** Strict validation keeps the kind system tight. No silent typos, no drift.
2. **`epic` is a formal kind.** It has specific semantics (implements-plan link, target of decompose/orchestrator).
3. **Rust enum.** Compile-time exhaustiveness, pattern matching, no stringly-typed dispatch. `TaskKind::from_str()` returns `Result` to enforce known kinds at the boundary.
4. **"Kind" not "type".** The new system uses "kind" terminology throughout (`TaskKind`, `parsed_kind()`, `kind_registry.rs`, `aiki task kind`). The legacy `task_type` field in stored events is read as-is; renaming it is a separate concern.



## Future Ideas

- User-defined kinds in `.aiki/kinds/` (extending the built-in registry)
- Kind-specific display in TUI (icons, colors, sections)
