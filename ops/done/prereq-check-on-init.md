# Add Prerequisite Checking to `aiki init` (Plan)

**Date**: 2026-03-09
**Status**: Draft
**Purpose**: Make `aiki init` check for required tools (git, jj) before attempting initialization, sharing code path with `aiki doctor`.

**Related Documents**:
- `cli/docs/getting-started.md#L48:53` (lists prerequisites)
- `cli/src/commands/doctor.rs:48-63` (existing prereq check implementation)
- `cli/src/commands/init.rs` (target file)

---

## Executive Summary
`aiki doctor` checks for required prerequisites (git, jj) and provides clear error messages if any are missing. However, `aiki init` does not perform these checks, leading to cryptic errors when users run init without having the required tools installed. This plan adds prerequisite validation to `aiki init` by refactoring the existing check logic from `doctor` into a shared module.

## Problem

`aiki doctor` already has the logic to check these prerequisites (see `check_command_version()` at cli/src/commands/doctor.rs:18-28 and usage at lines 48-63), but this code is not shared with `init`.

## Goal
- Make `aiki init` fail fast with helpful error messages when prerequisites are missing
- Share the prerequisite checking logic between `doctor` and `init` to avoid duplication
- Maintain the same error message format and quality between both commands

## Scope
- In scope:
  - Extract `check_command_version()` and prerequisite checking logic to a shared module
  - Add prerequisite validation to `aiki init` before any initialization work begins
  - Update both `doctor` and `init` to use the shared code path
  - Ensure error messages guide users to install missing tools

- Out of scope:
  - Installing prerequisites automatically
  - Checking for specific versions of tools (only that they exist and respond to `--version`)
  - Modifying the prerequisite list itself

## Proposed behavior

### Shared Module
Create `cli/src/prerequisites.rs` with:
- `check_command_version(cmd: &str) -> Option<String>` - checks if a command exists and returns its version
- `check_prerequisites(quiet: bool) -> Result<()>` - validates all prerequisites and returns error if any are missing
- `PREREQUISITES: &[(&str, &str)]` - shared constant defining the required tools and their descriptions

### `aiki init` behavior
Before attempting any initialization (detecting repo, setting up hooks, etc.):
1. Call `check_prerequisites(quiet)`
2. If any prerequisites are missing, fail with a clear error message like:
   ```
   Error: Missing required prerequisites

   The following tools are required but not found:
     ✗ Jujutsu version control (jj)
       → Install from https://martinvonz.github.io/jj/latest/install-and-setup/

   Run `aiki doctor` for a full system check.
   ```
3. Only proceed with initialization if all checks pass

### `aiki doctor` behavior
Continue to show prerequisite status as it does now (cli/src/commands/doctor.rs:48-63), but use the shared `check_prerequisites()` logic under the hood. Keep the same output format for consistency.

## Implementation plan

1. **Create shared module** (`cli/src/prerequisites.rs`)
   - Move `check_command_version()` from `doctor.rs`
   - Move `prerequisites` array to a shared constant
   - Create `check_prerequisites(quiet: bool) -> Result<()>` that:
     - Iterates through all prerequisites
     - Collects missing tools
     - Returns Ok(()) if all found, Err with helpful message if any missing
   - Add to `cli/src/lib.rs` module declarations

2. **Update `aiki doctor`** (`cli/src/commands/doctor.rs`)
   - Import from `crate::prerequisites`
   - Replace inline prerequisite checking with call to shared module
   - Keep existing output format (✓/✗ with version info)

3. **Update `aiki init`** (`cli/src/commands/init.rs`)
   - Import `crate::prerequisites::check_prerequisites`
   - Add prerequisite check at the very start of `run()`, before `RepoDetector::new()`
   - If check fails, return early with the error
   - In quiet mode, suppress per-tool output but still fail on missing prerequisites

4. **Test the changes**
   - Verify `aiki doctor` still works and shows same output
   - Verify `aiki init` fails gracefully when prerequisites are missing
   - Verify `aiki init` succeeds when all prerequisites are present
   - Test both quiet and non-quiet modes

## Acceptance criteria
- [ ] `check_command_version()` moved to shared `cli/src/prerequisites.rs` module
- [ ] `aiki init` checks prerequisites before any initialization work
- [ ] `aiki init` fails with clear error message when prerequisites are missing
- [ ] `aiki doctor` continues to work with same output format
- [ ] Both commands use the same prerequisite list and checking logic
- [ ] Error messages guide users to install missing tools
- [ ] `aiki init --quiet` still fails on missing prerequisites but with minimal output
- [ ] No code duplication between `doctor.rs` and `init.rs` for prerequisite checking

## Open questions
- Should `--quiet` mode in `init` show any output about prerequisites, or only fail silently? (Recommend: show missing prereqs even in quiet mode, as this is a blocking error)
- Should we provide install links in error messages? (Recommend: yes, at least for jj which is less common)
