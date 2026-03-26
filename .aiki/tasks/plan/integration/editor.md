---
version: 1.0.0
type: plan
assignee: claude-code
interactive: true
---

# Editor Integration Plan: {{data.editor_name}}

Create a hook-based integration plan for **{{data.editor_name}}**.

**User's guidance:** {{data.initial_idea}}

**For new plans:** Work through the subtasks in order.

**For existing plans:**
1. Read and summarize current state
2. Identify which phase the plan is in
3. Mark already-completed subtasks as `--wont-do`
4. Start the appropriate subtask for where work is needed

**Throughout:**
- Write to the plan file incrementally (don't wait until the end)
- Keep the conversation focused on gathering facts, then writing
- Track open questions as subtasks under "Resolve open questions"

## Reference Implementation

The Claude Code integration at `cli/src/editors/claude_code/` is the canonical
reference. Read these files to understand the architecture before drafting:

| File | Role |
|------|------|
| `editors/claude_code/mod.rs` | Entry point: stdin → parse → dispatch → output → exit |
| `editors/claude_code/events.rs` | Vendor JSON → `AikiEvent` mapping (source discrimination) |
| `editors/claude_code/session.rs` | Session creation and version caching |
| `editors/claude_code/output.rs` | `HookResult` → vendor-specific JSON output |
| `editors/claude_code/tools.rs` | Tool name classification and input parsing |

## Plan Structure

The output plan at `ops/now/<slug>-hooks.md` should follow this structure:

    # <Editor Name> Hooks Plan

    ## Goal
    ## Current State
    ## New Upstream Capability
      - Reference links
      - Hook Payloads (example input + output JSON per event)
    ## Design Direction
      - Event coverage table (hooks vs OTLP vs TTL)
      - Target split
    ## Proposed Architecture
      1. Global hook config (target config block)
      2. Stdin payload adapter (event mapping table, module structure)
      3. Session identity and mode
      4. OTLP scope (what hooks cover vs what OTLP still covers)
    ## Migration Plan (phased with exit criteria)
    ## Open Questions
    ## Risks
    ## Success Criteria

## Subtasks

### Clarify editor details

Ask only what you can't figure out on your own. Review any context the user
already provided above, then ask about anything still missing from this
minimal set:

1. **What editor/agent are we integrating?** (name + slug for file paths)
2. **Where are the hook docs?** (URL — docs page, PR, or GitHub repo)

That's it. Everything else can be discovered by:
- Checking `cli/src/editors/` for existing integration state
- Reading the upstream docs/source for hook events, config format, payloads
- Checking `cli/src/config.rs` for existing installer patterns

Skip any question the user already answered. Close when you have a name and
a starting URL to research from.

### Research upstream hook system

Using the docs/source identified in the previous step:

1. Fetch upstream hook documentation and source code
2. Identify all supported hook events and their invocation model:
   - What events exist? (session start, stop, tool use, prompt submit, etc.)
   - How are hooks configured? (global config, repo-local, both?)
   - How are hooks invoked? (stdin JSON, CLI args, env vars?)
   - Can hooks return output? (stdout JSON, exit codes?)
   - Do hooks run synchronously or asynchronously?
3. For each hook event, fetch or document the **input payload schema**:
   - What fields are provided? (session_id, cwd, model, etc.)
   - Which fields are required vs optional?
   - Are there discriminator fields? (e.g., `source` on SessionStart)
4. For each hook event, fetch or document the **output schema**:
   - What fields can the hook return?
   - Can hooks inject context into the agent? (additionalContext, systemMessage)
   - Can hooks block/approve/deny actions?
5. Write findings to the plan file under "Hook Payloads" with example JSON
   for every event (input and output)

Close with summary of events found and any gaps vs Claude Code.

### Map events to Aiki lifecycle

Build the event mapping. For each upstream hook event, determine:

1. Which `AikiEvent` variant it maps to
2. Whether a discriminator field (like `source`) splits it into multiple events
3. What fields from the payload map to which `AikiEvent` fields
4. Whether the hook output needs to return data (context injection, decisions)

Write two tables to the plan:

**Event coverage table** (what hooks cover vs what needs OTLP/other):

| Aiki Event | Source | Editor Hook | Discriminator | OTLP needed? |
|---|---|---|---|---|

**Detailed event mapping** (for the events.rs implementation):

| Editor `hook_event_name` | discriminator | → Aiki Event |
|---|---|---|

Pay special attention to:
- SessionStart source discrimination (startup/resume/clear/compact)
- Whether a session-end hook exists or if TTL cleanup is needed
- Whether stop/turn-complete is per-turn or per-session
- Pre-tool vs post-tool hook availability

Close with mapping tables written to the plan.

### Draft architecture and installer

Write the Proposed Architecture section:

1. **Hook config** — Target config block showing exactly what `aiki hooks
   install` should produce. Address coexistence with existing config (OTLP,
   notify, other hooks) and idempotency.

2. **Stdin payload adapter** — Module structure mirroring `editors/claude_code/`:
   - `events.rs` — Vendor event enum, payload structs, `AikiEvent` builders,
     source discrimination. Include Rust pseudocode for key dispatch.
   - `session.rs` — Session creation via `AikiSession::for_hook()`
   - `output.rs` — Per-event output builders matching upstream schemas
   - `mod.rs` — Entry point with special lifecycle handling

3. **Session identity** — UUID derivation, `AIKI_TASK` propagation, session
   mode for background runs.

4. **OTLP scope** — What hooks now cover vs what OTLP still handles.

Close with architecture written to the plan.

### Write migration phases

Write the phased migration plan. Typical phases:

1. Research and instrumentation (debug logging, payload docs)
2. Core lifecycle hooks (session start, turn start/complete)
3. Tool hooks (pre-tool, post-tool if available)
4. Installer and config migration
5. Integration verification and testing

Each phase needs:
- Numbered implementation steps
- Exit criteria
- What can be validated independently

Adjust phases based on what's actually available for this editor. Close with
migration plan written.

### Resolve open questions

Review the plan so far and identify remaining unknowns. Create a subtask for
each open question:

```bash
aiki task add --subtask-of {{parent.id}} "OQ: <question>"
```

Common open questions for editor integrations:
- Payload differences between interactive and background/exec modes?
- Does the editor expose session IDs consistently across modes?
- What metadata is available for session records (model, version)?
- Are hook payloads stable or experimental/subject to change?

Work through each question subtask with the user. Update the plan based on
answers. Close this subtask when all question subtasks are resolved.

### Validate completeness

Read the complete plan and verify:

| Check | Criteria |
|-------|----------|
| Payloads documented | All hook events have example input + output JSON |
| Event mapping complete | Every Aiki lifecycle event has a source (hook, OTLP, or TTL) |
| Module structure defined | events.rs, session.rs, output.rs, mod.rs specified |
| Installer designed | Target config shown, idempotency addressed |
| Migration phased | Clear phases with exit criteria |
| Gaps identified | Missing hooks documented with workarounds |
| Open questions listed | Remaining unknowns called out |

For each issue found, add a subtask:
```bash
aiki task add --subtask-of {{parent.id}} "<Category>: <description>"
```

Work through each issue, update the plan, close the subtask. When all issues
are resolved, close this validation subtask.

### Confirm completion

Ask the user if the plan is complete and ready for implementation.

1. If yes: close this task with summary "Plan ready at ops/now/<slug>-hooks.md"
2. If the user wants to continue editing: close as wont_do
