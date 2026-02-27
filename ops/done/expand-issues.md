---

---

# Expand Review Issue Structured Data

**Date**: 2026-02-27
**Status**: Draft
**Purpose**: Add severity and location fields to review issues

**Related Documents**:
- [Better Review-Fix Contract](../done/better-review-fix-contract.md) - Original issue system design
- [Review Status Helpers](../done/review-status-helpers.md) - `data.issue_count` and `data.approved`
- [Gerrit Comment.java](https://gerrit.googlesource.com/gerrit/+/refs/heads/master/java/com/google/gerrit/extensions/client/Comment.java) - Gerrit's comment data model (path, line, range, fixSuggestions)

---

## Executive Summary

Review issues currently store only `data.issue = "true"` alongside free-form text. This gives downstream consumers (fix tasks, spawn conditions, display) no structured context — they can't prioritize critical issues over nits, or know which file is affected without parsing prose. Adding `severity` and `location` fields makes issues actionable for both humans and automation.

---

## User Experience

### CLI Syntax

```bash
# Severity (default: medium)
aiki review issue add <review-id> "Missing null check" --severity high
aiki review issue add <review-id> "Consider renaming var" --severity low

# Location — file, single line, or line range (repeatable)
aiki review issue add <review-id> "Missing null check" --file src/auth.rs
aiki review issue add <review-id> "Missing null check" --file src/auth.rs:42
aiki review issue add <review-id> "Duplicated logic" --file src/auth.rs:42-50

# Multiple files (repeatable flag)
aiki review issue add <review-id> "Caller not updated after signature change" \
  --file src/auth.rs:42-50 --file src/main.rs:108

# Both
aiki review issue add <review-id> "Missing null check" --severity high --file src/auth.rs:42-50

# Shorthand severity flags
aiki review issue add <review-id> "Missing null check" --high --file src/auth.rs:42
aiki review issue add <review-id> "Consider renaming var" --low
```

### Severity Values

| Value | Meaning | Fix priority |
|-------|---------|--------------|
| `high` | Must fix — incorrect behavior, bug, or contract violation | Highest |
| `medium` | Should fix — suboptimal, missing, or inconsistent | **Default** |
| `low` | Could fix — style, naming, cosmetic | Lowest |

Default is `medium` — aligns with how agents already naturally classify findings (analysis of ~50 historical review issues showed agents reach for "High" and "Medium" organically, with most untagged issues being medium-severity).

### Location Format

`--file <path>[:<line>[-<end_line>]]` — a file path relative to the repo root, optionally with a line or line range.

Examples:
- `--file src/auth.rs` (file only)
- `--file src/auth.rs:42` (single line)
- `--file src/auth.rs:42-50` (line range)

`--file` is repeatable — use it multiple times for issues that span files:
```bash
--file src/auth.rs:42-50 --file src/main.rs:108
```

Inspired by [Gerrit's Range model](https://gerrit.googlesource.com/gerrit/+/refs/heads/master/java/com/google/gerrit/extensions/client/Comment.java) (`startLine`, `startCharacter`, `endLine`, `endCharacter`). We implement line-level ranges now; character-level precision can be added later without breaking changes.

### Display

`aiki review show` and `aiki review issue list` format issues with severity and location:

```
### Issues (3)
  high: Missing null check in auth handler (src/auth.rs:42-50)
  high: SQL injection in query builder (src/db.rs:108)
  low: Consider renaming `x` to `count` (src/utils.rs:10)
```

Location is reassembled from decomposed fields for display:
- `path` only → `(src/auth.rs)`
- `path` + `start_line` → `(src/auth.rs:42)`
- `path` + `start_line` + `end_line` → `(src/auth.rs:42-50)`
- multiple locations → `(src/auth.rs:42-50, src/main.rs:108)`
- no location fields → no suffix

---

## How It Works

### Data Model

Issues are comments with structured `data` fields. The existing `HashMap<String, String>` on `TaskComment` already supports arbitrary keys — no schema changes needed.

**Before:**
```
data: { "issue": "true" }
```

**After (single file):**
```
data: {
  "issue": "true",
  "severity": "high",     // "high" | "medium" | "low"
  "path": "src/auth.rs",  // optional, file path relative to repo root
  "start_line": "42",     // optional, 1-based line number
  "end_line": "50"         // optional, defaults to start_line if omitted
}
```

**After (multiple files):**
```
data: {
  "issue": "true",
  "severity": "high",
  "locations": "src/auth.rs:42-50,src/main.rs:108"  // comma-separated
}
```

**Storage rule:** When a single `--file` is provided, decompose into `path`, `start_line`, `end_line` for easy downstream access. When multiple `--file` flags are given, store as a comma-separated `locations` field (the packed format is acceptable here since multi-file issues are less common and consumers will iterate anyway).

A helper function `parse_locations(comment) -> Vec<Location>` normalizes both representations, so downstream code always works with a `Vec<Location>` regardless of how it was stored.

### Comment Storage

`comment_on_task()` already accepts a `HashMap<String, String>` for data — `run_issue_add` just needs to insert the additional keys.

### Backward Compatibility

- Issues without `severity` are treated as `medium` (the default)
- Issues without `path`/`start_line`/`end_line` display without a location — same as today
- `get_issue_comments()` filter logic is unchanged (`data.issue == "true"`)
- `data.issue_count` counting is unchanged

---

## Downstream Consumers

### 1. `aiki fix` (fix.rs)

**Current:** Creates one fix subtask per issue, using `comment.text` as the description.

**Change:** Include severity and location in the fix subtask name/description so the fix agent has structured context:
- `"Fix: [high] Missing null check (src/auth.rs:42-50)"` instead of `"Fix: Missing null check in auth handler"`
- Sort issues by severity when creating subtasks (high first)

### 2. `aiki review show` / `aiki review issue list` (review.rs)

**Change:** Format issues with severity prefix and location suffix (see Display section above).

### 3. Review template (aiki/review.md)

**Change:** Update the template instructions to teach agents the new flags:
```bash
aiki review issue add {{parent.id}} "Description" --severity high --file path/to/file.rs:42-50
```

### 4. Spawn conditions (future)

With severity data, spawn conditions could eventually express things like:
```yaml
when: "data.high_count > 0"   # only spawn fix for high-severity issues
```

This is out of scope for now — we'd need to count by severity at close time. For now, `data.issue_count` stays as-is (total count).

---

## Implementation Plan

### Phase 1: CLI + Storage

1. **Add flags to `ReviewIssueSubcommands::Add`** — `--severity`, `--file` (repeatable via `Vec<String>`), plus `--high`/`--low` shorthands
2. **Add `Location` struct and parser** — parse `path`, `path:line`, `path:line-end_line` format. Validate line numbers are positive integers. Don't validate file exists (issues may reference files from a diff)
3. **Update `run_issue_add()`** — single file: decompose into `path`, `start_line`, `end_line` data keys. Multiple files: store as comma-separated `locations` field
4. **Add `parse_locations()` helper** — normalizes both storage formats into `Vec<Location>` for downstream consumers
5. **Validate severity values** — reject anything not in `{high, medium, low}`

### Phase 2: Display

6. **Update `run_issue_list()`** — show severity prefix and location suffix (use `parse_locations()` for display)
7. **Update `show_review()`** — same formatting in the review show output
8. **Sort issues by severity** in both display paths (high → medium → low)

### Phase 3: Downstream

9. **Update `create_fix_task()` in fix.rs** — include severity/location in fix subtask descriptions
10. **Update review template** (`.aiki/templates/aiki/review.md`) — teach agents the new flags
11. **Update fix template** (`.aiki/templates/aiki/fix.md`) — mention that issues now have severity/location context
12. **Update `cli/src/commands/agents_template.rs`** — add `aiki review issue add` with `--severity` and `--file` flags to the AIKI_BLOCK_TEMPLATE so all agents learn the new syntax via `aiki init` / `aiki doctor`
13. **Update `AGENTS.md`** — add review issue instructions to the Code Reviews section (currently only mentions `aiki task comment` for issues, should teach `aiki review issue add --severity --file`)

---

## Open Questions

(none currently — severity values and location format are settled)

---
