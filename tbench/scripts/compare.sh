#!/bin/bash
# Run both baseline (vanilla Claude Code) and treatment (Claude Code + Aiki)
# on Terminal-Bench and print results for comparison.
#
# Usage:
#   ./scripts/compare.sh
#   ./scripts/compare.sh --task-id hello-world    # Single task comparison
#   ./scripts/compare.sh --model anthropic/claude-sonnet-4-20250514
#
# Environment:
#   OAuth auth required: ~/.claude/.credentials.json must exist
#   TBENCH_CONCURRENT  - Number of concurrent tasks (default: 4)
#   TBENCH_DATASET     - Dataset version (default: terminal-bench-core==0.1.1)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONCURRENT="${TBENCH_CONCURRENT:-4}"
DATASET="${TBENCH_DATASET:-terminal-bench-core==0.1.1}"

if [ ! -f "${HOME}/.claude/.credentials.json" ]; then
    echo "Error: ${HOME}/.claude/.credentials.json not found." >&2
    echo "Run 'claude' once on host to authenticate, then retry." >&2
    exit 1
fi

BASE_ARGS=()
TREATMENT_ARGS=()
HOST_AUTH_KWARG_SET=false

while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--agent-kwarg" ]]; then
        if [[ $# -lt 2 ]]; then
            echo "Error: --agent-kwarg requires a key=value argument." >&2
            exit 1
        fi

        if [[ "$2" == "use_host_auth=true" ]]; then
            TREATMENT_ARGS+=("$1" "$2")
            HOST_AUTH_KWARG_SET=true
        elif [[ "$2" == use_host_auth=* ]]; then
            echo "Error: only use_host_auth=true is supported." >&2
            exit 1
        else
            TREATMENT_ARGS+=("$1" "$2")
        fi
        shift 2
        continue
    fi

    BASE_ARGS+=("$1")
    TREATMENT_ARGS+=("$1")
    shift
done

if [[ "$HOST_AUTH_KWARG_SET" == "false" ]]; then
    TREATMENT_ARGS+=("--agent-kwarg" "use_host_auth=true")
fi

echo "============================================"
echo "Terminal-Bench: Baseline vs Aiki Comparison"
echo "============================================"
echo ""
echo "Dataset: $DATASET"
echo "Concurrency: $CONCURRENT"
echo ""

# --- Baseline run ---
echo ">>> Running BASELINE (vanilla Claude Code)..."
echo ""
tb run \
    --agent claude-code \
    --dataset "$DATASET" \
    --n-concurrent "$CONCURRENT" \
    "${BASE_ARGS[@]}" 2>&1 | tee /tmp/tbench-baseline.log

echo ""
echo ">>> Baseline complete."
echo ""

# --- Treatment run ---
echo ">>> Running TREATMENT (Claude Code + Aiki)..."
echo ""
cd "$SCRIPT_DIR"
tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --dataset "$DATASET" \
    --n-concurrent "$CONCURRENT" \
    "${TREATMENT_ARGS[@]}" 2>&1 | tee /tmp/tbench-aiki.log

echo ""
echo ">>> Treatment complete."
echo ""
echo "============================================"
echo "Results saved to:"
echo "  Baseline:  /tmp/tbench-baseline.log"
echo "  Treatment: /tmp/tbench-aiki.log"
echo "============================================"
