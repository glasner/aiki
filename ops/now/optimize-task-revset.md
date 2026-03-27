# Optimize Task Revset Queries

**Status:** In progress (Phase 1 ready)

## Problem

`build_task_revset_pattern` has two performance issues that compound as subtask depth grows. See individual phase plans for details and benchmarks.

## Phases

### Phase 1: Remove dot-notation clauses — [revset-phase1-remove-dot-notation.md](revset-phase1-remove-dot-notation.md)

Quick fix. Remove dead `task={id}.` substring clauses from `build_task_revset_pattern`. Saves ~10-20% per query. Single file change (`cli/src/commands/task.rs`).

### Phase 2: Add ancestor task IDs to provenance — [revset-phase2-ancestor-provenance.md](revset-phase2-ancestor-provenance.md)

Structural fix. Embed ancestor task IDs in provenance metadata at write time so epic queries need only one clause instead of N. Touches provenance write path and revset query builder.
