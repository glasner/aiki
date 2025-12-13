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
    // Lazy environment variable lookup function
    // Called on-demand when a $VAR is not found in variables
    env_lookup: Option<Box<dyn Fn(&str) -> Option<String>>>,
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
            env_lookup: None,
        }
    }

    /// Set the environment variable lookup function
    ///
    /// This enables lazy per-key lookup of environment variables instead of
    /// eagerly collecting all env vars into a HashMap. This ensures that
    /// runtime `std::env::set_var` / `remove_var` mutations are immediately
    /// visible during flow execution.
    ///
    /// # Example
    ///
    /// ```ignore
    /// resolver.set_env_lookup(|name| std::env::var(name).ok());
    /// ```
    pub fn set_env_lookup<F>(&mut self, lookup: F)
    where
        F: Fn(&str) -> Option<String> + 'static,
    {
        self.env_lookup = Some(Box::new(lookup));
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
            // Include all variables in cache - JSON variables can be used both as
            // $var (raw value) and $var.field (field access)
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

        // Fast path: no variables configured and no env lookup
        if self.cached_patterns.is_empty()
            && self.json_variables.is_empty()
            && self.env_lookup.is_none()
        {
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

        // Lazy env var lookup: resolve remaining $VAR patterns via env_lookup
        if let Some(ref lookup) = self.env_lookup {
            result = self.resolve_env_vars_lazy(&result, lookup);
        }

        result
    }

    /// Resolve environment variables lazily using the lookup function
    ///
    /// Finds $VAR patterns not already resolved and looks them up on-demand.
    /// This ensures runtime env var mutations are immediately visible.
    fn resolve_env_vars_lazy(
        &self,
        input: &str,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> String {
        let mut result = input.to_string();
        let mut pos = 0;

        while let Some(dollar_pos) = result[pos..].find('$') {
            let abs_pos = pos + dollar_pos;

            // Extract variable name (alphanumeric + underscore)
            let name_start = abs_pos + 1;
            if name_start >= result.len() {
                break;
            }

            let name_end = result[name_start..]
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .map(|i| name_start + i)
                .unwrap_or(result.len());

            if name_end > name_start {
                let var_name = &result[name_start..name_end];

                // Skip if it looks like field access ($var.field) - already handled
                if result[name_end..].starts_with('.') {
                    pos = name_end;
                    continue;
                }

                // Skip if this is a known variable (already in cache)
                if self.variables.contains_key(var_name) {
                    pos = name_end;
                    continue;
                }

                // Try lazy env lookup
                if let Some(value) = lookup(var_name) {
                    // Replace the exact occurrence we scanned (abs_pos..name_end)
                    // Using replace_range instead of replacen to avoid corrupting
                    // overlapping variable names (e.g., $VAR inside $VAR1)
                    result.replace_range(abs_pos..name_end, &value);
                    // Don't advance pos - string changed, continue from same position
                    continue;
                }
            }

            pos = abs_pos + 1;
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

    #[test]
    fn test_lazy_env_lookup() {
        let mut resolver = VariableResolver::new();

        // Set up lazy lookup that returns a fixed value for TEST_VAR
        resolver.set_env_lookup(|name| {
            if name == "TEST_VAR" {
                Some("lazy_value".to_string())
            } else {
                None
            }
        });

        // Should resolve via lazy lookup
        assert_eq!(resolver.resolve("Value: $TEST_VAR"), "Value: lazy_value");

        // Unknown vars should remain unchanged
        assert_eq!(resolver.resolve("Value: $UNKNOWN"), "Value: $UNKNOWN");
    }

    #[test]
    fn test_lazy_env_lookup_priority() {
        let mut resolver = VariableResolver::new();

        // Add a regular variable
        resolver.add_var("MY_VAR", "regular_value");

        // Set up lazy lookup that would return different value
        resolver.set_env_lookup(|name| {
            if name == "MY_VAR" {
                Some("env_value".to_string())
            } else {
                None
            }
        });

        // Regular variable should take priority over env lookup
        assert_eq!(resolver.resolve("Value: $MY_VAR"), "Value: regular_value");
    }

    #[test]
    fn test_lazy_env_lookup_overlapping_prefix() {
        // Regression test: $VAR should not corrupt $VAR1 when VAR is defined but VAR1 is not
        let mut resolver = VariableResolver::new();
        resolver.set_env_lookup(|name| {
            if name == "VAR" {
                Some("value".to_string())
            } else {
                None
            }
        });

        // $VAR1 should remain unchanged, $VAR should be replaced
        assert_eq!(
            resolver.resolve("$VAR1 $VAR"),
            "$VAR1 value"
        );

        // Order shouldn't matter
        assert_eq!(
            resolver.resolve("$VAR $VAR1"),
            "value $VAR1"
        );
    }

    #[test]
    fn test_lazy_env_lookup_with_real_env() {
        use std::env;

        let mut resolver = VariableResolver::new();
        resolver.set_env_lookup(|name| env::var(name).ok());

        // Set a test env var
        env::set_var("AIKI_TEST_LAZY_VAR", "test_value_123");

        // Should resolve
        assert_eq!(
            resolver.resolve("Value: $AIKI_TEST_LAZY_VAR"),
            "Value: test_value_123"
        );

        // Clean up
        env::remove_var("AIKI_TEST_LAZY_VAR");

        // After removal, should not resolve (lazy lookup sees the change)
        let mut resolver2 = VariableResolver::new();
        resolver2.set_env_lookup(|name| env::var(name).ok());
        assert_eq!(
            resolver2.resolve("Value: $AIKI_TEST_LAZY_VAR"),
            "Value: $AIKI_TEST_LAZY_VAR"
        );
    }
}
