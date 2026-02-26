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
    "$@" 2>&1 | tee /tmp/tbench-baseline.log

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
    "$@" 2>&1 | tee /tmp/tbench-aiki.log

echo ""
echo ">>> Treatment complete."
echo ""
echo "============================================"
echo "Results saved to:"
echo "  Baseline:  /tmp/tbench-baseline.log"
echo "  Treatment: /tmp/tbench-aiki.log"
echo "============================================"
