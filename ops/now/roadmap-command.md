# Roadmap Command

**Date**: 2026-03-18
**Status**: Draft
**Purpose**: A strategic view over epics, grouped by maturity stage

**Related Documents**:
- [Feedback Command](../future/feedback-command.md) - Deferred: zero-friction intake funnel that feeds the roadmap

---

## Executive Summary

The roadmap is not a separate data model — it's a **lens on epics** built on the existing `--data` key-value system. Epics are tagged with `--data roadmap=now|next|future` and `aiki roadmap` renders a compact grouped view by querying `aiki epic list --by-data roadmap`. Promotion between stages is human-only and acts as a forcing function: advancing an epic requires answering progressively more questions.

No generated `ROADMAP.md` file. The command *is* the index.

---

## User Experience

```bash
# View the roadmap (compact, grouped by stage)
aiki roadmap                          # sugar for: aiki epic list --by-data roadmap

# Add a new epic to the roadmap (lands in `future` by default)
aiki epic add "Plugin Ecosystem" --data roadmap=future

# Move an epic between stages
aiki epic set <epic-id> --data roadmap=next    # future → next (validates required data)
aiki epic set <epic-id> --data roadmap=now     # next → now
aiki epic set <epic-id> --data roadmap=future  # demote back to future

# Filter by stage
aiki roadmap --now                    # sugar for: aiki epic list --by-data roadmap --data-value now
aiki roadmap --next
aiki roadmap --future
```

### The `--by-data` Flag

`--by-data <key>` is a general-purpose grouping flag on `aiki task list` and `aiki epic list`. It groups output by the values of a given data key. Any data key works — `roadmap` is just one use case.

```bash
# Group tasks by any data key
aiki task list --by-data area          # group by area=frontend, area=backend, etc.
aiki epic list --by-data roadmap      # group by roadmap=now, roadmap=next, etc.
aiki task list --by-data team         # group by team=platform, team=growth, etc.
```

### Output Format

```
$ aiki roadmap

now:
  Plugin Ecosystem [L]          Discoverable, installable plugins with a registry
  User Edit Detection [M]       Separate human vs AI edits for accurate attribution

next:
  ACP Integration [L]           Native IDE support via Agent Control Protocol
  Metrics & Learning [M]        Track agent effectiveness, learn from patterns
  Branched Sessions [S]         Parallel agent work on feature branches

future:
  Enterprise Compliance [XL]    Audit trails, policy enforcement, SSO
  Multi-Agent Orchestration [XL] Coordinated agent teams with conflict resolution
  Gemini Support [S]            Extend provenance to Gemini CLI
```

The `[L]` size markers come from `--data effort=L` on each epic.

---

## How It Works

### Data Model

No new fields. Roadmap uses the existing `data: HashMap<String, String>` on tasks/epics:

| Data Key | Values | Required | When |
|----------|--------|----------|------|
| `roadmap` | `now` \| `next` \| `future` | Yes | On creation via `--data roadmap=future` |
| `effort` | `S` \| `M` \| `L` \| `XL` | No | Required for `roadmap=next` |

Existing epic fields that the roadmap uses:
- `name` — the one-liner
- `description` — problem statement (required for `next`)
- `depends_on` — epic dependencies (optional, shown when present)
- `priority` — `p0`–`p3` (used for ordering within a roadmap stage)

### Storage

Same JJ-backed event-sourced system as tasks/epics. No new storage mechanism — `data` is already event-sourced via `TaskEvent::Created` and `TaskEvent::Updated`.

### Promotion Rules

Setting `roadmap` to a higher stage is a forcing function. Each stage transition validates that required data keys are populated:

**`future` → `next`** requires:
- `description` — problem statement (why this matters)
- `effort` data key — T-shirt size (S/M/L/XL)

**`next` → `now`** requires:
- Everything from `next` (already satisfied)
- No additional data — the signal is "we're actively working on this"

If a required field is missing, the command fails with a message telling you what's needed:

```
$ aiki epic set abc123 --data roadmap=next
Error: Cannot set roadmap to 'next' — missing required fields:
  - description: Problem statement (why this matters)
  - effort: Size estimate (S/M/L/XL), set with --data effort=M

Run: aiki epic edit abc123
```

### Demotion

Setting `roadmap` to a lower stage requires no validation — you can always deprioritize.

### Rendering

`aiki roadmap` queries all epics that have a `roadmap` data key, groups by value (`now`, `next`, `future`), orders by priority within each group, and renders the compact format. Epics without a `roadmap` data key don't appear on the roadmap.

---

## Relationship to Other Systems

### Feedback (deferred)
`aiki feedback new` captures raw ideas. Triage creates or links to epics. Feedback is *input* to the roadmap, not a stage within it. See [feedback-command.md](../future/feedback-command.md).

### Tasks
Epics on the roadmap can have tasks underneath them (existing parent/child relationship). Moving an epic to `now` doesn't auto-generate tasks — that happens naturally when someone starts working.

### Plan Files
Some epics may have associated plan docs in `ops/`. This is optional — the roadmap doesn't require or manage plan files. An epic can link to a plan via its `source` field if one exists.

---

## Implementation Plan

### Phase 1: `--by-data` on list commands
- Add `--by-data <key>` flag to `aiki task list`
- Add `--by-data <key>` flag to `aiki epic list`
- Group output by values of the given data key, ordered by priority within each group
- Items without the data key are omitted from grouped output

### Phase 2: Roadmap Command (sugar)
- `aiki roadmap` — sugar for `aiki epic list --by-data roadmap`
- Stage filter flags (`--now`, `--next`, `--future`) filter to a single data value
- Compact rendering with effort size `[L]` markers from `effort` data key

### Phase 3: Promotion Validation
- When setting `--data roadmap=next`, validate that `description` and `effort` data key exist
- Error messages tell the user exactly what's missing
- Demotion (to lower stages) skips validation

---

## Open Questions

1. Should `done` epics appear on the roadmap? (e.g., `aiki roadmap --done` or `aiki roadmap --all`)
2. Ordering within a stage: priority only, or also manual ordering?
3. Should the compact view show dependency arrows between items?
4. Should `--by-data` show a count per group? (e.g., `now (2):`)
5. What ordering should `--by-data` use for group headers? Alphabetical, or a defined order per key?
