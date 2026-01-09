# Humanized Tasks

**Status**: Future Idea  
**Related**: Task System Phase 6+

---

## Goal

Make tasks more ergonomic for human users.

---

## Features

- `--display` flag for human-readable output (Phases 1-5 are XML-only)
- Prefix matching for task IDs (JJ-style, useful for typing)
- Colored output with emojis
- Interactive task selection

---

## Examples

### Human-Readable Output

```bash
aiki task list --display

Ready Tasks (3)
───────────────
1. 🔴 err-abc: Fix null check in auth.ts [p0]
2. 🔴 err-def: Fix missing return [p0]
3. 🟡 warn-ghi: Consider using const [p1]
```

### Prefix Matching

```bash
# Matches err-abc if unique
aiki task start err-a
```

---

## Implementation Notes

- Add `--display` flag to all commands
- Implement terminal color support with `termcolor` or similar
- Use fuzzy matching for task ID prefixes
- Consider interactive mode with `inquire` or `dialoguer` crate
