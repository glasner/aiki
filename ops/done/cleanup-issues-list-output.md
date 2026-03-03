# Improve `aiki review issue list` output format

## Context

The `aiki review issue list <review>` output is too bare and hard to scan. Currently it produces:
```
  high: Missing null check (src/auth.rs:42)
  medium: Consider using const (src/utils.rs:10-15)
```

No header, no structure, no review context. Goal: render issue list as a TUI view matching the visual language of the workflow/epic views (`cli/src/tui/`).

## Mockups

### Normal: 3 issues, short text fits on one line

```
[80 cols]
 [rrmqxnps] Review fix-auth-subtasks  3 issues       ← dim [id], hi+bold name, yellow count
                                                      ← blank row
 ⎿ [high] Missing null check                                        src/auth.rs:42 ← dim ⎿, red [high]+text, dim location right-aligned
 ⎿ [medium] Consider using const                                src/utils.rs:10-15 ← dim ⎿, yellow [medium]+text, dim location
 ⎿ [low] Trailing whitespace                                           README.md:3 ← dim ⎿, dim [low]+text, dim location
```

### Long text: wraps to continuation lines

```
[80 cols]
 [rrmqxnps] Review fix-auth-subtasks  2 issues
                                                      ← blank row
 ⎿ [high] The null check is missing in the auth      src/auth.rs:42 ← location right-aligned on first line
          handler which could cause a panic when                     ← continuation: indented to align under text start
          the token is None                                          ← continuation: same indent, severity color
 ⎿ [medium] Consider using const for repeated    src/utils.rs:10-15
            string literals to improve clarity
```

Wrap rules:
- Location is right-aligned on the **first line** of each issue
- Text wraps at word boundaries to avoid splitting the location
- Continuation lines indent to align with text start (past `⎿ [severity] `)
- Continuation lines use the same severity color as the first line
- Available text width = 80 - left_pad(1) - connector(2) - badge_width - gap(2) - location_width

### Empty: no issues found

```
[80 cols]
 [rrmqxnps] Review fix-auth-subtasks  No issues      ← dim [id], hi+bold name, green "No issues"
```

Single line, no blank row, no tree.

### Single issue

```
[80 cols]
 [rrmqxnps] Review fix-auth-subtasks  1 issue        ← singular "issue"
                                                      ← blank row
 ⎿ [high] Missing null check                                        src/auth.rs:42
```

### No location

```
[80 cols]
 [rrmqxnps] Review fix-auth-subtasks  1 issue
                                                      ← blank row
 ⎿ [medium] Consider refactoring this module          ← text uses full width, no right-aligned suffix
```

When there's no location, text gets the full width (80 - left_pad - connector - badge) before wrapping.

## Layout rules

- Width: 80 columns (matches workflow/epic views)
- Row 0: header — 1-char left pad, `[short_id]` dim, space, name hi+bold, 2-space gap, count
- Row 1: blank separator (omitted when empty)
- Rows 2+: issue lines — 1-char left pad, `⎿` dim, space, `[severity]` + text in severity color, location dim right-aligned flush to col 80
- Severity sort: high → medium → low (existing `severity_order()`)
- Height: 1 (empty) or 2 + sum of lines per issue (each issue is 1 + continuation_lines)

### Text wrapping

The text column sits between the badge and the location:

```
|1|⎿ |[severity] |text................................|  |location.........|
 ^  ^             ^                                    ^
 pad connector    text_start                           gap  right-aligned loc
```

- `text_start` = 1 (pad) + 2 (⎿ + space) + badge_len + 1 (space after badge)
- `text_width` = 80 - text_start - 2 (gap) - location_len
- If no location: `text_width` = 80 - text_start
- Word-wrap the issue text into `text_width` columns
- First line: render text + location on same row
- Continuation lines: indent to `text_start`, no connector, same severity color

## Severity → Theme color mapping

| Severity | Color | Style |
|----------|-------|-------|
| high | `theme.red` | `[high]` badge + issue text both red |
| medium | `theme.yellow` | `[medium]` badge + issue text both yellow |
| low | `theme.dim` | `[low]` badge + issue text both dim |

## Files to modify

1. **`cli/src/tui/views/issue_list.rs`** (new) — `render_issue_list()` view
   - Takes sorted issue list (severity, text, locations) + review short_id + review name + `&Theme`
   - Composes header line + issue tree lines into a `Buffer`
   - Follows EpicTree rendering patterns: `⎿` connectors, right-aligned metadata
   - Implements word-wrap for long issue text with continuation line indentation

2. **`cli/src/tui/views/mod.rs`** — add `pub mod issue_list;`

3. **`cli/src/commands/review.rs`** — `run_issue_list()` (lines 898-937)
   - Replace `emit_stderr` closure with: build TUI view → `buffer_to_ansi` → print
   - Import `tui::views::issue_list::render_issue_list`, `tui::buffer_ansi::buffer_to_ansi`, `tui::theme::{detect_mode, Theme}`

## Reuse

- `get_issue_comments()`, `comment_severity()`, `severity_order()`, `parse_locations()`, `format_locations()` — existing helpers in review.rs
- `Theme`, `detect_mode()` — from `tui::theme`
- `buffer_to_ansi()` — from `tui::buffer_ansi`
- EpicTree patterns: `⎿` connector, dim style, right-aligned metadata, hi/bold header

## Verification

1. `cargo build` — compiles cleanly
2. `cargo test` — existing tests pass
3. Add unit tests in `issue_list.rs` (buffer text assertions, matching EpicTree test style)
4. Test cases should include: short text, long wrapping text, no location, empty list
5. Manual: `aiki review issue list <review-id>` shows themed output
