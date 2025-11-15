use std::collections::HashMap;

/// Resolves variables in strings (e.g., $event.file_path, $cwd, $HOME)
pub struct VariableResolver {
    variables: HashMap<String, String>,
}

impl VariableResolver {
    /// Create a new variable resolver
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    /// Add a variable
    pub fn add_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(key.into(), value.into());
    }

    /// Add event variables (from ExecutionContext)
    pub fn add_event_vars(&mut self, event_vars: &HashMap<String, String>) {
        for (key, value) in event_vars {
            self.variables
                .insert(format!("event.{}", key), value.clone());
        }
    }

    /// Add environment variables
    pub fn add_env_vars(&mut self, env_vars: &HashMap<String, String>) {
        self.variables.extend(env_vars.clone());
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
    pub fn resolve(&self, input: &str) -> String {
        let mut result = input.to_string();

        // Sort variables by length (longest first) to handle overlapping names correctly
        // e.g., $event.file_path before $event.file
        let mut vars: Vec<_> = self.variables.iter().collect();
        vars.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));

        for (key, value) in vars {
            let pattern = format!("${}", key);
            result = result.replace(&pattern, value);
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
    fn test_resolve_event_variable() {
        let mut resolver = VariableResolver::new();
        let mut event_vars = HashMap::new();
        event_vars.insert("file_path".to_string(), "/path/to/file.rs".to_string());
        resolver.add_event_vars(&event_vars);

        assert_eq!(
            resolver.resolve("Editing $event.file_path"),
            "Editing /path/to/file.rs"
        );
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
        let resolver = VariableResolver::new();
        assert_eq!(resolver.resolve("Plain text"), "Plain text");
    }

    #[test]
    fn test_resolve_undefined_variable() {
        let resolver = VariableResolver::new();
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
    fn test_resolve_mixed_variables() {
        let mut resolver = VariableResolver::new();

        let mut event_vars = HashMap::new();
        event_vars.insert("file_path".to_string(), "test.rs".to_string());
        resolver.add_event_vars(&event_vars);

        resolver.add_var("cwd", "/project");

        let mut env_vars = HashMap::new();
        env_vars.insert("HOME".to_string(), "/home/user".to_string());
        resolver.add_env_vars(&env_vars);

        assert_eq!(
            resolver.resolve("File $event.file_path in $cwd (home: $HOME)"),
            "File test.rs in /project (home: /home/user)"
        );
    }

    #[test]
    fn test_resolve_empty_string() {
        let resolver = VariableResolver::new();
        assert_eq!(resolver.resolve(""), "");
    }

    #[test]
    fn test_resolve_dollar_sign_without_variable() {
        let resolver = VariableResolver::new();
        assert_eq!(resolver.resolve("Price: $100"), "Price: $100");
    }
}
