# Milestone 1.2: PostResponse Event

This document outlines the implementation plan for the PostResponse event system (Milestone 1.2), based on the analysis in [response-strategy-comparison.md](./response-strategy-comparison.md).

See [milestone-1.md](./milestone-1.md) for the full Milestone 1 overview.

---

## Overview

The PostResponse event allows flows to validate agent responses and request follow-up work. 

**Key Decision:** Implement concatenation strategy (show all issues at once) with smart stuck detection from the start.

**Syntax:** See [Shared Syntax Pattern](./milestone-1.md#shared-syntax-pattern) in milestone-1.md for the `autoreply:` action syntax (short form and explicit form).

---

## Core Features

**Goal:** Get PostResponse working with concatenation and smart stuck detection.

### 1. Response Concatenation

Multiple `autoreply:` actions accumulate into one message:

```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors
  - if: $ts_errors > 0
    then:
      autoreply: "Fix TypeScript errors"  # Short form - defaults to append
  
  - let: lint_errors = self.count_lint_errors
  - if: $lint_errors > 0
    then:
      autoreply: "Fix linting issues"
```

**Syntax:**
- **Short form:** `autoreply: "string"` - Defaults to `append`
- **Explicit form:** `autoreply: { prepend: "...", append: "..." }` - Full control

**Behavior:**
- All checks run (no short-circuit)
- Autoreplies separated by `---` divider
- Agent sees comprehensive feedback
- `autoreply` indicates system-generated response (not user input)

### 2. Smart Stuck Detection

Track which autoreply actions fired across iterations:

```rust
pub struct PostResponseEvent {
    pub response: String,
    pub session_id: Option<String>,
    pub files_edited: Vec<PathBuf>,
    pub timestamp: DateTime<Utc>,
    pub loop_count: u32,              // Total iterations
    pub active_checks: Vec<String>,   // Check IDs that fired autoreplies
    pub same_checks_count: u32,       // Loops with same checks failing
    pub last_checks_hash: Option<String>,
}

impl PostResponseEvent {
    pub fn is_stuck(&self) -> bool {
        self.same_checks_count >= 2
    }
}
```

**Content-based check IDs:**
Each `autoreply:` action gets a deterministic ID based on its content:

```rust
// At parse time, hash the YAML node
let action_yaml = "autoreply: \"Fix TypeScript errors\"";
let check_id = sha256(action_yaml)[..8];  // "a3f9b2c1"
```

**Stuck detection logic:**
1. Track which check IDs fired autoreplies this iteration
2. Hash the sorted list of check IDs
3. Compare to previous iteration's hash
4. If same checks fired → increment `same_checks_count`
5. If different checks → reset counter

**Why this works:**
- ✅ **Immune to message text changes** - Runtime variable substitution doesn't affect the hash
- ✅ **Deterministic** - Same flow action = same check ID
- ✅ **Detects actual stuck state** - Same checks failing repeatedly
- ✅ **Detects progress** - Fewer checks firing = different hash
- ✅ **Zero overhead** - Computed once at parse time
- ✅ **No flow author work** - Completely automatic

**Example:**
```yaml
PostResponse:
  - if: $ts_errors > 0
    then:
      autoreply: "Fix $ts_errors TypeScript errors"  # check_id: "a3f9b2c1"
  
  - if: $lint_errors > 0
    then:
      autoreply: "Fix linting issues"                # check_id: "7e4d2a89"
```

```
Loop 1: active_checks = ["a3f9b2c1", "7e4d2a89"] → hash = "xyz123"
        Agent fixes some TS errors but not all
        
Loop 2: active_checks = ["a3f9b2c1", "7e4d2a89"] → hash = "xyz123" ← STUCK!
        same_checks_count = 2
        
Loop 3: active_checks = ["7e4d2a89"]             → hash = "abc456" ← PROGRESS!
        same_checks_count resets to 1
```

### 3. Automatic Stuck Detection Behavior

When the agent gets stuck (same errors persist), the system automatically adjusts behavior:

**Flow authors write simple checks:**
```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors
  - if: $ts_errors > 0
    then:
      autoreply: "Fix TypeScript errors"          # check_id: "a3f9b2c1"
  
  - let: lint_errors = self.count_lint_errors
  - if: $lint_errors > 0
    then:
      autoreply: "Fix linting issues"             # check_id: "7e4d2a89"
```

**How it works:**

1. **All checks always run** - Both TypeScript and linting checks execute every iteration
2. **Track which checks fire** - `active_checks = ["a3f9b2c1", "7e4d2a89"]`
3. **Hash detects stuck** - If same checks fired as last iteration, increment `same_checks_count`
4. **Engine filters message** - Based on stuck state, send different messages to agent

**Message sent to agent:**

- **Attempt 1:** Show all responses concatenated
  ```
  Fix TypeScript errors
  
  ---
  
  Fix linting issues
  ```
  - `active_checks = ["a3f9b2c1", "7e4d2a89"]`
  - `same_checks_count = 1`

- **Attempt 2 (same checks fire):** Show only the first response
  ```
  ⚠️ These errors persist after 2 attempts.
  
  Focus ONLY on this issue:
  
  Fix TypeScript errors
  ```
  - `active_checks = ["a3f9b2c1", "7e4d2a89"]` (both still ran!)
  - `same_checks_count = 2` (hash matched previous)
  - Engine sends only first autoreply to agent

- **Attempt 3 (still stuck):** Bail out with suggestion
  ```
  ⚠️ Unable to fix after 3 attempts.
  
  Consider:
  - Reverting changes: jj undo
  - Asking for help
  - Breaking the task into smaller steps
  ```
  - `active_checks = ["a3f9b2c1", "7e4d2a89"]`
  - `same_checks_count = 3`
  - Hit max iterations limit

**Benefits:**
- All checks run every iteration (complete validation)
- Stuck detection based on which checks fire (not message text)
- Engine automatically filters messages to reduce agent overwhelm
- Flow authors don't need conditional logic
- Consistent behavior across all flows

---

## Implementation Tasks

### Shared Message Builder

- [ ] Create `cli/src/flows/actions/message_builder.rs` - Shared parser for prompt/autoreply/commit_message pattern
  - [ ] Parse short form: `action: "string"` → `{ append: "string" }`
  - [ ] Parse explicit form: `action: { prepend: [...], append: [...] }`
  - [ ] Support both single strings and lists for prepend/append
  - [ ] File path detection (if string is valid file path that exists, read contents)
  - [ ] Return unified `MessageBuilder { prepends: Vec<String>, appends: Vec<String> }`

### Core Engine

- [ ] Add `PostResponse` event type to flow engine
- [ ] Implement autoreply concatenation logic (separate with `---`)
- [ ] Add `autoreply:` action using shared `MessageBuilder` parser
- [ ] Hook into agent's message processing loop
- [ ] Implement automatic message filtering based on stuck state:
  - [ ] `same_checks_count == 1`: Send all autoreplies concatenated
  - [ ] `same_checks_count == 2`: Send only first autoreply with warning prepended
  - [ ] `same_checks_count >= 3`: Send bailout message with suggestions
  - [ ] Note: All checks run every iteration, filtering happens when building final message

### Session State Management

- [ ] Track `loop_count` per session
- [ ] Generate content-based check IDs at parse time (hash YAML node)
- [ ] Track `active_checks` (which check IDs fired autoreplies)
- [ ] Store last checks hash in session state
- [ ] Track `same_checks_count` counter
- [ ] Implement checks hash comparison logic (hash sorted check ID list)
- [ ] Add max iteration limit (default: 3)
- [ ] Reset counters on new session

### Flow DSL Variables (Optional)

- [ ] Expose `$event.loop_count` to flows (for custom logic)
- [ ] Expose `$event.same_checks_count` to flows (for custom logic)
- [ ] Expose `$event.active_checks` to flows (for debugging)
- [ ] Add `$event.is_stuck()` helper (for custom logic)

### Testing

- [ ] Unit tests: `MessageBuilder` parser (short form, explicit form, file detection)
- [ ] Unit tests: autoreply concatenation with `append:`
- [ ] Unit tests: content-based check ID generation (deterministic)
- [ ] Unit tests: checks hash computation (deterministic, order-independent)
- [ ] Unit tests: counter increment/reset logic
- [ ] Unit tests: session state persistence
- [ ] Unit tests: message filtering logic (based on `same_checks_count`)
- [ ] Integration tests: multi-check PostResponse flows with multiple autoreplies
- [ ] Integration tests: stuck detection across multiple iterations (same checks)
- [ ] Integration tests: progress detection (fewer checks firing)
- [ ] Integration tests: automatic message filtering (show all → show first → bailout)
- [ ] E2E tests: real agent interaction with TypeScript errors
- [ ] E2E tests: cascading errors scenario (agent introduces new bugs)

### Documentation

- [ ] Tutorial: "Writing PostResponse Flows"
- [ ] Cookbook: Common patterns (multi-check, adaptive)
- [ ] Reference: Full DSL syntax for PostResponse
- [ ] Examples: Real-world flows (TS + linting, test validation)
- [ ] Architecture: Flow engine integration points
- [ ] Troubleshooting: "My flow is stuck in a loop"

---

## Success Criteria

✅ Agent receives concatenated feedback from multiple checks  
✅ Loop stops after max iterations with clear error message  
✅ System detects when same errors persist across iterations  
✅ `same_response_count` increments correctly  
✅ Counter resets when response changes  
✅ Flow authors can write multi-check PostResponse flows  
✅ Flow authors can write adaptive guidance patterns  
✅ Session state persists across aiki commands  
✅ Examples show progressive specificity and scope reduction  

---

## Built-in Behaviors

The system automatically handles common stuck scenarios:

1. **Comprehensive Feedback First** - Show all issues at once (most efficient)
2. **Auto-Scope Reduction** - Focus on first issue when stuck
3. **Smart Bailout** - Suggest reverting or asking for help after max attempts
4. **Adaptive Strategy** - Adjust behavior based on agent success

Flow authors get these benefits without writing conditional logic.

---

## Expected Timeline

**2 weeks**

---

## Session State Storage

Session state persists across PostResponse iterations:

```
.aiki/.session-state/
├── loop-count.txt              # Total iterations
├── active-checks.json          # JSON array of check IDs that fired
├── last-checks-hash.txt        # Hash of last active checks list
└── same-checks-count.txt       # Stuck counter
```

**Format:** Simple text files, one value per file  
**Lifetime:** Cleared on `aiki session start` or manual session reset  
**Access:** Available to flow engine via `$event` variables

---

## Future Enhancements

These are potential additions after the core system is working:

### 1. Custom Stuck Detection Logic

Allow flow authors to override automatic behavior:

```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors
  - if: $ts_errors > 0
    then:
      autoreply: "Fix TypeScript errors"
  
  # Override automatic stuck behavior with explicit prepend
  - if: $event.same_response_count >= 2
    then:
      autoreply:
        prepend: |
          Custom guidance for this specific flow.
          Try this alternative approach...
```

**Benefit:** Flow-specific stuck handling for edge cases  
**Complexity:** Low (just expose variables to flows)

### 2. Per-Check Loop Tracking

Track stuck state per individual check, not just overall response:

```yaml
PostResponse:
  - if: $typescript_errors.stuck_count >= 3
    then:
      autoreply: "Skip TypeScript for now, focus on linting"
```

**Benefit:** More granular detection of problematic checks  
**Complexity:** Moderate (track multiple counters)

### 3. User-Configurable Limits

Allow setting limits in `.aiki/config.toml`:

```toml
[flow.post_response]
max_iterations = 5
stuck_threshold = 2
```

**Benefit:** Workspace-wide defaults  
**Complexity:** Low (config schema changes)

---

## Success Metrics

### Quantitative

- **Iteration reduction:** Average loop_count decreases for complex tasks
- **Success rate:** % of tasks that complete within max_iterations
- **Stuck detection accuracy:** False positive/negative rate for is_stuck()

### Qualitative

- **Flow author feedback:** Can they express validation logic clearly?
- **Agent behavior:** Does progressive guidance help stuck agents?
- **User satisfaction:** Fewer complaints about infinite loops

---

## Risk Mitigation

### Risk 1: Response Hashing False Positives

**Risk:** Hash collisions or minor formatting changes trigger false "stuck" detection

**Mitigation:**
- Use crypto-quality hash (SHA-256)
- Normalize whitespace before hashing
- Sort error messages for consistent ordering
- Test with real-world error message variations

### Risk 2: Session State Corruption

**Risk:** `.aiki/.session-state/` files get out of sync or corrupted

**Mitigation:**
- Atomic file writes (write to temp, then rename)
- Validate on read (ignore corrupted files)
- Auto-reset on validation failure
- Clear documentation on manual cleanup

### Risk 3: Agent Confusion with Concatenation

**Risk:** Too many issues at once overwhelms agent

**Mitigation:**
- Flow authors can use adaptive patterns to reduce scope
- Stuck detection auto-identifies problematic scenarios
- Documentation shows scope reduction patterns
- Examples demonstrate progressive guidance

---

## Decision Log

### Why Concatenation Strategy?

1. **Matches human workflow** - Comprehensive feedback like code review
2. **Fewer iterations** - More efficient when it works
3. **Better for simple tasks** - Most tasks have few, independent issues
4. **Enables composition** - Multiple included flows can all contribute

### Why Add Stuck Detection?

1. **Prevents worst case** - Cascading errors with no escape
2. **Best of both worlds** - Efficiency + safety
3. **Enables smart patterns** - Progressive guidance, auto-scope reduction
4. **Worth the complexity** - Marginal cost, significant benefit

### Why Not Short-Circuit?

1. **Can add later** - Easy future enhancement
2. **Most flows don't need it** - Concatenation works for 90% of cases
3. **Flow authors can simulate it** - Use conditional logic if needed
4. **Keep it simple** - Reduce initial implementation scope

---

## Next Steps

1. **Review this document**
2. **Create GitHub issue** for PostResponse implementation
3. **Start implementation** - Begin with core engine changes
4. **Iterate based on testing** - Adjust patterns as needed

---

## References

- [response-strategy-comparison.md](./response-strategy-comparison.md) - Full analysis of concatenate vs. short-circuit
- [the-aiki-way.md](../the-aiki-way.md) - Phase 8 includes PostResponse vision
- [ROADMAP.md](../ROADMAP.md) - Strategic context for flows system
