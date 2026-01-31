# Review JJ Changes by Revset

**Status**: Future Idea
**Related**: [Code Review System](../now/code-review-task-native.md)

---

## Summary

Add `--changes <revset>` flag to `aiki review` to review JJ changes directly by revset, without requiring a task context.

## Motivation

Currently, `aiki review` operates on:
- **session** - All closed tasks in current session (default)
- **task** - Changes associated with a specific task (`aiki review <task-id>`)

Adding `--changes` would allow reviewing arbitrary JJ changes directly:

```bash
# Review working copy change
aiki review --changes @

# Review range of changes from trunk to working copy
aiki review --changes "trunk()..@"

# Review specific change
aiki review --changes abc123
```

## Use Cases

1. **Ad-hoc reviews** - Review changes that aren't associated with a task
2. **Pre-commit reviews** - Review working copy before committing
3. **Branch reviews** - Review all changes on a feature branch
4. **Historical reviews** - Review past changes for audit purposes

## Design Notes

- The `--changes` flag would accept any valid JJ revset expression
- Review task would be created with `data.scope` set to the revset
- Changed files would be determined by `jj diff -r <revset>`
- Could be combined with `--loop` for iterative review cycles

## Implementation Considerations

- Need to resolve revset to list of changes and files
- Handle empty revsets gracefully (no changes to review)
- Consider how this interacts with task-based provenance tracking
