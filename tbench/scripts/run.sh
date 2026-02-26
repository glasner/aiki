#!/bin/bash
# Run Claude Code + Aiki on Terminal-Bench
#
# Usage:
#   ./scripts/run.sh                          # Full run with defaults
#   ./scripts/run.sh --task-id hello-world    # Single task
#   ./scripts/run.sh --model anthropic/claude-sonnet-4-20250514  # Specific model
#
# Environment:
#   ANTHROPIC_API_KEY  - Required
#   TBENCH_CONCURRENT  - Number of concurrent tasks (default: 4)
#   TBENCH_DATASET     - Dataset version (default: terminal-bench-core==2.0)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONCURRENT="${TBENCH_CONCURRENT:-4}"
DATASET="${TBENCH_DATASET:-terminal-bench-core==2.0}"

if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "Error: ANTHROPIC_API_KEY must be set" >&2
    exit 1
fi

cd "$SCRIPT_DIR"

exec tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --dataset "$DATASET" \
    --n-concurrent "$CONCURRENT" \
    "$@"
