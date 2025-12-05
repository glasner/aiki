# HookResponse Field Examples from cli/src/handlers.rs

This document shows actual examples of `user_message`, `agent_message`, and `metadata` values returned by the handler functions.

## Field Purposes

| Field | Audience | Purpose |
|-------|----------|---------|
| `user_message` | Human user (IDE UI) | User-facing feedback: success messages, warnings, errors |
| `agent_message` | AI agent | Technical details, error context, explanations |
| `metadata` | System (ACP proxy, hooks) | Data to be processed programmatically |

---

## SessionStart Handler Examples

### Success Case
```rust
HookResponse::success().with_metadata(vec![
    ("session_initialized".to_string(), "true".to_string()),
    ("aiki_version".to_string(), "0.1.0".to_string()),
])
```
- **user_message**: `None`
- **agent_message**: `None`
- **metadata**: `[("session_initialized", "true"), ("aiki_version", "0.1.0")]`

### Failed with Warnings
```rust
HookResponse::success_with_message("⚠️ Session started with warnings")
    .with_agent_message("Some initialization actions failed: Could not find .jj directory")
```
- **user_message**: `"⚠️ Session started with warnings"`
- **agent_message**: `"Some initialization actions failed: Could not find .jj directory"`
- **metadata**: `[]`

### Blocking Failure
```rust
HookResponse::blocking_failure(
    "❌ Failed to initialize session: Repository not found",
    Some("Please run 'aiki init' or 'aiki doctor' to fix setup.".to_string()),
)
```
- **user_message**: `"❌ Failed to initialize session: Repository not found"`
- **agent_message**: `"Please run 'aiki init' or 'aiki doctor' to fix setup."`
- **metadata**: `[]`
- **exit_code**: `Some(2)` (blocks operation)

---

## PrePrompt Handler Examples

### Success Case
```rust
HookResponse::success()
    .with_metadata(vec![("modified_prompt".to_string(), final_prompt)])
```
- **user_message**: `None`
- **agent_message**: `None`
- **metadata**: `[("modified_prompt", "User's prompt with injected context...")]`

### Failure Case (Graceful Degradation)
```rust
HookResponse::success()
    .with_metadata(vec![("modified_prompt".to_string(), original_prompt)])
// Note: Errors are logged to stderr, not returned in HookResponse
```
- **user_message**: `None`
- **agent_message**: `None`
- **metadata**: `[("modified_prompt", "Original prompt unchanged")]`
- **Note**: Failures gracefully fall back to original prompt

---

## PostFileChange Handler Examples

### Success Case
```rust
HookResponse::success_with_message(
    "✅ Provenance recorded for 3 files"
)
```
- **user_message**: `"✅ Provenance recorded for 3 files"`
- **agent_message**: `None`
- **metadata**: `[]`

### Partial Failure
```rust
HookResponse::success_with_message(
    "⚠️ Provenance partially recorded for 5 files"
)
.with_agent_message("Some actions failed: JJ command timed out")
```
- **user_message**: `"⚠️ Provenance partially recorded for 5 files"`
- **agent_message**: `"Some actions failed: JJ command timed out"`
- **metadata**: `[]`

### Blocking Failure
```rust
HookResponse::failure(
    "⚠️ Provenance recording blocked: Not in a JJ repository",
    Some("Changes saved but provenance tracking failed. Please check your JJ setup.".to_string()),
)
```
- **user_message**: `"⚠️ Provenance recording blocked: Not in a JJ repository"`
- **agent_message**: `"Changes saved but provenance tracking failed. Please check your JJ setup."`
- **metadata**: `[]`
- **exit_code**: `Some(0)` (non-blocking despite failure)

---

## PostResponse Handler Examples

### Success with Autoreply
```rust
HookResponse::success()
    .with_metadata(vec![("autoreply".to_string(), autoreply)])
```
- **user_message**: `None`
- **agent_message**: `None`
- **metadata**: `[("autoreply", "Please run the tests again with verbose output")]`

### Success without Autoreply
```rust
HookResponse::success()
```
- **user_message**: `None`
- **agent_message**: `None`
- **metadata**: `[]`

### Failure Case (Graceful Degradation)
```rust
HookResponse::success()
// Note: Errors are logged to stderr, no autoreply generated
```
- **user_message**: `None`
- **agent_message**: `None`
- **metadata**: `[]`
- **Note**: All failures result in no autoreply

---

## PrepareCommitMessage Handler Examples

### Success Case
```rust
HookResponse::success_with_message("✅ Co-authors added")
    .with_metadata(vec![
        ("aiki_version".to_string(), "0.1.0".to_string()),
        ("flow".to_string(), "aiki/core".to_string()),
    ])
```
- **user_message**: `"✅ Co-authors added"`
- **agent_message**: `None`
- **metadata**: `[("aiki_version", "0.1.0"), ("flow", "aiki/core")]`

### Partial Failure
```rust
HookResponse::success_with_message("⚠️ Co-authors partially added")
    .with_agent_message("Some actions failed: Could not parse author from JJ log")
```
- **user_message**: `"⚠️ Co-authors partially added"`
- **agent_message**: `"Some actions failed: Could not parse author from JJ log"`
- **metadata**: `[]`

### Blocking Failure
```rust
HookResponse::blocking_failure(
    "❌ Commit blocked: No commits found in JJ log",
    Some("Fix the error and try committing again.".to_string()),
)
```
- **user_message**: `"❌ Commit blocked: No commits found in JJ log"`
- **agent_message**: `"Fix the error and try committing again."`
- **metadata**: `[]`
- **exit_code**: `Some(2)` (blocks commit)

---

## Key Patterns

### user_message Patterns
- ✅ Success: `"✅ Operation succeeded"`
- ⚠️ Warning: `"⚠️ Operation completed with warnings"`
- ❌ Error: `"❌ Operation failed: reason"`

### agent_message Patterns
- Technical details: `"Some actions failed: JJ command timed out"`
- Instructions: `"Please run 'aiki init' or 'aiki doctor' to fix setup."`
- Context: `"Changes saved but provenance tracking failed. Please check your JJ setup."`

### metadata Patterns
- Prompt modification: `[("modified_prompt", "...")]`
- Autoreply: `[("autoreply", "...")]`
- Version info: `[("aiki_version", "0.1.0")]`
- Status flags: `[("session_initialized", "true")]`

---

## Current Gaps in ACP Proxy

Looking at `cli/src/commands/acp.rs`:

### PrePrompt Handler
- ✅ Extracts `metadata["modified_prompt"]`
- ❌ **Ignores** `user_message` (no user feedback on validation)
- ❌ **Ignores** `agent_message` (no agent context injection)
- ❌ **Ignores** `exit_code` (cannot block invalid prompts)

### PostResponse Handler
- ✅ Extracts `metadata["autoreply"]`
- ❌ **Ignores** `user_message` (no user feedback on validation)
- ❌ **Ignores** `agent_message` (no context explanation)
- ❌ **Ignores** `exit_code` (no failure signaling)

This means flows cannot:
- Show validation results to users in ACP mode
- Inject technical context for agents
- Block operations based on validation rules
