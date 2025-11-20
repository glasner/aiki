use std::collections::HashMap;

/// Resolves variables in strings (e.g., $event.file_paths, $cwd, $HOME)
pub struct VariableResolver {
    variables: HashMap<String, String>,
    // Cached sorted patterns: ("$key", "value")
    cached_patterns: Vec<(String, String)>,
    cache_valid: bool,
}

impl VariableResolver {
    /// Create a new variable resolver
    #[must_use]
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            cached_patterns: Vec::new(),
            cache_valid: false,
        }
    }

    /// Add a variable
    pub fn add_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(key.into(), value.into());
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
            .map(|(k, v)| (format!("${}", k), v.clone()))
            .collect();

        // Sort by pattern length (longest first) to handle overlapping names correctly
        // e.g., $event.file_path before $event.file
        self.cached_patterns
            .sort_by_key(|(pattern, _)| std::cmp::Reverse(pattern.len()));

        self.cache_valid = true;
    }

    /// Resolve all variables in a string
    ///
    /// Supports:
    /// - $event.* variables (e.g., $event.file_path)
    /// - $cwd, $agent
    /// - $ENV_VAR environment variables
    ///
    /// Example:
    /// ```ignore
    /// let mut resolver = VariableResolver::new();
    /// resolver.add_var("event.file_path", "/path/to/file.rs");
    /// resolver.add_var("cwd", "/home/user/project");
    ///
    /// let result = resolver.resolve("File: $event.file_path in $cwd");
    /// assert_eq!(result, "File: /path/to/file.rs in /home/user/project");
    /// ```
    pub fn resolve(&mut self, input: &str) -> String {
        // Fast path: no variables in input at all
        if !input.contains('$') {
            return input.to_string();
        }

        // Ensure cache is built (amortized cost)
        self.rebuild_cache();

        // Fast path: no variables configured
        if self.cached_patterns.is_empty() {
            return input.to_string();
        }

        // Perform substitutions
        let mut result = input.to_string();

        for (pattern, value) in &self.cached_patterns {
            // Only do replacement if pattern exists in the string
            if result.contains(pattern.as_str()) {
                result = result.replace(pattern, value);
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
        resolver.add_var("event.file_path", "long");

        // Should resolve longest match first
        assert_eq!(resolver.resolve("$event.file_path"), "long");
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
}
