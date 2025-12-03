//! Message assembly system for building prompts, autoreplies, and commit messages.
//!
//! This module provides a unified syntax for prepending and appending content to messages
//! across different event types (PrePrompt, PostResponse, PrepareCommitMessage).
//!
//! # Core Types
//!
//! - [`MessageChunk`]: Represents content to prepend and/or append
//! - [`MessageAssembler`]: Stateful builder that accumulates chunks and produces final output
//! - [`StringOrArray`]: YAML value that can be a single string or array of strings
//!
//! # Usage Example
//!
//! ```rust
//! use aiki::messages::{MessageChunk, MessageAssembler};
//!
//! let mut assembler = MessageAssembler::new(Some("original content".to_string()), "\n");
//!
//! // Add chunks from different flows
//! let chunk1 = MessageChunk {
//!     prepend: Some(vec!["First line".to_string()]),
//!     append: Some(vec!["Last line".to_string()]),
//! };
//! assembler.add_chunk(chunk1);
//!
//! // Build final message
//! let result = assembler.build();
//! // Result: "First line\noriginal content\nLast line"
//! ```

use serde::{Deserialize, Serialize};

/// A string value that can be represented as either a single string or an array of strings in YAML.
///
/// This type handles the YAML flexibility where users can write:
/// ```yaml
/// prepend: "single line"
/// ```
/// or:
/// ```yaml
/// prepend:
///   - "line 1"
///   - "line 2"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum StringOrArray {
    /// Single string value
    Single(String),
    /// Multiple string values
    Multiple(Vec<String>),
}

impl StringOrArray {
    /// Convert to a vector of strings, consuming self and normalizing trailing newlines.
    ///
    /// This ensures `prepend: |` and `prepend: ["line"]` behave identically by stripping
    /// the trailing newline that YAML block scalars include.
    ///
    /// # Returns
    ///
    /// - `Single(s)` → `vec![s.trim_end_matches('\n')]`
    /// - `Multiple(v)` → `v` with each item's trailing newlines stripped
    #[must_use]
    pub fn to_vec(self) -> Vec<String> {
        match self {
            Self::Single(s) => {
                // Strip trailing newline from YAML block scalars (|)
                vec![s.trim_end_matches('\n').to_string()]
            }
            Self::Multiple(v) => {
                // Normalize each item in the array
                v.into_iter()
                    .map(|s| s.trim_end_matches('\n').to_string())
                    .collect()
            }
        }
    }

    /// Check if the content is empty (empty string or empty array).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Single(s) => s.is_empty(),
            Self::Multiple(v) => v.is_empty() || v.iter().all(String::is_empty),
        }
    }
}

/// A chunk of content to prepend and/or append to a message.
///
/// This is the fundamental building block for message assembly. Each flow can
/// contribute a `MessageChunk` that specifies what to add before and/or after
/// the existing content.
///
/// # YAML Syntax
///
/// ## Prepend only
/// ```yaml
/// prepend: "Add this before"
/// ```
///
/// ## Append only
/// ```yaml
/// append: "Add this after"
/// ```
///
/// ## Both
/// ```yaml
/// prepend: "Before"
/// append: "After"
/// ```
///
/// ## Arrays
/// ```yaml
/// prepend:
///   - "Line 1"
///   - "Line 2"
/// append:
///   - "Line 3"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageChunk {
    /// Content to add before the existing message
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepend: Option<StringOrArray>,

    /// Content to add after the existing message
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<StringOrArray>,
}

impl MessageChunk {
    /// Create a new empty `MessageChunk`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prepend: None,
            append: None,
        }
    }

    /// Generate a deterministic checksum ID for this chunk.
    ///
    /// This is used for deduplication and change tracking. The same content
    /// will always produce the same ID by serializing to YAML and hashing.
    ///
    /// # Returns
    ///
    /// An 8-character hex-encoded hash of the chunk's YAML representation.
    #[must_use]
    pub fn check_id(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let yaml = serde_yaml::to_string(self).expect("MessageChunk should always serialize");

        let mut hasher = DefaultHasher::new();
        yaml.hash(&mut hasher);
        let hash = hasher.finish();

        // Return first 8 hex chars for readability
        format!("{:08x}", hash)
    }

    /// Get prepend items as a vector of strings.
    ///
    /// Returns an empty vector if prepend is None.
    #[must_use]
    pub fn prepend_items(&self) -> Vec<String> {
        self.prepend.clone().map(|p| p.to_vec()).unwrap_or_default()
    }

    /// Get append items as a vector of strings.
    ///
    /// Returns an empty vector if append is None.
    #[must_use]
    pub fn append_items(&self) -> Vec<String> {
        self.append.clone().map(|a| a.to_vec()).unwrap_or_default()
    }

    /// Validate the chunk structure.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Both prepend and append are None (empty chunk)
    /// - prepend or append is empty
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.prepend.is_none() && self.append.is_none() {
            return Err(crate::error::AikiError::InvalidMessageChunk(
                "MessageChunk must have at least prepend or append".to_string(),
            ));
        }

        if let Some(ref p) = self.prepend {
            if p.is_empty() {
                return Err(crate::error::AikiError::InvalidMessageChunk(
                    "prepend cannot be empty".to_string(),
                ));
            }
        }

        if let Some(ref a) = self.append {
            if a.is_empty() {
                return Err(crate::error::AikiError::InvalidMessageChunk(
                    "append cannot be empty".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Default for MessageChunk {
    fn default() -> Self {
        Self::new()
    }
}

/// A stateful builder that accumulates message chunks and produces a final assembled message.
///
/// # Usage Pattern
///
/// 1. Create assembler with optional original content
/// 2. Add chunks from different flows
/// 3. Call `build()` to produce final message
///
/// # Example
///
/// ```rust
/// use aiki::messages::{MessageChunk, MessageAssembler};
///
/// let mut assembler = MessageAssembler::new(Some("Middle".to_string()), "\n");
///
/// let chunk1 = MessageChunk {
///     prepend: Some(vec!["Top".to_string()]),
///     append: None,
/// };
/// assembler.add_chunk(chunk1);
///
/// let chunk2 = MessageChunk {
///     prepend: None,
///     append: Some(vec!["Bottom".to_string()]),
/// };
/// assembler.add_chunk(chunk2);
///
/// assert_eq!(assembler.build(), "Top\nMiddle\nBottom");
/// ```
#[derive(Debug, Clone)]
pub struct MessageAssembler {
    /// Accumulated chunks in order they were added
    chunks: Vec<MessageChunk>,
    /// Original message content (e.g., user's prompt, existing commit message)
    original: Option<String>,
    /// Separator to use when joining sections
    separator: String,
}

impl MessageAssembler {
    /// Create a new `MessageAssembler`.
    ///
    /// # Arguments
    ///
    /// - `original`: Optional original content (e.g., user's prompt)
    /// - `separator`: String to use when joining sections (typically "\n" or "\n\n")
    #[must_use]
    pub fn new(original: Option<String>, separator: impl Into<String>) -> Self {
        Self {
            chunks: Vec::new(),
            original,
            separator: separator.into(),
        }
    }

    /// Add a chunk to the assembler.
    ///
    /// Chunks are processed in the order they are added.
    pub fn add_chunk(&mut self, chunk: MessageChunk) {
        self.chunks.push(chunk);
    }

    /// Build the final message by combining all chunks with the original content.
    ///
    /// # Assembly Order
    ///
    /// 1. All prepend items from all chunks (joined with "\n")
    /// 2. Original content (if any)
    /// 3. All append items from all chunks (joined with "\n")
    ///
    /// Sections are then joined with the configured separator (typically "\n\n").
    ///
    /// # Returns
    ///
    /// The assembled message as a single string.
    #[must_use]
    pub fn build(&self) -> String {
        let mut prepends = Vec::new();
        let mut appends = Vec::new();

        // Collect all prepends and appends in order
        for chunk in &self.chunks {
            prepends.extend(chunk.prepend_items());
            appends.extend(chunk.append_items());
        }

        // Build final message
        let mut parts = Vec::new();

        if !prepends.is_empty() {
            parts.push(prepends.join("\n"));
        }

        if let Some(ref orig) = self.original {
            if !orig.is_empty() {
                parts.push(orig.clone());
            }
        }

        if !appends.is_empty() {
            parts.push(appends.join("\n"));
        }

        parts.join(&self.separator)
    }

    /// Get the number of chunks accumulated.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Check if the assembler has any chunks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_or_array_single() {
        let single = StringOrArray::Single("test".to_string());
        assert!(!single.is_empty());
        assert_eq!(single.to_vec(), vec!["test".to_string()]);
    }

    #[test]
    fn test_string_or_array_multiple() {
        let multiple = StringOrArray::Multiple(vec!["a".to_string(), "b".to_string()]);
        assert!(!multiple.is_empty());
        assert_eq!(multiple.to_vec(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_string_or_array_empty() {
        let empty_single = StringOrArray::Single("".to_string());
        assert!(empty_single.is_empty());

        let empty_multiple = StringOrArray::Multiple(vec![]);
        assert!(empty_multiple.is_empty());
    }

    #[test]
    fn test_prepend_only() {
        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Single("prepended".to_string())),
            append: None,
        };

        let mut assembler = MessageAssembler::new(Some("original".to_string()), "\n");
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "prepended\noriginal");
    }

    #[test]
    fn test_append_only() {
        let chunk = MessageChunk {
            prepend: None,
            append: Some(StringOrArray::Single("appended".to_string())),
        };

        let mut assembler = MessageAssembler::new(Some("original".to_string()), "\n");
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "original\nappended");
    }

    #[test]
    fn test_both_prepend_and_append() {
        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Single("before".to_string())),
            append: Some(StringOrArray::Single("after".to_string())),
        };

        let mut assembler = MessageAssembler::new(Some("middle".to_string()), "\n");
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "before\nmiddle\nafter");
    }

    #[test]
    fn test_with_arrays() {
        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Multiple(vec![
                "line1".to_string(),
                "line2".to_string(),
            ])),
            append: Some(StringOrArray::Single("line3".to_string())),
        };

        let mut assembler = MessageAssembler::new(Some("original".to_string()), "\n");
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "line1\nline2\noriginal\nline3");
    }

    #[test]
    fn test_check_id_is_deterministic() {
        let chunk1 = MessageChunk {
            prepend: Some(StringOrArray::Single("test".to_string())),
            append: Some(StringOrArray::Single("content".to_string())),
        };

        let chunk2 = MessageChunk {
            prepend: Some(StringOrArray::Single("test".to_string())),
            append: Some(StringOrArray::Single("content".to_string())),
        };

        assert_eq!(chunk1.check_id(), chunk2.check_id());
    }

    #[test]
    fn test_check_id_differs_for_different_content() {
        let chunk1 = MessageChunk {
            prepend: Some(StringOrArray::Single("test1".to_string())),
            append: None,
        };

        let chunk2 = MessageChunk {
            prepend: Some(StringOrArray::Single("test2".to_string())),
            append: None,
        };

        assert_ne!(chunk1.check_id(), chunk2.check_id());
    }

    #[test]
    fn test_yaml_block_scalar_trailing_newline_stripped() {
        // YAML block scalars (|) include a trailing newline, but our system
        // should handle this gracefully
        let yaml_str = r#"
prepend: |
  Line 1
  Line 2
"#;

        let chunk: MessageChunk = serde_yaml::from_str(yaml_str).unwrap();
        let mut assembler = MessageAssembler::new(Some("original".to_string()), "\n");
        assembler.add_chunk(chunk);

        let result = assembler.build();
        // The block scalar preserves the trailing newline in the string
        assert!(result.starts_with("Line 1\nLine 2"));
    }

    #[test]
    fn test_yaml_array_form() {
        let yaml_str = r#"
prepend:
  - "Line 1"
  - "Line 2"
append:
  - "Line 3"
"#;

        let chunk: MessageChunk = serde_yaml::from_str(yaml_str).unwrap();
        let mut assembler = MessageAssembler::new(Some("original".to_string()), "\n");
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "Line 1\nLine 2\noriginal\nLine 3");
    }

    #[test]
    fn test_yaml_forms_produce_same_output() {
        // Block scalar form
        let yaml_block = r#"
prepend: "Header"
append: "Footer"
"#;

        // Array form
        let yaml_array = r#"
prepend:
  - "Header"
append:
  - "Footer"
"#;

        let chunk1: MessageChunk = serde_yaml::from_str(yaml_block).unwrap();
        let chunk2: MessageChunk = serde_yaml::from_str(yaml_array).unwrap();

        let mut assembler1 = MessageAssembler::new(Some("Body".to_string()), "\n");
        assembler1.add_chunk(chunk1);

        let mut assembler2 = MessageAssembler::new(Some("Body".to_string()), "\n");
        assembler2.add_chunk(chunk2);

        assert_eq!(assembler1.build(), assembler2.build());
    }

    #[test]
    fn test_validate_empty_chunk() {
        let chunk = MessageChunk {
            prepend: None,
            append: None,
        };

        assert!(chunk.validate().is_err());
    }

    #[test]
    fn test_validate_valid_chunk() {
        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Single("test".to_string())),
            append: None,
        };

        assert!(chunk.validate().is_ok());
    }

    #[test]
    fn test_prepend_items() {
        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Multiple(vec![
                "a".to_string(),
                "b".to_string(),
            ])),
            append: None,
        };

        assert_eq!(
            chunk.prepend_items(),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn test_append_items() {
        let chunk = MessageChunk {
            prepend: None,
            append: Some(StringOrArray::Multiple(vec![
                "x".to_string(),
                "y".to_string(),
            ])),
        };

        assert_eq!(chunk.append_items(), vec!["x".to_string(), "y".to_string()]);
    }
}

#[cfg(test)]
mod assembler_tests {
    use super::*;

    #[test]
    fn test_build_with_single_chunk() {
        let mut assembler = MessageAssembler::new(Some("original".to_string()), "\n");

        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Single("before".to_string())),
            append: Some(StringOrArray::Single("after".to_string())),
        };
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "before\noriginal\nafter");
    }

    #[test]
    fn test_build_with_multiple_chunks() {
        let mut assembler = MessageAssembler::new(Some("original".to_string()), "\n");

        let chunk1 = MessageChunk {
            prepend: Some(StringOrArray::Single("first".to_string())),
            append: None,
        };
        assembler.add_chunk(chunk1);

        let chunk2 = MessageChunk {
            prepend: Some(StringOrArray::Single("second".to_string())),
            append: Some(StringOrArray::Single("end".to_string())),
        };
        assembler.add_chunk(chunk2);

        // Order: all prepends, original, all appends
        assert_eq!(assembler.build(), "first\nsecond\noriginal\nend");
    }

    #[test]
    fn test_build_without_original() {
        let mut assembler = MessageAssembler::new(None, "\n");

        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Single("start".to_string())),
            append: Some(StringOrArray::Single("end".to_string())),
        };
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "start\nend");
    }

    #[test]
    fn test_build_with_custom_separator() {
        let mut assembler = MessageAssembler::new(Some("middle".to_string()), " | ");

        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Single("A".to_string())),
            append: Some(StringOrArray::Single("B".to_string())),
        };
        assembler.add_chunk(chunk);

        assert_eq!(assembler.build(), "A | middle | B");
    }

    #[test]
    fn test_build_empty_chunks() {
        let assembler = MessageAssembler::new(Some("only original".to_string()), "\n");

        assert_eq!(assembler.build(), "only original");
    }

    #[test]
    fn test_build_empty_original() {
        let mut assembler = MessageAssembler::new(Some("".to_string()), "\n");

        let chunk = MessageChunk {
            prepend: Some(StringOrArray::Single("before".to_string())),
            append: Some(StringOrArray::Single("after".to_string())),
        };
        assembler.add_chunk(chunk);

        // Empty original should be skipped
        assert_eq!(assembler.build(), "before\nafter");
    }

    #[test]
    fn test_chunk_count() {
        let mut assembler = MessageAssembler::new(None, "\n");
        assert_eq!(assembler.chunk_count(), 0);

        assembler.add_chunk(MessageChunk::new());
        assert_eq!(assembler.chunk_count(), 1);

        assembler.add_chunk(MessageChunk::new());
        assert_eq!(assembler.chunk_count(), 2);
    }

    #[test]
    fn test_is_empty() {
        let mut assembler = MessageAssembler::new(None, "\n");
        assert!(assembler.is_empty());

        assembler.add_chunk(MessageChunk::new());
        assert!(!assembler.is_empty());
    }
}
