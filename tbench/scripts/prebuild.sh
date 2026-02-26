#!/bin/bash
# Pre-build the aiki binary for use in Terminal-Bench containers.
#
# Run this ONCE before starting benchmark runs. The binary is placed at
# /tmp/aiki-prebuilt/aiki, which the setup script looks for.
#
# Usage:
#   ./scripts/prebuild.sh
#
# For cross-compilation to match container architecture:
#   CARGO_TARGET=x86_64-unknown-linux-gnu ./scripts/prebuild.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="/tmp/aiki-prebuilt"
TARGET="${CARGO_TARGET:-}"

echo "Building aiki from $REPO_ROOT/cli ..."

mkdir -p "$OUTPUT_DIR"

cd "$REPO_ROOT/cli"

if [ -n "$TARGET" ]; then
    echo "Cross-compiling for target: $TARGET"
    cargo build --release --target "$TARGET"
    cp "target/$TARGET/release/aiki" "$OUTPUT_DIR/aiki"
else
    cargo build --release
    cp target/release/aiki "$OUTPUT_DIR/aiki"
fi

chmod +x "$OUTPUT_DIR/aiki"

echo ""
echo "Binary built: $OUTPUT_DIR/aiki"
echo "Size: $(du -h "$OUTPUT_DIR/aiki" | cut -f1)"
echo ""
echo "The aiki-setup.sh.j2 script will pick this up automatically."
echo "Set AIKI_BINARY_PATH=$OUTPUT_DIR/aiki if using a custom location."
