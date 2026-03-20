---
draft: false
---

# Aiki Coauthor

**Date**: 2026-03-20
**Status**: Draft
**Purpose**: Add aiki itself as a `Co-Authored-By` trailer on every commit made through the aiki workflow.

**Related Documents**:
- [aiki/git-coauthors plugin](../../cli/src/flows/plugins/aiki_git_coauthors.yaml) — Existing coauthor plugin
- [aiki/default plugin](../../cli/src/flows/plugins/aiki_default.yaml) — Default plugin that includes git-coauthors
- [Author extraction](../../cli/src/provenance/authors.rs) — Blame-based author detection
- [generate_coauthors()](../../cli/src/flows/core/functions.rs) — Native function that produces trailers

---

## Executive Summary

Currently, `aiki/git-coauthors` detects AI agents (Claude, Cursor, Codex, Gemini) from `git blame` on staged changes and appends `Co-Authored-By:` trailers. Aiki itself — the orchestrator — is never credited.

This plan adds aiki as a coauthor on every commit processed through the plugin, producing output like:

```
Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: aiki <noreply@aiki.sh>
```

---

## Decisions

| Question | Decision |
|----------|----------|
| Name and email | `aiki <noreply@aiki.sh>` |
| Conditional vs unconditional | Unconditional — every commit when plugin is active |
| Implementation approach | Option A — modify `generate_coauthors()` in Rust |
| Ordering | aiki trailer comes last, after all agent trailers |

---

## How It Works

The `generate_coauthors` native function in `functions.rs` calls `AuthorsCommand::get_authors()`, which:
1. Gets staged files via `git diff --cached`
2. Blames each file to detect AI agents
3. Maps agent types to name/email pairs
4. Returns formatted `Co-Authored-By:` trailers

**Change**: After generating agent-based coauthors, unconditionally append `Co-Authored-By: aiki <noreply@aiki.sh>` as the final trailer. This happens on every commit when the `aiki/git-coauthors` plugin is active.

---

## Implementation Plan

### Phase 1: Modify `generate_coauthors()` in `functions.rs`

1. In the `generate_coauthors()` function (~line 894 of `cli/src/flows/core/functions.rs`), after the existing coauthor trailers are generated, append `Co-Authored-By: aiki <noreply@aiki.sh>` as the last line.
2. Handle the edge case where no AI agents were detected — the aiki trailer should still be appended (result may be just the aiki trailer alone).
3. Ensure no duplicate aiki trailers if the function is called multiple times.

### Phase 2: Update tests

1. Update existing coauthor tests in `git_hooks_tests.rs` to expect the aiki trailer.
2. Add a test case for commits with no AI agents detected — should still produce the aiki trailer.

---
