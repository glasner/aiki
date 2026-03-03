# Agent Names: Abbreviations → Lowercase Names

## Change

Replace abbreviated agent badges (`cc`, `cur`) with lowercase full names (`claude`, `cursor`, `codex`, `gemini`) in TUI rendering.

## Current → New

| Current | New | Color |
|---------|-----|-------|
| `cc` | `claude` | cyan |
| `cur` | `cursor` | magenta |
| (n/a) | `codex` | fg (default) |
| (n/a) | `gemini` | fg (default) |

## Scope: `cli/src/tui/` only

### 1. `builder.rs` — `agent_label()`

The source of all display labels. Maps `agent_type` / assignee → display string.

```
line 70:  Some(a) if a.contains("claude-code") || a == "cc" => Some("cc".to_string()),
line 71:  Some(a) if a.contains("cursor") || a == "cur" => Some("cur".to_string()),
```

Change to:
```rust
Some(a) if a.contains("claude-code") || a == "cc" || a == "claude" => Some("claude".to_string()),
Some(a) if a.contains("cursor") || a == "cur" => Some("cursor".to_string()),
Some(a) if a.contains("codex") => Some("codex".to_string()),
Some(a) if a.contains("gemini") => Some("gemini".to_string()),
```

Tests to update:
- `agent_label_from_data` (line 724): assert `"claude"` not `"cc"`, `"cursor"` not `"cur"`
- `agent_label_from_assignee` (line 738): assert `"claude"` not `"cc"`, accept `"cc"` as input still

### 2. `widgets/epic_tree.rs` — `agent_badge()`

Has its own parallel mapping (duplicates builder logic for the epic tree widget).

```
line 50:  Some(a) if a.contains("claude-code") || a == "cc" => Some(("cc", ...cyan))
line 53:  Some(a) if a.contains("cursor") || a == "cur" => Some(("cur", ...magenta))
```

Change to:
```rust
Some(a) if a.contains("claude") || a == "cc" => Some(("claude", ...cyan)),
Some(a) if a.contains("cursor") || a == "cur" => Some(("cursor", ...magenta)),
Some(a) if a.contains("codex") => Some(("codex", ...fg)),
Some(a) if a.contains("gemini") => Some(("gemini", ...fg)),
```

Tests to update:
- `agent_badge_shown` (line 492+): use `"claude"` in agent field, assert `"claude"` in output

### 3. `widgets/stage_list.rs` — `agent_style()`

Color lookup by agent string. Currently matches `"cc"` and `"cur"`.

```
line 76:  "cc" => Style::default().fg(theme.cyan),
line 77:  "cur" => Style::default().fg(theme.magenta),
```

Change to:
```rust
"claude" => Style::default().fg(theme.cyan),
"cursor" => Style::default().fg(theme.magenta),
```

Tests to update (all in stage_list.rs):
- `fix_stage_with_children` (line 506+): `"cur"` → `"cursor"`, `"cc"` → `"claude"` in test data + assertions
- `fix_with_review_fix_gate` (line 544+): same
- `agent_badge_colors` (line 668+): same

### 4. `views/workflow.rs` — tests only

Test data uses `"cc"` and `"cur"` as agent values:
- `stage_with_children` (line 434+): `"cc"` → `"claude"`, `"cur"` → `"cursor"`
- assertions (line 458-459): `contains("cc")` → `contains("claude")`, etc.

### 5. `AGENTS.md` — mockup format docs

Update the reference mockup (line 90-91):
```
 ⎿ ✓ Explore webhook requirements  claude  8s  ← dim ⎿, green ✓, text name, cyan claude, dim 8s
 ⎿ ▸ Implement route handler      cursor       ← yellow ▸, magenta cursor
```

And the annotation rules (line 103-104).

## Not in scope

- `agents/types.rs` (`as_str()` returns `"claude-code"`, `"cursor"` etc.) — these are serialization identifiers, not display labels
- `commands/` — agent display in non-TUI output
- Task storage, session storage, history — all use canonical `agent_type` strings
