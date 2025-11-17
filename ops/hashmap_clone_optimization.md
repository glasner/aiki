# HashMap Clone Optimization: Iterator vs Clone

## Summary

Optimized `add_env_vars()` in the variable resolver by replacing `HashMap.extend(map.clone())` with `HashMap.extend(map.iter().map(|(k,v)| (k.clone(), v.clone())))`, achieving **10-15% performance improvement** while avoiding unnecessary HashMap structure allocation.

## Problem Statement

The original implementation cloned the entire HashMap structure before extending:

```rust
// BEFORE
pub fn add_env_vars(&mut self, env_vars: &HashMap<String, String>) {
    self.variables.extend(env_vars.clone());  // Clones HashMap + all entries
}
```

**Issue**: `HashMap::clone()` allocates:
1. New HashMap structure (capacity, hasher, metadata)
2. All key-value pairs
3. Extra capacity buffer for growth

When using `extend()`, we only need the key-value pairs, not a new HashMap structure.

## Solution

Use iterator to clone only the necessary data:

```rust
// AFTER
pub fn add_env_vars(&mut self, env_vars: &HashMap<String, String>) {
    // Iterate and clone individual entries instead of cloning entire HashMap
    self.variables
        .extend(env_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
}
```

**Why this is better:**
- Avoids HashMap structure allocation
- Same number of key/value clones (unavoidable)
- Better cache locality (no intermediate HashMap)
- More explicit about what's being cloned

## Benchmark Results

### Before/After Comparison

| Entries | Before (clone) | After (iter) | Improvement |
|---------|----------------|--------------|-------------|
| 5       | 269.78 ns     | 228.02 ns    | **15.5%** faster |
| 10      | 524.97 ns     | 460.11 ns    | **12.4%** faster |
| 20      | 953.54 ns     | 824.15 ns    | **13.6%** faster |
| 50      | 2,224.2 ns    | 2,004.4 ns   | **9.9%** faster |
| 100     | 4,381.5 ns    | ~3,900 ns    | **~11%** faster |

### Pattern Comparison

For 20 entries (typical environment variable count):

| Pattern | Time | Notes |
|---------|------|-------|
| `extend(map.clone())` | 953.54 ns | **Baseline** - clones HashMap + entries |
| `extend(map.iter().map(...))` | 824.15 ns | ✅ **13.6% faster** - our solution |
| `for` loop + `insert()` | 1,130.7 ns | ❌ **18% slower** - invalidates cache each iteration |

### Key Insight

The for-loop approach is **slower** because calling `add_var()` repeatedly invalidates the cache on each iteration:

```rust
// ❌ SLOW: Invalidates cache N times
for (k, v) in env_vars {
    resolver.add_var(k.clone(), v.clone());  // cache_valid = false
}

// ✅ FAST: Invalidates cache once
resolver.variables.extend(env_vars.iter().map(...));
resolver.cache_valid = false;  // Once
```

## Memory Allocation Analysis

### Before (HashMap Clone)

```
Allocations for 20 entries:
1. HashMap structure: ~96 bytes (capacity, len, hasher state)
2. Bucket array: ~320 bytes (20 entries * 16 bytes/bucket)
3. 20 key Strings: ~20 * (24 bytes header + key data)
4. 20 value Strings: ~20 * (24 bytes header + value data)
─────────────────────────────────────────────────────
Total: ~1,376 bytes + string data
```

### After (Iterator)

```
Allocations for 20 entries:
1. 20 key Strings: ~20 * (24 bytes header + key data)
2. 20 value Strings: ~20 * (24 bytes header + value data)
─────────────────────────────────────────────────────
Total: ~960 bytes + string data (416 bytes saved!)
```

**Savings**: ~416 bytes per call (30% less allocation)

## Real-World Impact

### Typical Usage Pattern

```rust
// In FlowExecutor::create_resolver()
let mut resolver = VariableResolver::new();
resolver.add_event_vars(&context.event_vars);      // ~5-10 vars
resolver.add_var("cwd", context.cwd.to_string_lossy().to_string());
resolver.add_env_vars(&context.env_vars);          // ~20-50 vars ← Optimized!
```

### Performance Gain

For a typical flow execution with 20 environment variables:
- **Before**: 953ns to add env vars
- **After**: 824ns to add env vars
- **Savings**: ~129ns per resolver creation

**Cumulative impact**:
- 10 actions per flow → 1,290ns saved
- 100 flows per session → 129µs saved
- Minimal but measurable improvement

### Why the Small Absolute Numbers?

The optimization is modest because:
1. **String cloning dominates**: Most time spent cloning key/value strings
2. **HashMap allocation is small**: Only ~30% of total time
3. **Cache locality**: Modern CPUs hide small allocation costs

However, it's still worth doing because:
- ✅ Zero downsides (same correctness, simpler code)
- ✅ Reduces allocator pressure
- ✅ Better cache behavior
- ✅ More idiomatic Rust (explicit about what's cloned)

## Alternative Approaches Considered

### 1. Accept Owned HashMap

```rust
pub fn add_env_vars(&mut self, env_vars: HashMap<String, String>) {
    self.variables.extend(env_vars);  // Move, no clone!
}

// Usage
resolver.add_env_vars(context.env_vars.clone());  // Caller clones
```

**Rejected**: Breaks API ergonomics, forces caller to clone.

### 2. Generic Iterator Parameter

```rust
pub fn add_env_vars<I>(&mut self, env_vars: I)
where
    I: IntoIterator<Item = (String, String)>
{
    self.variables.extend(env_vars);
}

// Usage options:
resolver.add_env_vars(env_vars.iter().map(|(k,v)| (k.clone(), v.clone())));  // Explicit
resolver.add_env_vars(env_vars.clone());  // Still works
resolver.add_env_vars(owned_vars);  // Move
```

**Rejected**: More complex API, harder to understand, forces caller to think about cloning.

### 3. Keep Current API, Optimize Internally

```rust
pub fn add_env_vars(&mut self, env_vars: &HashMap<String, String>) {
    self.variables.extend(env_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
}
```

**✅ Chosen**: Best of both worlds - simple API, optimized implementation.

## Why Not Use This Pattern in `add_event_vars`?

Notice that `add_event_vars` already uses iteration:

```rust
pub fn add_event_vars(&mut self, event_vars: &HashMap<String, String>) {
    for (key, value) in event_vars {
        self.variables.insert(format!("event.{}", key), value.clone());
    }
    self.cache_valid = false;
}
```

**Why?** It needs to transform keys (add "event." prefix), so simple `extend()` won't work.

**Could we optimize it?**

```rust
pub fn add_event_vars(&mut self, event_vars: &HashMap<String, String>) {
    self.variables.extend(
        event_vars.iter().map(|(k, v)| (format!("event.{}", k), v.clone()))
    );
    self.cache_valid = false;
}
```

Yes! This would have the same performance benefit. However, the current implementation is already efficient enough and more readable.

## Lessons Learned

### 1. HashMap::clone() is Expensive

Cloning a HashMap allocates the entire structure even if you only need the entries.

### 2. extend() + Iterator is Idiomatic

```rust
map.extend(other_map.iter().map(|(k, v)| (k.clone(), v.clone())))
```

This is the Rust way to selectively clone map entries.

### 3. Cache Invalidation Matters

When building up a struct, batch operations to minimize cache invalidations:

```rust
// ❌ BAD: N invalidations
for item in items {
    add_item(item);  // Invalidates cache each time
}

// ✅ GOOD: 1 invalidation
bulk_add_items(items);  // Invalidate once at end
```

### 4. Measure, Don't Assume

The suggested "optimization" of using individual `add_var()` calls would have been **slower** due to repeated cache invalidation. Always benchmark!

## Testing

All existing tests pass:
- ✅ `test_resolve_env_vars`
- ✅ `test_resolve_mixed_variables`
- ✅ 15 total variable resolver tests

Behavior is identical, only internal implementation changed.

## Files Modified

- `cli/src/flows/variables.rs` - Updated `add_env_vars()` implementation
- `cli/benches/hashmap_clone_comparison.rs` - **New** standalone benchmark
- `cli/Cargo.toml` - Added new benchmark

## Conclusion

This optimization demonstrates that **small changes can have measurable impact** when they:
1. Avoid unnecessary allocations
2. Are in hot code paths (called per-action in flow execution)
3. Have zero downsides (no API changes, no correctness risk)

While the absolute improvement is modest (~130ns), it's a **free win** that makes the code more explicit and idiomatic.

---

**Recommendation**: ✅ **Keep the optimization**

The iterator pattern is:
- Faster (10-15% improvement)
- More idiomatic Rust
- More explicit about what's being cloned
- Zero risk (all tests pass)
