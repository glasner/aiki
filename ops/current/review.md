# Hooks-to-Events Phases 1–2 Review

## Findings



3. **~~Claude's `shell.done` always reports success~~** FIXED
   - ~~`build_shell_done_event` hard-codes `exit_code = 0` and never attempts to distinguish stdout versus stderr (`cli/src/vendors/claude_code.rs:309-332`).~~
   - Now parses `tool_response` from PostToolUse payload using `BashToolResponse` struct to extract actual `exitCode`, `stdout`, and `stderr` from Claude Code's Bash tool response.

4. **Inconsistent error logging in Claude Code vendor**
   - `build_change_done_event` uses `eprintln!` for warnings (`cli/src/vendors/claude_code.rs:279, 284`) while all other warning paths in the same file use `debug_log`.
   - This inconsistency means these warnings always print to stderr regardless of debug mode, potentially confusing users with internal implementation details.

5. 

6. **Duplicate `ToolType` enum definitions**
   - Both vendors define their own `ToolType` enum (`cli/src/vendors/claude_code.rs:107-118`, `cli/src/vendors/cursor.rs:77-84`).
   - While the variants differ slightly (Claude has `Shell` and `ReadOnly`, Cursor only has `FileChange` and `Mcp`), this duplication could lead to drift. Consider extracting common tool classification logic if the lists grow.

7. **Magic strings for tool name classification**
   - Tool names are matched via inline string literals (`cli/src/vendors/claude_code.rs:122-133`, `cli/src/vendors/cursor.rs:91-95`).
   - No constants or static sets are defined, making it harder to audit which tools trigger which events across the codebase.

---
