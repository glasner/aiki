# Learn from Skill Injection (Plan)

## Goal
Capture a deliberate rollout plan for improving skill-driven instruction quality without losing AGENTS portability.

## Context
- Recent attempt to compact AGENTS and move instructions into one workflow sidecar removed too much detail.
- Need a safer plan for what stays in AGENTS vs what moves into skills.

## Plan

1. Define mandatory AGENTS hard requirements
   - Keep only execution preconditions and irreversible constraints.
   - Preserve exact command required before state-changing work.

2. Inventory current workflow instructions
   - List commands, anti-patterns, review flow, and escalation rules.
   - Tag which instructions are policy-critical vs convenience.

3. Design skill decomposition
   - aiki-task-workflow (core stateful lifecycle + delegation)
   - aiki-review (review + issue severity + fix flow)
   - Optional: aiki-conflict-resolution (JJ conflict handling)

4. Draft migration test
   - Create a short diff-based checklist comparing AGENTS vs skill content coverage.
   - Validate with a pilot request before broad rollout.

5. Rollout and evidence
   - Apply first in a feature branch.
   - Run one non-production task to verify delegation + task-close proof paths.
   - Log any lost detail immediately into a new memory/done note.

## Acceptance
- AGENTS remains short and action-complete.
- No required instruction is lost when compacting.
- New plan is committed in ops/now/learn-from-skill.md.
