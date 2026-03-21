#!/usr/bin/env bash
# cross-contamination-guard.sh — Detect workspace cross-contamination during
# concurrent async task absorption.
#
# Validates that isolated workspaces under /tmp/aiki/<repo-id>/ maintain strict
# session boundaries: each workspace directory maps to exactly one session, no
# files leak between workspaces, and absorbed workspaces leave no residual state.
#
# Exit codes:
#   0 — all checks pass (no cross-contamination detected)
#   1 — cross-contamination or invariant violation detected
#
# Usage:
#   ./cross-contamination-guard.sh [repo-root]
#   AIKI_WORKSPACES_DIR=/custom/path ./cross-contamination-guard.sh [repo-root]

set -euo pipefail

REPO_ROOT="${1:-.}"
REPO_ROOT="$(cd "$REPO_ROOT" && pwd)"
WORKSPACES_BASE="${AIKI_WORKSPACES_DIR:-/tmp/aiki}"

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

echo "=== Cross-Contamination Guard ==="
echo "repo: $REPO_ROOT"
echo "workspaces base: $WORKSPACES_BASE"
echo ""

# --------------------------------------------------------------------------
# 1. Resolve repo-id to find workspace directory
# --------------------------------------------------------------------------
REPO_ID=""
if [ -f "$REPO_ROOT/.jj/aiki/repo-id" ]; then
    REPO_ID="$(cat "$REPO_ROOT/.jj/aiki/repo-id")"
elif [ -f "$REPO_ROOT/.aiki/repo-id" ]; then
    REPO_ID="$(cat "$REPO_ROOT/.aiki/repo-id")"
fi

if [ -z "$REPO_ID" ]; then
    echo "SKIP: No repo-id found — not an aiki-managed repo (or workspaces never created)"
    exit 0
fi

REPO_WS_DIR="$WORKSPACES_BASE/$REPO_ID"
echo "repo-id: $REPO_ID"
echo "workspace dir: $REPO_WS_DIR"
echo ""

if [ ! -d "$REPO_WS_DIR" ]; then
    echo "OK: No workspace directory exists — nothing to check"
    exit 0
fi

# --------------------------------------------------------------------------
# 2. Check: No stale absorb lock
# --------------------------------------------------------------------------
LOCK_FILE="$REPO_WS_DIR/.absorb.lock"
if [ -f "$LOCK_FILE" ]; then
    LOCK_PID="$(cat "$LOCK_FILE" 2>/dev/null | tr -d '[:space:]')"
    if [ -n "$LOCK_PID" ] && kill -0 "$LOCK_PID" 2>/dev/null; then
        check "absorb lock is held by live process (pid=$LOCK_PID)" 0
    else
        check "no stale absorb lock" 1 "lock file exists with dead/missing pid=$LOCK_PID"
    fi
else
    check "no stale absorb lock" 0
fi

# --------------------------------------------------------------------------
# 3. Check: Each workspace directory contains a valid .jj pointer
# --------------------------------------------------------------------------
echo ""
echo "--- Workspace integrity ---"
WS_COUNT=0
ORPHAN_DIRS=()

for ws_dir in "$REPO_WS_DIR"/*/; do
    [ -d "$ws_dir" ] || continue
    session_id="$(basename "$ws_dir")"

    # Skip dotfiles/lock artifacts
    [[ "$session_id" == .* ]] && continue

    WS_COUNT=$((WS_COUNT + 1))

    if [ -f "$ws_dir/.jj/working_copy/checkout" ] || [ -d "$ws_dir/.jj" ]; then
        check "workspace $session_id has .jj metadata" 0
    else
        ORPHAN_DIRS+=("$session_id")
        check "workspace $session_id has .jj metadata" 1 "directory exists but no .jj — orphaned?"
    fi
done

echo ""
echo "--- Session boundary isolation ---"

# --------------------------------------------------------------------------
# 4. Check: No session directory contains files belonging to another session
#    A workspace should only have jj commits traceable to its own session.
#    We check that jj workspace list from repo root confirms each workspace
#    name matches the expected "aiki-<session-id>" pattern.
# --------------------------------------------------------------------------
if command -v jj >/dev/null 2>&1; then
    JJ_WS_LIST="$(jj workspace list --ignore-working-copy -R "$REPO_ROOT" 2>/dev/null || true)"

    # Parse workspace names that follow the aiki-<uuid> pattern
    # jj workspace list outputs "name: change_id ..." — strip trailing colon
    ACTIVE_WS_NAMES=()
    while IFS= read -r line; do
        ws_name="$(echo "$line" | awk '{print $1}' | tr -d ':')"
        if [[ "$ws_name" == aiki-* ]]; then
            ACTIVE_WS_NAMES+=("$ws_name")
        fi
    done <<< "$JJ_WS_LIST"

    # For each active jj workspace, verify its directory exists
    for ws_name in "${ACTIVE_WS_NAMES[@]}"; do
        session_uuid="${ws_name#aiki-}"
        ws_path="$REPO_WS_DIR/$session_uuid"
        if [ -d "$ws_path" ]; then
            check "jj workspace '$ws_name' has matching directory" 0
        else
            check "jj workspace '$ws_name' has matching directory" 1 \
                "jj knows workspace but directory missing at $ws_path"
        fi
    done

    # Reverse check: each directory should have a matching jj workspace
    for ws_dir in "$REPO_WS_DIR"/*/; do
        [ -d "$ws_dir" ] || continue
        session_id="$(basename "$ws_dir")"
        [[ "$session_id" == .* ]] && continue
        expected_name="aiki-$session_id"
        if echo "$JJ_WS_LIST" | grep -q "^${expected_name}[: ]"; then
            check "directory $session_id has matching jj workspace" 0
        else
            check "directory $session_id has matching jj workspace" 1 \
                "directory exists but jj doesn't know about workspace '$expected_name' — leaked?"
        fi
    done
else
    echo "  SKIP: jj not available — cannot verify workspace/directory consistency"
fi

# --------------------------------------------------------------------------
# 5. Check: No workspace directory has uncommitted changes from a different
#    session's PID file (cross-contamination indicator)
# --------------------------------------------------------------------------
echo ""
echo "--- PID file isolation ---"
for ws_dir in "$REPO_WS_DIR"/*/; do
    [ -d "$ws_dir" ] || continue
    session_id="$(basename "$ws_dir")"
    [[ "$session_id" == .* ]] && continue

    pid_file="$ws_dir/.aiki-session-pid"
    if [ -f "$pid_file" ]; then
        pid_session="$(cat "$pid_file" 2>/dev/null | head -1 | tr -d '[:space:]')"
        if [ "$pid_session" = "$session_id" ] || [ -z "$pid_session" ]; then
            check "workspace $session_id PID file matches session" 0
        else
            check "workspace $session_id PID file matches session" 1 \
                "PID file contains session '$pid_session' — cross-contamination!"
        fi
    fi
done

# --------------------------------------------------------------------------
# 6. Summary
# --------------------------------------------------------------------------
echo ""
echo "=== Summary ==="
echo "Workspaces found: $WS_COUNT"
echo "Checks run: $CHECKS"
echo "Passed: $PASS"
echo "Failed: $((CHECKS - PASS))"
echo ""

if [ "$FAIL" -ne 0 ]; then
    echo "RESULT: CROSS-CONTAMINATION DETECTED"
    exit 1
else
    echo "RESULT: ALL CLEAR — no cross-contamination detected"
    exit 0
fi
