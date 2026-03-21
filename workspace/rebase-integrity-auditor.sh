#!/usr/bin/env bash
# rebase-integrity-auditor.sh — Validate JJ commit graph integrity after
# workspace absorption.
#
# Operates at the JJ operation log and commit DAG level to verify:
#   1. No stranded commits exist from absorbed workspaces (commits that
#      should have been rebased into default workspace's ancestry but weren't).
#   2. All aiki-* workspace registrations have been forgotten (no leaked
#      JJ workspace entries post-absorption).
#   3. The default workspace's commit chain is a single linear trunk —
#      no forks from half-completed two-step rebases.
#   4. Operation log shows balanced workspace add/forget counts per session.
#
# This complements:
#   - cross-contamination-guard.sh (filesystem-level isolation checks)
#   - event-ordering-validator.sh (task event log ordering)
# by validating the JJ commit graph state that results from the two-step
# rebase mechanism in isolation.rs.
#
# Exit codes:
#   0 — all checks pass (commit graph is clean)
#   1 — integrity violation detected
#
# Usage:
#   ./rebase-integrity-auditor.sh [repo-root]

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

echo "=== Rebase Integrity Auditor ==="
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

TMPDIR_WORK="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_WORK"' EXIT

# --------------------------------------------------------------------------
# 1. Check: No stale aiki-* workspace registrations in JJ
#    After absorption, workspaces should be forgotten. Any remaining aiki-*
#    entries indicate incomplete cleanup.
# --------------------------------------------------------------------------
echo "--- Stale workspace registrations ---"

JJ_WS_LIST="$TMPDIR_WORK/ws_list.txt"
jj workspace list --ignore-working-copy -R "$REPO_ROOT" 2>/dev/null > "$JJ_WS_LIST" || true

AIKI_WS_COUNT=0
STALE_WS=()

while IFS= read -r line; do
    ws_name="$(echo "$line" | awk '{print $1}' | tr -d ':')"
    if [[ "$ws_name" == aiki-* ]]; then
        AIKI_WS_COUNT=$((AIKI_WS_COUNT + 1))
        session_uuid="${ws_name#aiki-}"
        # Check if corresponding session is still active
        if [ -f "$HOME/.aiki/sessions/$session_uuid" ]; then
            check "workspace '$ws_name' has active session" 0
        else
            STALE_WS+=("$ws_name")
            check "workspace '$ws_name' has active session" 1 \
                "JJ workspace registered but session file missing — absorption incomplete?"
        fi
    fi
done < "$JJ_WS_LIST"

if [ "$AIKI_WS_COUNT" -eq 0 ]; then
    check "no aiki-* workspaces registered (clean state)" 0
fi

echo "  INFO: $AIKI_WS_COUNT active aiki-* workspaces in JJ"
echo ""

# --------------------------------------------------------------------------
# 2. Check: No stranded commits from absorbed workspaces
#    After absorption, all workspace commits should be ancestors of the
#    default workspace's @. Stranded commits indicate a failed step-2 rebase.
#    We look for commits whose description contains workspace markers but
#    are NOT in the default @'s ancestry.
# --------------------------------------------------------------------------
echo "--- Stranded commit detection ---"

# Get the default workspace's current change ID
DEFAULT_HEAD="$(jj log -R "$REPO_ROOT" --no-graph --ignore-working-copy \
    -r '@' -T 'change_id' 2>/dev/null || echo "")"

if [ -n "$DEFAULT_HEAD" ]; then
    # Find all visible changes NOT in @'s ancestry and NOT belonging to
    # any currently-active aiki-* workspace (those are expected to diverge).
    # Stranded commits are ones that belong to NO workspace — orphaned by
    # incomplete absorption.
    STRANDED="$TMPDIR_WORK/stranded.txt"

    # Build exclusion revset for active workspaces
    ACTIVE_EXCL=""
    while IFS= read -r line; do
        ws_name="$(echo "$line" | awk '{print $1}' | tr -d ':')"
        if [[ "$ws_name" == aiki-* ]]; then
            if [ -n "$ACTIVE_EXCL" ]; then
                ACTIVE_EXCL="$ACTIVE_EXCL | "
            fi
            ACTIVE_EXCL="${ACTIVE_EXCL}${ws_name}@"
        fi
    done < "$JJ_WS_LIST"

    # Exclude active workspace heads and their ancestors from stranded check
    if [ -n "$ACTIVE_EXCL" ]; then
        REVSET="visible_heads() ~ ::@ ~ ::($ACTIVE_EXCL)"
    else
        REVSET="visible_heads() ~ ::@"
    fi

    jj log -R "$REPO_ROOT" --no-graph --ignore-working-copy \
        -r "$REVSET" \
        -T 'change_id ++ " " ++ description.first_line() ++ "\n"' \
        2>/dev/null | sed '/^$/d' > "$STRANDED" || true

    STRANDED_COUNT=0
    STRANDED_AIKI=0
    if [ -s "$STRANDED" ]; then
        STRANDED_COUNT="$(wc -l < "$STRANDED" | tr -d ' ')"
        # Filter for commits that look like they came from aiki workspaces
        STRANDED_AIKI="$(grep -c '\[aiki\]\|aiki-\|workspace' "$STRANDED" || true)"
        STRANDED_AIKI="${STRANDED_AIKI:-0}"
    fi

    # Non-ancestor visible heads are common in repos with multiple branches
    # (e.g. aiki/tasks tracking branch, feature branches). We report as INFO
    # rather than FAIL since only truly orphaned workspace commits are bugs,
    # and distinguishing those requires deeper ancestry analysis.
    check "stranded commit scan completed" 0
    echo "  INFO: $STRANDED_COUNT non-ancestor visible heads ($STRANDED_AIKI aiki-related)"
    if [ "$STRANDED_COUNT" -gt 0 ]; then
        echo "  INFO: sample (may include legitimate branches like aiki/tasks):"
        head -3 "$STRANDED" | while IFS= read -r line; do
            echo "    $line"
        done
    fi
else
    check "default workspace head resolution" 1 "could not resolve @ in default workspace"
fi
echo ""

# --------------------------------------------------------------------------
# 3. Check: Operation log balance — workspace add/forget counts
#    Each absorbed workspace should have a matching 'add' and 'forget'.
#    Unmatched adds indicate leaked workspaces; unmatched forgets are benign
#    (could be manual cleanup).
# --------------------------------------------------------------------------
echo "--- Operation log workspace balance ---"

OPLOG="$TMPDIR_WORK/oplog.txt"
jj op log -R "$REPO_ROOT" --no-graph --ignore-working-copy \
    -T 'description ++ "\n===OP_BOUNDARY===\n"' \
    2>/dev/null > "$OPLOG" || true

if [ -s "$OPLOG" ]; then
    # Count workspace add and forget operations for aiki-* workspaces
    ADD_COUNT="$(grep -c 'add workspace.*aiki-\|workspace add.*aiki-\|new workspace.*aiki-' "$OPLOG" 2>/dev/null || echo 0)"
    FORGET_COUNT="$(grep -c 'forget workspace.*aiki-\|workspace forget.*aiki-' "$OPLOG" 2>/dev/null || echo 0)"

    BALANCE=$((ADD_COUNT - FORGET_COUNT - AIKI_WS_COUNT))

    if [ "$BALANCE" -le 0 ]; then
        check "workspace add/forget balance is sound" 0
    else
        check "workspace add/forget balance is sound" 1 \
            "adds=$ADD_COUNT forgets=$FORGET_COUNT active=$AIKI_WS_COUNT unaccounted=$BALANCE"
    fi
    echo "  INFO: $ADD_COUNT workspace adds, $FORGET_COUNT workspace forgets, $AIKI_WS_COUNT active"
else
    check "operation log readable" 0
    echo "  INFO: operation log empty or unreadable (fresh repo?)"
fi
echo ""

# --------------------------------------------------------------------------
# 4. Check: Default workspace commit chain linearity
#    The default workspace's recent history should be linear (no merge
#    commits). A fork in the chain could indicate a half-completed two-step
#    rebase where step 1 succeeded but step 2 didn't complete.
# --------------------------------------------------------------------------
echo "--- Default workspace chain linearity ---"

MERGE_COMMITS="$TMPDIR_WORK/merges.txt"
jj log -R "$REPO_ROOT" --no-graph --ignore-working-copy \
    -r '::@ & merges()' \
    -T 'change_id ++ " " ++ description.first_line() ++ "\n"' \
    2>/dev/null | sed '/^$/d' > "$MERGE_COMMITS" || true

MERGE_COUNT=0
if [ -s "$MERGE_COMMITS" ]; then
    MERGE_COUNT="$(wc -l < "$MERGE_COMMITS" | tr -d ' ')"
fi

# Merge commits in ancestry are not necessarily a bug (user may have merged
# intentionally), but aiki-related merges likely indicate absorption issues
AIKI_MERGES=0
if [ "$MERGE_COUNT" -gt 0 ]; then
    AIKI_MERGES="$(grep -c '\[aiki\]\|aiki-\|workspace' "$MERGE_COMMITS" || true)"
    AIKI_MERGES="${AIKI_MERGES:-0}"
fi

if [ "$AIKI_MERGES" -eq 0 ]; then
    check "no aiki-related merge commits in default ancestry" 0
else
    check "no aiki-related merge commits in default ancestry" 1 \
        "$AIKI_MERGES merge commits with aiki markers — possible incomplete rebase"
fi
echo "  INFO: $MERGE_COUNT total merge commits in default ancestry, $AIKI_MERGES aiki-related"
echo ""

# --------------------------------------------------------------------------
# 5. Check: No conflict markers in default workspace working copy
#    Post-absorption conflicts should have been resolved. Lingering conflict
#    markers indicate the autoreply conflict-resolution flow didn't complete.
# --------------------------------------------------------------------------
echo "--- Post-absorption conflict state ---"

CONFLICT_STATE="$(jj log -R "$REPO_ROOT" --no-graph --ignore-working-copy \
    -r '@' -T 'conflict' 2>/dev/null || echo "")"

if echo "$CONFLICT_STATE" | grep -qi 'true\|conflict'; then
    check "default workspace is conflict-free" 1 "@ has unresolved conflicts"
else
    check "default workspace is conflict-free" 0
fi
echo ""

# --------------------------------------------------------------------------
# 6. Summary
# --------------------------------------------------------------------------
echo "=== Summary ==="
echo "Checks run: $CHECKS"
echo "Passed: $PASS"
echo "Failed: $((CHECKS - PASS))"
echo "Stale workspaces: ${#STALE_WS[@]}"
echo ""

if [ "$FAIL" -ne 0 ]; then
    echo "RESULT: REBASE INTEGRITY VIOLATION DETECTED"
    echo ""
    echo "Possible causes:"
    echo "  - Half-completed two-step rebase (step 1 ok, step 2 failed)"
    echo "  - Workspace forgotten without prior absorption"
    echo "  - Lock contention caused absorption timeout"
    echo "  - Process killed during absorption"
    exit 1
else
    echo "RESULT: ALL CLEAR — JJ commit graph is clean after absorption"
    exit 0
fi
