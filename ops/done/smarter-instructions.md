# Smarter Instructions: AGENTS.md / CLAUDE.md Unification

**Date**: 2026-03-18
**Status**: Draft
**Purpose**: Automatically detect and use the user's preferred instruction filename (AGENTS.md or CLAUDE.md), and symlink the other so both tools work seamlessly.

**Related Documents**:
- [init.rs](../../cli/src/commands/init.rs) - `ensure_agents_md()` creates/updates AGENTS.md
- [doctor.rs](../../cli/src/commands/doctor.rs) - checks AGENTS.md for version
- [hooks.yaml](../../cli/src/flows/core/hooks.yaml) - context hint references AGENTS.md
- [agents_template.rs](../../cli/src/commands/agents_template.rs) - template content

---

## Executive Summary

Today aiki always writes to `AGENTS.md` and has no awareness of `CLAUDE.md`. But Claude Code reads `CLAUDE.md` as its project instructions file. The user currently works around this by manually symlinking `CLAUDE.md → AGENTS.md`. We need aiki to detect which file the user prefers, write to that one, and create a symlink for the other — so both Claude Code and other tools (Cursor, Codex, Copilot) find the instructions without manual setup.

---

## Background: Which Tools Read Which File

| Tool | Primary file | Also reads |
|------|-------------|------------|
| Claude Code | `CLAUDE.md` | — |
| Cursor | `.cursor/rules`, `.cursorrules` | `AGENTS.md` (recent) |
| Codex | `AGENTS.md` | `CODEX.md` |
| Copilot | `.github/copilot-instructions.md` | `AGENTS.md` |
| Generic convention | `AGENTS.md` | — |

`AGENTS.md` is the emerging cross-tool convention. `CLAUDE.md` is Claude Code's native file. These are the only two we need to handle — the others are tool-specific and don't conflict.

---

## How It Works

### Detection Logic (priority order)

When `aiki init` or `aiki doctor` runs, determine the "canonical" instruction file:

1. **Both exist and one is a symlink** → canonical = the real file (symlink target). No action needed beyond version check.

2. **Both exist and both are real files** → **conflict**. Aiki must not silently pick one.
   - Print a clear warning explaining the situation
   - Ask the user to choose (or provide a `--instructions-file` flag)
   - Never silently overwrite or merge

3. **Only `CLAUDE.md` exists (real file)** → canonical = `CLAUDE.md`. Symlink `AGENTS.md → CLAUDE.md`.

4. **Only `AGENTS.md` exists (real file)** → canonical = `AGENTS.md`. Symlink `CLAUDE.md → AGENTS.md`.

5. **Neither exists** → create `AGENTS.md` (the cross-tool default) and symlink `CLAUDE.md → AGENTS.md`.

### The Symlink

The non-canonical file becomes a relative symlink to the canonical file:
- If canonical is `AGENTS.md`: `CLAUDE.md → AGENTS.md`
- If canonical is `CLAUDE.md`: `AGENTS.md → CLAUDE.md`

Both are at repo root, so the symlink is always just the filename (no path).

### Runtime Context Hint

The hooks.yaml context line currently says:
```
"If you haven't already, read AGENTS.md to learn more about aiki"
```

This should be updated dynamically based on which file is canonical. Or, since both files always exist (one real + one symlink), the hint could simply remain as-is — agents will find the file regardless. Keeping it static avoids adding detection logic to the hot path.

**Decision: Keep the hint static.** Both filenames resolve to the same content. The hint's purpose is to remind agents to read the file; the actual filename doesn't matter since both exist.

---

## Use Cases

### Case 1: Fresh repo, no instruction files
```
$ aiki init
✓ Created AGENTS.md with task system instructions
✓ Created CLAUDE.md → AGENTS.md symlink
```

### Case 2: Existing CLAUDE.md (Claude Code user)
```
$ aiki init
✓ Added <aiki> block to CLAUDE.md
✓ Created AGENTS.md → CLAUDE.md symlink
```

### Case 3: Existing AGENTS.md (current behavior, plus symlink)
```
$ aiki init
✓ AGENTS.md already has <aiki> block
✓ Created CLAUDE.md → AGENTS.md symlink
```

### Case 4: Both files exist as real files
```
$ aiki init
⚠ Both AGENTS.md and CLAUDE.md exist as separate files.
  Aiki needs one canonical file with a symlink for the other.
  Options:
    aiki init --instructions-file AGENTS.md   (keep AGENTS.md, user must merge CLAUDE.md content)
    aiki init --instructions-file CLAUDE.md   (keep CLAUDE.md, user must merge AGENTS.md content)
  Run with one of these flags to resolve.
```

### Case 5: Doctor check
```
$ aiki doctor
Agent Instructions:
  ✓ CLAUDE.md has current <aiki> block (canonical)
  ✓ AGENTS.md → CLAUDE.md (symlink)
```

### Case 6: Doctor fix — symlink missing
```
$ aiki doctor --fix
Agent Instructions:
  ✓ AGENTS.md has current <aiki> block (canonical)
  ⚠ CLAUDE.md symlink missing
    ✓ Created CLAUDE.md → AGENTS.md symlink
```

---

## Implementation Plan

### Phase 1: Refactor `ensure_agents_md` → `ensure_instructions`

**File: `cli/src/commands/init.rs`**

Replace `ensure_agents_md()` with `ensure_instructions()`:

```rust
/// Canonical instruction filenames
const AGENTS_MD: &str = "AGENTS.md";
const CLAUDE_MD: &str = "CLAUDE.md";

/// Determine which instruction file is canonical and ensure both exist.
fn ensure_instructions(repo_root: &Path, preferred: Option<&str>, quiet: bool) -> Result<()> {
    let agents_path = repo_root.join(AGENTS_MD);
    let claude_path = repo_root.join(CLAUDE_MD);

    let agents_exists = agents_path.exists();  // follows symlinks
    let claude_exists = claude_path.exists();
    let agents_is_symlink = agents_path.symlink_metadata()
        .map(|m| m.file_type().is_symlink()).unwrap_or(false);
    let claude_is_symlink = claude_path.symlink_metadata()
        .map(|m| m.file_type().is_symlink()).unwrap_or(false);

    // Determine canonical file
    let canonical = match (agents_exists, claude_exists, agents_is_symlink, claude_is_symlink) {
        // Both exist, one is symlink → canonical is the real one
        (true, true, true, false) => CLAUDE_MD,
        (true, true, false, true) => AGENTS_MD,
        // Both exist, both real → conflict
        (true, true, false, false) => {
            if let Some(pref) = preferred {
                pref  // user specified --instructions-file
            } else {
                anyhow::bail!(
                    "Both AGENTS.md and CLAUDE.md exist as separate files.\n\
                     Aiki needs one canonical file with a symlink for the other.\n\
                     Options:\n\
                     \taiki init --instructions-file AGENTS.md\n\
                     \taiki init --instructions-file CLAUDE.md\n\
                     Run with one of these flags to resolve."
                );
            }
        }
        // Only one exists
        (true, false, false, _) => AGENTS_MD,
        (false, true, _, false) => CLAUDE_MD,
        // Neither exists
        _ => preferred.unwrap_or(AGENTS_MD),
    };

    // Write/update the <aiki> block in the canonical file
    ensure_aiki_block(repo_root, canonical, quiet)?;

    // Create symlink for the other file
    let other = if canonical == AGENTS_MD { CLAUDE_MD } else { AGENTS_MD };
    ensure_symlink(repo_root, canonical, other, quiet)?;

    Ok(())
}
```

### Phase 2: Add symlink helper

```rust
/// Create a symlink from `link_name` → `target_name` in repo_root.
/// No-op if symlink already exists and points to the right target.
fn ensure_symlink(repo_root: &Path, target_name: &str, link_name: &str, quiet: bool) -> Result<()> {
    let link_path = repo_root.join(link_name);

    if link_path.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        if let Ok(target) = fs::read_link(&link_path) {
            if target == Path::new(target_name) {
                if !quiet { println!("✓ {} → {} (symlink)", link_name, target_name); }
                return Ok(());
            }
        }
        // Wrong target — remove and recreate
        fs::remove_file(&link_path)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(target_name, &link_path)
        .context(format!("Failed to create {} → {} symlink", link_name, target_name))?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target_name, &link_path)
        .context(format!("Failed to create {} → {} symlink", link_name, target_name))?;

    if !quiet { println!("✓ Created {} → {} symlink", link_name, target_name); }
    Ok(())
}
```

### Phase 3: Rename `ensure_aiki_block` (extract from current `ensure_agents_md`)

Extract the block-writing logic from current `ensure_agents_md()` into a filename-agnostic `ensure_aiki_block(repo_root, filename, quiet)` that works on whichever file it's given.

### Phase 4: Update doctor.rs

Refactor the AGENTS.md check to use the same detection logic:
- Find canonical file (same detection as init)
- Check `<aiki>` block version in canonical file
- Check symlink exists and points correctly
- `--fix` can repair missing/broken symlinks

### Phase 5: Add `--instructions-file` flag to init

Add an optional `--instructions-file <AGENTS.md|CLAUDE.md>` argument to `aiki init` for resolving the both-exist conflict. Also useful if users want to explicitly choose.

### Phase 6: Shared detection module (optional)

Extract canonical-file detection into a shared utility (e.g., `cli/src/instructions.rs`) so init.rs and doctor.rs don't duplicate logic. Could also be used by future commands.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Both files exist, no `--instructions-file` flag | Print clear error with resolution options, exit non-zero |
| Symlink creation fails (permissions) | Print error, continue (non-fatal — instructions still work via canonical file) |
| Canonical file is a broken symlink | Treat as "not exists", create fresh |
| Windows without symlink permissions | Fall back to copying instead of symlinking, with a warning |
| User deletes symlink after init | `aiki doctor` detects and `--fix` recreates it |

---

## Open Questions

1. **Should we extract shared detection logic into a utility module?** Both init.rs and doctor.rs need the same canonical-file detection. A shared `instructions::detect_canonical()` in a common module would avoid duplication.

2. **Windows symlink fallback**: Windows requires elevated permissions for symlinks (or Developer Mode). Should we fall back to a file copy with a comment header like `<!-- This file is managed by aiki. Edit AGENTS.md instead. -->`? Or just warn and skip?

3. **Should `aiki doctor` report which file is canonical?** e.g., `✓ CLAUDE.md is canonical, AGENTS.md is symlink` — useful for debugging.

---
