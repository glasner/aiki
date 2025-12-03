# Milestone 1.2: PostResponse Event

**Status**: 🔴 Not Started  
**Priority**: High (blocks task-based validation workflows)  
**Complexity**: Medium  
**Timeline**: 1 week

## Overview

The PostResponse event fires after the agent completes its response, allowing flows to validate output, detect errors, and decide whether to send an autoreply.

**Key Decision:** Use the shared MessageChunk/MessageAssembler infrastructure for building autoreply content.

**Syntax:** See [Shared Syntax Pattern](./milestone-1.md#shared-syntax-pattern) in milestone-1.md for the `autoreply:` action syntax (short form and explicit form).

---

## Core Features

### 1. Event Timing

PostResponse fires after the agent finishes generating its response but before showing it to the user:

```
User sends prompt
    ↓
Agent generates response
    ↓
PostResponse event fires  ← Flow can validate/react
    ↓
Response shown to user (potentially with autoreply prepended/appended)
```

### 2. Autoreply Action

The `autoreply:` action sends an additional message to the agent:

```yaml
PostResponse:
  # Short form - defaults to append
  - let: errors = self.count_typescript_errors
  - if: $errors > 0
    then:
      autoreply: "Fix the TypeScript errors above before continuing."

  # Explicit form - with prepend and append
  - let: warnings = self.count_warnings
  - if: $warnings > 0
    then:
      autoreply:
        prepend: "⚠️ Warnings detected:"
        append: "Address these warnings when you have time."
```

**How it works:**
1. Agent completes response
2. PostResponse event fires
3. Flow evaluates conditions (error counts, lint results, etc.)
4. If conditions met, flow adds autoreply chunks
5. PostResponse handler assembles final autoreply via MessageAssembler
6. Autoreply sent to agent as new prompt

### 3. Multiple Autoreply Actions

Can use multiple `autoreply:` actions in the same flow:

```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors
  - if: $ts_errors > 0
    then:
      autoreply: "Fix TypeScript errors first."
  
  - let: test_failures = self.count_test_failures
  - if: $test_failures > 0
    then:
      autoreply: "Tests are failing. Run `npm test` to see details."
  
  - let: build_errors = self.count_build_errors
  - if: $build_errors > 0
    then:
      autoreply: "Build failed. Run `npm run build` to diagnose."
```

**Accumulation order:**
- All autoreply chunks accumulate in order
- Final autoreply assembled via MessageAssembler
- Single autoreply message sent to agent (not multiple messages)

---

## Event Structure

```rust
use crate::flows::{MessageChunk, MessageAssembler};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

pub struct PostResponseEvent {
    pub response: String,                      // Agent's original response (immutable)
    pub autoreply_assembler: MessageAssembler, // Owns MessageAssembler for building autoreply
    pub session_id: Option<String>,            // Current session
    pub timestamp: DateTime<Utc>,
    pub working_directory: PathBuf,
    pub modified_files: Vec<PathBuf>,          // Files modified by agent
}

impl PostResponseEvent {
    pub fn new(response: String) -> Self {
        Self {
            response,
            autoreply_assembler: MessageAssembler::new(None, "\n\n"),
            session_id: None,
            timestamp: Utc::now(),
            working_directory: std::env::current_dir().unwrap(),
            modified_files: Vec::new(),
        }
    }
    
    pub fn add_autoreply(&mut self, chunk: MessageChunk) {
        self.autoreply_assembler.add_chunk(chunk);
    }
    
    pub fn build_autoreply(&self) -> String {
        self.autoreply_assembler.build()
    }
    
    pub fn has_autoreply(&self) -> bool {
        !self.autoreply_assembler.build().is_empty()
    }
}
```

**Event lifecycle:**
1. Agent completes response
2. `PostResponseEvent` created with response and empty `MessageAssembler`
3. Flow engine executes PostResponse flow
4. `autoreply:` actions parsed as MessageChunk, added to `autoreply_assembler`
5. Handler calls `build_autoreply()` to assemble final message
6. If non-empty, autoreply sent to agent

---

## Use Cases

### Use Case 1: Error Detection

```yaml
PostResponse:
  - let: rust_errors = self.count_rust_errors
  - if: $rust_errors > 0
    then:
      autoreply:
        prepend: "🚨 Build failed with errors:"
        append: "Fix these errors before continuing. Run `cargo check` again after fixing."
```

**Result:** Agent receives structured feedback about build errors

### Use Case 2: Lint Warnings

```yaml
PostResponse:
  - let: eslint_warnings = self.count_eslint_warnings
  - if: $eslint_warnings > 0 && $eslint_warnings < 10
    then:
      autoreply: "ESLint found $eslint_warnings warnings. Consider fixing them."
  
  - if: $eslint_warnings >= 10
    then:
      autoreply:
        prepend: "⚠️ Too many ESLint warnings ($eslint_warnings)"
        append: "Run `npm run lint:fix` to auto-fix, or disable rules in .eslintrc if intentional."
```

**Result:** Graduated response based on warning severity

### Use Case 3: Test Validation

```yaml
PostResponse:
  - let: test_results = self.run_tests
  - if: $test_results.failed > 0
    then:
      autoreply: |
        Tests failed:
        - Passed: $test_results.passed
        - Failed: $test_results.failed
        
        Run `npm test` to see details and fix failing tests.
```

**Result:** Agent receives test results and actionable next steps

### Use Case 4: Security Checks

```yaml
PostResponse:
  - let: secrets = self.detect_secrets_in_response
  - if: $secrets | length > 0
    then:
      autoreply:
        prepend: "🔒 SECURITY ALERT: Detected potential secrets in your response!"
        append: |
          Remove these before committing:
          $secrets
          
          Use environment variables or secret management instead.
```

**Result:** Immediate feedback on security issues

---

## Flow Context Variables

The PostResponse event provides these helper functions:

```rust
impl PostResponseEvent {
    // Error detection
    pub fn count_typescript_errors(&self) -> usize { /* ... */ }
    pub fn count_rust_errors(&self) -> usize { /* ... */ }
    pub fn count_build_errors(&self) -> usize { /* ... */ }
    
    // Lint/warning detection
    pub fn count_eslint_warnings(&self) -> usize { /* ... */ }
    pub fn count_clippy_warnings(&self) -> usize { /* ... */ }
    
    // Test results
    pub fn run_tests(&self) -> TestResults { /* ... */ }
    
    // Security
    pub fn detect_secrets_in_response(&self) -> Vec<SecretMatch> { /* ... */ }
    
    // File validation
    pub fn get_modified_files(&self) -> Vec<PathBuf> { /* ... */ }
}
```

These are available in flows as:
```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors
  - let: warnings = self.count_eslint_warnings
  - let: test_results = self.run_tests
```

---

## Implementation Tasks

**Pre-requisite:** Complete Milestone 1.0 (MessageChunk/MessageAssembler) and Milestone 1.1 (PrePrompt)

### Core Engine

- [ ] Add `PostResponseEvent` struct to `cli/src/events.rs` (owns `MessageAssembler`)
- [ ] Add `autoreply:` action to flow DSL (parses as `MessageChunk`)
- [ ] Implement `cli/src/flows/actions/autoreply.rs` that creates MessageChunk from YAML
- [ ] Add PostResponse handler: `cli/src/flows/handlers/post_response.rs` (calls `event.add_autoreply(chunk)`)
- [ ] Implement error handling (graceful degradation: skip autoreply on error)
- [ ] Hook into vendor response lifecycle

### Vendor Integration

- [ ] `cli/src/vendors/claude_code.rs` - Hook after response generated
- [ ] `cli/src/vendors/cursor.rs` - Hook after response generated
- [ ] `cli/src/vendors/acp.rs` - Hook after response generated
- [ ] Ensure autoreply sent to agent if non-empty
- [ ] Preserve session ID across event

### Helper Functions

- [ ] Implement `count_typescript_errors()` - Parse `tsc` output (with timeout/error handling)
- [ ] Implement `count_rust_errors()` - Parse `cargo check` output (with timeout/error handling)
- [ ] Implement `count_build_errors()` - Generic build error detection
- [ ] Implement `count_eslint_warnings()` - Parse ESLint output
- [ ] Implement `run_tests()` - Execute test suite, parse results (with timeout)
- [ ] Implement `detect_secrets_in_response()` - Regex/heuristic secret detection
- [ ] Add timeout limits and error handling to all external commands

### Testing

- [ ] Unit tests: MessageChunk parsing from YAML `autoreply:` action
- [ ] Unit tests: PostResponseEvent owns MessageAssembler correctly
- [ ] Unit tests: Multiple autoreply chunks accumulate correctly
- [ ] Unit tests: Helper functions (error counting, test parsing)
- [ ] Unit tests: Error handling (helper function crashes, timeouts)
- [ ] Integration tests: PostResponse flow execution
- [ ] Integration tests: Multiple `autoreply:` actions
- [ ] Integration tests: Conditional autoreplies
- [ ] Integration tests: Flow error triggers graceful degradation
- [ ] E2E tests: Real agent receives autoreply
- [ ] E2E tests: Error detection triggers autoreply
- [ ] E2E tests: Flow error shows warning but doesn't block response

### Documentation

- [ ] Tutorial: "Validating Agent Output with PostResponse"
- [ ] Cookbook: Common patterns (errors, warnings, tests, security)
- [ ] Reference: PostResponse DSL syntax
- [ ] Reference: Helper function API
- [ ] Examples: Real-world PostResponse flows

---

## Success Criteria

✅ PostResponse fires after agent completes response (all vendor integrations)  
✅ Short form `autoreply: "string"` works  
✅ Explicit form `autoreply: { prepend: [...], append: [...] }` works  
✅ MessageChunk correctly parses both forms from YAML  
✅ PostResponseEvent owns MessageAssembler instance  
✅ Multiple autoreply chunks accumulate correctly  
✅ Autoreply sent to agent only if non-empty  
✅ Helper functions work correctly (error counting, test parsing)  
✅ Conditional autoreplies based on validation results  
✅ Session ID is captured and available  
✅ Flow errors trigger graceful degradation (response shown, no autoreply)  
✅ User sees warning when flow fails, but response still delivered  
✅ Helper function timeouts and crashes handled gracefully  

---

## Error Handling

**Strategy: Graceful Degradation**

If a PostResponse flow fails, the agent's response is still shown to the user. The autoreply is skipped, but the workflow continues.

### Common Error Scenarios

#### 1. Helper Function Crash

```yaml
PostResponse:
  - let: ts_errors = self.count_typescript_errors  # tsc not found or crashes
  - if: $ts_errors > 0
    then:
      autoreply: "Fix TypeScript errors"
```

**User sees:**
```
[Agent's response shown normally]

⚠️ PostResponse flow failed: Helper function 'count_typescript_errors' crashed: tsc command not found
No autoreply generated.
```

#### 2. Helper Function Timeout

```yaml
PostResponse:
  - let: test_results = self.run_tests  # Tests run for >30s
  - if: $test_results.failed > 0
    then:
      autoreply: "Tests failed"
```

**User sees:**
```
[Agent's response shown normally]

⚠️ PostResponse flow failed: Helper function 'run_tests' timed out after 30s
No autoreply generated.
```

#### 3. Invalid YAML Syntax

```yaml
PostResponse:
  - autoreply:
      prepend:
        - "Text"
        - 123  # Invalid: should be string
```

**User sees:**
```
[Agent's response shown normally]

⚠️ PostResponse flow failed: Invalid YAML: expected string, got number
No autoreply generated.
```

#### 4. MessageChunk Validation Error

```yaml
PostResponse:
  - autoreply: {}  # Invalid: empty object
```

**User sees:**
```
[Agent's response shown normally]

⚠️ PostResponse flow failed: MessageChunk must have at least prepend or append
No autoreply generated.
```

### Implementation Pattern

```rust
impl PostResponseHandler {
    pub fn execute(&self, event: &mut PostResponseEvent, flow: &Flow) -> Result<()> {
        match self.execute_flow(event, flow) {
            Ok(()) => {
                // Flow succeeded - use autoreply if any
                Ok(())
            }
            Err(e) => {
                // Flow failed - log error, show warning, skip autoreply
                eprintln!("\n⚠️ PostResponse flow failed: {}", e);
                eprintln!("No autoreply generated.\n");
                
                // Clear any partial autoreply content
                event.autoreply_assembler = MessageAssembler::new(None, "\n\n");
                
                // Don't propagate error - graceful degradation
                Ok(())
            }
        }
    }
}
```

**Key points:**
- Error is logged and shown to user
- Agent's response is still delivered to user
- No autoreply is sent (avoids partial/corrupted autoreplies)
- User can continue working without interruption

### Helper Function Error Handling

Helper functions should handle errors internally and return safe defaults:

```rust
impl PostResponseEvent {
    pub fn count_typescript_errors(&self) -> Result<usize> {
        let output = Command::new("tsc")
            .arg("--noEmit")
            .timeout(Duration::from_secs(30))  // 30s timeout
            .output()?;
        
        if !output.status.success() {
            // Parse error count from stderr
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(parse_tsc_error_count(&stderr))
        } else {
            Ok(0)  // No errors
        }
    }
}
```

**Error handling at call site:**
```rust
// In flow execution
let ts_errors = event.count_typescript_errors()
    .unwrap_or_else(|e| {
        eprintln!("Warning: count_typescript_errors failed: {}", e);
        0  // Safe default: assume no errors
    });
```

### Future: Strict Mode (Not in Milestone 1.2)

For teams that want validation gates:

```yaml
# .aiki/config.toml
[flows]
error_handling = "strict"
```

**Strict mode behavior:**
- PostResponse error → Don't show agent response, show error to user
- User must fix flow before seeing response

**Use case:** Teams that require validation to always run (e.g., security checks must complete before accepting response).

---

## Example: Complete Error Validation Flow

```yaml
PostResponse:
  # Check TypeScript errors
  - let: ts_errors = self.count_typescript_errors
  - if: $ts_errors > 0
    then:
      autoreply:
        prepend: "🚨 TypeScript compilation failed with $ts_errors errors"
        append: "Run `npm run type-check` to see full error details."
  
  # Check ESLint warnings
  - let: eslint_warnings = self.count_eslint_warnings
  - if: $eslint_warnings > 0
    then:
      autoreply: "⚠️ ESLint found $eslint_warnings warnings. Consider fixing them."
  
  # Run tests
  - let: test_results = self.run_tests
  - if: $test_results.failed > 0
    then:
      autoreply: |
        ❌ Tests failed:
        - Passed: $test_results.passed
        - Failed: $test_results.failed
        
        Fix failing tests before committing.
  
  # Security check
  - let: secrets = self.detect_secrets_in_response
  - if: $secrets | length > 0
    then:
      autoreply:
        prepend: "🔒 SECURITY: Detected potential secrets!"
        append: "Remove secrets and use environment variables instead."
```

**Agent sees:**
```
🚨 TypeScript compilation failed with 3 errors

Run `npm run type-check` to see full error details.

⚠️ ESLint found 5 warnings. Consider fixing them.

❌ Tests failed:
- Passed: 42
- Failed: 2

Fix failing tests before committing.
```

---

## Technical Components

### File Structure

```
cli/src/
├── events.rs                          # PostResponseEvent struct (owns MessageAssembler)
├── flows/
│   ├── messages.rs                    # MessageChunk + MessageAssembler (see milestone-1.0)
│   ├── actions/
│   │   └── autoreply.rs               # Parses YAML to MessageChunk
│   ├── handlers/
│   │   └── post_response.rs           # PostResponse handler
│   ├── functions/                     # Helper functions
│   │   ├── count_typescript_errors.rs
│   │   ├── count_rust_errors.rs
│   │   ├── run_tests.rs
│   │   └── mod.rs
│   └── engine.rs                      # Flow execution
└── vendors/
    ├── claude_code.rs                 # Claude Code integration
    ├── cursor.rs                      # Cursor integration
    └── acp.rs                         # ACP integration
```

### Key Functions

```rust
// cli/src/flows/actions/autoreply.rs
use crate::flows::MessageChunk;

pub fn execute_autoreply(value: &Value, event: &mut PostResponseEvent) -> Result<()> {
    // Parse YAML into MessageChunk
    let chunk: MessageChunk = serde_yaml::from_value(value.clone())?;
    
    // Add chunk to event's assembler
    event.add_autoreply(chunk);
    
    Ok(())
}
```

---

## Expected Timeline

**Week 1**

- Days 1-2: Event structure, MessageAssembler integration
- Days 3-4: Vendor hooks, flow execution, helper functions
- Day 5: Testing and documentation

---

## Future Enhancements (Not in This Milestone)

### 1. Task-Based Workflows

Instead of text autoreplies, create structured tasks. **This is Milestone 1.3**.

```yaml
PostResponse:
  - let: errors = self.count_typescript_errors
  - for: error in $errors
    then:
      task.create:
        objective: "Fix: $error.message"
        file: $error.file
        line: $error.line
```

### 2. Smart Autoreply Throttling

Only send autoreply if agent hasn't seen similar error recently:

```yaml
PostResponse:
  - let: errors = self.count_rust_errors
  - if: $errors > 0 && !self.recently_notified_about("rust_errors")
    then:
      autoreply: "Fix Rust errors first."
```

### 3. Autoreply Templates

Reusable templates for common scenarios:

```yaml
PostResponse:
  - if: self.has_build_errors
    then:
      autoreply.template: "build-errors"  # Load from .aiki/templates/
```

---

## Relationship to Other Milestones

- **Depends on:** Milestone 1.0 (MessageChunk/MessageAssembler), Milestone 1.1 (PrePrompt pattern)
- **Enables:** Milestone 1.3 (Task System) - provides event hook for task creation
- **Parallel to:** Milestone 1.4 (Flow Composition) - can be developed independently

---

## References

- [milestone-1.md](./milestone-1.md) - Milestone 1 overview and shared syntax
- [milestone-1.0-message-assembler.md](./milestone-1.0-message-assembler.md) - MessageChunk/MessageAssembler infrastructure
- [milestone-1.1-preprompt.md](./milestone-1.1-preprompt.md) - Similar event pattern
- [milestone-1.3-task-system.md](./milestone-1.3-task-system.md) - Task-based alternative to text autoreplies
- [ROADMAP.md](../ROADMAP.md) - Strategic context
