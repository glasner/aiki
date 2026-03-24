# Fix Rhai non-bool operands in `&&`/`||` conditions

**Date**: 2026-03-20
**Status**: Reviewed
**Priority**: P1 — silently breaks conflict detection in workspace absorption
**Sourced from**: [stale-absorbtion-locks.md](isolation/stale-absorbtion-locks.md) (was Fix 3)

---

## Problem

Hook conditions using `and`/`or` (rewritten to `&&`/`||`) fail silently when an operand is not a bool. The error is caught by lenient mode and defaults to **false**, skipping the condition entirely.

### Trigger condition (hooks.yaml, lines 163 and 330)

```yaml
- if: absorb_result != "ok" and absorb_result != "0" and absorb_result
```

The trailing `and absorb_result` is a bare truthiness check. Rhai's `&&` operator requires bool on both sides — it does not auto-coerce i64, String, or any other type.

### What happens today

| `absorb_result` value | Coerced type | First `!=` | Second `!=` | `&& absorb_result` | Result |
|---|---|---|---|---|---|
| `"ok"` | String | false | — | — | **false** (correct — short-circuits) |
| `"0"` | **i64(0)** | **ERROR** (i64 vs String) | — | — | **false** (correct by accident) |
| `"src/foo.rs\n..."` | String | true | true | **ERROR** (String not bool) | **false** (WRONG — should be true) |

**Impact**: Conflict detection after workspace absorption is completely broken. When conflicts exist, the condition silently evaluates to false. The autoreply telling the agent to resolve conflicts never fires.

---

## Root Cause

The chain is:

1. `workspace_absorb_all()` returns `ActionResult { stdout: "ok" | "0" | <file_list> }`
2. `store_action_result()` stores `stdout` as a `String` in `let_vars`
3. `evaluate_condition()` calls `build_scope_from_flat()` → `coerce_to_dynamic()`:
   - `"true"/"false"` → `Dynamic::from(bool)` ✓
   - `"0"`, `"42"` → `Dynamic::from(i64)` — not usable in `&&`/`||`
   - `"ok"`, `"src/foo.rs"` → `Dynamic::from(String)` — not usable in `&&`/`||`
4. `rewrite_word_operators()` converts `and`/`or` to `&&`/`||`
5. Rhai evaluates `bool_expr && i64_or_string` → **"Data type incorrect: i64/string (expecting bool)"**
6. `ExpressionEvaluator::evaluate()` catches the error → returns `Ok(false)`

The `dynamic_to_bool` function (`expressions/mod.rs:574`) already handles all types correctly, but it only runs on the **final** result. Rhai fails **during** evaluation before reaching it.

---

## Proposed Fix

Register a `__truthy__` function in the Rhai engine that mirrors `dynamic_to_bool`, then preprocess bare boolean operands to wrap them in `__truthy__(...)`.

### Step 1: Register `__truthy__` in the Rhai engine

In `ExpressionEvaluator::new()` (`cli/src/expressions/mod.rs:34`):

```rust
// Register truthiness conversion so that non-bool types can be used
// as operands of && and || (Rhai requires bool for these operators).
engine.register_fn("__truthy__", |x: Dynamic| -> bool {
    dynamic_to_bool(&x)
});
```

### Step 2: Preprocessing pass to wrap bare boolean operands

After `rewrite_word_operators`, add a pass that detects bare identifier operands of `&&`/`||` and wraps them in `__truthy__(...)`.

A "bare operand" is an identifier (possibly dotted, e.g., `absorb_result` or `event.write`) that is **not** part of a comparison expression. Concretely: not immediately preceded or followed by `==`, `!=`, `<`, `>`, `<=`, `>=`.

```
// Before:
absorb_result != "ok" && absorb_result != "0" && absorb_result

// After:
absorb_result != "ok" && absorb_result != "0" && __truthy__(absorb_result)
```

Comparison operands are left alone — they already produce bool results.

#### Implementation strategy

**Preferred: AST-level rewriting.** After `rewrite_word_operators`, compile the expression with `engine.compile_expression()`, walk the AST to find `&&`/`||` nodes where an operand is a bare variable (not a comparison or function call), and rewrite those nodes to `__truthy__(var)`. This is more robust than string-level token manipulation.

**Fallback: token-level heuristic.** If Rhai doesn't expose enough AST API for walking/rewriting, use the token heuristic below with the full set of edge-case rules.

> **Note — operator overloading won't work here.** Rhai's `&&`/`||` are short-circuit operators baked into the language, not regular binary ops. They cannot be overridden with `register_fn`. The preprocessing approach is the only viable path.

### Detection heuristic (token-level fallback)

For each identifier token in the expression:
1. Look at the **next** non-whitespace token. If it's a comparison operator (`==`, `!=`, `<`, `>`, `<=`, `>=`), skip — it's a comparison LHS.
2. Look at the **previous** non-whitespace token. If it's a comparison operator, skip — it's a comparison RHS.
3. Otherwise, wrap in `__truthy__(...)`.

**Transparent tokens when scanning backward:** The `!` operator and `)` should be treated as transparent when determining whether an identifier is bare:
- `!` before an identifier: `!` also requires bool in Rhai, so wrap the identifier *inside* the negation: `!absorb_result` → `!__truthy__(absorb_result)`. When scanning backward from the identifier, treat `!` as transparent (it's not a comparison op, so the identifier is still bare).
- Parens around an identifier: `(absorb_result) && other` — the `)` after `absorb_result` is not a comparison operator. The identifier is still bare and should be wrapped: `(__truthy__(absorb_result)) && other`.

This correctly handles:
- `x != "ok" && x` → wraps second `x` only
- `x > 5 && approved` → wraps `approved`
- `!done` → `!__truthy__(done)` (wraps inside the negation)
- `a > 5 && (b == "yes" || c)` → wraps `c`
- `(absorb_result) && other` → wraps `absorb_result` inside the parens
- `event.write` → wraps `event.write` (bare dotted path)
- `x == 5` → no wrapping (both sides of comparison)

Edge cases to handle:
- Don't wrap inside string literals
- Don't wrap Rhai keywords (`true`, `false`, `if`, `else`, etc.)
- Don't wrap function calls (tokens followed by `(`)
- Don't wrap dotted access after function calls: `func_call().result` — the whole chain originates from a call, not a bare variable. Detect by scanning backward through the dotted path to see if it starts with a `)`-terminated call.
- Don't wrap numeric/string literals

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/expressions/mod.rs` | Register `__truthy__` function in `ExpressionEvaluator::new()`; add `wrap_bare_boolean_operands()` preprocessing pass called from `evaluate()` |
| `cli/src/expressions/tests.rs` | Add tests for int/string operands in `&&`/`||` conditions |

---

## Test Cases

These should all pass after the fix:

```rust
// Bare integer in && — the original bug
#[test]
fn test_int_operand_in_and() {
    // "42" coerces to i64(42), should be truthy in && context
    assert!(eval(
        r#"x != "ok" and x != "0" and x"#,
        &[("x", "42")]
    ));
}

// Bare string in && — also broken today
#[test]
fn test_string_operand_in_and() {
    assert!(eval(
        r#"x != "ok" and x != "0" and x"#,
        &[("x", "src/foo.rs")]
    ));
}

// Falsy values should still be falsy
#[test]
fn test_zero_operand_in_and() {
    assert!(!eval("x and y", &[("x", "true"), ("y", "0")]));
}

#[test]
fn test_empty_string_operand_in_and() {
    assert!(!eval("x and y", &[("x", "true"), ("y", "")]));
}

// Negated bare operand — ! also requires bool in Rhai
#[test]
fn test_negated_bare_operand() {
    // !absorb_result where absorb_result is a non-empty string
    assert!(!eval("!x", &[("x", "src/foo.rs")]));
}

// Real-world absorb_result condition
#[test]
fn test_absorb_result_conflict_detected() {
    assert!(eval(
        r#"absorb_result != "ok" and absorb_result != "0" and absorb_result"#,
        &[("absorb_result", "src/lib.rs")]
    ));
}

#[test]
fn test_absorb_result_ok() {
    assert!(!eval(
        r#"absorb_result != "ok" and absorb_result != "0" and absorb_result"#,
        &[("absorb_result", "ok")]
    ));
}

#[test]
fn test_absorb_result_no_workspaces() {
    assert!(!eval(
        r#"absorb_result != "ok" and absorb_result != "0" and absorb_result"#,
        &[("absorb_result", "0")]
    ));
}
```

---

## Scope

This fix is entirely within the expression evaluator. **No changes to hooks.yaml are needed** — the existing condition syntax should just work.

The fix also future-proofs any other hook condition that uses a bare variable as a boolean operand with `and`/`or`/`&&`/`||`.
