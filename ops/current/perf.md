# ACP Proxy Performance Analysis

**Date**: 2024-12-04  
**Analyzed File**: `cli/src/commands/acp.rs`  
**Status**: All Phase 1 events implemented, ready for optimization

---

## Executive Summary

The ACP proxy is **already well-optimized** in many areas (JSON serialization outside locks, ownership over cloning, minimal critical sections). The biggest performance wins come from reducing allocations in hot paths, particularly:

1. **Response accumulation** - Pre-allocate string capacity
2. **Session ID handling** - Use `Arc<str>` instead of repeated clones
3. **HashMap pre-allocation** - Avoid rehashing
4. **Path conversions** - Eliminate double allocations

**Estimated performance gain**: 10-20% reduction in allocations, 5-10% CPU reduction  
**Estimated effort for top 4**: ~50 minutes

---

## HIGH IMPACT OPTIMIZATIONS

### 1. ⚡ Pre-allocate Response Accumulator Strings

**Location**: Line 841  
**Issue**: `String::push_str()` on a growing string causes repeated allocations  
**Impact**: HIGH - Happens on every `agent_message_chunk` (dozens per response)

**Current Code**:
```rust
response_accumulator
    .entry(session_id.clone())
    .or_insert_with(String::new)
    .push_str(text);
```

**Fix**:
```rust
response_accumulator
    .entry(session_id.clone())
    .or_insert_with(|| String::with_capacity(4096))  // Typical response size
    .push_str(text);
```

**Trade-off**: Uses ~4KB more memory per active session, but eliminates O(n²) reallocation pattern

**Effort**: 2 minutes

---

### 2. ⚡ Use `Arc<str>` for Session IDs

**Location**: Multiple (lines 841, 851, 855, etc.)  
**Issue**: `session_id.clone()` for HashMap operations allocates new strings  
**Impact**: HIGH - Happens on every message with session_id (~20+ clones per message)

**Current Pattern**:
```rust
response_accumulator.entry(session_id.clone()).or_insert_with(String::new)
autoreply_counters.insert(session_id.to_string(), new_count);
```

**Fix**: Use `Arc<str>` for cheap pointer copies
```rust
// At extraction point:
let session_id = Arc::<str>::from(notification.session_id.as_str());

// Then use Arc::clone (cheap pointer copy) instead of String::clone
response_accumulator.entry(Arc::clone(&session_id))
```

**Trade-off**: Slight API complexity increase, but eliminates ~20+ string allocations per message

**Effort**: 30 minutes (requires updating HashMap types and function signatures)

---

### 3. ⚡ Pre-allocate HashMaps

**Location**: Lines 729-735  
**Issue**: HashMaps start with default capacity, causing rehashing  
**Impact**: MEDIUM - Only affects startup, but can save multiple rehash operations

**Current Code**:
```rust
let mut tool_call_contexts: HashMap<ToolCallId, ToolCallContext> = HashMap::new();
let mut prompt_requests: HashMap<JsonRpcId, String> = HashMap::new();
```

**Fix**:
```rust
let mut tool_call_contexts = HashMap::with_capacity(16);  // Typical concurrent tool calls
let mut prompt_requests = HashMap::with_capacity(8);      // Typical pending requests
let mut response_accumulator = HashMap::with_capacity(4); // Typical active sessions
let mut autoreply_counters = HashMap::with_capacity(4);
```

**Trade-off**: ~1KB extra memory, eliminates 2-3 rehashing operations per HashMap

**Effort**: 2 minutes

---

### 4. ⚡ Optimize Path Conversions

**Location**: Lines 1122-1126  
**Issue**: `to_string_lossy().to_string()` causes double allocation  
**Impact**: MEDIUM - Happens per file in tool_call (typically 1-10 files)

**Current Code**:
```rust
let file_paths: Vec<String> = context
    .paths
    .iter()
    .map(|p| p.to_string_lossy().to_string())
    .collect();
```

**Fix Option 1**: Keep as `PathBuf` in events (zero-copy)
```rust
file_paths: Vec<PathBuf>,
```

**Fix Option 2**: Use `Cow<str>` to avoid allocation for UTF-8 paths
```rust
let file_paths: Vec<Cow<str>> = context
    .paths
    .iter()
    .map(|p| p.to_string_lossy())
    .collect();
```

**Trade-off**: API change required, but saves 2 allocations per file

**Effort**: 15 minutes

---

## MEDIUM IMPACT OPTIMIZATIONS

### 5. 🟡 Use `.context()` for Error Messages

**Location**: Lines 1084, 1286, 1326, 1411  
**Issue**: `format!` and `anyhow::anyhow!` allocate even when errors don't occur  
**Impact**: LOW-MEDIUM - Only affects error paths

**Current Code**:
```rust
.map_err(|e| AikiError::Other(anyhow::anyhow!("Failed to serialize autoreply: {}", e)))
```

**Fix**:
```rust
.context("Failed to serialize autoreply")
.map_err(AikiError::Other)?
```

**Trade-off**: None - strictly better

**Effort**: 10 minutes

---

### 6. 🟡 Debug Logging Macro

**Location**: Throughout (lines 1339, 1386, 1433, etc.)  
**Issue**: `eprintln!` formatting always evaluated, even when not printed  
**Impact**: LOW - Only matters when AIKI_DEBUG is unset

**Current Pattern**:
```rust
if std::env::var("AIKI_DEBUG").is_ok() {
    eprintln!("[acp] Sent autoreply to agent: {} bytes", json.len());
}
```

**Fix**:
```rust
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!($($arg)*);
        }
    };
}

// Usage:
debug_log!("[acp] Sent autoreply to agent: {} bytes", json.len());
```

**Trade-off**: Macro adds complexity, but eliminates format allocations when debugging is off

**Effort**: 15 minutes

---

## LOW PRIORITY OPTIMIZATIONS

### 7. 🔵 Bounded Channels (Related to Issue #3)

**Location**: Lines 704-708  
**Issue**: Unbounded channels can grow indefinitely under load  
**Impact**: LOW - Only matters if event processing is slower than generation

**Current Code**:
```rust
let (metadata_tx, metadata_rx) = mpsc::channel::<StateMessage>();
let (autoreply_tx, autoreply_rx) = mpsc::channel::<AutoreplyChannelMessage>();
```

**Fix**:
```rust
let (metadata_tx, metadata_rx) = mpsc::sync_channel::<StateMessage>(32);
let (autoreply_tx, autoreply_rx) = mpsc::sync_channel::<AutoreplyChannelMessage>(8);
```

**Trade-off**: Can cause blocking if channels fill up, but prevents unbounded memory growth

**Effort**: 5 minutes

**Note**: This is part of Issue #3 from critical-review.md (unbounded channel growth)

---

### 8. 🔵 Mutex Poisoning Recovery (Reconsider Issue #5 Fix)

**Location**: Lines 1334, 1387, 1406  
**Issue**: Poisoning check happens on every lock (hot path)  
**Impact**: LOW - Negligible unless panics are frequent

**Current Code** (Issue #5 fix applied today):
```rust
let mut stdin = match agent_stdin.lock() {
    Ok(guard) => guard,
    Err(poisoned) => {
        eprintln!("Warning: Mutex poisoned...");
        poisoned.into_inner()
    }
};
```

**Alternative**: Remove recovery for hot paths
```rust
let mut stdin = agent_stdin.lock().expect("mutex not poisoned");
```

**Trade-off**: Less graceful degradation, but matches standard Rust patterns and is slightly faster

**Recommendation**: Keep current implementation - the defensive programming is worth the negligible overhead

---

## ALREADY OPTIMIZED ✅

These areas are already well-optimized and don't need changes:

1. **JSON serialization outside locks** ✅
   - Fixed in Issue #1 today
   - Data serialized before acquiring mutex

2. **StateMessage ownership** ✅
   - Enum takes ownership, no unnecessary clones
   - Efficient message passing

3. **Minimal critical sections** ✅
   - Lock held only during I/O operations
   - Reduced deadlock risk (Issue #1 fix)

4. **Three-thread architecture** ✅
   - Sound design with explicit state ownership
   - No lock-free opportunities without major refactor

5. **O(n) metadata drain loop** ✅
   - `try_recv()` is optimal for non-blocking drain
   - Typically drains 0-2 messages

---

## RECOMMENDED IMPLEMENTATION ORDER

| Priority | Optimization | Effort | Impact | Lines |
|----------|-------------|--------|--------|-------|
| 1 | Response accumulator capacity | 2 min | HIGH | 841 |
| 2 | HashMap pre-allocation | 2 min | MEDIUM | 729-735 |
| 3 | Error context | 10 min | MEDIUM | Multiple |
| 4 | Path conversion | 15 min | MEDIUM | 1122-1126 |
| 5 | Debug logging macro | 15 min | LOW | Throughout |
| 6 | Arc<str> for session IDs | 30 min | HIGH | Multiple |
| 7 | Bounded channels | 5 min | LOW | 704-708 |

**Quick wins (30 minutes)**: #1, #2, #3, #4  
**Full optimization (1.5 hours)**: All items

---

## PERFORMANCE CHARACTERISTICS

### Hot Paths (Per-Message Operations)

1. **JSON parsing**: Once per message (both directions)
2. **HashMap lookups**: 2-5 per message (session_id, request_id)
3. **String operations**: 10-30 per message (parsing, formatting)
4. **Channel sends/receives**: 1-3 per message (metadata, autoreplies)
5. **Mutex locks**: 1-2 per message (stdin writes)

### Memory Usage

- **Per-session state**: ~500 bytes (session_id, counters, accumulator)
- **Per-tool-call**: ~200 bytes (context, paths, content)
- **Per-pending-request**: ~100 bytes (request_id → session_id mapping)

**Typical memory footprint**: 2-5 KB per active session

### Allocation Patterns

**Current (per message)**:
- Session ID clones: ~20 allocations × ~32 bytes = 640 bytes
- Response accumulation: 1-10 reallocations (O(n²) growth)
- Path conversions: 2 allocations × 2-10 files = 4-20 allocations

**After optimization**:
- Session ID copies: 20 Arc clones (cheap pointer copies)
- Response accumulation: 0-1 reallocations (pre-allocated)
- Path conversions: 0-1 allocations (Cow or PathBuf)

**Estimated reduction**: 60-80% fewer allocations

---

## TESTING RECOMMENDATIONS

After implementing optimizations:

1. **Benchmarks**:
   ```bash
   cargo bench --bench acp_proxy
   ```

2. **Memory profiling**:
   ```bash
   valgrind --tool=massif target/release/aiki acp claude-code
   ```

3. **Allocation tracking**:
   ```bash
   RUSTFLAGS="-Z print-type-sizes" cargo build --release
   ```

4. **Load testing**:
   - Simulate 100 rapid messages
   - Monitor memory growth
   - Verify no leaks with long-running sessions

---

## NOTES

- The code is production-ready as-is
- Optimizations are incremental improvements, not blockers
- Focus on high-impact items first (#1, #2, #3)
- Profile before/after to validate improvements
- Consider optimizing after real-world usage data is collected

---

## RELATED ISSUES

- **Issue #1** (Deadlock): Fixed today ✅
- **Issue #2** (Race condition): Fixed today ✅
- **Issue #3** (Unbounded channels): Related to optimization #7
- **Issue #5** (Mutex poisoning): Fixed today ✅ (keep as-is despite minor overhead)

---

## CONCLUSION

The ACP proxy has a solid foundation with minimal obvious performance problems. The recommended optimizations are **nice-to-haves** that will provide measurable but not dramatic improvements. Prioritize:

1. Quick wins (#1, #2) for immediate 10-15% allocation reduction
2. Medium-term improvements (#3, #4, #6) for comprehensive optimization
3. Low-priority items (#5, #7) only if profiling shows they matter

**Total estimated effort**: 1.5 hours for full optimization  
**Estimated gain**: 10-20% allocation reduction, 5-10% CPU reduction
