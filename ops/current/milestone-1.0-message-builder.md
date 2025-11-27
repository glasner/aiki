# Milestone 1.0: MessageBuilder Shared Syntax

**Status**: 🔴 Not Started  
**Priority**: Critical (blocks 1.1, 1.2, and PrepareCommitMessage)  
**Complexity**: Low  
**Timeline**: 3-7 days

## Overview

Implement the shared MessageBuilder parser infrastructure that provides consistent syntax and behavior across message-building events:
- **PrePrompt** (`prompt:` action) - New in Milestone 1.1
- **PrepareCommitMessage** (`commit_message:` action) - Refactored from existing implementation

This milestone includes **refactoring the existing PrepareCommitMessage hook** to use the new MessageBuilder infrastructure, ensuring consistent syntax across all message-building events.

**Note:** PostResponse uses a task-based system instead of text-based autoreplies. See [milestone-1.2-post-response-and-tasks.md](./milestone-1.2-post-response-and-tasks.md) for details.

## Why This Comes First

PrePrompt and PrepareCommitMessage both need to:
1. Parse both short form (`action: "string"`) and explicit form (`action: { prepend: [...], append: [...] }`)
2. Handle multiple invocations (append to growing message)
3. Provide consistent error messages for malformed syntax

By creating shared infrastructure, we ensure:
- **Consistency**: Both events behave the same way
- **Maintainability**: Bug fixes apply everywhere
- **Testability**: Comprehensive tests in one place
- **Documentation**: Single source of truth for syntax
- **Refactoring benefit**: Existing PrepareCommitMessage hook gets cleaner, more maintainable code

## The Unified Syntax Pattern

### YAML Syntax Forms

The MessageBuilder accepts multiple YAML syntax forms:

```yaml
# 1. Block scalar with |
prepend: |
  line1
  line2
  
# 2. Array syntax
prepend:
  - "line1"
  - "line2"
  
# 3. Inline string with \n
prepend: "line1\nline2"
```

**Important differences:**
- **Form 1 (block scalar)**: Produces a single string `"line1\nline2"` (trailing newline stripped)
- **Form 2 (array)**: Produces two separate strings `["line1", "line2"]`
- **Form 3 (inline)**: Produces a single string `"line1\nline2"`

**Output behavior**: Forms 1 and 3 produce identical output when rendered. Form 2 produces the same output when joined with `\n`.

**Check ID behavior**: Forms 1 and 3 produce identical check IDs. Form 2 produces a different check ID because the YAML structure differs. This is intentional - changing YAML structure is considered a flow edit for stuck detection purposes.

### Explicit Form (Prepend and/or Append)

```yaml
# PrePrompt with prepend
PrePrompt:
  prompt:
    prepend: |
      🚨 CRITICAL: Read these files first:
        - ARCHITECTURE.md
        - SECURITY.md
    append: "After reading, confirm you understand the constraints."

# PostResponse with append
PostResponse:
  - let: rust_errors = self.count_rust_errors
  - if: $rust_errors > 0
    then:
      autoreply:
        append: "Fix the Rust errors above before continuing."

# PostResponse with both prepend and append
PostResponse:
  - let: rust_errors = self.count_rust_errors
  - if: $rust_errors > 0
    then:
      autoreply:
        prepend: "Before continuing, fix these Rust errors:"
        append: "Run `cargo check` again after fixing."

# PrepareCommitMessage with body and trailers
PrepareCommitMessage:
  - commit_message:
      body:
        append:
          - "Additional context about this change"
      trailers:
        append:
          - "Co-authored-by: AI Assistant <ai@example.com>"
          - "Ticket: PROJ-1234"

# PrepareCommitMessage with just trailers (most common)
PrepareCommitMessage:
  - commit_message:
      trailers:
        append: "Co-authored-by: AI Assistant <ai@example.com>"

# PrepareCommitMessage shortcuts - append/prepend at commit_message level routes to body
PrepareCommitMessage:
  - commit_message:
      append: "Additional context"  # Shortcut for body.append
      
  - commit_message:
      prepend: "Breaking change notice"  # Shortcut for body.prepend
```

**Note**: PrepareCommitMessage uses `commit_message:` action with `body:` and `trailers:` sub-fields:
- Each sub-field uses MessageBuilder (supports `prepend:` and `append:`)
- Body section goes after original message, before trailers (joined with double newline)
- Trailers section joined with single newline (not double)
- **Shortcut**: `append:`/`prepend:` at `commit_message:` level automatically routes to `body.append`/`body.prepend`

## Implementation: MessageBuilder Struct

### Location

```
cli/src/flows/actions/message_builder.rs
```

### Core Structure

```rust
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// String or array of strings for flexible YAML input
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray {
    Single(String),
    Multiple(Vec<String>),
}

impl StringOrArray {
    fn to_vec(self) -> Vec<String> {
        match self {
            StringOrArray::Single(s) => {
                // Normalize: strip trailing newline from YAML block scalars (|)
                // This ensures `prepend: |` and `prepend: ["line"]` behave identically
                vec![s.trim_end_matches('\n').to_string()]
            }
            StringOrArray::Multiple(v) => {
                // Normalize each item in the array
                v.into_iter()
                    .map(|s| s.trim_end_matches('\n').to_string())
                    .collect()
            }
        }
    }
}

/// Unified message builder for prompt/autoreply/commit_message actions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageBuilder {
    #[serde(default)]
    pub prepend: Option<StringOrArray>,
    
    #[serde(default)]
    pub append: Option<StringOrArray>,
}

impl MessageBuilder {
    /// Generate content-based check ID from YAML representation
    /// This is deterministic and immune to runtime variable substitution
    pub fn check_id(&self) -> String {
        let yaml = serde_yaml::to_string(self)
            .expect("MessageBuilder should always serialize");
        
        let mut hasher = DefaultHasher::new();
        yaml.hash(&mut hasher);
        let hash = hasher.finish();
        
        // Return first 8 hex chars for readability
        format!("{:08x}", hash)
    }
    
    /// Get prepend items
    pub fn prepend_items(&self) -> Vec<String> {
        self.prepend.clone().map(|p| p.to_vec()).unwrap_or_default()
    }
    
    /// Get append items
    pub fn append_items(&self) -> Vec<String> {
        self.append.clone().map(|a| a.to_vec()).unwrap_or_default()
    }
}
```

## Integration with Events

**Design Decision: Collect-Then-Apply Pattern**

Instead of applying each MessageBuilder immediately (which would cause prepends to stack in reverse order), events collect all MessageBuilders first, then apply them all at once when building the final message. This ensures:

1. **Intuitive ordering**: Flow A fires first → Flow A's content appears first
2. **Correct prepend order**: All prepends appear in the order flows fired
3. **Correct append order**: All appends appear in the order flows fired
4. **Single assembly point**: `build_prompt()` / `build_reply()` / `build_message()` methods

Each event implementation follows this pattern:

### PrePrompt Event

```rust
// cli/src/flows/events/preprompt.rs
use crate::flows::actions::message_builder::MessageBuilder;

pub struct PrePromptEvent {
    original_prompt: String,
    builders: Vec<MessageBuilder>,
}

impl PrePromptEvent {
    pub fn new(prompt: String) -> Self {
        Self {
            original_prompt: prompt,
            builders: Vec::new(),
        }
    }
    
    pub fn apply_prompt_action(&mut self, builder: MessageBuilder) {
        self.builders.push(builder);
    }
    
    pub fn build_prompt(&self) -> String {
        let mut prepends = Vec::new();
        let mut appends = Vec::new();
        
        // Collect all prepends and appends in order
        for builder in &self.builders {
            prepends.extend(builder.prepend_items());
            appends.extend(builder.append_items());
        }
        
        // Build final message: [prepends] + [original] + [appends]
        let mut parts = Vec::new();
        
        if !prepends.is_empty() {
            parts.push(prepends.join("\n"));
        }
        
        if !self.original_prompt.is_empty() {
            parts.push(self.original_prompt.clone());
        }
        
        if !appends.is_empty() {
            parts.push(appends.join("\n"));
        }
        
        parts.join("\n\n")
    }
}
```

### PostResponse Event

```rust
// cli/src/flows/events/postresponse.rs
use crate::flows::actions::message_builder::MessageBuilder;

pub struct PostResponseEvent {
    builders: Vec<MessageBuilder>,
}

impl PostResponseEvent {
    pub fn new() -> Self {
        Self {
            builders: Vec::new(),
        }
    }
    
    pub fn add_autoreply(&mut self, builder: MessageBuilder) {
        self.builders.push(builder);
    }
    
    pub fn build_reply(&self) -> String {
        let mut prepends = Vec::new();
        let mut appends = Vec::new();
        
        // Collect all prepends and appends in order
        for builder in &self.builders {
            prepends.extend(builder.prepend_items());
            appends.extend(builder.append_items());
        }
        
        // Build final message: [prepends] + [appends]
        // (No original message for autoreplies)
        let mut parts = Vec::new();
        
        if !prepends.is_empty() {
            parts.push(prepends.join("\n"));
        }
        
        if !appends.is_empty() {
            parts.push(appends.join("\n"));
        }
        
        parts.join("\n\n")
    }
}
```

### PrepareCommitMessage Event

```rust
// cli/src/flows/events/prepare_commit_message.rs
use crate::flows::actions::message_builder::MessageBuilder;

pub struct PrepareCommitMessageEvent {
    original_message: String,
    body_builders: Vec<MessageBuilder>,
    trailer_builders: Vec<MessageBuilder>,
}

impl PrepareCommitMessageEvent {
    pub fn new(message: String) -> Self {
        Self {
            original_message: message,
            body_builders: Vec::new(),
            trailer_builders: Vec::new(),
        }
    }
    
    pub fn apply_body_action(&mut self, builder: MessageBuilder) {
        self.body_builders.push(builder);
    }
    
    pub fn apply_trailers_action(&mut self, builder: MessageBuilder) {
        self.trailer_builders.push(builder);
    }
    
    pub fn build_message(&self) -> String {
        let mut parts = Vec::new();
        
        // Original commit message
        if !self.original_message.is_empty() {
            parts.push(self.original_message.clone());
        }
        
        // Body section: collect prepends and appends from all body builders
        let mut body_prepends = Vec::new();
        let mut body_appends = Vec::new();
        for builder in &self.body_builders {
            body_prepends.extend(builder.prepend_items());
            body_appends.extend(builder.append_items());
        }
        
        let mut body_parts = Vec::new();
        body_parts.extend(body_prepends);
        body_parts.extend(body_appends);
        if !body_parts.is_empty() {
            parts.push(body_parts.join("\n"));
        }
        
        // Trailers section: collect prepends and appends from all trailer builders
        // Special formatting: joined with single newline (not double)
        let mut trailer_prepends = Vec::new();
        let mut trailer_appends = Vec::new();
        for builder in &self.trailer_builders {
            trailer_prepends.extend(builder.prepend_items());
            trailer_appends.extend(builder.append_items());
        }
        
        let mut trailer_parts = Vec::new();
        trailer_parts.extend(trailer_prepends);
        trailer_parts.extend(trailer_appends);
        if !trailer_parts.is_empty() {
            let trailer_block = trailer_parts.join("\n"); // Single newline!
            parts.push(trailer_block);
        }
        
        // Join sections with double newline
        parts.join("\n\n")
    }
}
```

**Flow Parser Integration:**

The flow parser handles the nested `commit_message:` structure by recognizing it as a meta-action that contains sub-actions:

```rust
// When parsing PrepareCommitMessage YAML:
PrepareCommitMessage:
  - commit_message:
      body:
        append: "text"
      trailers:
        append: "Co-authored-by: ..."

// The parser:
// 1. Recognizes "commit_message" as a meta-action
// 2. Parses "body:" as a MessageBuilder and calls event.apply_body_action(builder)
// 3. Parses "trailers:" as a MessageBuilder and calls event.apply_trailers_action(builder)
// 4. Each MessageBuilder can have prepend:/append: fields

// Shortcut syntax - append/prepend at commit_message level routes to body:
PrepareCommitMessage:
  - commit_message:
      append: "Additional context"

// Parses as if user wrote:
PrepareCommitMessage:
  - commit_message:
      body:
        append: "Additional context"

// Implementation note: Parser checks for prepend:/append: at commit_message level first,
// and if found, wraps them in a body: action before dispatching.
```

## Testing Strategy

### Unit Tests

Test the MessageBuilder in isolation:

```rust
// cli/src/flows/actions/message_builder.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_prepend_only() {
        let builder = MessageBuilder {
            prepend: Some(StringOrArray::Single("header".to_string())),
            append: None,
        };
        assert_eq!(builder.prepend_items(), vec!["header"]);
        assert_eq!(builder.append_items(), Vec::<String>::new());
    }
    
    #[test]
    fn test_append_only() {
        let builder = MessageBuilder {
            prepend: None,
            append: Some(StringOrArray::Single("footer".to_string())),
        };
        assert_eq!(builder.prepend_items(), Vec::<String>::new());
        assert_eq!(builder.append_items(), vec!["footer"]);
    }
    
    #[test]
    fn test_both_prepend_and_append() {
        let builder = MessageBuilder {
            prepend: Some(StringOrArray::Single("header".to_string())),
            append: Some(StringOrArray::Single("footer".to_string())),
        };
        assert_eq!(builder.prepend_items(), vec!["header"]);
        assert_eq!(builder.append_items(), vec!["footer"]);
    }
    
    #[test]
    fn test_with_arrays() {
        let builder = MessageBuilder {
            prepend: Some(StringOrArray::Multiple(vec!["line1".to_string(), "line2".to_string()])),
            append: Some(StringOrArray::Multiple(vec!["line3".to_string(), "line4".to_string()])),
        };
        assert_eq!(builder.prepend_items(), vec!["line1", "line2"]);
        assert_eq!(builder.append_items(), vec!["line3", "line4"]);
    }
    
    #[test]
    fn test_check_id_is_deterministic() {
        let builder1 = MessageBuilder {
            prepend: None,
            append: Some(StringOrArray::Single("test".to_string())),
        };
        let builder2 = MessageBuilder {
            prepend: None,
            append: Some(StringOrArray::Single("test".to_string())),
        };
        assert_eq!(builder1.check_id(), builder2.check_id());
    }
    
    #[test]
    fn test_check_id_differs_for_different_content() {
        let builder1 = MessageBuilder {
            prepend: None,
            append: Some(StringOrArray::Single("test1".to_string())),
        };
        let builder2 = MessageBuilder {
            prepend: None,
            append: Some(StringOrArray::Single("test2".to_string())),
        };
        assert_ne!(builder1.check_id(), builder2.check_id());
    }
    
    #[test]
    fn test_yaml_block_scalar_trailing_newline_stripped() {
        // YAML block scalar (|) adds trailing newline - we strip it
        let block_scalar = MessageBuilder {
            prepend: Some(StringOrArray::Single("line1\nline2\n".to_string())),
            append: None,
        };
        
        // After stripping trailing newline, produces single string
        assert_eq!(block_scalar.prepend_items(), vec!["line1\nline2"]);
    }
    
    #[test]
    fn test_yaml_array_form() {
        // Array form produces multiple strings
        let array_form = MessageBuilder {
            prepend: Some(StringOrArray::Multiple(vec!["line1".to_string(), "line2".to_string()])),
            append: None,
        };
        
        assert_eq!(array_form.prepend_items(), vec!["line1", "line2"]);
    }
    
    #[test]
    fn test_yaml_forms_produce_same_output() {
        // Block scalar produces single string with embedded newline
        let block_scalar = MessageBuilder {
            prepend: Some(StringOrArray::Single("line1\nline2\n".to_string())),
            append: None,
        };
        
        // Array produces separate strings
        let array_form = MessageBuilder {
            prepend: Some(StringOrArray::Multiple(vec!["line1".to_string(), "line2".to_string()])),
            append: None,
        };
        
        // When joined, they produce the same output
        let block_output = block_scalar.prepend_items().join("\n");
        let array_output = array_form.prepend_items().join("\n");
        assert_eq!(block_output, array_output);
        
        // But their check IDs differ (different YAML structure)
        assert_ne!(block_scalar.check_id(), array_form.check_id());
    }
}
```

### Integration Tests

Test that events properly use MessageBuilder:

```rust
// cli/tests/preprompt_event.rs

#[test]
fn test_preprompt_with_prepend() {
    let mut event = PrePromptEvent::new("Original prompt".to_string());
    let builder = MessageBuilder {
        prepend: Some(StringOrArray::Single("Read ARCHITECTURE.md".to_string())),
        append: None,
    };
    event.apply_prompt_action(builder);
    
    let result = event.build_prompt();
    assert!(result.contains("Original prompt"));
    assert!(result.contains("Read ARCHITECTURE.md"));
}

#[test]
fn test_preprompt_with_multiple_appends() {
    let mut event = PrePromptEvent::new("Original prompt".to_string());
    
    let builder1 = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("First instruction".to_string())),
    };
    event.apply_prompt_action(builder1);
    
    let builder2 = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Second instruction".to_string())),
    };
    event.apply_prompt_action(builder2);
    
    let result = event.build_prompt();
    
    // Check order: original, then first, then second
    let first_pos = result.find("First instruction").unwrap();
    let second_pos = result.find("Second instruction").unwrap();
    assert!(first_pos < second_pos, "First instruction should appear before second");
}

#[test]
fn test_preprompt_order_with_prepends() {
    let mut event = PrePromptEvent::new("Original prompt".to_string());
    
    // Flow A fires first with prepend
    let builder1 = MessageBuilder {
        prepend: Some(StringOrArray::Single("Rule 1: Check types".to_string())),
        append: None,
    };
    event.apply_prompt_action(builder1);
    
    // Flow B fires second with prepend
    let builder2 = MessageBuilder {
        prepend: Some(StringOrArray::Single("Rule 2: Check security".to_string())),
        append: None,
    };
    event.apply_prompt_action(builder2);
    
    let result = event.build_prompt();
    
    // Check order: Rule 1 should appear before Rule 2 (not reversed!)
    let rule1_pos = result.find("Rule 1").unwrap();
    let rule2_pos = result.find("Rule 2").unwrap();
    let original_pos = result.find("Original prompt").unwrap();
    
    assert!(rule1_pos < rule2_pos, "Rule 1 should appear before Rule 2");
    assert!(rule2_pos < original_pos, "Both rules should appear before original prompt");
}

#[test]
fn test_commit_message_with_body_and_trailers() {
    let mut event = PrepareCommitMessageEvent::new("feat(auth): add JWT validation".to_string());
    
    // Body action
    let body_builder = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Implements token validation with expiry checks.".to_string())),
    };
    event.apply_body_action(body_builder);
    
    // Trailers action
    let trailers_builder = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Multiple(vec![
            "Co-authored-by: AI <ai@example.com>".to_string(),
            "Ticket: PROJ-1234".to_string(),
        ])),
    };
    event.apply_trailers_action(trailers_builder);
    
    let result = event.build_message();
    
    // Should have: original + body + trailers
    assert!(result.contains("feat(auth): add JWT validation"));
    assert!(result.contains("Implements token validation"));
    
    // Should have exactly one blank line before trailers
    assert!(result.contains("expiry checks.\n\nCo-authored-by"));
    
    // Should have single newline between trailers (not double)
    assert!(result.contains("Co-authored-by: AI <ai@example.com>\nTicket: PROJ-1234"));
    
    // Should NOT have double newline between trailers
    assert!(!result.contains("ai@example.com>\n\nTicket"));
}

#[test]
fn test_commit_message_trailers_only() {
    let mut event = PrepareCommitMessageEvent::new("feat: add feature".to_string());
    
    // Trailers action only
    let trailers_builder = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Co-authored-by: AI <ai@example.com>".to_string())),
    };
    event.apply_trailers_action(trailers_builder);
    
    let result = event.build_message();
    
    // Should have trailer formatting (single blank line before)
    assert!(result.contains("feat: add feature\n\nCo-authored-by"));
}

#[test]
fn test_commit_message_body_prepend_and_trailers() {
    let mut event = PrepareCommitMessageEvent::new("feat: add authentication".to_string());
    
    // Body with prepend
    let body_builder = MessageBuilder {
        prepend: Some(StringOrArray::Single("Breaking change: auth required".to_string())),
        append: None,
    };
    event.apply_body_action(body_builder);
    
    // Trailers
    let trailers_builder = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Reviewed-by: Senior Engineer".to_string())),
    };
    event.apply_trailers_action(trailers_builder);
    
    let result = event.build_message();
    
    // Prepend should appear in body section before trailers
    assert!(result.contains("Breaking change: auth required"));
    assert!(result.contains("Reviewed-by: Senior Engineer"));
    
    let breaking_pos = result.find("Breaking change").unwrap();
    let trailer_pos = result.find("Reviewed-by").unwrap();
    assert!(breaking_pos < trailer_pos, "Body should appear before trailers");
}

#[test]
fn test_commit_message_shortcut_routes_to_body() {
    let mut event = PrepareCommitMessageEvent::new("feat: add feature".to_string());
    
    // Using shortcut syntax at commit_message level
    // In practice, parser would convert `commit_message: { append: "text" }` 
    // to `body: { append: "text" }` before calling apply_body_action
    let body_builder = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Additional context".to_string())),
    };
    event.apply_body_action(body_builder);
    
    let result = event.build_message();
    
    // Should appear in body section (before any potential trailers)
    assert!(result.contains("feat: add feature"));
    assert!(result.contains("Additional context"));
    assert!(result.contains("feature\n\nAdditional context"));
}

#[test]
fn test_multiple_flows_adding_trailers() {
    let mut event = PrepareCommitMessageEvent::new("feat: add authentication".to_string());
    
    // Flow A adds a trailer
    let trailer_a = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Reviewed-by: Alice".to_string())),
    };
    event.apply_trailers_action(trailer_a);
    
    // Flow B adds another trailer
    let trailer_b = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Reviewed-by: Bob".to_string())),
    };
    event.apply_trailers_action(trailer_b);
    
    let result = event.build_message();
    
    // Both trailers should appear in flow order
    assert!(result.contains("Reviewed-by: Alice"));
    assert!(result.contains("Reviewed-by: Bob"));
    
    // Alice should appear before Bob (flow A fired first)
    let alice_pos = result.find("Reviewed-by: Alice").unwrap();
    let bob_pos = result.find("Reviewed-by: Bob").unwrap();
    assert!(alice_pos < bob_pos, "Flow A's trailer should appear before Flow B's");
    
    // Should have single newline between trailers (not double)
    assert!(result.contains("Alice\nReviewed-by: Bob"));
    assert!(!result.contains("Alice\n\nReviewed-by: Bob"));
}

#[test]
fn test_multiple_flows_adding_body() {
    let mut event = PrepareCommitMessageEvent::new("feat: add feature".to_string());
    
    // Flow A adds body content
    let body_a = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Context from Flow A".to_string())),
    };
    event.apply_body_action(body_a);
    
    // Flow B adds more body content
    let body_b = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Context from Flow B".to_string())),
    };
    event.apply_body_action(body_b);
    
    let result = event.build_message();
    
    // Both body additions should appear in flow order
    assert!(result.contains("Context from Flow A"));
    assert!(result.contains("Context from Flow B"));
    
    // Flow A should appear before Flow B
    let flow_a_pos = result.find("Context from Flow A").unwrap();
    let flow_b_pos = result.find("Context from Flow B").unwrap();
    assert!(flow_a_pos < flow_b_pos, "Flow A's content should appear before Flow B's");
    
    // Body sections joined with double newline
    assert!(result.contains("Flow A\nContext from Flow B"));
}

#[test]
fn test_multiple_flows_body_and_trailers_mixed() {
    let mut event = PrepareCommitMessageEvent::new("feat: implement auth".to_string());
    
    // Flow A adds body
    let body_a = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Implements JWT validation".to_string())),
    };
    event.apply_body_action(body_a);
    
    // Flow B adds trailer
    let trailer_b = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Reviewed-by: Alice".to_string())),
    };
    event.apply_trailers_action(trailer_b);
    
    // Flow C adds more body
    let body_c = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Adds token expiry checks".to_string())),
    };
    event.apply_body_action(body_c);
    
    // Flow D adds another trailer
    let trailer_d = MessageBuilder {
        prepend: None,
        append: Some(StringOrArray::Single("Ticket: AUTH-123".to_string())),
    };
    event.apply_trailers_action(trailer_d);
    
    let result = event.build_message();
    
    // Structure should be: original + all body + all trailers
    // Body order: Flow A, Flow C
    // Trailer order: Flow B, Flow D
    
    let original_pos = result.find("feat: implement auth").unwrap();
    let jwt_pos = result.find("JWT validation").unwrap();
    let expiry_pos = result.find("expiry checks").unwrap();
    let alice_pos = result.find("Reviewed-by: Alice").unwrap();
    let ticket_pos = result.find("Ticket: AUTH-123").unwrap();
    
    // Order verification
    assert!(original_pos < jwt_pos, "Original before body");
    assert!(jwt_pos < expiry_pos, "Flow A body before Flow C body");
    assert!(expiry_pos < alice_pos, "All body before trailers");
    assert!(alice_pos < ticket_pos, "Flow B trailer before Flow D trailer");
}
```

## Validation and Error Handling

### Valid Syntax

```yaml
# ✅ Simple string
prompt: "Read ARCHITECTURE.md"

# ✅ Explicit with prepend only
prompt:
  prepend: "Header text"

# ✅ Explicit with append only
prompt:
  append: "Footer text"

# ✅ Explicit with both
prompt:
  prepend: "Header"
  append: "Footer"

# ✅ Explicit with multiple items
prompt:
  prepend:
    - "Line 1"
    - "Line 2"
  append:
    - "Line 3"
    - "Line 4"

# ✅ Trailers only (PrepareCommitMessage)
commit_message:
  trailers:
    append:
      - "Co-authored-by: AI <ai@example.com>"
      - "Ticket: PROJ-1234"

# ✅ Body and trailers (PrepareCommitMessage)
commit_message:
  body:
    append: "Additional context about the change"
  trailers:
    append:
      - "Reviewed-by: Senior Engineer"
      - "Ticket: PROJ-1234"
```

### Invalid Syntax (Must Error)

```yaml
# ❌ Empty object
prompt: {}
# Error: MessageBuilder must have at least prepend or append

# ❌ Unknown field
prompt:
  insert: "text"
# Error: Unknown field 'insert'. Valid fields: 'prepend', 'append'

# ❌ Wrong type for prepend
prompt:
  prepend: 123
# Error: 'prepend' must be a string or array of strings

# ❌ Wrong type for append
prompt:
  append: true
# Error: 'append' must be a string or array of strings
```

### Validation Logic

```rust
impl MessageBuilder {
    pub fn validate(&self) -> Result<(), String> {
        if self.prepend.is_none() && self.append.is_none() {
            return Err("MessageBuilder must have at least one field: prepend or append".to_string());
        }
        Ok(())
    }
}
```

## Implementation Tasks

### Phase 1: Core MessageBuilder (3-5 days)

- [ ] Create `cli/src/flows/actions/message_builder.rs`
- [ ] Implement `MessageBuilder` enum with Simple and Explicit variants
- [ ] Implement `check_id()` method using `DefaultHasher`
- [ ] Implement `apply()` method
- [ ] Implement `validate()` method
- [ ] Write unit tests for all variants
- [ ] Test deterministic check ID generation
- [ ] Add serde serialization/deserialization tests

### Phase 2: Event Integration (2-3 days)

- [ ] Create `cli/src/flows/events/preprompt.rs` (for Milestone 1.1)
  - [ ] `PrePromptEvent` struct with MessageBuilder support
  - [ ] `apply_prompt_action()` method
  - [ ] `build_prompt()` method
- [ ] Refactor existing PrepareCommitMessage hook
  - [ ] Create `cli/src/flows/events/prepare_commit_message.rs`
  - [ ] Migrate existing logic to use MessageBuilder
  - [ ] Support `body:` and `trailers:` sub-fields
  - [ ] Maintain backward compatibility with existing flows
- [ ] Write integration tests for both events
- [ ] Document how event implementations use MessageBuilder

### Phase 3: Documentation and Migration (1-2 days)

- [ ] Update FLOW_SYNTAX.md with MessageBuilder examples
- [ ] Migrate existing flows to new syntax:
  - [ ] Update any existing PrepareCommitMessage flows to use new `body:` / `trailers:` syntax
  - [ ] Document migration path from old to new syntax
- [ ] Add troubleshooting section for common issues
- [ ] Create examples directory with sample flows
- [ ] Document PrepareCommitMessage refactoring in CHANGELOG

## Success Criteria

- [ ] All unit tests pass
- [ ] Integration tests pass for all three event types
- [ ] Check IDs are deterministic and unique
- [ ] Documentation is complete and clear
- [ ] Code review by team approved
- [ ] Ready for Milestone 1.1 (PrePrompt) to begin

## Dependencies

**Rust Crates:**
- `serde` - Serialization/deserialization
- `serde_yaml` - YAML parsing for check ID generation

**Aiki Components:**
- Flow parser (needs to parse MessageBuilder from YAML)

**Standard Library:**
- `std::collections::hash_map::DefaultHasher` - Built-in hashing for check IDs

## Blocks

This milestone blocks:
- **Milestone 1.1**: PrePrompt event (needs MessageBuilder for `prompt:` action)

**What This Delivers:**
- Consistent syntax for prompt modification (PrePrompt) 
- Refactored PrepareCommitMessage with cleaner, more maintainable code
- Shared testing and validation infrastructure

**Note:** Milestone 1.2 (PostResponse) does not depend on MessageBuilder - it uses a task-based system instead.

## Notes

- The same MessageBuilder code is used by PrePrompt and PrepareCommitMessage, ensuring consistency
- PrepareCommitMessage refactoring is included in this milestone (not a future milestone)
- Keep the implementation simple - just string manipulation, no file system interaction

## See Also

- [Milestone 1.1: PrePrompt Event](./milestone-1.1-preprompt.md) - Uses MessageBuilder for `prompt:` action
- [Milestone 1.2: PostResponse & Task System](./milestone-1.2-post-response-and-tasks.md) - Task-based system (doesn't use MessageBuilder)
- [Milestone 1: Event System Overview](./milestone-1.md) - Context for the full event system
