# Future Enhancement: Inline Flow Actions

**Status:** Deferred to future milestone  
**Related:** Milestone 1.3 (Flow Composition)

---

## Overview

This document describes the **inline flow action** feature (`flow:` action) that allows flows to invoke other flows at specific points in the action list, rather than just at the beginning (before:) or end (after:).

**Key difference from before/after:**
- `before:` / `after:` - Always execute, fixed position
- `flow:` action - Executes at that point in the action list, can be conditional

---

## Feature Description

### Syntax

```yaml
PostResponse:
  - let: error_count = self.count_errors
  
  - if: $error_count > 0
    then:
      - flow: aiki/detailed-lint  # Runs NOW (not in before/after)
  
  - shell: echo "All checks passed"
```

### Behavior

- `flow:` action executes **the same event** from the referenced flow
- Example: `flow: aiki/quick-lint` inside `PostResponse` runs `quick-lint`'s `PostResponse` actions
- The invoked flow is atomic (runs its own before/after if it has them)
- Can be used in conditional branches (`if`, `switch`)
- Subject to the same cycle detection as before/after flows

---

## Use Cases

### Conditional Flow Invocation

```yaml
PostResponse:
  - let: files = self.get_edited_files
  
  - if: $files contains ".ts"
    then:
      - flow: aiki/typescript-check
  
  - if: $files contains ".rs"
    then:
      - flow: aiki/rust-check
```

### Dynamic Flow Selection

```yaml
PostResponse:
  - let: language = self.detect_language
  
  - switch: $language
    cases:
      "typescript":
        - flow: aiki/typescript-check
      "rust":
        - flow: aiki/rust-check
      "python":
        - flow: aiki/python-check
```

### Progressive Validation

```yaml
PostResponse:
  - flow: aiki/quick-lint       # Fast checks first
  
  - let: quick_passed = $?.success
  
  - if: $quick_passed
    then:
      - flow: aiki/thorough-lint  # Slow checks only if quick checks passed
```

---

## Implementation Considerations

### Action Enum

```rust
pub enum Action {
    Shell { command: String, ... },
    Let { expression: String },
    If { condition: String, ... },
    Flow { path: String },        // NEW: Inline flow invocation
    // ...
}
```

### FlowComposer Changes

```rust
impl FlowComposer<'_> {
    fn compose_action(&mut self, action: &Action, event: &mut dyn Event) -> Result<()> {
        match action {
            Action::Flow { path } => {
                // Inline flow invocation - delegate to compose_flow() for composition
                self.compose_flow(path, event)?;
            }
            _ => {
                // All other actions - delegate to FlowExecutor
                self.executor.execute_action(action, event)?;
            }
        }
        Ok(())
    }
}
```

### Cycle Detection

Runtime cycles must be detected:

```yaml
# flow-a.yml
PostResponse:
  - flow: ./flow-b.yml

# flow-b.yml
PostResponse:
  - flow: ./flow-a.yml  # ERROR: Circular dependency detected
```

Self-invocation must also be prevented:

```yaml
# my-workflow.yml
PostResponse:
  - if: $counter < 10
    then:
      flow: ./my-workflow.yml  # ERROR: Circular dependency (self-invocation)
```

---

## Why Deferred

1. **Complexity**: Adds significant complexity to action execution
2. **Use cases unclear**: Most composition needs are met by before/after
3. **Alternative approach**: Can achieve similar results with multiple flows and before/after
4. **Focus**: Want to validate before/after model first before adding inline invocation

---

## Migration Path

When this feature is implemented:

1. Existing flows using before/after continue to work
2. New flows can optionally use `flow:` actions
3. No breaking changes to existing syntax

---

## Related Documents

- [milestone-1.3-flow-composition.md](../now/milestone-1.3-flow-composition.md) - Current flow composition implementation
- [ROADMAP.md](../ROADMAP.md) - Strategic context
