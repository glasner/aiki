use std::collections::HashMap;

/// Resolves variables in strings (e.g., $event.file_paths, $cwd, $HOME)
/// Also supports JSON field access (e.g., $metadata.author for JSON variables)
pub struct VariableResolver {
    variables: HashMap<String, String>,
    // Cached sorted patterns: ("$key", "value")
    cached_patterns: Vec<(String, String)>,
    cache_valid: bool,
    // Track which variables contain JSON for field access
    json_variables: HashMap<String, serde_json::Value>,
}

impl VariableResolver {
    /// Create a new variable resolver
    #[must_use]
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            cached_patterns: Vec::new(),
            cache_valid: false,
            json_variables: HashMap::new(),
        }
    }

    /// Add a variable
    pub fn add_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();

        // Try to parse as JSON - if successful, store for field access
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&value) {
            if json.is_object() {
                self.json_variables.insert(key.clone(), json);
            }
        }

        self.variables.insert(key, value);
        self.cache_valid = false; // Invalidate cache
    }

    /// Add environment variables
    pub fn add_env_vars(&mut self, env_vars: &HashMap<String, String>) {
        // Iterate and clone individual entries instead of cloning entire HashMap
        self.variables
            .extend(env_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
        self.cache_valid = false; // Invalidate cache
    }

    /// Rebuild the pattern cache if needed
    fn rebuild_cache(&mut self) {
        if self.cache_valid {
            return;
        }

        self.cached_patterns = self
            .variables
            .iter()
            .filter(|(k, _)| !self.json_variables.contains_key(*k)) // Exclude JSON variables
            .map(|(k, v)| (format!("${}", k), v.clone()))
            .collect();

        // Sort by pattern length (longest first) to handle overlapping names correctly
        // e.g., $event.file_paths before $event.file
        self.cached_patterns
            .sort_by_key(|(pattern, _)| std::cmp::Reverse(pattern.len()));

        self.cache_valid = true;
    }

    /// Resolve all variables in a string
    ///
    /// Supports:
    /// - $event.* variables (e.g., $event.file_paths)
    /// - $cwd, $agent
    /// - $ENV_VAR environment variables
    /// - JSON field access (e.g., $metadata.author, $metadata.description)
    ///
    /// Example:
    /// ```ignore
    /// let mut resolver = VariableResolver::new();
    /// resolver.add_var("event.file_paths", "/path/to/file.rs");
    /// resolver.add_var("cwd", "/home/user/project");
    /// resolver.add_var("metadata", r#"{"author":"Claude","description":"..."}"#);
    ///
    /// let result = resolver.resolve("File: $event.file_paths by $metadata.author in $cwd");
    /// assert_eq!(result, "File: /path/to/file.rs by Claude in /home/user/project");
    /// ```
    pub fn resolve(&mut self, input: &str) -> String {
        // Fast path: no variables in input at all
        if !input.contains('$') {
            return input.to_string();
        }

        // First, resolve JSON field access (e.g., $metadata.author)
        let mut result = self.resolve_json_fields(input);

        // Ensure cache is built (amortized cost)
        self.rebuild_cache();

        // Fast path: no variables configured
        if self.cached_patterns.is_empty() && self.json_variables.is_empty() {
            return result;
        }

        // Perform substitutions for regular (non-JSON) variables only
        // JSON variables were already handled above and are excluded from cache
        for (pattern, value) in &self.cached_patterns {
            // Only do replacement if pattern exists in the string
            // Skip if it looks like field access (pattern followed by '.')
            // to avoid replacing $regular when $regular.field is present
            let pattern_with_dot = format!("{}.", pattern);
            if result.contains(&pattern_with_dot) {
                // This is field access - don't replace the base variable
                continue;
            }

            if result.contains(pattern.as_str()) {
                result = result.replace(pattern, value);
            }
        }

        result
    }

    /// Resolve JSON field access patterns like $metadata.author
    fn resolve_json_fields(&self, input: &str) -> String {
        let mut result = input.to_string();

        // Look for patterns like $var.field
        for (var_name, json_value) in &self.json_variables {
            // Find all occurrences of $var_name.field in the input
            let prefix = format!("${}.", var_name);

            // Simple pattern matching for field access
            let mut pos = 0;
            while let Some(start) = result[pos..].find(&prefix) {
                let abs_start = pos + start;
                let field_start = abs_start + prefix.len();

                // Extract field name (alphanumeric + underscore until whitespace or special char)
                let field_end = result[field_start..]
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .map(|i| field_start + i)
                    .unwrap_or(result.len());

                if field_end > field_start {
                    let field_name = &result[field_start..field_end];

                    // Try to extract the field value from JSON
                    if let Some(field_value) = json_value.get(field_name) {
                        let replacement = match field_value {
                            serde_json::Value::String(s) => s.clone(),
                            _ => field_value.to_string().trim_matches('"').to_string(),
                        };

                        // Replace $var.field with the actual value
                        let pattern = format!("{}{}", prefix, field_name);
                        result = result.replace(&pattern, &replacement);

                        // Continue searching from the same position (string changed)
                        pos = abs_start;
                        continue;
                    }
                }

                // Move past this match to continue searching
                pos = abs_start + prefix.len();
            }
        }

        result
    }
}

impl Default for VariableResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_simple_variable() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("name", "test");

        assert_eq!(resolver.resolve("Hello $name"), "Hello test");
    }

    #[test]
    fn test_resolve_multiple_variables() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("cwd", "/home/user");
        resolver.add_var("agent", "claude-code");

        assert_eq!(
            resolver.resolve("Running in $cwd with $agent"),
            "Running in /home/user with claude-code"
        );
    }

    #[test]
    fn test_resolve_same_variable_multiple_times() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("value", "42");

        assert_eq!(
            resolver.resolve("$value + $value = answer"),
            "42 + 42 = answer"
        );
    }

    #[test]
    fn test_resolve_no_variables() {
        let mut resolver = VariableResolver::new();
        assert_eq!(resolver.resolve("Plain text"), "Plain text");
    }

    #[test]
    fn test_resolve_undefined_variable() {
        let mut resolver = VariableResolver::new();
        assert_eq!(resolver.resolve("Value: $undefined"), "Value: $undefined");
    }

    #[test]
    fn test_resolve_overlapping_variable_names() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("event.file", "short");
        resolver.add_var("event.file_paths", "long");

        // Should resolve longest match first
        assert_eq!(resolver.resolve("$event.file_paths"), "long");
    }

    #[test]
    fn test_resolve_env_vars() {
        let mut resolver = VariableResolver::new();
        let mut env_vars = HashMap::new();
        env_vars.insert("HOME".to_string(), "/home/user".to_string());
        env_vars.insert("PATH".to_string(), "/usr/bin".to_string());
        resolver.add_env_vars(&env_vars);

        assert_eq!(
            resolver.resolve("Home: $HOME, Path: $PATH"),
            "Home: /home/user, Path: /usr/bin"
        );
    }

    #[test]
    fn test_resolve_empty_string() {
        let mut resolver = VariableResolver::new();
        assert_eq!(resolver.resolve(""), "");
    }

    #[test]
    fn test_resolve_dollar_sign_without_variable() {
        let mut resolver = VariableResolver::new();
        assert_eq!(resolver.resolve("Price: $100"), "Price: $100");
    }

    #[test]
    fn test_cache_invalidation() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("x", "1");

        assert_eq!(resolver.resolve("$x"), "1");

        // Add new variable - should invalidate cache
        resolver.add_var("x", "2");

        assert_eq!(resolver.resolve("$x"), "2");
    }

    #[test]
    fn test_cache_reuse() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("x", "value");

        // First resolve builds cache
        let _ = resolver.resolve("$x");
        assert!(resolver.cache_valid);

        // Second resolve should reuse cache
        let _ = resolver.resolve("$x");
        assert!(resolver.cache_valid);
    }

    #[test]
    fn test_fast_path_no_dollar_sign() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("x", "value");

        // Should return quickly without building cache
        let result = resolver.resolve("no variables here");
        assert_eq!(result, "no variables here");
    }

    #[test]
    fn test_fast_path_no_configured_variables() {
        let mut resolver = VariableResolver::new();

        // No variables configured - should return quickly
        let result = resolver.resolve("$something");
        assert_eq!(result, "$something");
    }

    #[test]
    fn test_json_field_access() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("metadata", r#"{"author":"Claude <noreply@anthropic.com>","description":"[aiki]\nagent=claude\n[/aiki]"}"#);

        assert_eq!(
            resolver.resolve("Author: $metadata.author"),
            "Author: Claude <noreply@anthropic.com>"
        );
    }

    #[test]
    fn test_json_field_access_multiple_fields() {
        let mut resolver = VariableResolver::new();
        resolver.add_var(
            "metadata",
            r#"{"author":"Claude","description":"Test description"}"#,
        );

        let result = resolver.resolve("$metadata.author wrote: $metadata.description");
        assert_eq!(result, "Claude wrote: Test description");
    }

    #[test]
    fn test_json_field_access_with_regular_vars() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("metadata", r#"{"author":"Claude"}"#);
        resolver.add_var("cwd", "/tmp");

        let result = resolver.resolve("$metadata.author in $cwd");
        assert_eq!(result, "Claude in /tmp");
    }

    #[test]
    fn test_json_field_access_nonexistent_field() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("metadata", r#"{"author":"Claude"}"#);

        // Should leave undefined field references unchanged
        let result = resolver.resolve("$metadata.nonexistent");
        assert_eq!(result, "$metadata.nonexistent");
    }

    #[test]
    fn test_json_field_access_non_json_variable() {
        let mut resolver = VariableResolver::new();
        resolver.add_var("regular", "plain text");

        // Should not treat as JSON
        let result = resolver.resolve("$regular.field");
        assert_eq!(result, "$regular.field");
    }
}
