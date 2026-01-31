# Expanded Diff Format Flags

**Date**: 2026-01-29
**Status**: Future
**Purpose**: Add output format flags to `aiki task diff` for different use cases

**Depends On**:
- [Task Diff Command](../now/task-diff.md) - Base `aiki task diff` implementation

---

## Summary

Add format flags to `aiki task diff` to control output format, supporting different workflows and external tool integration.

## Format Flags

### Summary Format (`--summary`)

Show file names with insertion/deletion counts:

```bash
aiki task diff xqrmnpst --summary
```

**Output**:
```
src/auth.ts       | 7 +++++--
src/middleware.ts | 4 +++-
2 files changed, 8 insertions(+), 3 deletions(-)
```

**Use cases**:
- Quick overview of what changed
- Integrating into status displays
- File-level change metrics

### Stat Format (`--stat`)

Show histogram of changes:

```bash
aiki task diff xqrmnpst --stat
```

**Output**:
```
src/auth.ts       | 7 ++++---
src/middleware.ts | 4 ++--
2 files changed, 8 insertions(+), 3 deletions(-)
```

**Use cases**:
- Visual representation of change magnitude
- Identifying large changes

### Name Only (`--name-only`)

Show only changed file names:

```bash
aiki task diff xqrmnpst --name-only
```

**Output**:
```
src/auth.ts
src/middleware.ts
```

**Use cases**:
- Piping to other commands
- Scripting and automation
- Quick file list for review

### Git Format (`--git`)

Output Git unified diff format instead of jj's native format:

```bash
aiki task diff xqrmnpst --git > task-xqrmnpst.patch
```

**Output**:
```
diff --git a/src/auth.ts b/src/auth.ts
index abc123..def456 100644
--- a/src/auth.ts
+++ b/src/auth.ts
@@ -40,4 +40,6 @@ function validateUser(user) {
   if (!user) {
+    throw new Error('User is null');
+  }
   return user.name;
 }
```

**Use cases**:
- Generating patches for other repositories
- GitHub/GitLab PR APIs
- Patch application tools (`git apply`, `patch`)
- Diff viewers and code review platforms
- Sharing with tools that expect unified diff

## Implementation

Pass flags through to jj's diff command:

```rust
fn generate_task_diff(
    repo_path: &Path,
    task_id: &str,
    include_subtasks: bool,
    format: DiffFormat,
) -> Result<String> {
    let pattern = build_task_pattern(task_id, include_subtasks);
    let from_revset = format!("parents(roots({}))", pattern);
    let to_revset = format!("heads({})", pattern);

    let mut cmd = Command::new("jj");
    cmd.arg("diff")
        .arg("--from").arg(&from_revset)
        .arg("--to").arg(&to_revset)
        .current_dir(repo_path);

    match format {
        DiffFormat::Default => {},
        DiffFormat::Summary => { cmd.arg("--summary"); },
        DiffFormat::Stat => { cmd.arg("--stat"); },
        DiffFormat::NameOnly => { cmd.arg("--name-only"); },
        DiffFormat::Git => { cmd.arg("--git"); },
    }

    let output = cmd.output()?;
    Ok(String::from_utf8(output.stdout)?)
}
```

## Priority

Low - jj's native format works well for the primary use case (agent code review). These flags are useful for:
- External tool integration
- Automation and scripting
- Different human workflows

Consider implementing only when there's a clear need for these formats.

## Future Enhancements

### Combined Flags

Allow combining flags:

```bash
aiki task diff xqrmnpst --git --stat
```

### Custom Output Templates

```bash
aiki task diff xqrmnpst --format='{{path}}: +{{insertions}} -{{deletions}}'
```
