#!/usr/bin/env bash
# Benchmark script for aiki test suite
# Run on different machines and compare the output.

set -euo pipefail

MANIFEST="/Users/glasner/code/aiki/cli/Cargo.toml"
RUNS="${1:-3}"

echo "=== Aiki Test Suite Benchmark ==="
echo "Machine: $(hostname)"
echo "CPU:     $(sysctl -n machdep.cpu.brand_string 2>/dev/null || lscpu 2>/dev/null | grep 'Model name' | sed 's/.*: //')"
echo "Cores:   $(nproc 2>/dev/null || sysctl -n hw.ncpu)"
echo "RAM:     $(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%.0f GB", $1/1073741824}' || free -h 2>/dev/null | awk '/Mem:/{print $2}')"
echo "OS:      $(uname -srm)"
echo "Rust:    $(rustc --version)"
echo "Date:    $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo ""

# Clean build first so we're timing from the same starting point
echo "--- Clean build (not timed) ---"
cargo build --manifest-path "$MANIFEST" 2>&1 | tail -1
echo ""

echo "--- Running test suite $RUNS time(s) ---"
times=()
for i in $(seq 1 "$RUNS"); do
    echo -n "Run $i/$RUNS ... "
    start=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
    cargo test --manifest-path "$MANIFEST" --lib 2>&1 | tail -1
    end=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
    elapsed_ms=$(( (end - start) / 1000000 ))
    elapsed_s=$(awk "BEGIN {printf \"%.2f\", $elapsed_ms / 1000}")
    times+=("$elapsed_ms")
    echo "  -> ${elapsed_s}s"
done

echo ""
echo "=== Results ==="

# Compute min, max, avg
min=${times[0]}
max=${times[0]}
sum=0
for t in "${times[@]}"; do
    sum=$((sum + t))
    (( t < min )) && min=$t
    (( t > max )) && max=$t
done
avg=$((sum / RUNS))

printf "  Min: %.2fs\n" "$(awk "BEGIN {print $min / 1000}")"
printf "  Max: %.2fs\n" "$(awk "BEGIN {print $max / 1000}")"
printf "  Avg: %.2fs\n" "$(awk "BEGIN {print $avg / 1000}")"
echo ""
echo "Copy this block to compare across machines."
