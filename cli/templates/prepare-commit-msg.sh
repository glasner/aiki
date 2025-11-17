#!/usr/bin/env bash
# Aiki Git Hook: prepare-commit-msg
#
# This hook executes the PrepareCommitMessage flow section to modify the commit message.
# Typically used for adding Co-authored-by: lines, but can add any content.

COMMIT_MSG_FILE=$1
COMMIT_SOURCE=$2
SHA1=$3

# Only run for normal commits (not merge, squash, etc.)
# COMMIT_SOURCE is empty for initial commit, "message" for -m/-F, "template" for -t
# We want to skip "merge", "squash", "commit" (amend), but run for empty/"message"/"template"
if [ "$COMMIT_SOURCE" = "merge" ] || [ "$COMMIT_SOURCE" = "squash" ] || [ "$COMMIT_SOURCE" = "commit" ]; then
    exit 0
fi

# Chain to previous hook if it exists
# __PREVIOUS_HOOK_PATH__ will be replaced during installation
PREVIOUS_HOOK="__PREVIOUS_HOOK_PATH__"
if [ "$PREVIOUS_HOOK" != "NOT_SET" ] && [ "$PREVIOUS_HOOK" != "EMPTY" ] && [ -n "$PREVIOUS_HOOK" ]; then
    # Resolve to absolute path if relative
    if [[ "$PREVIOUS_HOOK" != /* ]]; then
        PREVIOUS_HOOK="$(git rev-parse --show-toplevel)/$PREVIOUS_HOOK"
    fi

    # Check if previous hook exists and is executable
    PREVIOUS_HOOK_FILE="$PREVIOUS_HOOK/prepare-commit-msg"
    if [ -x "$PREVIOUS_HOOK_FILE" ]; then
        # Call previous hook with same arguments
        "$PREVIOUS_HOOK_FILE" "$@"
        HOOK_EXIT=$?
        if [ $HOOK_EXIT -ne 0 ]; then
            exit $HOOK_EXIT
        fi
    fi
fi

# Dispatch PrepareCommitMessage event through the event bus
# The flow has full responsibility for modifying the commit message file
# via $event.commit_msg_file
export AIKI_COMMIT_MSG_FILE="$COMMIT_MSG_FILE"
aiki event prepare-commit-msg 2>/dev/null

# Always exit 0 - we never want to block commits
exit 0
