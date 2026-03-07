---
status: draft
---

# Implementation: Flow Integration

**Date**: 2026-02-03
**Status**: Ready for implementation
**Phase**: 4 of 4
**Related**: [Workflow Commands Overview](workflow-commands.md)

---

## Overview

Integrate `aiki plan`, `aiki decompose`, and `aiki build` commands with the flow/hook system to enable declarative automation workflows.

## Deliverables

- `plan:` flow action
- `decompose:` flow action
- `build:` flow action
- Sugar triggers: `plan.completed`, `decompose.completed`, `build.completed`

## Files to Create/Modify

- `cli/src/flows/types.rs` - Add action variants
- `cli/src/flows/engine.rs` - Add handlers
- `cli/src/flows/sugar.rs` - Add sugar triggers

---

## Flow Actions

### `plan:` Action

Launches an interactive plan authoring session.

```yaml
on:
  - trigger: user_request
    actions:
      - plan: ops/now/my-feature.md
```

**Parameters:**
- `path` (optional) - Path to plan file. If omitted, prompts user for description
- `template` (optional) - Template to use (default: `plan`)
- `agent` (optional) - Agent for session (default: `claude-code`)

**Behavior:**
- Launches `aiki plan <path>` in interactive mode
- Waits for session to complete
- Emits `plan.completed` event with plan file path

### `decompose:` Action

Creates an epic from a plan file.

```yaml
on:
  - trigger: plan.completed
    actions:
      - decompose:
          plan: "{{ event.plan_path }}"
```

**Parameters:**
- `plan` (required) - Path to plan file
- `async` (optional) - Run asynchronously (default: false)
- `start` (optional) - Start implementation after decomposition (default: false)
- `template` (optional) - Decompose template (default: `decompose`)
- `agent` (optional) - Agent for decomposition (default: `claude-code`)

**Behavior:**
- Launches `aiki decompose <plan-path>` with specified options
- If `async: false`, waits for decomposition to complete
- Emits `decompose.completed` event with epic task ID

### `build:` Action

Orchestrates execution of an epic.

```yaml
on:
  - trigger: decompose.completed
    actions:
      - build:
          plan: "{{ event.plan_path }}"
```

**Parameters:**
- `plan` (required) - Path to plan file
- `async` (optional) - Run asynchronously (default: false)
- `template` (optional) - Build template (default: `build`)
- `agent` (optional) - Agent for build (default: `claude-code`)

**Behavior:**
- Launches `aiki build <plan-path>` with specified options
- If `async: false`, waits for build to complete
- Emits `build.completed` event with build task ID

---

## Sugar Triggers

### `plan.completed`

Fired when a plan authoring session completes.

**Event data:**
```json
{
  "plan_path": "ops/now/my-feature.md",
  "plan_task_id": "xtuttnyv...",
  "sections": ["Vision", "Requirements", "Open Questions"]
}
```

### `decompose.completed`

Fired when a decompose task completes.

**Event data:**
```json
{
  "plan_path": "ops/now/my-feature.md",
  "decompose_task_id": "decompose1234...",
  "epic_task_id": "epic5678...",
  "subtask_count": 5
}
```

### `build.completed`

Fired when a build task completes.

**Event data:**
```json
{
  "plan_path": "ops/now/my-feature.md",
  "impl_task_id": "impl5678...",
  "build_task_id": "build9012...",
  "subtasks_completed": 5,
  "duration_ms": 120000,
  "status": "success"
}
```

---

## Example Workflows

### Full Pipeline Automation

```yaml
# .aiki/flows/auto-implement.yml
on:
  - trigger: plan.completed
    actions:
      - decompose:
          plan: "{{ event.plan_path }}"
      
  - trigger: decompose.completed
    actions:
      - build:
          plan: "{{ event.plan_path }}"
      
  - trigger: build.completed
    actions:
      - review:
          task: "{{ event.build_task_id }}"
      
  - trigger: review.completed
    actions:
      - fix:
          review: "{{ event.review_task_id }}"
```

### Async Build with Notification

```yaml
# .aiki/flows/async-build.yml
on:
  - trigger: decompose.completed
    actions:
      - build:
          plan: "{{ event.plan_path }}"
          async: true
      - notify:
          message: "Build started for {{ event.plan_path }}"
  
  - trigger: build.completed
    actions:
      - notify:
          message: "Build finished: {{ event.status }}"
```

### Conditional Build

```yaml
# .aiki/flows/conditional-build.yml
on:
  - trigger: decompose.completed
    condition: "{{ event.subtask_count < 10 }}"
    actions:
      - build:
          plan: "{{ event.plan_path }}"
      
  - trigger: decompose.completed
    condition: "{{ event.subtask_count >= 10 }}"
    actions:
      - notify:
          message: "Plan has {{ event.subtask_count }} subtasks - review before building"
```

---

## Implementation Notes

### Action Handlers

Each action handler should:
1. Parse action parameters from flow YAML
2. Construct appropriate CLI command
3. Execute command (sync or async based on parameters)
4. Capture output and parse task IDs
5. Emit completion event with relevant data

### Event Emission

Events should be emitted:
- After command completes successfully
- With all relevant metadata (task IDs, file paths, etc.)
- In a structured format (JSON) for easy consumption by subsequent triggers

### Error Handling

If a workflow action fails:
- Emit a `<action>.failed` event with error details
- Stop the workflow chain (don't trigger dependent actions)
- Log error for user inspection

---

## Testing

### Test Cases

1. **Plan → Decompose → Build chain**
   - Create plan interactively
   - Verify `plan.completed` event fires
   - Verify decompose action triggers
   - Verify `decompose.completed` event fires
   - Verify build action triggers
   - Verify `build.completed` event fires

2. **Async build**
   - Trigger async build
   - Verify build task ID returned immediately
   - Verify build runs in background
   - Verify `build.completed` event fires when done

3. **Error handling**
   - Trigger decompose with invalid plan path
   - Verify `decompose.failed` event fires
   - Verify subsequent actions don't trigger

4. **Conditional workflows**
   - Create epic with different subtask counts
   - Verify condition evaluation
   - Verify correct action branch executes

---

## Future Enhancements (v2)

**Parallel Actions:**
- Execute multiple actions concurrently
- Wait for all to complete before emitting event

**Action Templates:**
- Reusable action configurations
- Parameterized action blocks

**Workflow Visualization:**
- Show active workflows in TUI
- Display event flow and action execution

**Workflow Debugging:**
- Step through workflow execution
- Inspect event data at each step
- Replay workflows for testing
