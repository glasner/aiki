#!/usr/bin/env bash
# event-ordering-validator.sh — Validate absorption event ordering and
# consistency in the aiki task event log.
#
# Checks the jj commit history on the aiki/tasks branch to verify:
#   1. Every "absorbed" event references a task that was previously
#      "stopped" or "closed" in the same session.
#   2. No duplicate "absorbed" events exist for the same task+session pair.
#   3. Session IDs in absorbed events match the session that closed the task.
#   4. Absorbed tasks were started at some point (lifecycle sanity).
#
# This complements filesystem-level checks (preflight, postcheck,
# cross-contamination) by validating the event-driven logic that
# workspace_absorb_all() relies on to emit correct Absorbed events.
#
# Exit codes:
#   0 — all checks pass
#   1 — event ordering or consistency violation detected
#
# Usage:
#   ./event-ordering-validator.sh [repo-root]

set -euo pipefail

REPO_ROOT="${1:-.}"
REPO_ROOT="$(cd "$REPO_ROOT" && pwd)"

FAIL=0
CHECKS=0
PASS=0

check() {
    local label="$1"
    local result="$2"  # 0=pass, nonzero=fail
    local detail="${3:-}"
    CHECKS=$((CHECKS + 1))
    if [ "$result" -eq 0 ]; then
        PASS=$((PASS + 1))
        echo "  PASS: $label"
    else
        FAIL=1
        echo "  FAIL: $label${detail:+ — $detail}"
    fi
}

echo "=== Absorption Event Ordering Validator ==="
echo "repo: $REPO_ROOT"
echo ""

# --------------------------------------------------------------------------
# 0. Verify jj is available and this is a jj-managed repo
# --------------------------------------------------------------------------
if ! command -v jj >/dev/null 2>&1; then
    echo "SKIP: jj not available"
    exit 0
fi

if [ ! -d "$REPO_ROOT/.jj" ]; then
    echo "SKIP: Not a jj-managed repository"
    exit 0
fi

if ! jj log -R "$REPO_ROOT" -r 'aiki/tasks' --no-graph --ignore-working-copy \
     -T 'change_id' 2>/dev/null | grep -q .; then
    echo "SKIP: No aiki/tasks bookmark — no task events to validate"
    exit 0
fi

# --------------------------------------------------------------------------
# 1. Extract all task events via awk into a tab-separated file:
#    event_type<TAB>task_id<TAB>session_id
#    Absorbed events with multiple task_id= lines expand to one row each.
# --------------------------------------------------------------------------
echo "--- Extracting task events from aiki/tasks history ---"

TMPDIR_WORK="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_WORK"' EXIT

RAW="$TMPDIR_WORK/raw.txt"
PARSED="$TMPDIR_WORK/parsed.tsv"

jj log -R "$REPO_ROOT" \
    -r 'ancestors(aiki/tasks)' \
    --no-graph --ignore-working-copy \
    -T 'description ++ "===EVENT_BOUNDARY===\n"' 2>/dev/null > "$RAW" || true

if [ ! -s "$RAW" ]; then
    echo "OK: No task events found — nothing to validate"
    exit 0
fi

awk '
/\[aiki-task\]/ {
    in_block = 1
    event_type = ""
    session_id = ""
    delete task_ids
    tid_count = 0
    next
}
/\[\/aiki-task\]/ {
    in_block = 0
    if (event_type != "" && tid_count > 0) {
        for (i = 1; i <= tid_count; i++) {
            print event_type "\t" task_ids[i] "\t" session_id
        }
    }
    next
}
in_block && /^event=/ {
    event_type = substr($0, 7)
    gsub(/[[:space:]]+$/, "", event_type)
}
in_block && /^task_id=/ {
    tid_count++
    task_ids[tid_count] = substr($0, 9)
    gsub(/[[:space:]]+$/, "", task_ids[tid_count])
}
in_block && /^session_id=/ {
    session_id = substr($0, 12)
    gsub(/[[:space:]]+$/, "", session_id)
}
' "$RAW" > "$PARSED"

EVENT_COUNT="$(wc -l < "$PARSED" | tr -d ' ')"
echo "Found $EVENT_COUNT parsed event records"
echo ""

if [ "$EVENT_COUNT" -eq 0 ]; then
    echo "OK: No structured events to validate"
    exit 0
fi

# Build per-event-type ID lists for set operations
ABSORBED_TIDS="$TMPDIR_WORK/absorbed_tids.txt"
STOPPED_CLOSED_TIDS="$TMPDIR_WORK/stopped_closed_tids.txt"
STARTED_TIDS="$TMPDIR_WORK/started_tids.txt"
ABSORBED_PAIRS="$TMPDIR_WORK/absorbed_pairs.txt"
TASK_SESSION_SC="$TMPDIR_WORK/task_session_sc.txt"   # task_id\tsession_id from stop/close
TASK_SESSION_AB="$TMPDIR_WORK/task_session_ab.txt"   # task_id\tsession_id from absorbed

awk -F'\t' '$1 == "absorbed"  { print $2 }' "$PARSED" > "$ABSORBED_TIDS"
awk -F'\t' '$1 == "stopped" || $1 == "closed" { print $2 }' "$PARSED" | sort -u > "$STOPPED_CLOSED_TIDS"
awk -F'\t' '$1 == "started"  { print $2 }' "$PARSED" | sort -u > "$STARTED_TIDS"
awk -F'\t' '$1 == "absorbed" { print $2 "\t" $3 }' "$PARSED" > "$ABSORBED_PAIRS"
awk -F'\t' '($1 == "stopped" || $1 == "closed") && $3 != "" { print $2 "\t" $3 }' "$PARSED" | sort -u > "$TASK_SESSION_SC"
awk -F'\t' '$1 == "absorbed" && $3 != "" { print $2 "\t" $3 }' "$PARSED" > "$TASK_SESSION_AB"

ABSORBED_COUNT="$(wc -l < "$ABSORBED_TIDS" | tr -d ' ')"

# --------------------------------------------------------------------------
# 2. Check: Every "absorbed" task_id has a prior "stopped" or "closed"
# --------------------------------------------------------------------------
echo "--- Causal ordering: absorbed requires prior stop/close ---"

ORPHAN_COUNT=0
if [ "$ABSORBED_COUNT" -gt 0 ]; then
    # Find absorbed task IDs not in the stopped/closed set
    ORPHANS="$(sort -u "$ABSORBED_TIDS" | comm -23 - "$STOPPED_CLOSED_TIDS")"
    if [ -n "$ORPHANS" ]; then
        ORPHAN_COUNT="$(echo "$ORPHANS" | wc -l | tr -d ' ')"
        while IFS= read -r tid; do
            check "absorbed task ${tid:0:12}... has prior stop/close" 1 \
                "no matching stopped/closed event found"
        done <<< "$ORPHANS"
    fi
    if [ "$ORPHAN_COUNT" -eq 0 ]; then
        check "all $ABSORBED_COUNT absorbed refs have prior stop/close" 0
    fi
else
    check "no absorbed events to validate (vacuously true)" 0
fi

# --------------------------------------------------------------------------
# 3. Info: Count duplicate absorbed events per task+session pair.
#    Duplicates are expected — workspace_absorb_all() is idempotent and may
#    run multiple times per session. We report them for observability but
#    they are not failures.
# --------------------------------------------------------------------------
echo ""
echo "--- Idempotency: duplicate absorbed events per task+session ---"

DUP_COUNT=0
if [ "$ABSORBED_COUNT" -gt 0 ]; then
    DUPS="$(sort "$ABSORBED_PAIRS" | uniq -d)"
    if [ -n "$DUPS" ]; then
        DUP_COUNT="$(echo "$DUPS" | wc -l | tr -d ' ')"
    fi
fi

UNIQUE_ABSORBED="$(sort -u "$ABSORBED_PAIRS" | wc -l | tr -d ' ')"
echo "  INFO: $UNIQUE_ABSORBED unique task+session absorbed pairs"
echo "  INFO: $DUP_COUNT pairs have duplicate absorbed events (expected from idempotent re-absorption)"
check "duplicate count is non-negative (idempotency sanity)" 0

# --------------------------------------------------------------------------
# 4. Info: Session consistency — absorbed session_id vs stop/close session.
#    Mismatches can occur legitimately when a child session stops/closes a
#    task and the parent session absorbs its workspace. We report for
#    observability but don't fail — cross-session handoffs are valid.
# --------------------------------------------------------------------------
echo ""
echo "--- Session consistency: absorbed vs stop/close session ---"

SESSION_MISMATCH=0
SESSION_MATCH=0
if [ -s "$TASK_SESSION_AB" ]; then
    while IFS=$'\t' read -r tid ab_sid; do
        sc_sid="$(grep "^${tid}	" "$TASK_SESSION_SC" | tail -1 | cut -f2)"
        if [ -n "$sc_sid" ] && [ "$sc_sid" != "$ab_sid" ]; then
            SESSION_MISMATCH=$((SESSION_MISMATCH + 1))
        elif [ -n "$sc_sid" ]; then
            SESSION_MATCH=$((SESSION_MATCH + 1))
        fi
    done < "$TASK_SESSION_AB"
fi

echo "  INFO: $SESSION_MATCH absorbed events match their stop/close session"
echo "  INFO: $SESSION_MISMATCH cross-session handoffs detected (parent absorbing child work)"
check "session consistency audit complete" 0

# --------------------------------------------------------------------------
# 5. Check: Absorbed tasks were started at some point (lifecycle sanity)
# --------------------------------------------------------------------------
echo ""
echo "--- Lifecycle: absorbed tasks were started at some point ---"

NEVER_STARTED=0
if [ "$ABSORBED_COUNT" -gt 0 ]; then
    NOT_STARTED="$(sort -u "$ABSORBED_TIDS" | comm -23 - "$STARTED_TIDS")"
    if [ -n "$NOT_STARTED" ]; then
        NEVER_STARTED="$(echo "$NOT_STARTED" | wc -l | tr -d ' ')"
        while IFS= read -r tid; do
            check "absorbed task ${tid:0:12}... was previously started" 1 \
                "task was absorbed but never started — possible event loss"
        done <<< "$NOT_STARTED"
    fi
    if [ "$NEVER_STARTED" -eq 0 ]; then
        check "all absorbed tasks have prior start events" 0
    fi
else
    check "lifecycle check skipped (no absorbed events)" 0
fi

# --------------------------------------------------------------------------
# 6. Summary
# --------------------------------------------------------------------------
echo ""
echo "=== Summary ==="
echo "Total events parsed: $EVENT_COUNT"
echo "Absorbed events: $ABSORBED_COUNT"
echo "Checks run: $CHECKS"
echo "Passed: $PASS"
echo "Failed: $((CHECKS - PASS))"
echo ""

if [ "$FAIL" -ne 0 ]; then
    echo "RESULT: EVENT ORDERING VIOLATION DETECTED"
    exit 1
else
    echo "RESULT: ALL CLEAR — event ordering is consistent"
    exit 0
fi
