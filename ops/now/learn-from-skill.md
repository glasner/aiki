# Learn from Skill Injection (Plan)

**Date**: 2026-03-09
**Status**: Draft
**Purpose**: Define and lock a reusable plan format before reworking AGENTS/skill instructions.

**Related Documents**:
- `ops/done/fix-not-running.md`
- `ops/done/command-startup.md`
- `ops/done/build-screen-cleanup.md`
- `ops/done/task-template-formalization.md`
- `ops/done/short-flags-for-workflow.md`

---

## Executive Summary
Recent AGENTS compacting removed some needed instruction detail. To prevent repeating that regression, we will adopt a standard plan structure for `ops/now` before any future AGENTS/skill edits.

## Problem
Plan docs in `ops/now` have not been using one stable heading template, which increases risk of missing required execution rules or review flow details during AGENTS/skill work.

## Goal
Create a durable, repeatable plan format that captures required constraints (`aiki task` preconditions, delegation, evidence) while remaining easy to update.

## Scope
- In scope:
  - Update `ops/now/learn-from-skill.md` to match the standard.
  - Record the standard in durable memory for future adherence.
  - No AGENTS/CLI code changes in this pass.

- Out of scope:
  - Re-implementing the full AGENTS/sidecar redesign.
  - Moving runtime logic.

## Proposed behavior
All future AGENTS/skill plans should use this required structure:

**Header block**
- Title
- `**Date**`, `**Status**`, `**Purpose**`
- Optional `**Related Documents**`

**Plan body sections**
- `## Executive Summary` (for non-trivial changes)
- `## Problem`
- `## Goal`
- `## Scope`
- `## Proposed behavior`
- `## Implementation plan`
- `## Acceptance criteria`
- `## Risks / caveats`
- Optional `## Immediate next steps`

## Implementation plan

### Phase 1 — Apply template now
1. Keep this file as the canonical now-plan template example.
2. Use explicit headers above.
3. Require numbered phases under `Implementation plan`.

### Phase 2 — Enforce by usage
1. For next AGENTS/skill update, start from this template.
2. Validate each section exists before editing state.
3. Include evidence/validation requirements in every closeout summary.

## Acceptance criteria
1. This plan includes metadata + all required sections.
2. Future AGENTS/skill plans include `Problem`, `Goal`, `Scope`, `Proposed behavior`, `Implementation plan`, and `Acceptance criteria`.
3. Memory records the source files used to define the template.

## Risks / caveats
- Overly rigid format can slow one-off notes, so keep this as a default for planning/implementation docs.
- Legacy existing plans may remain inconsistent unless backfilled intentionally.
