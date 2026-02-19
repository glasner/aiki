//! Validation utilities for identifiers and names across Aiki
//!
//! This module centralizes validation logic to ensure consistency across:
//! - Template loop variables ({% for item in collection %})
//! - Hook/flow `let` bindings (let: var = value)
//! - Any other user-defined identifiers

/// Validates a template variable name (strict lowercase-only identifiers)
///
/// Rules:
/// - Must start with lowercase letter or underscore: `[a-z_]`
/// - Rest must be lowercase letters, digits, or underscores: `[a-z0-9_]*`
///
/// This is the canonical validation for:
/// - Template loop variables: `{% for item in collection %}`
/// - Template conditionals using variable names
///
/// # Examples
///
/// ```
/// use aiki::validation::is_valid_template_identifier;
///
/// assert!(is_valid_template_identifier("item"));
/// assert!(is_valid_template_identifier("_private"));
/// assert!(is_valid_template_identifier("var123"));
/// assert!(is_valid_template_identifier("my_var"));
///
/// assert!(!is_valid_template_identifier("")); // empty
/// assert!(!is_valid_template_identifier("123var")); // starts with digit
/// assert!(!is_valid_template_identifier("CamelCase")); // uppercase not allowed
/// assert!(!is_valid_template_identifier("my-var")); // hyphen not allowed
/// ```
#[must_use]
pub fn is_valid_template_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };

    // First character must be lowercase letter or underscore
    if !matches!(first, 'a'..='z' | '_') {
        return false;
    }

    // Remaining characters must be lowercase letters, digits, or underscores
    chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

/// Validates a flow/hook variable name (allows uppercase for compatibility)
///
/// Rules:
/// - Must start with letter (any case) or underscore: `[A-Za-z_]`
/// - Rest must be letters, digits, or underscores: `[A-Za-z0-9_]*`
///
/// This is the canonical validation for:
/// - Hook/flow `let` bindings: `let: description = event.file_paths`
/// - Flow step aliases
///
/// # Examples
///
/// ```
/// use aiki::validation::is_valid_flow_identifier;
///
/// assert!(is_valid_flow_identifier("description"));
/// assert!(is_valid_flow_identifier("CamelCase"));
/// assert!(is_valid_flow_identifier("_private"));
/// assert!(is_valid_flow_identifier("var123"));
///
/// assert!(!is_valid_flow_identifier("")); // empty
/// assert!(!is_valid_flow_identifier("123var")); // starts with digit
/// assert!(!is_valid_flow_identifier("my-var")); // hyphen not allowed
/// ```
#[must_use]
pub fn is_valid_flow_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };

    // First character must be letter (any case) or underscore
    if !first.is_alphabetic() && first != '_' {
        return false;
    }

    // Remaining characters must be alphanumeric or underscore
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_identifier_valid() {
        assert!(is_valid_template_identifier("item"));
        assert!(is_valid_template_identifier("comment"));
        assert!(is_valid_template_identifier("c"));
        assert!(is_valid_template_identifier("_private"));
        assert!(is_valid_template_identifier("var123"));
        assert!(is_valid_template_identifier("my_var"));
        assert!(is_valid_template_identifier("file_path"));
    }

    #[test]
    fn test_template_identifier_invalid() {
        // Empty
        assert!(!is_valid_template_identifier(""));

        // Starts with digit
        assert!(!is_valid_template_identifier("123var"));

        // Uppercase not allowed (strict lowercase-only)
        assert!(!is_valid_template_identifier("CamelCase"));
        assert!(!is_valid_template_identifier("Item"));
        assert!(!is_valid_template_identifier("MY_VAR"));

        // Special characters not allowed
        assert!(!is_valid_template_identifier("my-var")); // hyphen
        assert!(!is_valid_template_identifier("my.var")); // dot
        assert!(!is_valid_template_identifier("my var")); // space
        assert!(!is_valid_template_identifier("$var")); // dollar sign
        assert!(!is_valid_template_identifier("my/var")); // slash
    }

    #[test]
    fn test_flow_identifier_valid() {
        assert!(is_valid_flow_identifier("description"));
        assert!(is_valid_flow_identifier("desc"));
        assert!(is_valid_flow_identifier("_private"));
        assert!(is_valid_flow_identifier("var123"));
        assert!(is_valid_flow_identifier("my_var"));
        assert!(is_valid_flow_identifier("CamelCase")); // uppercase allowed
        assert!(is_valid_flow_identifier("MyVar"));
    }

    #[test]
    fn test_flow_identifier_invalid() {
        // Empty
        assert!(!is_valid_flow_identifier(""));

        // Starts with digit
        assert!(!is_valid_flow_identifier("123var"));

        // Special characters not allowed
        assert!(!is_valid_flow_identifier("my-var")); // hyphen
        assert!(!is_valid_flow_identifier("my.var")); // dot
        assert!(!is_valid_flow_identifier("my var")); // space
        assert!(!is_valid_flow_identifier("$var")); // dollar sign
    }
}
