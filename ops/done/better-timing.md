# Better Timing Architecture Plan

## Problem

The current timing implementation in `engine.rs` violates DRY and creates maintenance burden:

1. **Duplicate functions**: Every execution function has a `_with_timing` variant
   - `execute_statements()` vs `execute_statements_with_timing()`
   - `execute_statement()` vs `execute_statement_with_timing()`
   - `execute_if()` vs `execute_if_with_timing()`
   - `execute_switch()` vs `execute_switch_with_timing()`

2. **Code drift risk**: Changes to one function must be manually copied to the other

3. **Complexity in wrong place**: Timing logic embedded in the core flow engine instead of being a separate concern

## Current Usage

**Timing is ONLY used in tests:**
- `cli/tests/test_timing_infrastructure.rs` - 4 tests (all test timing infrastructure itself - meta-tests)
- `cli/tests/test_end_to_end_flow.rs` - 5 uses (use timing as proxy to verify statement execution)

**The benchmark (`cli/src/commands/benchmark.rs`) does NOT use timing structs** - it just wraps calls with `Instant::now()` directly.

**Analysis:** The tests don't actually need timing data:
- `test_timing_infrastructure.rs` - Tests the timing infrastructure itself (delete when removing timing)
- `test_end_to_end_flow.rs` - Uses timing to count/verify statements executed (should test actual behavior instead)

The real flow engine behavior is already covered by 80+ unit tests in `engine.rs` that don't use timing.

## Proposed Solution

**Remove all timing code from `engine.rs`. Delete the tests that depend on it.**

The benchmark already times externally using `Instant::now()`, so no changes needed there.

### Architecture

```rust
// Production code (engine.rs) - single, simple implementation
impl FlowEngine {
    pub fn execute_statements(
        statements: &[FlowStatement],
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        // Simple, single implementation
        // No timing overhead
    }
}

// Benchmark (commands/benchmark.rs) - already does this
let start = Instant::now();
let result = FlowEngine::execute_statements(&flow.post_file_change, &mut state)?;
let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
```

### What Gets Removed

From `cli/src/flows/engine.rs`:

1. ❌ `FlowTiming` struct (32-37)
2. ❌ `StatementTiming` struct (59-66)
3. ❌ `FlowEngine::execute_statements_with_timing()` (240-301)
4. ❌ `FlowEngine::execute_statement_with_timing()` (326-355)
5. ❌ `FlowEngine::execute_if_with_timing()` (1256-1283)
6. ❌ `FlowEngine::execute_switch_with_timing()` (1286-1324)
7. ❌ `statement_type_name()` helper (358-364)
8. ❌ `action_type_name()` helper (367-381)
9. ❌ All `Instant::now()` calls in core execution paths

**Lines removed: ~400+ lines**

### What Gets Deleted

**Test files that only exist because of timing:**
1. ❌ `cli/tests/test_timing_infrastructure.rs` - Only tests timing infrastructure itself
2. ❌ `cli/tests/test_end_to_end_flow.rs` - Uses timing to verify execution (redundant with engine.rs unit tests)

**No migration needed** - these tests don't provide value beyond what's already covered by the 80+ unit tests in `engine.rs`.

### Benefits

1. ✅ **Simpler engine.rs** - Single code path for all execution
2. ✅ **No drift risk** - Only one implementation to maintain
3. ✅ **Better separation of concerns** - Timing is orthogonal to flow execution
4. ✅ **Easier to optimize** - No conditional timing overhead in production
5. ✅ **Clearer intent** - Tests that care about timing make it explicit

### Migration Steps

#### Phase 1: Remove from engine.rs
1. Delete `FlowTiming` and `StatementTiming` structs
2. Delete all `_with_timing` methods
3. Remove timing-related helpers
4. Update `mod.rs` exports

#### Phase 2: Delete obsolete tests
1. Delete `cli/tests/test_timing_infrastructure.rs` entirely (only tests timing infrastructure)
2. Delete `cli/tests/test_end_to_end_flow.rs` entirely (behavior already covered by engine.rs unit tests)
3. Verify coverage: Run `cargo test` - should still have 80+ flow engine tests passing

#### Phase 3: Verify
1. All tests still pass
2. Benchmark still works (it already doesn't use timing structs)
3. Production code has zero timing overhead

## Why This Is The Right Approach

1. **Tests don't need timing** - The 2 test files using timing don't test anything meaningful:
   - `test_timing_infrastructure.rs` - Tests the timing infrastructure (circular)
   - `test_end_to_end_flow.rs` - Uses timing to count statements (weak assertion)

2. **Real tests already exist** - The 80+ unit tests in `engine.rs` test actual flow behavior without timing

3. **Benchmark already works** - `commands/benchmark.rs` already times externally with `Instant::now()`

4. **Simpler is better** - Removing timing reduces cognitive load and maintenance burden

## Impact on Fix #2a

This supersedes Fix #2a from `fix.md`. Instead of:
- ❌ Two public methods (`execute_statements` and `execute_statements_with_timing`)
- ❌ Internal helper with flag (`execute_statements_with_options`)

We have:
- ✅ One public method (`execute_statements`)
- ✅ Zero timing overhead in production
- ✅ Tests handle their own timing if needed
