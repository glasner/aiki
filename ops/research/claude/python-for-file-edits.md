---
date: 2026-02-25
context: Workspace isolation file editing issue
---

# Python File Edits Not Persisting in JJ Workspace

## Problem

When using Python scripts to edit files in an isolated JJ workspace, the changes appear to succeed but are not tracked by JJ and don't get absorbed back to the main repository.

## What Happened

### Context
- Working in isolated workspace: `/tmp/aiki/7f50e063/29e0d834`
- Task: Update `ops/now/loop-flags.md` with intelligent spawning approach
- Multiple sections needed updates (templates, design decisions, implementation plan, etc.)

### Approach Used
Used Python inline scripts via bash to perform complex find/replace operations:

```python
with open('ops/now/loop-flags.md', 'r') as f:
    content = f.read()

old_section = """..."""
new_section = """..."""

if old_section in content:
    content = content.replace(old_section, new_section)
    with open('ops/now/loop-flags.md', 'w') as f:
        f.write(content)
    print("Updated section")
```

### Observations

1. **Python reported success**: Each operation printed success messages
2. **File size unchanged**: ls -la showed same timestamp/size as before edits
3. **JJ saw no changes**: jj status showed "The working copy has no changes"
4. **Content verification failed**: grep for updated content returned nothing
5. **Changes didn't persist**: File content was identical to pre-edit state

### Verification Commands

```bash
# Before edits
ls -la ops/now/loop-flags.md
# -rw-r--r--  1 glasner  wheel  35020 Feb 25 09:08

# After ~10 Python edit operations
ls -la ops/now/loop-flags.md  
# -rw-r--r--  1 glasner  wheel  35020 Feb 25 09:08
# Same timestamp, same size!

grep -c "intelligent spawn" ops/now/loop-flags.md
# (returned nothing)

jj status
# The working copy has no changes.
```

## Hypotheses

### 1. File Descriptor / Buffer Issue
Python might be writing to a file handle that's not properly flushing in the workspace environment.

### 2. Workspace Filesystem Oddities
The /tmp/aiki/* isolated workspace might have special filesystem behavior that interferes with standard file I/O.

### 3. JJ Working Copy Snapshot Issue
JJ might snapshot the working copy at workspace creation, and direct file modifications bypass JJ's tracking.

### 4. Permission/Lock Issue
The workspace isolation mechanism might have file locks or permission settings that silently fail Python writes.

## What Should Have Worked

Based on standard Python file I/O:
- open(..., 'w') should truncate and write
- Changes should be visible immediately to subsequent reads
- File timestamp should update
- JJ should see working copy changes

## Alternative Approaches That Might Work

### 1. Use MCP File Edit Tools
Instead of Python file I/O, use the MCP tools provided:
- mcp__acp__Edit
- mcp__acp__Write

### 2. Use Shell Redirection
cat > file.md with heredoc

### 3. Use sed/awk for in-place edits
sed -i for direct file modification

### 4. Use JJ-aware edit command
jj edit or proper editor invocation

## Questions

1. **Is this a known limitation of workspace isolation?**
   - Should agents avoid Python file I/O in workspaces?
   - Is there documentation on workspace-safe file operations?

2. **Do MCP tools work correctly in workspaces?**
   - mcp__acp__Write
   - mcp__acp__Edit

3. **Should workspace isolation show warnings?**
   - If direct file I/O is problematic, should the system warn agents?

4. **Is there a preferred file editing pattern?**
   - For complex multi-section updates
   - For large file refactoring

## Recommended Action

For now, agents should:
1. Use MCP file tools instead of Python file I/O
2. Verify changes with jj status after edits
3. Fallback to delegation for complex multi-section updates
4. Document limitations when discovered

## Related Issues

- Workspace isolation mechanism: How does it work?
- File tracking in JJ: How does working copy detection work?
- Agent file editing best practices
