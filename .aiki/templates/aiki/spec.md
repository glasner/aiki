---
version: 1.0.0
type: spec
assignee: claude-code
interactive: true
---

# Spec: {{data.spec_path}}

Guide the user through creating or refining a spec document.

# Instructions

You are helping the user write a spec at `{{data.spec_path}}`.

**For new specs:** Work through the subtasks in order (they are defined at the end of this template).

**For existing specs:**
1. Read and summarize current state
2. Identify which phase the spec is in (incomplete sections? unresolved questions?)
3. Mark already-completed subtasks as `wont_do` (e.g., if spec is already drafted, close "Clarify user intent" and "Draft initial spec" as `--wont-do`)
4. Start the appropriate subtask for where work is needed
5. Work through remaining subtasks as needed

**Throughout:**
- Write to the spec file incrementally (don't wait until the end)
- Keep the conversation focused
- Use subtasks to track progress through the spec workflow
- Track open questions as subtasks under "Resolve open questions", NOT in the spec file's "Open Questions" section

## Spec Structure (suggested)

Use this structure for new specs:

    # <Title>

    **Date**: <today>
    **Status**: Draft
    **Purpose**: <one line>

    **Related Documents**:
    - [Related Doc](path/to/doc.md) - Brief description

    ---

    ## Executive Summary

    <2-3 sentences describing what this is and why>

    ---

    ## User Experience

    <If applicable - command syntax and options>

    ---

    ## How It Works

    <Explanation of the mechanism/flow>

    ---

    ## Use Cases

    <Concrete examples of how this is used>

    ---

    ## Implementation Plan

    <Phases and deliverables>

    ---

    ## Error Handling

    <If applicable - error scenarios and handling>

    ---

    ## Open Questions

    1. ...
    2. ...

    ---


# Subtasks

## Clarify user intent and requirements

1. Ask clarifying questions where you have them:
   - What problem does this solve?
   - Who is the user/audience?
   - What are the key requirements?
   - Any constraints or non-goals?
3. Close subtask when you have enough clarity to draft the spec

## Draft initial spec structure

1. Create the spec file using the template structure
2. Fill out Summary, Requirements, and Open Questions sections
3. Write to the file incrementally as you work
4. Close subtask when initial draft is complete

## Resolve open questions

1. Work through each question subtask with the user
2. Update spec based on answers
3. Close each question subtask when resolved
4. Add any new questions that emerge as additional subtasks
5. Close this subtask when all question subtasks are resolved

## Validate completeness

Evaluate the spec for quality and readiness. For each issue found, add it as a subtask under this validation task.

**Evaluation criteria:**

| Category | What to Check |
|----------|---------------|
| **Completeness** | All sections filled out, no TODOs or placeholders |
| **Clarity** | Requirements unambiguous, clear acceptance criteria |
| **Implementability** | Can be decomposed into tasks, technical details sufficient |
| **UX** | User experience considered, command syntax intuitive, errors defined |

**Workflow:**

1. Read the complete spec
2. Evaluate against each category
3. For each issue found, add a subtask:
   ```bash
   aiki task add --parent {{parent.id}} "<Category>: <brief description>"
   ```
4. Once all issues are identified, work through each subtask with the user:
   - Discuss the issue
   - Update the spec to address it
   - Close the subtask when resolved
5. When all issue subtasks are closed, close this validation subtask
6. If no issues found, close immediately with "Spec validated - no issues found"
