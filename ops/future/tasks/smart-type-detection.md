# Smart Type Detection

**Status**: Future Idea  
**Related**: Task System, Task Types

---

## Goal

Auto-detect task type from name patterns.

---

## Features

- Pattern-based type detection from task names
- Configurable patterns per project

---

## Examples

### Auto-Detect Rules

```
"Fix ..." → error
"Add ..." → feature
"Refactor ..." → chore
"Update ..." → chore
```

### Usage

```bash
aiki task add "Fix null check"
→ Type: error (auto-detected)

aiki task add "Add dark mode"
→ Type: feature (auto-detected)
```

---

## Implementation Notes

- Requires Task Types to be implemented first
- Use regex patterns for matching
- Allow project-level configuration in `.aiki/config.toml`
- Fallback to manual type selection if pattern doesn't match
