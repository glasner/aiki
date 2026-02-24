---
status: draft
---

# Rename Flow Actions to Commands

**Date**: 2026-01-20
**Status**: Design Proposal
**Purpose**: Improve terminology clarity by renaming "flow actions" to "commands"

---

## Executive Summary

Rename "flow actions" to "commands" to improve clarity and avoid confusion with task templates.

**Key Changes:**
1. **"Flow actions" → "Commands"** - Clearer, more specific terminology
2. **Keep "flows"** - No ecosystem collision, already distinct from templates
3. **Ban "workflow"** - Avoid ambiguity in documentation

**Rationale:**
- "Flows contain commands" reads naturally
- Aligns with common terminology (steps, commands)
- Avoids GitHub Actions ecosystem confusion
- Low migration cost compared to renaming flows themselves

---

## Problem Statement

### Current Terminology Issues

1. **"Flow actions" is awkward and repetitive**
   - "The flow action executes a jj action"
   - Double use of "action" in different contexts

2. **"Workflow" is ambiguous**
   - "Code review workflow" - is this a flow or a template?
   - Used inconsistently in docs to mean either automation or task structure

3. **Potential confusion with GitHub Actions**
   - Users familiar with GitHub expect "action" = reusable component
   - We use "action" for primitives within flows

### Why Not Rename "Flows" to "Actions"?

**Ecosystem collision:**
- GitHub Actions: "Action" = reusable component, "Workflow" = YAML file
- Renaming flows → actions inverts this popular mental model
- Would confuse users coming from GitHub Actions

**Migration cost:**
- Rename directory: `cli/src/flows/` → `cli/src/actions/`
- Rename user directory: `.aiki/flows/` → `.aiki/actions/`
- Update all documentation and examples
- High churn for marginal benefit

**"Flow" is already clear:**
- Suggests sequential progression through steps
- Distinct from "template" (blueprint metaphor)
- No existing confusion in practice

---

## Proposed Solution

### 1. Rename "Flow Actions" → "Commands"

**Current (confusing):**
```yaml
# A flow with flow actions
session.started:
  - jj: new              # A flow action
  - shell: aiki init     # A flow action
  - context: "Hello"     # A flow action
```

**Proposed (clear):**
```yaml
# A flow with commands
session.started:
  - jj: new              # A command
  - shell: aiki init     # A command
  - context: "Hello"     # A command
```

**Terminology:**
- "Flows contain commands"
- "Commands are the building blocks of flows"
- "Available commands: `jj:`, `shell:`, `context:`, `if:`, `let:`, etc."

### 2. Keep "Flows" Unchanged

Flows remain flows:
- `.aiki/flows/aiki/core.yaml`
- `cli/src/flows/`
- `aiki flows list` (future command)

### 3. Ban "Workflow" from Documentation

Replace ambiguous "workflow" terminology:

| ❌ Avoid | ✅ Use Instead |
|---------|---------------|
| "workflow automation" | "flow automation" or just "flows" |
| "task workflow" | "task template" or "task structure" |
| "review workflow" | "review flow" (automation) or "review template" (task blueprint) |
| "workflow system" | "flow system" |

---

## Benefits

### 1. Clarity

**Before:**
- "Flow actions are the actions within flows"
- "Actions" used for two different things

**After:**
- "Commands are the building blocks of flows"
- Clear, unambiguous terminology

### 2. Natural Language

"Flows contain commands" reads better than:
- "Flows contain flow actions"
- "Flows contain actions" (which actions?)

### 3. Ecosystem Alignment

Aligns with common terminology:
- **GitHub Actions**: Workflows contain "steps"
- **GitLab CI**: Pipelines contain "jobs" with "script" commands
- **Make**: Makefiles contain "commands"
- **Shell scripts**: Scripts contain "commands"

"Commands" is universally understood as "things that execute."

### 4. Template Distinction

Clear separation:
- **Flows** (automation) contain **commands** (primitives)
- **Task templates** (blueprints) define **tasks** (work items)

No terminology overlap or confusion.

### 5. Low Migration Cost

**Code changes:**
- Rename internal types: `FlowAction` → `FlowCommand` or just `Command`
- Update documentation and comments
- No user-facing file structure changes
- No breaking changes to YAML syntax

**Documentation changes:**
- Global find/replace: "flow action" → "command"
- Update terminology section
- Clarify flow vs. template distinction

---

## Implementation

### Phase 1: Code Refactoring

**Types and structs:**
```rust
// Before
pub enum FlowAction {
    Jj { ... },
    Shell { ... },
    Context { ... },
}

// After
pub enum Command {
    Jj { ... },
    Shell { ... },
    Context { ... },
}
```

**Files to update:**
- `cli/src/flows/types.rs` - Rename types
- `cli/src/flows/engine.rs` - Update terminology in code
- `cli/src/flows/loader.rs` - Update comments
- `cli/src/flows/core/functions.rs` - Update documentation

**Variable naming:**
```rust
// Before
let action = parse_flow_action(node)?;
execute_action(action)?;

// After
let command = parse_command(node)?;
execute_command(command)?;
```

### Phase 2: Documentation Updates

**Files to update:**
- `README.md` - Update flow examples
- `AGENTS.md` - Update terminology if mentioned
- `ops/now/code-review-task-native.md` - Update flow references
- `ops/now/task-templates.md` - Ensure consistent terminology

**Global replacements:**
- "flow action" → "command"
- "flow actions" → "commands"
- Ban "workflow" in favor of specific terms

**New terminology guide:**
```markdown
## Aiki Terminology

- **Flow**: Event-driven automation defined in YAML files
- **Command**: A primitive operation within a flow (e.g., `jj:`, `shell:`, `context:`)
- **Task**: A unit of work tracked by aiki
- **Task Template**: A blueprint for creating tasks with predefined structure
- **Event**: A trigger point in the editor lifecycle (e.g., `session.started`, `change.completed`)
```

### Phase 3: Examples and Comments

Update all example YAML files:
```yaml
# Before comment:
# This flow action runs jj new

# After comment:
# This command runs jj new
session.started:
  - jj: new
```

### Phase 4: Error Messages

Update error messages to use new terminology:
```rust
// Before
return Err(Error::InvalidFlowAction("unknown action: foo".into()));

// After
return Err(Error::InvalidCommand("unknown command: foo".into()));
```

---

## Migration Impact

### User Impact: **None**

- No changes to `.aiki/flows/` directory structure
- No changes to YAML syntax
- No changes to file locations
- Purely internal terminology change

### Developer Impact: **Low**

- Code rename: `FlowAction` → `Command`
- Documentation updates
- No API breaking changes
- No file structure changes

### Timeline

**Phase 1-2**: One refactoring session (2-3 hours)
**Phase 3-4**: Follow-up cleanup (1 hour)

Total: Single focused work session

---

## Alternative Considered: Full Rename (Flows → Actions)

**Rejected because:**

1. **Ecosystem confusion**
   - Conflicts with GitHub Actions mental model
   - GitHub: Action = component, Workflow = file
   - Aiki would invert this: Action = file, Command = component

2. **High migration cost**
   - Rename `cli/src/flows/` → `cli/src/actions/`
   - Rename `.aiki/flows/` → `.aiki/actions/`
   - Update all user configurations
   - Potential breaking changes for existing users

3. **Marginal benefit**
   - "Flow" already clearly distinct from "template"
   - No reported confusion about "flow" as a concept
   - Primary issue is "flow actions", not "flows"

4. **"Flow" is semantically accurate**
   - Describes sequential progression through steps
   - Common in programming (control flow, data flow)
   - Natural mental model for event-driven automation

---

## Summary

| Aspect | Current | After Rename |
|--------|---------|--------------|
| Automation file | Flow | Flow *(unchanged)* |
| Primitives within flows | Flow actions | Commands |
| Task blueprint | Template | Template *(unchanged)* |
| Ambiguous term | "Workflow" | Banned from docs |

**Result:**
- "Flows contain commands that respond to events"
- "Task templates define the structure and instructions for tasks"
- Clear, unambiguous terminology with minimal migration cost

**Next Steps:**
1. Get approval on terminology change
2. Create task for implementation
3. Execute Phase 1-2 (code + docs)
4. Review and cleanup (Phase 3-4)
