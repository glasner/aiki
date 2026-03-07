#!/usr/bin/env bash
# CI guardrail: detect unannnotated direct stderr writes in the monitoring hot path.
#
# Any eprintln!/eprint!/write!(stderr)/writeln!(stderr) in the scanned files
# MUST be annotated with `// stderr-ok: <reason>` or be inside `#[cfg(test)]`.
# This prevents regressions where a new stderr write corrupts the LiveScreen
# alternate-screen rendering.

set -euo pipefail

# Check if a given line number falls inside a #[cfg(test)] block by reading the
# source file and tracking brace depth from the #[cfg(test)] attribute.
in_test_block() {
  local file="$1" target_line="$2"
  awk -v target="$target_line" '
    BEGIN { in_test = 0; depth = 0; cfg_test_line = 0; result = 1; target_in_test = 0 }

    /^[[:space:]]*#\[cfg\(test\)\]/ {
      cfg_test_line = NR
    }

    {
      line = $0
      # Strip string literals and line comments for accurate brace counting
      gsub(/"([^"\\]|\\.)*"/, "", line)
      gsub(/\/\/.*$/, "", line)

      n = split(line, chars, "")
      for (i = 1; i <= n; i++) {
        if (chars[i] == "{") {
          if (cfg_test_line > 0 && !in_test) {
            # First { after #[cfg(test)] — entering test block
            in_test = 1
            depth = 1
            cfg_test_line = 0
          } else if (in_test) {
            depth++
            cfg_test_line = 0
          }
        } else if (chars[i] == "}" && in_test) {
          depth--
          if (depth <= 0) {
            in_test = 0
            depth = 0
          }
        }
        # Track if target line was ever inside a test block during brace processing
        if (NR == target && in_test) target_in_test = 1
      }

      # If #[cfg(test)] is pending but we hit a ; at depth 0, it was a non-block item
      if (cfg_test_line > 0 && !in_test && line ~ /;/) {
        cfg_test_line = 0
      }
    }

    NR == target {
      result = (in_test || target_in_test) ? 0 : 1
      exit
    }

    END { exit result }
  ' "$file"
}

matches=$(
  grep -rn --include='*.rs' -E 'eprintln!|eprint!|writeln!\(stderr|write!\(stderr' \
    cli/src/tasks/ \
    cli/src/tui/live_screen.rs \
  | grep -v 'stderr-ok' \
  || true
)

violations=""
while IFS= read -r line; do
  [ -z "$line" ] && continue
  file="${line%%:*}"
  rest="${line#*:}"
  lineno="${rest%%:*}"
  if ! in_test_block "$file" "$lineno"; then
    violations="${violations:+${violations}
}${line}"
  fi
done <<< "$matches"

if [ -n "$violations" ]; then
  echo "ERROR: Unannotated stderr writes found in monitoring hot-path files:"
  echo ""
  echo "$violations"
  echo ""
  echo "Each direct stderr write must be annotated with:"
  echo "  // stderr-ok: <reason why this is safe>"
  echo ""
  echo "Valid reasons:"
  echo "  - pre-LiveScreen (runs before alternate screen enters)"
  echo "  - post-LiveScreen (runs after alternate screen exits)"
  echo "  - after LiveScreen dropped"
  echo "  - write-path only, never called during monitoring"
  echo "  - spawn evaluation, never called during monitoring"
  echo "  - template validation, never called during monitoring"
  exit 1
fi

echo "OK: No unannotated stderr writes in monitoring hot-path files."
