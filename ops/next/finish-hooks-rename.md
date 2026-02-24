---
status: draft
---

# Finish Flows → Hooks Rename

## Context

The rename from "flows" to "hooks" was started in `ops/done/rename-to-hooks.md` but significant work remains. The types were renamed (`Flow` → `Hook`, `FlowEngine` → `HookEngine`, etc.) but the directory structure, module names, comments, and documentation still use "flows" terminology.

**Why this matters:** The mixed terminology causes confusion. Agents use "flows" language when explaining features, and documentation is inconsistent.

## Audit Results

### Code Changes Required

| Category | Files | Priority |
|----------|-------|----------|
| Directory rename | 1 directory (15 files) | P0 |
| Module declarations | 2 files | P0 |
| Source comments/docstrings | 58 files | P1 |
| YAML comments | 1 file | P1 |
| Test variable names | ~10 files | P2 |

### Documentation Changes Required

| Category | Files | Priority |
|----------|-------|----------|
| README.md | 1 file (~25 refs) | P0 |
| AGENTS.md | 1 file | P1 |
| cli/src/CLAUDE.md | 1 file | P1 |
| ops/now/ active docs | ~5 files | P1 |
| ops/future/ docs | ~10 files | P2 |
| ops/done/ historical | 50+ files | Skip |

## Implementation Plan

### Phase 1: Directory and Module Rename (P0)

**1.1 Rename directory**
```bash
git mv cli/src/flows cli/src/hooks
```

**1.2 Update module declarations**

`cli/src/main.rs`:
```rust
// Before
mod flows;

// After
mod hooks;
```

`cli/src/lib.rs`:
```rust
// Before
pub mod flows;

// After
pub mod hooks;
```

**1.3 Update all imports**

Search and replace across codebase:
- `use crate::flows::` → `use crate::hooks::`
- `crate::flows::` → `crate::hooks::`
- `aiki::flows::` → `aiki::hooks::`

Files affected:
- `cli/src/flows/*.rs` (internal imports)
- `cli/src/events/prelude.rs`
- `cli/src/cache.rs`
- `cli/src/bin/otel_decode.rs`

**1.4 Update error.rs test paths**

```rust
// Before
canonical_path: "/project/.aiki/hooks/aiki/flow-a.yml"

// After
canonical_path: "/project/.aiki/hooks/aiki/hook-a.yml"
```

### Phase 2: Comments and Docstrings (P1)

**2.1 Source file comments**

Update comments in these files:
- `cli/src/hooks/loader.rs` - "load flows" → "load hooks"
- `cli/src/hooks/composer.rs` - "loading flows" → "loading hooks"
- `cli/src/hooks/core/functions.rs` - "context injection in flows" → "context injection in hooks"
- `cli/src/hooks/core/hooks.yaml` - "customized in user flows" → "customized in user hooks"

Pattern to search: `\bflows?\b` (case-insensitive, word boundary)

**2.2 Docstring examples**

Update doc examples that show import paths:
```rust
// Before
//! use aiki::flows::context::{...};

// After
//! use aiki::hooks::context::{...};
```

### Phase 3: Documentation (P0-P1)

**3.1 README.md (P0)**

Major sections to update:
- "Flows" section header → "Hooks"
- "Flows are declarative YAML" → "Hooks are declarative YAML"
- `.aiki/flows/` → `.aiki/hooks/`
- `~/.aiki/flows/` → `~/.aiki/hooks/`
- "Default Flows" → "Default Hooks"
- "Flow capabilities" → "Hook capabilities"
- "Flow locations" → "Hook locations"
- All YAML examples showing `name: "my-flow"` → `name: "my-hook"`

**3.2 AGENTS.md (P1)**

Check for any flow references (audit found 1 match for "Workflow" which is correct).

**3.3 cli/src/CLAUDE.md (P1)**

Update error variant documentation:
```markdown
- **Flows**: `InvalidLetSyntax(String)`, ...
// Should be:
- **Hooks**: `InvalidLetSyntax(String)`, ...
```

**3.4 Active ops docs (P1)**

Files in `ops/now/` to update:
- `ops/now/impl-flow-integration.md` - Uses `.aiki/flows/` paths
- `ops/now/the-aiki-way.md` - References flows
- Other active planning docs

### Phase 4: Test Variables (P2)

Update variable names for consistency:
```rust
// Before
let flow_path = temp_dir.path().join(".aiki/hooks/aiki/simple.yml");

// After
let hook_path = temp_dir.path().join(".aiki/hooks/aiki/simple.yml");
```

Files with `flow_path` variables:
- `cli/src/hooks/loader.rs` (tests)
- `cli/src/hooks/composer.rs` (tests)
- `cli/src/hooks/hook_resolver.rs` (tests)

### Phase 5: Skip (Historical)

**Do NOT update:**
- `ops/done/*.md` - Historical accuracy
- `ops/ROADMAP.md` - Historical planning document
- Git commit messages

## Search Patterns

For systematic replacement:

```bash
# Find all "flow" references in source code
rg -i '\bflows?\b' cli/src --type rust

# Find all ".aiki/flows" references
rg '\.aiki/flows' .

# Find "flow" in comments only (harder, manual review)
rg '//.*flow|/\*.*flow' cli/src --type rust
```

## Verification

After completion:

```bash
# Should return nothing in source code (excluding done/ and ROADMAP.md)
rg -i '\bflows?\b' cli/src --type rust | grep -v test

# Should return nothing for paths
rg '\.aiki/flows' . | grep -v ops/done | grep -v ROADMAP

# Build should pass
cargo build

# Tests should pass
cargo test
```

## Risks

### Risk: Breaking imports
**Mitigation:** Run `cargo build` after Phase 1 to catch any missed imports.

### Risk: Missing some references
**Mitigation:** Use comprehensive `rg` searches before marking complete.

### Risk: Breaking external tools
**Mitigation:** None - internal rename doesn't affect external interfaces.

## Success Criteria

- [ ] `cli/src/hooks/` directory exists (not `cli/src/flows/`)
- [ ] No `mod flows` declarations in lib.rs or main.rs
- [ ] No `use crate::flows::` imports in source code
- [ ] README.md uses "hooks" terminology throughout
- [ ] `cargo build` succeeds
- [ ] `cargo test` succeeds
- [ ] `rg -i '\bflows?\b' cli/src` returns only:
  - Historical comments (if any kept intentionally)
  - The word "workflow" (which is correct)

## Subtasks

1. [ ] Rename `cli/src/flows/` → `cli/src/hooks/`
2. [ ] Update module declarations in main.rs and lib.rs
3. [ ] Update all `use crate::flows::` imports
4. [ ] Update source comments and docstrings
5. [ ] Update README.md
6. [ ] Update cli/src/CLAUDE.md
7. [ ] Update active ops/now/ docs
8. [ ] Update test variable names (optional)
9. [ ] Final verification with rg searches
