#!/bin/bash
# Run Claude Code + Aiki on Terminal-Bench
#
# Usage:
#   ./scripts/run.sh                          # Full run with defaults
#   ./scripts/run.sh --task-id hello-world    # Single task
#   ./scripts/run.sh --model anthropic/claude-sonnet-4-20250514  # Specific model
#
# Environment:
#   OAuth auth required: ~/.claude/.credentials.json must exist
#   TBENCH_CONCURRENT  - Number of concurrent tasks (default: 4)
#   TBENCH_DATASET     - Dataset version (default: terminal-bench-core==0.1.1)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONCURRENT="${TBENCH_CONCURRENT:-4}"
DATASET="${TBENCH_DATASET:-terminal-bench-core==0.1.1}"
USER_ARGS=()
HOST_AUTH_ARG_SET=false

cd "$SCRIPT_DIR"

if [ ! -f "${HOME}/.claude/.credentials.json" ]; then
    echo "Error: ${HOME}/.claude/.credentials.json not found." >&2
    echo "Run 'claude' once on host to authenticate, then retry." >&2
    exit 1
fi

while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--agent-kwarg" ]]; then
        if [[ $# -lt 2 ]]; then
            echo "Error: --agent-kwarg requires a key=value argument." >&2
            exit 1
        fi

        if [[ "$2" == "use_host_auth=true" ]]; then
            HOST_AUTH_ARG_SET=true
        elif [[ "$2" == use_host_auth=* ]]; then
            echo "Error: only use_host_auth=true is supported." >&2
            exit 1
        else
            USER_ARGS+=("$1" "$2")
        fi
        shift 2
        continue
    fi

    USER_ARGS+=("$1")
    shift
done

if [[ "$HOST_AUTH_ARG_SET" == "false" ]]; then
    USER_ARGS=(--agent-kwarg use_host_auth=true "${USER_ARGS[@]}")
fi

exec tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --dataset "$DATASET" \
    --n-concurrent "$CONCURRENT" \
    "${USER_ARGS[@]}"
