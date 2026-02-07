# Implementation: Flow Integration

**Date**: 2026-02-03
**Status**: Ready for implementation
**Phase**: 4 of 4
**Related**: [Workflow Commands Overview](workflow-commands.md)

---

## Overview

Integrate `aiki spec`, `aiki plan`, and `aiki build` commands with the flow/hook system to enable declarative automation workflows.

## Deliverables

- `spec:` flow action
- `plan:` flow action
- `build:` flow action
- Sugar triggers: `spec.completed`, `plan.completed`, `build.completed`

## Files to Create/Modify

- `cli/src/flows/types.rs` - Add action variants
- `cli/src/flows/engine.rs` - Add handlers
- `cli/src/flows/sugar.rs` - Add sugar triggers

---

## Flow Actions

### `spec:` Action

Launches an interactive spec authoring session.

```yaml
on:
  - trigger: user_request
    actions:
      - spec: ops/now/my-feature.md
```

**Parameters:**
- `path` (optional) - Path to spec file. If omitted, prompts user for description
- `template` (optional) - Template to use (default: `aiki/spec`)
- `agent` (optional) - Agent for session (default: `claude-code`)

**Behavior:**
- Launches `aiki spec <path>` in interactive mode
- Waits for session to complete
- Emits `spec.completed` event with spec file path

### `plan:` Action

Creates a planning task from a spec file.

```yaml
on:
  - trigger: spec.completed
    actions:
      - plan:
          spec: "{{ event.spec_path }}"
```

**Parameters:**
- `spec` (required) - Path to spec file
- `async` (optional) - Run asynchronously (default: false)
- `start` (optional) - Start implementation after planning (default: false)
- `template` (optional) - Planning template (default: `aiki/plan`)
- `agent` (optional) - Agent for planning (default: `claude-code`)

**Behavior:**
- Launches `aiki plan <spec-path>` with specified options
- If `async: false`, waits for planning to complete
- Emits `plan.completed` event with implementation task ID

### `build:` Action

Orchestrates execution of an implementation plan.

```yaml
on:
  - trigger: plan.completed
    actions:
      - build:
          spec: "{{ event.spec_path }}"
```

**Parameters:**
- `spec` (required) - Path to spec file
- `async` (optional) - Run asynchronously (default: false)
- `template` (optional) - Build template (default: `aiki/build`)
- `agent` (optional) - Agent for build (default: `claude-code`)

**Behavior:**
- Launches `aiki build <spec-path>` with specified options
- If `async: false`, waits for build to complete
- Emits `build.completed` event with build task ID

---

## Sugar Triggers

### `spec.completed`

Fired when a spec authoring session completes.

**Event data:**
```json
{
  "spec_path": "ops/now/my-feature.md",
  "spec_task_id": "xtuttnyv...",
  "sections": ["Vision", "Requirements", "Open Questions"]
}
```

### `plan.completed`

Fired when a planning task completes.

**Event data:**
```json
{
  "spec_path": "ops/now/my-feature.md",
  "planning_task_id": "plan1234...",
  "impl_task_id": "impl5678...",
  "subtask_count": 5
}
```

### `build.completed`

Fired when a build task completes.

**Event data:**
```json
{
  "spec_path": "ops/now/my-feature.md",
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
  - trigger: spec.completed
    actions:
      - plan:
          spec: "{{ event.spec_path }}"
      
  - trigger: plan.completed
    actions:
      - build:
          spec: "{{ event.spec_path }}"
      
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
  - trigger: plan.completed
    actions:
      - build:
          spec: "{{ event.spec_path }}"
          async: true
      - notify:
          message: "Build started for {{ event.spec_path }}"
  
  - trigger: build.completed
    actions:
      - notify:
          message: "Build finished: {{ event.status }}"
```

### Conditional Build

```yaml
# .aiki/flows/conditional-build.yml
on:
  - trigger: plan.completed
    condition: "{{ event.subtask_count < 10 }}"
    actions:
      - build:
          spec: "{{ event.spec_path }}"
      
  - trigger: plan.completed
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

1. **Spec → Plan → Build chain**
   - Create spec interactively
   - Verify `spec.completed` event fires
   - Verify plan action triggers
   - Verify `plan.completed` event fires
   - Verify build action triggers
   - Verify `build.completed` event fires

2. **Async build**
   - Trigger async build
   - Verify build task ID returned immediately
   - Verify build runs in background
   - Verify `build.completed` event fires when done

3. **Error handling**
   - Trigger plan with invalid spec path
   - Verify `plan.failed` event fires
   - Verify subsequent actions don't trigger

4. **Conditional workflows**
   - Create plan with different subtask counts
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
