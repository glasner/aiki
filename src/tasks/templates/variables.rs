//! Variable substitution for task templates
//!
//! Provides safe, single-pass variable substitution with these guarantees:
//! - Single-pass evaluation: Variables are substituted once, values are never re-evaluated
//! - No recursion: Values containing {braces} are inserted as literal text
//! - Safe by design: Substitution happens after frontmatter parsing
//! - Deterministic: Same inputs always produce same output

use crate::error::{AikiError, Result};
use std::collections::HashMap;

/// Context for variable substitution
#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    /// Built-in variables (id, assignee, priority, type, created)
    pub builtins: HashMap<String, String>,
    /// Data variables (accessible as {data.key})
    pub data: HashMap<String, String>,
    /// Source variable (accessible as {source})
    pub source: Option<String>,
}

impl VariableContext {
    /// Create a new empty context
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a built-in variable
    pub fn set_builtin(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.builtins.insert(key.into(), value.into());
    }

    /// Set a data variable
    pub fn set_data(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.data.insert(key.into(), value.into());
    }

    /// Set the source variable
    pub fn set_source(&mut self, value: impl Into<String>) {
        self.source = Some(value.into());
    }

    /// Resolve a variable reference to its value
    fn resolve(&self, var_ref: &str) -> Option<String> {
        // Check for data.* variables first
        if let Some(key) = var_ref.strip_prefix("data.") {
            return self.data.get(key).cloned();
        }

        // Check for source variable
        if var_ref == "source" {
            return self.source.clone();
        }

        // Check built-in variables
        self.builtins.get(var_ref).cloned()
    }
}

/// Substitute variables in text using the given context
///
/// # Substitution Rules
///
/// - `{var}` - Substitute built-in variable
/// - `{data.key}` - Substitute data variable
/// - `{source}` - Substitute source variable
/// - `{{` and `}}` - Escape to literal `{` and `}`
///
/// # Errors
///
/// Returns an error if a referenced variable is not found in the context.
pub fn substitute(text: &str, ctx: &VariableContext) -> Result<String> {
    substitute_with_template_name(text, ctx, None)
}

/// Substitute variables with template name for better error messages
pub fn substitute_with_template_name(
    text: &str,
    ctx: &VariableContext,
    template_name: Option<&str>,
) -> Result<String> {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                // Escaped brace: {{ -> {
                chars.next();
                result.push('{');
            } else {
                // Start of variable reference
                let mut var_ref = String::new();
                let mut found_close = false;

                for c2 in chars.by_ref() {
                    if c2 == '}' {
                        found_close = true;
                        break;
                    }
                    var_ref.push(c2);
                }

                if !found_close {
                    // Unclosed brace, treat as literal
                    result.push('{');
                    result.push_str(&var_ref);
                } else if var_ref.is_empty() {
                    // Empty variable reference: {} -> {}
                    result.push_str("{}");
                } else {
                    // Resolve variable
                    match ctx.resolve(&var_ref) {
                        Some(value) => result.push_str(&value),
                        None => {
                            let hint = if var_ref.starts_with("data.") {
                                format!(
                                    "Use: --data {}=<value>",
                                    var_ref.strip_prefix("data.").unwrap_or(&var_ref)
                                )
                            } else if var_ref == "source" {
                                "Use: --source <value>".to_string()
                            } else {
                                format!("Variable '{}' is not a recognized built-in variable", var_ref)
                            };

                            let template_info = template_name
                                .map(|n| format!("\n  In template: {}", n))
                                .unwrap_or_default();

                            return Err(AikiError::TemplateVariableNotFound {
                                variable: var_ref,
                                hint,
                                template_info,
                            });
                        }
                    }
                }
            }
        } else if c == '}' {
            if chars.peek() == Some(&'}') {
                // Escaped brace: }} -> }
                chars.next();
                result.push('}');
            } else {
                // Single closing brace, keep as-is
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

/// Find all variable references in text (for validation)
pub fn find_variables(text: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                // Escaped brace, skip
                chars.next();
            } else {
                // Start of variable reference
                let mut var_ref = String::new();
                let mut found_close = false;

                for c2 in chars.by_ref() {
                    if c2 == '}' {
                        found_close = true;
                        break;
                    }
                    var_ref.push(c2);
                }

                if found_close && !var_ref.is_empty() {
                    vars.push(var_ref);
                }
            }
        } else if c == '}' && chars.peek() == Some(&'}') {
            // Escaped brace, skip
            chars.next();
        }
    }

    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_builtin() {
        let mut ctx = VariableContext::new();
        ctx.set_builtin("assignee", "claude-code");
        ctx.set_builtin("priority", "p1");

        let result = substitute("Assigned to: {assignee}, Priority: {priority}", &ctx).unwrap();
        assert_eq!(result, "Assigned to: claude-code, Priority: p1");
    }

    #[test]
    fn test_substitute_data() {
        let mut ctx = VariableContext::new();
        ctx.set_data("scope", "@");
        ctx.set_data("files", "src/auth.rs, src/crypto.rs");

        let result = substitute("Review {data.scope} (files: {data.files})", &ctx).unwrap();
        assert_eq!(result, "Review @ (files: src/auth.rs, src/crypto.rs)");
    }

    #[test]
    fn test_substitute_source() {
        let mut ctx = VariableContext::new();
        ctx.set_source("file:ops/now/feature.md");

        let result = substitute("Build from {source}", &ctx).unwrap();
        assert_eq!(result, "Build from file:ops/now/feature.md");
    }

    #[test]
    fn test_substitute_escape_braces() {
        let ctx = VariableContext::new();

        let result = substitute("Use {{data.foo}} syntax for variables", &ctx).unwrap();
        assert_eq!(result, "Use {data.foo} syntax for variables");
    }

    #[test]
    fn test_substitute_escape_both_braces() {
        let ctx = VariableContext::new();

        let result = substitute("{{escaped}} and }}trailing}}", &ctx).unwrap();
        assert_eq!(result, "{escaped} and }trailing}");
    }

    #[test]
    fn test_substitute_missing_variable() {
        let ctx = VariableContext::new();

        let result = substitute("Hello {data.missing}", &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("data.missing"));
        assert!(err.to_string().contains("--data missing="));
    }

    #[test]
    fn test_substitute_no_recursion() {
        let mut ctx = VariableContext::new();
        ctx.set_data("value", "{data.other}");

        let result = substitute("Result: {data.value}", &ctx).unwrap();
        // The value should be literal, not recursively evaluated
        assert_eq!(result, "Result: {data.other}");
    }

    #[test]
    fn test_substitute_empty_braces() {
        let ctx = VariableContext::new();

        let result = substitute("Empty {} braces", &ctx).unwrap();
        assert_eq!(result, "Empty {} braces");
    }

    #[test]
    fn test_substitute_unclosed_brace() {
        let ctx = VariableContext::new();

        let result = substitute("Unclosed {brace", &ctx).unwrap();
        assert_eq!(result, "Unclosed {brace");
    }

    #[test]
    fn test_substitute_single_closing_brace() {
        let ctx = VariableContext::new();

        let result = substitute("Single } brace", &ctx).unwrap();
        assert_eq!(result, "Single } brace");
    }

    #[test]
    fn test_find_variables() {
        let vars = find_variables("Review {data.scope} assigned to {assignee} from {source}");
        assert_eq!(vars, vec!["data.scope", "assignee", "source"]);
    }

    #[test]
    fn test_find_variables_with_escapes() {
        let vars = find_variables("{{escaped}} and {real} and {{also_escaped}}");
        assert_eq!(vars, vec!["real"]);
    }

    #[test]
    fn test_variable_context_builder() {
        let mut ctx = VariableContext::new();
        ctx.set_builtin("id", "abc123");
        ctx.set_data("scope", "@");
        ctx.set_source("file:plan.md");

        assert_eq!(ctx.resolve("id"), Some("abc123".to_string()));
        assert_eq!(ctx.resolve("data.scope"), Some("@".to_string()));
        assert_eq!(ctx.resolve("source"), Some("file:plan.md".to_string()));
        assert_eq!(ctx.resolve("unknown"), None);
    }
}
