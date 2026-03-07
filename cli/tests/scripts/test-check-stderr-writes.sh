#!/usr/bin/env bash
# Regression tests for the in_test_block() function in check-stderr-writes.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Source in_test_block from the guardrail script (function only, no side effects)
eval "$(sed -n '/^in_test_block()/,/^}/p' "$SCRIPT_DIR/check-stderr-writes.sh")"

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

pass=0
fail=0

assert_not_in_test() {
  local desc="$1" file="$2" line="$3"
  if in_test_block "$file" "$line"; then
    echo "FAIL: $desc (expected NOT in test block, got in test block)"
    fail=$((fail + 1))
  else
    echo "PASS: $desc"
    pass=$((pass + 1))
  fi
}

assert_in_test() {
  local desc="$1" file="$2" line="$3"
  if in_test_block "$file" "$line"; then
    echo "PASS: $desc"
    pass=$((pass + 1))
  else
    echo "FAIL: $desc (expected in test block, got NOT in test block)"
    fail=$((fail + 1))
  fi
}

# --- Test 1: False-negative regression ---
# #[cfg(test)] on a single item must NOT exempt subsequent production code.
cat > "$tmpdir/false_negative.rs" << 'RUST'
#[cfg(test)]
const TEST_TIMEOUT: u64 = 5;

fn production_code() {
    eprintln!("bug!");
}
RUST
assert_not_in_test "single-item #[cfg(test)] does not exempt production eprintln" \
  "$tmpdir/false_negative.rs" 5

# --- Test 2: mod tests block correctly exempted ---
cat > "$tmpdir/mod_tests.rs" << 'RUST'
fn production() {}

#[cfg(test)]
mod tests {
    fn test_something() {
        eprintln!("test output");
    }
}
RUST
assert_in_test "#[cfg(test)] mod tests exempts eprintln inside" \
  "$tmpdir/mod_tests.rs" 6

# --- Test 3: pub mod tests also works ---
cat > "$tmpdir/pub_mod.rs" << 'RUST'
fn production() {}

#[cfg(test)]
pub mod tests {
    fn t() {
        eprintln!("ok");
    }
}
RUST
assert_in_test "#[cfg(test)] pub mod tests exempts eprintln inside" \
  "$tmpdir/pub_mod.rs" 6

# --- Test 4: production code after mod tests closes is flagged ---
cat > "$tmpdir/after_mod.rs" << 'RUST'
#[cfg(test)]
mod tests {
    fn t() { }
}

fn later() {
    eprintln!("should be flagged");
}
RUST
assert_not_in_test "eprintln after mod tests closes is flagged" \
  "$tmpdir/after_mod.rs" 7

# --- Test 5: mixed single-item and mod tests ---
cat > "$tmpdir/mixed.rs" << 'RUST'
#[cfg(test)]
const FOO: u32 = 42;

fn mid() {
    eprintln!("violation");
}

#[cfg(test)]
mod tests {
    fn t() {
        eprintln!("ok in test");
    }
}
RUST
assert_not_in_test "eprintln between single-item cfg(test) and mod tests is flagged" \
  "$tmpdir/mixed.rs" 5
assert_in_test "eprintln inside mod tests is exempted" \
  "$tmpdir/mixed.rs" 11

# --- Test 6: #[cfg(test)] fn block is exempt ---
cat > "$tmpdir/cfg_test_fn.rs" << 'RUST'
#[cfg(test)]
fn test_helper() {
    eprintln!("test output");
}
RUST
assert_in_test "#[cfg(test)] fn block exempts eprintln inside" \
  "$tmpdir/cfg_test_fn.rs" 3

# --- Test 7: #[cfg(test)] impl block is exempt ---
cat > "$tmpdir/cfg_test_impl.rs" << 'RUST'
#[cfg(test)]
impl MyStruct {
    fn debug_print(&self) {
        eprintln!("debug: {:?}", self);
    }
}
RUST
assert_in_test "#[cfg(test)] impl block exempts eprintln inside" \
  "$tmpdir/cfg_test_impl.rs" 4

# --- Test 8: inline single-line #[cfg(test)] mod is exempt ---
cat > "$tmpdir/inline_mod.rs" << 'RUST'
#[cfg(test)] mod tests { fn t() { eprintln!("ok"); } }
RUST
assert_in_test "inline single-line #[cfg(test)] mod exempts eprintln" \
  "$tmpdir/inline_mod.rs" 1

# --- Test 9: Multi-line function signature (gap > 2 lines) should be exempt ---
cat > "$tmpdir/multiline_sig.rs" << 'RUST'
#[cfg(test)]
fn multiline_sig(
    a: u32,
    b: u32,
) -> bool {
    eprintln!("test output");
    true
}
RUST
assert_in_test "multi-line #[cfg(test)] fn signature exempts eprintln inside" \
  "$tmpdir/multiline_sig.rs" 6

# --- Test 10: #[cfg(test)] on a const with semicolon should NOT enter test block ---
cat > "$tmpdir/cfg_test_const.rs" << 'RUST'
#[cfg(test)]
const TIMEOUT: u64 = 5;

fn production() {
    eprintln!("violation");
}
RUST
assert_not_in_test "#[cfg(test)] const with semicolon does not exempt production eprintln" \
  "$tmpdir/cfg_test_const.rs" 5

# --- Test 11: #[cfg(test)] on a use statement with semicolon should NOT enter test block ---
cat > "$tmpdir/cfg_test_use.rs" << 'RUST'
#[cfg(test)]
use std::io::Write;

fn production() {
    eprintln!("violation");
}
RUST
assert_not_in_test "#[cfg(test)] use with semicolon does not exempt production eprintln" \
  "$tmpdir/cfg_test_use.rs" 5

# --- Summary ---
echo ""
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
