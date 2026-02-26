## Bug: `aiki plan` Doesn't Feed Interactive Prompt Text to the Agent

**Severity**: High (confirmed by user)
**Location**: `cli/src/commands/plan.rs`

### Root Cause (two issues)

**Issue 1 — Prompt passthrough:** When the user types text in the interactive prompt (`prompt_multiline_input` at line 379), that text is NOT included in the prompt sent to the spawned Claude process. The Claude process receives only:
```
Run `aiki task start <id>` to begin working on this plan task.
```

**Issue 2 — Tangled data flow:** The code maintains three overlapping variables (`args_idea`, `user_text`, `initial_idea`) that get merged in confusing ways:

```
args_idea    = from filename ("Dark Mode")
args_text    = trailing CLI args ("add JWT auth")
user_text    = interactive prompt OR args_text (fallback)
initial_idea = merge(args_idea, user_text) via colon separator
```

Then `create_plan_task` receives BOTH the merged `initial_idea` AND raw `user_text`, and `build_user_context` tries to format them as separate Topic/Guidance — producing duplication when both exist:
```
**Topic:** Dark Mode: add JWT auth     ← merged, contains user text
**User guidance:**
> add JWT auth                          ← same text again
```

### Fix: Simplify to Single `initial_idea`

Collapse everything into one context string. The interactive prompt is just another way to provide it — it fires only when no text was given on the command line.

**Scenarios:**

| Command | `initial_idea` | Interactive prompt? |
|---------|---------------|-------------------|
| `aiki plan dark-mode.md add JWT auth` | "Dark Mode: add JWT auth" | No (text on CLI) |
| `aiki plan dark-mode.md` | "Dark Mode" + interactive text | Yes (no text on CLI) |
| `aiki plan dark-mode.md` → user types "add JWT auth" | "Dark Mode: add JWT auth" | Yes |
| `aiki plan dark-mode.md` → user presses Esc | "Dark Mode" | Yes (skipped) |
| `aiki plan Add user authentication` | "Add user authentication" | No (autogen) |

**Code changes in `run_plan()`:**

```rust
// 1. Compute initial_idea from args first
let mut initial_idea = if !args_text.is_empty() {
    if args_idea.is_empty() {
        args_text
    } else {
        format!("{}: {}", args_idea, args_text)
    }
} else {
    args_idea
};

// 2. Interactive prompt only fires when no text was provided on CLI
let has_cli_text = !args_text.is_empty();
if initial_idea_needs_input(&mode, has_cli_text) && io::stdin().is_terminal() {
    let header = format!("Plan: {}", plan_path.display());
    if let Some(text) = prompt_multiline_input(&header)? {
        if initial_idea.is_empty() {
            initial_idea = text;
        } else {
            initial_idea = format!("{}: {}", initial_idea, text);
        }
    }
}

// 3. Single value flows downstream — no separate user_text
create_plan_task(cwd, &plan_path, &initial_idea, is_new, ...);

// 4. Include in Claude prompt
let prompt = if initial_idea.is_empty() {
    format!("Run `aiki task start {}` to begin working on this plan task.", plan_task_id)
} else {
    format!(
        "Run `aiki task start {}` to begin working on this plan task.\n\nUser's guidance: {}",
        plan_task_id, initial_idea
    )
};
```

**Downstream simplifications:**

- Remove `user_text` parameter from `create_plan_task()`
- Remove `build_user_context()` — replaced by single `{{data.initial_idea}}` in template
- Template uses `{{data.initial_idea}}` directly instead of `{{data.user_context}}`
- No more duplication in template output

**Helper for when to prompt:**

```rust
/// Interactive prompt fires when:
/// - Not autogen mode (description IS the idea)
/// - No trailing CLI text was provided (args_text was empty)
///
/// When args_text is non-empty, initial_idea already contains the user's
/// guidance merged in, so we skip the prompt. When args_text is empty,
/// initial_idea is only the filename-derived topic (e.g., "Dark Mode"),
/// and the user may want to add more specific guidance interactively.
fn initial_idea_needs_input(mode: &PlanMode, has_cli_text: bool) -> bool {
    !matches!(mode, PlanMode::Autogen { .. }) && !has_cli_text
}
```

The caller passes `has_cli_text: !args_text.is_empty()` so the function has a clear boolean signal rather than trying to infer intent from the merged `initial_idea` string. This avoids the contradiction where the prose says "only prompt when no CLI text" but the implementation would prompt unconditionally for all non-autogen modes.

Note: even in Edit/CreateAtPath modes where `initial_idea` comes from the filename, we still prompt *when no CLI text was given* — the filename-derived idea ("Dark Mode") is a topic, and the user might want to add more specific guidance. The prompt is the opportunity to do so.
