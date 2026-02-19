# Variable Access and Interpolation Unification

**Status**: Plan  
**Related**: `rhai-for-conditionals.md` (expression evaluation unification)

## Executive Summary

This plan migrates aiki from two different variable interpolation systems (`$var` in hooks, `{{var}}` in templates) to a unified `{{var}}` syntax throughout. 

**Key architectural decision**: Extract a shared interpolation core (`cli/src/interpolation.rs`) that both hooks and templates use. This provides:
- Single source of truth for `{{var}}` parsing
- Reduced surface area for divergent bugs
- Easier Phase 2 (namespace alignment) implementation
- Net -90 LOC despite adding shared infrastructure

**Migration strategy**: Hard break in one release. No backward compatibility for `$var` syntax.

**Effort**: ~9 hours (3 hours for shared core extraction, 6 hours for `$var` → `{{var}}` migration)

## Problem

We have **two different variable access and interpolation systems** in aiki:

### 1. Hooks (`.aiki/hooks/**/*.yml`)

**Variable Access**: `$var` prefix syntax
- `$event.file_paths` - Event variables
- `$cwd` - System variables
- `$HOME` - Environment variables
- `$message` - Let-bound variables

**Interpolation**: Variables are interpolated directly into strings
```yaml
- shell: echo "Files: $event.file_paths in $cwd"
- log: "Processing $event.file_paths"
- jj: describe -m "$message"
```

**Implementation**: `cli/src/flows/variables.rs` (`VariableResolver`)
- Uses `resolver.resolve(string)` to replace `$var` with values
- Single-pass string replacement
- Supports lazy evaluation and JSON field access (`$metadata.author`)
- Env vars resolved lazily via lookup function

### 2. Task Templates (`.aiki/templates/**/*.md`)

**Variable Access**: No prefix, used in double-brace interpolation
- Variables accessed as `data.key`, `item.field`, `source.id`
- No `$` prefix needed

**Interpolation**: `{{var}}` Tera-style double-brace syntax
```markdown
# Review {{data.scope.name}}

Files: {{data.files}}
Source: {{source}}
Parent task: {{parent.id}}
Item: {{item.text}}
```

**Implementation**: `cli/src/tasks/templates/variables.rs` (`VariableContext`)
- Uses `substitute(text, ctx)` to replace `{{var}}` with values
- Single-pass variable substitution
- Explicit error when variable not found
- No recursion (safe by design)

## Inconsistencies

| Feature | Hooks | Templates |
|---------|-------|-----------|
| Variable prefix | `$var` | No prefix (inside `{{}}`) |
| Interpolation syntax | `$var` (direct) | `{{var}}` (Tera-style) |
| Namespaces | `event.*`, `session.*`, env vars, let vars | `data.*`, `source.*`, `parent.*`, `item.*` |
| Field access | `$event.task.type` | `{{data.scope.name}}` |
| Unknown variable | Remains unchanged (`$unknown` → `$unknown`) | Error with hint |
| Implementation | `VariableResolver` (~400 LOC) | `VariableContext` (~300 LOC) |

## Desired State

**Guiding Principle**: Task templates are the reference implementation.

### Unified Syntax

1. **Variable access**: No `$` prefix needed
   - Variables are bare names: `event.task.type`, `data.scope`, `cwd`
   
2. **String values**: Supports both bare names and `{{var}}` interpolation
   - **Bare variable reference**: `prep.ai_author` (when the entire value is just a variable)
     - **Scope**: Only in YAML fields that accept a pure variable reference (`with_author`, `let` RHS, `set` values). NOT in free-form text fields (`shell`, `log`, `jj`, `failure`, `stop`).
   - **Interpolation**: `{{var}}` for embedding in text (works in ALL fields)
   - **Mixed**: `"Author: {{prep.ai_author}}"` (variable in literal text)

3. **Interpolation syntax**: `{{var}}` double-brace (Tera-style)
   - `{{event.file_paths}}` - Event variables
   - `{{data.scope}}` - Data variables
   - `{{cwd}}` - System variables
   - `{{message}}` - Let-bound variables

### Migration Target

**Hooks (YAML)**:
```yaml
# Current (deprecated)
- shell: echo "Files: $event.file_paths in $cwd"
- log: "Task: $event.task.type"
- if: $event.task.type == "review"
- with_author: "$prep.ai_author"

# New (unified)
- shell: echo "Files: {{event.file_paths}} in {{cwd}}"
- log: "Task: {{event.task.type}}"
- if: event.task.type == "review"  # Bare name in conditions (via Rhai)
- with_author: prep.ai_author      # Bare name for pure variable reference
- with_author: "AI {{prep.name}}"  # {{}} for interpolation in strings
```

**Templates (Markdown)** - no change needed:
```markdown
# Review {{data.scope.name}}

Files: {{data.files}}
```

## Solution

**Migration Strategy**: Hard break — no backward compatibility period.

**Architecture Decision**: Extract shared interpolation core to avoid code duplication and reduce surface area for divergent bugs.

### Phase 0: Extract Shared Interpolation Core (New)

**Goal**: Create a shared interpolation implementation that both `VariableResolver` and `VariableContext` can use.

**Why**: After migration, both systems will use `{{var}}` syntax with the same field access patterns. Sharing the parsing and substitution logic will:
- Provide a single source of truth for `{{var}}` parsing
- Reduce test surface area (core parsing tested once)
- Make Phase 2 (namespace alignment) easier
- Prevent divergent bugs in edge case handling

#### Step 0.1: Create `cli/src/interpolation.rs`

Create a new module with shared interpolation primitives:

```rust
//! Shared variable interpolation primitives
//!
//! Provides core parsing and substitution logic for {{var}} syntax
//! used by both hooks (VariableResolver) and templates (VariableContext).

/// A variable reference found during template parsing
#[derive(Debug, Clone, PartialEq)]
pub struct VariableRef {
    /// Start position in original string
    pub start: usize,
    /// End position in original string (exclusive)
    pub end: usize,
    /// Variable name (trimmed, without {{ }})
    pub name: String,
}

/// Parse variable references from text using {{var}} syntax
///
/// Returns vector of (start_pos, end_pos, var_name) tuples.
/// Handles edge cases:
/// - Unclosed braces: `{{var` treated as literal text
/// - Empty refs: `{{}}` treated as literal text
/// - Whitespace: `{{ var }}` trimmed to `var`
/// - Nested braces: Only outermost `{{}}` pairs are recognized
///
/// # Example
/// ```
/// let refs = parse_template("Hello {{name}}, you have {{count}} items");
/// assert_eq!(refs.len(), 2);
/// assert_eq!(refs[0].name, "name");
/// assert_eq!(refs[1].name, "count");
/// ```
pub fn parse_template(text: &str) -> Vec<VariableRef> {
    let mut refs = Vec::new();
    // Use char_indices() to get byte offsets, not char positions.
    // This is critical: chars().enumerate() gives sequential char counts,
    // which differ from byte offsets for non-ASCII text. Since start/end
    // are used to slice the original string (text[start..end]), they must
    // be valid byte offsets.
    let mut chars = text.char_indices().peekable();

    while let Some((pos, c)) = chars.next() {
        if c == '{' {
            if chars.peek().map(|(_, ch)| *ch) == Some('{') {
                // Start of variable reference: {{
                chars.next(); // consume second {
                let ref_start = pos;

                // Read until }}
                let mut var_name = String::new();
                let mut found_close = false;

                while let Some((end_pos, c2)) = chars.next() {
                    if c2 == '}' {
                        if chars.peek().map(|(_, ch)| *ch) == Some('}') {
                            chars.next(); // consume second }
                            // end_pos is byte offset of first }, +2 for both } chars
                            let ref_end = end_pos + 2;
                            found_close = true;

                            // Only add if variable name is non-empty after trimming
                            let trimmed = var_name.trim();
                            if !trimmed.is_empty() {
                                refs.push(VariableRef {
                                    start: ref_start,
                                    end: ref_end,
                                    name: trimmed.to_string(),
                                });
                            }
                            break;
                        }
                        // Single }, not end of variable
                        var_name.push(c2);
                    } else {
                        var_name.push(c2);
                    }
                }

                // If unclosed or empty, continue scanning from after {{
                if !found_close {
                    // Reset position to continue scanning
                    // (already advanced by the while loop)
                }
            }
        }
    }

    refs
}

/// Substitute variables in text using a lookup function
///
/// The lookup function receives variable names and returns their values.
/// Returns `Err` with list of unknown variable names if any variables couldn't be resolved.
///
/// # Example
/// ```
/// let result = substitute_template(
///     "Hello {{name}}!",
///     |var| if var == "name" { Some("World".to_string()) } else { None }
/// );
/// assert!(result.is_ok());
/// assert_eq!(result.unwrap(), "Hello World!");
/// ```
pub fn substitute_template<F>(text: &str, lookup: F) -> Result<String, Vec<String>>
where
    F: Fn(&str) -> Option<String>,
{
    let refs = parse_template(text);
    
    if refs.is_empty() {
        return Ok(text.to_string());
    }

    let mut result = String::with_capacity(text.len());
    let mut last_pos = 0;
    let mut unknown_vars = Vec::new();

    for var_ref in refs {
        // Add text before this variable
        result.push_str(&text[last_pos..var_ref.start]);

        // Try to resolve variable
        match lookup(&var_ref.name) {
            Some(value) => result.push_str(&value),
            None => {
                unknown_vars.push(var_ref.name.clone());
                // Still include the original text for error context
                result.push_str(&text[var_ref.start..var_ref.end]);
            }
        }

        last_pos = var_ref.end;
    }

    // Add remaining text
    result.push_str(&text[last_pos..]);

    if unknown_vars.is_empty() {
        Ok(result)
    } else {
        Err(unknown_vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_template_single_var() {
        let refs = parse_template("Hello {{name}}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "name");
        assert_eq!(refs[0].start, 6);
        assert_eq!(refs[0].end, 14);
    }

    #[test]
    fn test_parse_template_multiple_vars() {
        let refs = parse_template("{{a}} and {{b}} and {{c}}");
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].name, "a");
        assert_eq!(refs[1].name, "b");
        assert_eq!(refs[2].name, "c");
    }

    #[test]
    fn test_parse_template_with_whitespace() {
        let refs = parse_template("{{ name }}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "name");
    }

    #[test]
    fn test_parse_template_empty_braces() {
        let refs = parse_template("Empty {{}} braces");
        assert_eq!(refs.len(), 0); // Empty refs are ignored
    }

    #[test]
    fn test_parse_template_unclosed() {
        let refs = parse_template("Unclosed {{brace");
        assert_eq!(refs.len(), 0); // Unclosed refs are ignored
    }

    #[test]
    fn test_parse_template_single_brace() {
        let refs = parse_template("Single {brace}");
        assert_eq!(refs.len(), 0); // Single braces don't count
    }

    #[test]
    fn test_parse_template_dotted_names() {
        let refs = parse_template("{{data.key}} and {{event.task.id}}");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].name, "data.key");
        assert_eq!(refs[1].name, "event.task.id");
    }

    #[test]
    fn test_parse_template_non_ascii() {
        // Multi-byte chars before variable: byte offsets != char positions
        let text = "Héllo {{name}}";  // é is 2 bytes in UTF-8
        let refs = parse_template(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "name");
        // Verify byte offsets allow correct slicing
        assert_eq!(&text[refs[0].start..refs[0].end], "{{name}}");
        assert_eq!(&text[..refs[0].start], "Héllo ");
    }

    #[test]
    fn test_substitute_template_non_ascii() {
        let result = substitute_template("こんにちは {{name}}!", |var| {
            if var == "name" { Some("世界".to_string()) } else { None }
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "こんにちは 世界!");
    }

    #[test]
    fn test_substitute_template_basic() {
        let result = substitute_template("Hello {{name}}!", |var| {
            if var == "name" { Some("World".to_string()) } else { None }
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello World!");
    }

    #[test]
    fn test_substitute_template_multiple() {
        let result = substitute_template("{{a}} + {{b}} = {{c}}", |var| {
            match var {
                "a" => Some("1".to_string()),
                "b" => Some("2".to_string()),
                "c" => Some("3".to_string()),
                _ => None,
            }
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "1 + 2 = 3");
    }

    #[test]
    fn test_substitute_template_missing_var() {
        let result = substitute_template("Hello {{name}}!", |_| None);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), vec!["name"]);
    }

    #[test]
    fn test_substitute_template_no_vars() {
        let result = substitute_template("Plain text", |_| None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Plain text");
    }
}
```

**Files changed**:
- `cli/src/interpolation.rs` (new file, ~150 LOC)
- `cli/src/lib.rs` (add `mod interpolation;`)

#### Step 0.2: Refactor `VariableContext` to use shared core

Update `cli/src/tasks/templates/variables.rs` to use the shared interpolation:

```rust
use crate::interpolation::{parse_template, substitute_template};

pub fn substitute_with_template_name(
    text: &str,
    ctx: &VariableContext,
    template_name: Option<&str>,
) -> Result<String> {
    let refs = parse_template(text);
    
    // Fast path: no variables
    if refs.is_empty() {
        return Ok(text.to_string());
    }

    let mut result = String::with_capacity(text.len());
    let mut last_pos = 0;
    let mut line_number = 1;

    for var_ref in refs {
        // Track line numbers for error messages
        line_number += text[last_pos..var_ref.start].matches('\n').count();

        // Add text before this variable
        result.push_str(&text[last_pos..var_ref.start]);

        // Try to resolve variable
        match ctx.resolve(&var_ref.name) {
            Some(value) => result.push_str(&value),
            None => {
                // Generate helpful error message
                let hint = generate_error_hint(&var_ref.name);
                let template_info = match template_name {
                    Some(n) => format!("\n  In template: {} (line {})", n, line_number),
                    None => format!("\n  At line: {}", line_number),
                };
                return Err(AikiError::TemplateVariableNotFound {
                    variable: var_ref.name.clone(),
                    hint,
                    template_info,
                });
            }
        }

        last_pos = var_ref.end;
    }

    // Add remaining text
    result.push_str(&text[last_pos..]);

    Ok(result)
}

fn generate_error_hint(var_name: &str) -> String {
    if var_name.starts_with("data.") {
        format!(
            "Use: --data {}=<value>",
            var_name.strip_prefix("data.").unwrap_or(var_name)
        )
    } else if var_name.starts_with("source.") {
        format!(
            "Variable 'source.{}' requires --source with a valid source string (e.g., --source task:<id>)",
            var_name.strip_prefix("source.").unwrap_or(var_name)
        )
    } else if var_name.starts_with("parent.") {
        format!(
            "Variable 'parent.{}' is only available in subtask templates",
            var_name.strip_prefix("parent.").unwrap_or(var_name)
        )
    } else if var_name.starts_with("item.") {
        format!(
            "Variable 'item.{}' is only available in dynamic subtask templates (when iterating over a data source)",
            var_name.strip_prefix("item.").unwrap_or(var_name)
        )
    } else if var_name == "source" {
        "Use: --source <value>".to_string()
    } else {
        format!("Variable '{}' is not a recognized built-in variable", var_name)
    }
}
```

**Changes**:
- Use `parse_template()` for parsing (delete custom parsing logic, ~40 LOC deleted)
- Keep domain-specific error handling and variable resolution
- Net: ~40 LOC deleted, better maintainability

**Files changed**:
- `cli/src/tasks/templates/variables.rs` (~40 LOC deleted, reuses shared parsing)

### Phase 1: Replace `$var` with `{{var}}` (Breaking Change)

**Goal**: Replace VariableResolver's `$var` syntax with `{{var}}` using the shared interpolation core.

#### Step 1.1: Refactor `VariableResolver` to use shared core

Update `cli/src/flows/variables.rs`:

```rust
use crate::interpolation::{parse_template, substitute_template};

impl VariableResolver {
    /// Resolve {{var}} interpolation in a string (NO bare-name fallback).
    ///
    /// Use this for YAML fields that contain free-form text where bare strings
    /// should be treated as literals (shell commands, log messages, jj args, etc.).
    ///
    /// Returns `Err(AikiError::VariableNotFound)` for unknown variables inside `{{}}`.
    pub fn resolve(&mut self, input: &str) -> Result<String, AikiError> {
        // Fast path: no {{ }} markers
        if !input.contains('{') {
            return Ok(input.to_string());
        }

        // Use shared interpolation core
        match substitute_template(input, |var_name| self.get_variable(var_name)) {
            Ok(result) => Ok(result),
            Err(unknown_vars) => {
                // Env-var candidates (ALL_CAPS_DIGITS_UNDERSCORES) get empty-string passthrough
                let non_env_unknowns: Vec<_> = unknown_vars.iter()
                    .filter(|v| !Self::is_env_var_candidate(v))
                    .collect();

                if non_env_unknowns.is_empty() {
                    // All unknowns were env-var candidates — re-substitute with empty
                    let result = substitute_template(input, |var_name| {
                        self.get_variable(var_name)
                            .or_else(|| {
                                if Self::is_env_var_candidate(var_name) {
                                    Some(String::new())
                                } else {
                                    None
                                }
                            })
                    }).unwrap();
                    Ok(result)
                } else {
                    let first_unknown = &non_env_unknowns[0];
                    Err(AikiError::VariableNotFound {
                        variable: first_unknown.to_string(),
                        hint: self.generate_hook_error_hint(first_unknown),
                    })
                }
            }
        }
    }

    /// Resolve with bare-name fallback: if the entire input is a single variable
    /// name (no {{}}), look it up directly. Otherwise, fall back to interpolation.
    ///
    /// Use this ONLY for YAML fields that accept a pure variable reference as their
    /// entire value: `with_author`, `let` RHS, `set` values.
    ///
    /// NOT for: `shell`, `log`, `jj`, `failure`, `stop` — these contain free-form
    /// text where bare strings like "echo hello" or "true" must be literal.
    ///
    /// **Bare-name disambiguation**: A bare string is treated as a variable reference
    /// ONLY if it passes `is_variable_name()` AND resolves successfully. If it looks
    /// like a variable name but doesn't resolve, it falls through to literal treatment
    /// (returned as-is). This prevents misclassifying literal values like `true`,
    /// `false`, `v1.2.3`, or `my.config.key` as broken variable references.
    ///
    /// **Env-var consistency**: Bare env-var candidates (e.g., `HOME`) that are unset
    /// resolve to empty string — matching the behavior of `{{HOME}}` in `resolve()`.
    /// This ensures `with_author: HOME` and `with_author: "{{HOME}}"` behave
    /// identically when HOME is not set.
    ///
    /// To force variable lookup (and get an error if undefined), use `{{var}}` syntax:
    /// - `with_author: prep.ai_author` — bare name, resolved if defined, literal if not
    /// - `with_author: "{{prep.ai_author}}"` — explicit lookup, errors if undefined
    pub fn resolve_or_lookup(&mut self, input: &str) -> Result<String, AikiError> {
        // If the entire input is a bare variable name, try direct lookup first
        if !input.contains('{') && self.is_variable_name(input) {
            if let Some(value) = self.get_variable(input) {
                return Ok(value);
            }
            // Bare name not found. For env-var candidates, substitute empty string
            // (consistent with {{VAR}} behavior in resolve()).
            if Self::is_env_var_candidate(input) {
                return Ok(String::new());
            }
            // Non-env bare name not found — treat as literal string, not an error.
            // If the user wants a hard error on missing variables, they should
            // use {{var}} syntax instead.
            return Ok(input.to_string());
        }

        // Otherwise, use standard interpolation ({{var}} errors on unknown)
        self.resolve(input)
    }

    /// Get variable value (handles regular vars, lazy vars, JSON fields, env vars)
    fn get_variable(&mut self, var_name: &str) -> Option<String> {
        // 1. Check regular variables first
        if let Some(value) = self.variables.get(var_name) {
            return Some(value.clone());
        }

        // 2. Check lazy variables (compute on first access)
        if let Some(Some(compute)) = self.lazy_variables.remove(var_name) {
            let value = compute();
            self.variables.insert(var_name.to_string(), value.clone());
            self.cache_valid = false;
            return Some(value);
        }

        // 3. Check JSON field access (e.g., metadata.author)
        if let Some((base, field)) = var_name.split_once('.') {
            if let Some(json_value) = self.json_variables.get(base) {
                if let Some(field_value) = json_value.get(field) {
                    let value = match field_value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => field_value.to_string().trim_matches('"').to_string(),
                    };
                    return Some(value);
                }
            }
        }

        // 4. Try environment variable lookup (if configured)
        if let Some(ref lookup) = self.env_lookup {
            if let Some(value) = lookup(var_name) {
                return Some(value);
            }
        }

        None
    }

    /// Returns true if the name looks like an environment variable (ALL_CAPS_DIGITS_UNDERSCORES).
    /// Matches `^[A-Z_][A-Z0-9_]*$` — e.g., HOME, HTTP2_PORT, PYTHON3_HOME.
    fn is_env_var_candidate(name: &str) -> bool {
        let mut chars = name.chars();
        match chars.next() {
            Some(first) if first.is_ascii_uppercase() || first == '_' => {
                chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            }
            _ => false,
        }
    }

    fn generate_hook_error_hint(&self, var_name: &str) -> String {
        if var_name.starts_with("event.") {
            "Variable not found in event context. Available: event.file_paths, event.task.id, etc.".to_string()
        } else if var_name.starts_with("session.") {
            "Variable not found in session context. Available: session.task.id, session.cwd, etc.".to_string()
        } else {
            format!("Variable '{}' not defined. Use 'let: {}=<value>' to define it.", var_name, var_name)
        }
    }

    /// Check if string looks like a variable name (no spaces, valid identifier)
    fn is_variable_name(&self, s: &str) -> bool {
        if s.is_empty() || s.contains(char::is_whitespace) {
            return false;
        }
        // Must start with letter or underscore
        let first = s.chars().next().unwrap();
        if !first.is_alphabetic() && first != '_' {
            return false;
        }
        // Rest can be alphanumeric, underscore, or dot
        s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    }
}
```

**Changes**:
- **Remove** all `$var` resolution code (~200 LOC deleted)
- **Remove** manual `{{var}}` parsing (use shared core, ~60 LOC deleted)
- **Add** `get_variable()` helper that handles lazy/JSON/env vars (~40 LOC)
- **Add** `generate_hook_error_hint()` for domain-specific error messages (~15 LOC)
- **Add** `resolve_or_lookup()` for fields that accept bare variable names (~25 LOC)
- **Add** `is_env_var_candidate()` for env-var passthrough logic (~5 LOC)
- **Change** `resolve()` return type from `String` to `Result<String, AikiError>` (interpolation only, no bare-name fallback)
- **Split** bare-name resolution into opt-in `resolve_or_lookup()` to prevent misclassifying literal scalars as variables
- Net: ~260 LOC deleted, ~85 LOC added = **~175 LOC reduction**

**Files changed**:
- `cli/src/flows/variables.rs` (~185 LOC net deletion, uses shared interpolation)
- Tests updated to use `{{var}}` and bare names (~100 LOC changed)
- All call sites updated for `Result` return type (see call-site migration below)

#### Step 1.1a: Call-site migration for `resolve() → Result`

Changing `VariableResolver::resolve()` from `String` to `Result<String, AikiError>` requires updating every call site. This is a mechanical but extensive change.

**`cli/src/flows/engine.rs`** (~25 call sites):

All action handlers call `resolver.resolve(...)` and currently use the return value directly as a `String`. Each must be updated to propagate the error with `?`.

**Important**: Most call sites use `resolve()` (interpolation only). Only fields that accept a pure variable reference as their entire value use `resolve_or_lookup()`. This prevents literal strings like `"echo hello"`, `"true"`, or `"v1.2.3"` from being misinterpreted as variable names.

| Action handler | Method | Migration |
|---------------|--------|-----------|
| `execute_shell()` | `resolve()` | `resolver.resolve(&action.shell)?;` — free-form command text |
| `execute_jj()` | `resolve()` | `resolver.resolve(&action.jj)?;` — free-form jj args |
| `execute_log()` | `resolve()` | `resolver.resolve(&action.log)?;` — free-form log message |
| `execute_task_run()` | `resolve()` | `resolver.resolve(&action.task_run.task_id)?;` — task ID field; use `{{var}}` for variable task IDs |
| `execute_failure()` | `resolve()` | `resolver.resolve(&action.failure)?;` — free-form error message |
| `execute_stop()` | `resolve()` | `resolver.resolve(&action.reason)?;` — free-form reason text |
| `evaluate_switch()` | `resolve()` | `resolver.resolve(&stmt.expression)?;` — expression text |
| `evaluate_condition()` | `resolve()` | `resolver.resolve(expression)?;` — expression text |
| `with_author` handler | `resolve_or_lookup()` | `resolver.resolve_or_lookup(&action.with_author)?;` — value is a variable ref |
| `let` RHS handler | `resolve_or_lookup()` | `resolver.resolve_or_lookup(&rhs)?;` — RHS may be a bare variable |
| `set` value handler | `resolve_or_lookup()` | `resolver.resolve_or_lookup(&value)?;` — value may be a bare variable |
| Various `.map()` calls | `resolve()` | `.map(\|a\| resolver.resolve(a)).transpose()?` |

**Rule of thumb**: If the YAML field's value can be a standalone variable name as its entire content (like `with_author: prep.ai_author`), use `resolve_or_lookup()`. If the field contains free-form text, commands, or expressions (like `shell: echo hello`), use `resolve()`.

All these action handlers already return `Result<_, AikiError>`, so the `?` operator works directly.

**`cli/src/flows/context.rs`** (1 function):

`ContextChunk::resolve_variables()` takes `FnMut(&str) -> String`. Update signature:

```rust
// Before
pub fn resolve_variables<F>(self, mut resolver: F) -> Self
where F: FnMut(&str) -> String

// After
pub fn resolve_variables<F>(self, mut resolver: F) -> Result<Self, AikiError>
where F: FnMut(&str) -> Result<String, AikiError>
```

**Callers of `resolve_variables`** (2 call sites in `engine.rs`):
```rust
// Before
.resolve_variables(|s| resolver.resolve(s));

// After
.resolve_variables(|s| resolver.resolve(s))?;
```

**`cli/src/flows/variables.rs` tests** (~30 assertions):

All test calls `resolver.resolve(...)` must be updated. Since tests expect specific values, use `.unwrap()` for success cases and `.unwrap_err()` for error cases:

```rust
// Before (success case)
assert_eq!(resolver.resolve("Hello $name"), "Hello test");

// After (success case)
assert_eq!(resolver.resolve("Hello {{name}}").unwrap(), "Hello test");

// Before (silent passthrough of unknown)
assert_eq!(resolver.resolve("Value: $undefined"), "Value: $undefined");

// After (error on unknown)
let err = resolver.resolve("Value: {{undefined}}").unwrap_err();
assert!(matches!(err, AikiError::VariableNotFound { .. }));
```

**`cli/src/error.rs`** (1 new variant):

Add `VariableNotFound` variant to `AikiError`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AikiError {
    // ... existing variants ...

    #[error("Variable '{variable}' not found: {hint}")]
    VariableNotFound {
        variable: String,
        hint: String,
    },
}
```

**Summary of files to touch**:

| File | Changes | Effort |
|------|---------|--------|
| `cli/src/flows/variables.rs` | Return type change + tests (~30 assertions) | Medium |
| `cli/src/flows/engine.rs` | Add `?` to ~25 call sites + `.transpose()?` for `.map()` calls | Mechanical |
| `cli/src/flows/context.rs` | Update `resolve_variables` signature + body | Small |
| `cli/src/error.rs` | Add `VariableNotFound` variant | Small |

**Acceptance checks**:
- [ ] `cargo check` passes (all call sites compile with new `Result` type)
- [ ] `cargo test` passes (all tests updated for `Result`/`.unwrap()`)
- [ ] Unknown variable in hook produces helpful error message (manual test)
- [ ] Known variables resolve correctly (no regressions)

#### Step 1.2: Update hook condition syntax (depends on `rhai-for-conditionals.md`)

Hook conditions will use Rhai (from rhai-for-conditionals.md), which expects:
```yaml
# Old (pre-migration)
- if: $event.task.type == "review"

# New (Rhai-based, no prefix)
- if: event.task.type == "review"
```

**Note**: This is handled by the Rhai migration (Phase 2 of `rhai-for-conditionals.md`). No additional work needed here.

#### Step 1.3: Update all templates and tests

Update all files in the codebase that use `$var` syntax.

**Migration sweep — deterministic validation plan:**

The migration must rewrite aiki-specific `$var` patterns while preserving shell env vars and documentation examples showing the old syntax (in "before/after" diffs). This requires explicit rules, not ad-hoc grep exclusions.

##### Rewrite targets (MUST be converted)

These are aiki-specific variable patterns. Every match must be converted to `{{var}}` or bare-name syntax.

| Pattern | Regex | Scope |
|---------|-------|-------|
| Event vars | `\$event\.[a-zA-Z_.]+` | All `.rs`, `.yml`, `.md` files |
| Session vars | `\$session\.[a-zA-Z_.]+` | All `.rs`, `.yml`, `.md` files |
| System vars | `\$cwd` | All `.rs`, `.yml`, `.md` files |
| Let-bound vars | `\$message`, `\$metadata`, `\$description` | All `.rs`, `.yml`, `.md` files |
| Prep vars | `\$prep\.[a-zA-Z_.]+` | All `.rs`, `.yml`, `.md` files |

##### Denylist — MUST NOT be rewritten

| Pattern | Why | Example |
|---------|-----|---------|
| Shell env vars | Legitimate shell syntax | `$PATH`, `$HOME`, `$USER`, `$$`, `$?`, `$!` |
| Regex `$` anchors | Regex end-of-line | `pattern$`, `\$` |
| Markdown code fences showing old syntax in "before" examples | Documentation of the migration itself | `- shell: echo "$event.file_paths"` inside a `diff` block preceded by `-` |
| YAML comments explaining old behavior | Not executed | `# Old: $event.task.id` |

##### Allowlist — files to scan

| Location | File types | Contains |
|----------|-----------|----------|
| `cli/src/**/*.rs` | Rust source | Inline string literals, doc comments, test fixtures |
| `cli/tests/**/*.rs` | Rust test files | Test fixture strings |
| `.aiki/hooks/**/*.yml` | Bundled hook definitions | Actual hook actions (runtime code) |
| `.aiki/templates/**/*.md` | Bundled templates | Template content |
| `docs/**/*.md`, `*.md` (root) | Documentation | Example code blocks |
| `examples/**/*` | Example configs | Example hook/template files |

##### Validation procedure

**Step 1: Generate candidate list**

```bash
# Find all aiki $var occurrences (rewrite targets only)
grep -rnE '\$(event|session|prep|cwd|message|metadata|description)\b' \
  --include='*.rs' --include='*.yml' --include='*.yaml' --include='*.md' \
  . > /tmp/aiki_migration_candidates.txt

echo "Total candidates: $(wc -l < /tmp/aiki_migration_candidates.txt)"
```

**Step 2: Classify each candidate**

For each match, classify it as one of:
- **REWRITE**: Aiki variable in executable context (hook action, test fixture, code) → convert to `{{var}}` or bare name
- **SKIP-DOCS**: Appears in a "before" example in documentation (migration guide, diff block) → leave as-is to show the old syntax
- **SKIP-SHELL**: Shell env var (`$HOME`, `$USER`, etc.) → leave as-is

**Step 3: Execute rewrites**

Only rewrite candidates classified as **REWRITE**. Leave SKIP-* candidates unchanged.

**Step 4: Post-migration validation (zero-tolerance checks)**

```bash
# These patterns should have ZERO matches in executable code after migration.
# Matches in documentation "before" examples are acceptable and must be
# manually verified to be inside diff/example blocks.

# Check 1: No aiki $var in Rust source (excluding comments showing old syntax)
grep -rnE '\$(event|session|prep)\.' --include='*.rs' . | \
  grep -v '// Old:' | grep -v '// Before' | grep -v '// deprecated' | \
  grep -v '#\[doc' && echo "FAIL: Found unrewritten aiki vars in Rust source" || echo "PASS"

# Check 2: No aiki $var in hook YAML files
grep -rnE '\$(event|session|prep|cwd|message|metadata)\b' \
  --include='*.yml' --include='*.yaml' .aiki/ && \
  echo "FAIL: Found unrewritten aiki vars in hooks" || echo "PASS"

# Check 3: No aiki $var in test fixtures (Rust string literals)
grep -rnE '"[^"]*\$(event|session|prep|cwd|message|metadata)[^"]*"' \
  --include='*.rs' cli/tests/ && \
  echo "FAIL: Found unrewritten aiki vars in test fixtures" || echo "PASS"

# Check 4: Documentation review — list remaining $var in .md files for manual audit
grep -rnE '\$(event|session|prep|cwd|message|metadata)\b' \
  --include='*.md' . > /tmp/aiki_migration_docs_remaining.txt
echo "Remaining in docs (manual review): $(wc -l < /tmp/aiki_migration_docs_remaining.txt)"
echo "Each must be inside a 'before' example or diff block."
```

**Step 5: Fixture-based smoke test**

```bash
# Run all tests to catch any runtime regressions from the rewrite
cargo test --workspace

# If any test references old $var syntax, it will either:
# - Fail to compile (if the Rust code was updated but test fixtures weren't)
# - Fail at runtime (if a hook/template test exercises the old syntax)
```

##### Acceptance criteria

- [ ] Zero matches from Check 1 (Rust source)
- [ ] Zero matches from Check 2 (hook YAML)
- [ ] Zero matches from Check 3 (test fixtures)
- [ ] All matches from Check 4 (docs) manually verified as "before" examples
- [ ] `cargo test --workspace` passes
- [ ] Shell env vars (`$PATH`, `$HOME`, `$USER`) confirmed unchanged in hook shell actions

#### Step 1.4: Document unified syntax

Update `HOOKS.md`:
```markdown
## Variable Interpolation

Use `{{var}}` syntax for string interpolation:

```yaml
- shell: echo "Files: {{event.file_paths}}"
- log: "Task {{event.task.id}} completed"
```

**Note**: Shell environment variables like `$PATH`, `$HOME`, `$USER` are passed through unchanged to the shell. Only aiki variables use the `{{var}}` syntax.

```yaml
# Shell env vars work as-is
- shell: echo "User: $USER, Home: $HOME"

# Aiki variables use {{var}}
- shell: echo "Task: {{event.task.id}}"

# Mix both
- shell: echo "$USER processing {{event.file_paths}}"
```

## Conditions

Use bare variable names (no `$` or `{{}}`) in conditions:

```yaml
- if: event.task.type == "review"
  then:
    - log: "Review task"
```
```

### Phase 2: Align Namespaces (Future)

**Goal**: Make variable namespaces consistent between hooks and templates.

Currently:
- Hooks: `event.*`, `session.*`, `cwd`, env vars, let-bound vars (no prefix)
- Templates: `data.*`, `source.*`, `parent.*`, `item.*`, `loop.*`

**Potential unification**:
- Both systems support `event.*` for event data
- Both systems support `data.*` for custom data
- Templates get access to `session.*`, `cwd` (when relevant)
- Hooks get access to `loop.*` (if loop support added)

**Not part of this plan** - defer to future work. Current namespaces are domain-appropriate:
- Hooks care about events, sessions, environment
- Templates care about task data, parent tasks, loop items

## Implementation Plan

### Two-Phase Migration (Breaking Change)

#### Phase 0: Extract Shared Interpolation Core (~3 hours)

**Tasks**:
1. Create `cli/src/interpolation.rs` with shared parsing/substitution primitives
2. Add comprehensive tests for edge cases (unclosed braces, empty refs, whitespace)
3. Refactor `VariableContext` to use shared core (delete ~40 LOC of custom parsing)
4. Update `cli/src/lib.rs` to include new module
5. Verify all template tests still pass

**Deliverables**:
- Shared `parse_template()` and `substitute_template()` functions
- Single source of truth for `{{var}}` parsing
- `VariableContext` uses shared core with zero behavioral changes
- ~150 LOC added (new module), ~40 LOC deleted (template parsing)
- Net: +110 LOC (core infrastructure)

**Files**:
- `cli/src/interpolation.rs` (new file, ~150 LOC with tests)
- `cli/src/lib.rs` (add `mod interpolation;`)
- `cli/src/tasks/templates/variables.rs` (~40 LOC deleted, uses shared parsing)

#### Phase 1: Replace `$var` with `{{var}}` (~6 hours)

**Tasks**:
1. Refactor `VariableResolver` to use shared interpolation core
2. Remove all `$var` resolution code (~260 LOC total deletion)
3. Add `get_variable()` helper for lazy/JSON/env var resolution (~40 LOC)
4. Add bare variable name detection (~20 LOC)
5. Update all tests to use `{{var}}` syntax
6. Carefully update bundled hooks/templates (comprehensive search, preserve shell env vars)
7. Update `HOOKS.md` documentation

**Deliverables**:
- `{{var}}` syntax works in all hook actions
- `$var` syntax completely removed (no backward compat)
- `VariableResolver` uses shared interpolation core
- All tests pass with new syntax
- All repo templates/hooks updated
- Documentation updated

**Files**:
- `cli/src/flows/variables.rs` (~260 LOC deleted, ~60 LOC added = net -200 LOC)
- `cli/src/flows/variables.rs` tests (~150 LOC changed to use `{{var}}`)
- `cli/tests/` test fixtures (update `$var` → `{{var}}`)
- `HOOKS.md` (documentation update)
- Any bundled hook files (update to `{{var}}` syntax)

#### Total Effort: ~9 hours

**Code Impact Summary**:
- Phase 0: +110 LOC (shared core infrastructure)
- Phase 1: -200 LOC (VariableResolver simplification: -260 deleted, +60 added for new features)
- **Net: -90 LOC** (cleaner, more maintainable code)

## Testing Strategy

### Unit Tests

**Phase 0: Shared Interpolation Core (`cli/src/interpolation.rs`)**:
- `parse_template()` finds all `{{var}}` references
- Single variable: `"Hello {{name}}"` → one ref
- Multiple variables: `"{{a}} and {{b}}"` → two refs
- Dotted names: `"{{data.key}}"` → one ref with name "data.key"
- Whitespace trimming: `"{{ name }}"` → name is "name"
- Empty braces: `"Empty {{}}"` → no refs (ignored)
- Unclosed braces: `"{{var"` → no refs (ignored)
- Single braces: `"{brace}"` → no refs (not variables)
- Non-ASCII text: `"Héllo {{name}}"` → correct byte offsets for slicing
- Non-ASCII substitution: `"こんにちは {{name}}!"` → correct replacement
- `substitute_template()` replaces found variables
- Unknown variables error: `"{{unknown}}"` → `Err(vec!["unknown"])`
- Mixed text and variables: `"Hello {{name}}!"` works

**Phase 1: VariableResolver**:
- `{{var}}` interpolation works: `"Task {{count}} ready"`
- `{{event.task.type}}` field access works
- `{{event.file_paths}}` resolves correctly
- Bare variable names work: `prep.ai_author` → resolved value
- Bare names with dots: `event.task.type`, `prep.ai_author`
- Bare names with underscores: `task_count`, `my_var`
- `{{cwd}}` system variables work
- `{{message}}` let-bound variables work
- Empty `{{}}` left unchanged
- Unclosed `{{var` left unchanged
- Lazy variable resolution: `{{session.task.id}}`
- JSON field access: `{{metadata.author}}`
- Literal strings (no variables) pass through unchanged
- **Equivalence test**: `"{{foo}}"` and `"foo"` resolve identically when `foo` is defined
  - Ensures interpolation and bare name resolution return the same value
  - Tests that ordering (interpolation first, then bare fallback) works correctly
- **Unknown variable errors**: `"{{unknown}}"` → `Err(AikiError::VariableNotFound { .. })`
  - Unknown bare variable in `resolve_or_lookup`: `"unknown_var"` → literal passthrough (returned as-is)
  - Unknown `{{var}}` in `resolve_or_lookup`: `"{{unknown_var}}"` → error with hint (explicit lookup)
  - Unknown in interpolation (`resolve`): `"Hello {{typo}}"` → error with hint
  - Domain-specific hints: `event.` prefix → event context hint, `session.` → session hint
  - Env-like names (all caps): `"MY_VAR"` → env var hint
- **Bare-name disambiguation** (in `resolve_or_lookup` only):
  - `"true"` → literal `"true"` (not treated as variable error)
  - `"v1.2.3"` → literal `"v1.2.3"` (not a valid variable name, literal passthrough)
  - `"foo.bar"` → resolved if `foo.bar` is defined, literal `"foo.bar"` if not
  - `"{{foo.bar}}"` → resolved if defined, **error** if not (explicit lookup)
- **Env-var consistency** (both paths produce identical results):
  - `resolve("{{HOME}}")` with HOME unset → `""` (empty string)
  - `resolve_or_lookup("HOME")` with HOME unset → `""` (empty string)
  - `resolve("{{HOME}}")` with HOME="/Users/me" → `"/Users/me"`
  - `resolve_or_lookup("HOME")` with HOME="/Users/me" → `"/Users/me"`

**Breaking change validation**:
- `$var` syntax **does not work** (stays as literal text)
- Tests using `$var` updated to `{{var}}`
- No backward compatibility code remains
- Unknown variables now error instead of silently passing through

**VariableContext** (Phase 0 regression tests):
- All existing template tests still pass
- Behavior unchanged (uses shared core internally)
- Error messages still show helpful hints
- Line number tracking still works

### Integration Tests

**Hook execution**:
- Hooks with `{{var}}` syntax execute correctly
- Shell actions with `{{event.file_paths}}` work
- Log actions with `{{event.task.id}}` work
- JJ actions with `{{message}}` work
- Let bindings with `{{variable}}` work

**Template rendering** (Phase 0 regression):
- Templates continue to work as before
- `{{var}}` syntax unchanged
- All existing template tests pass

## Migration Guide

### For Hook Authors

**This is a breaking change.** `$var` syntax will no longer work after this migration, and unknown variables will now error instead of silently passing through.

**Update your hooks manually**:

**For string interpolation**, replace `$var` with `{{var}}`:
```diff
# shell: action
- shell: echo "Processing $event.file_paths"
+ shell: echo "Processing {{event.file_paths}}"

# log: action
- log: "Task $event.task.id started"
+ log: "Task {{event.task.id}} started"

# jj: action with let-bound variable
- jj: describe -m "$message"
+ jj: describe -m "{{message}}"
```

**For pure variable references**, use bare names (no `$` or `{{}}`):
```diff
# with_author: action
- with_author: "$prep.ai_author"
+ with_author: prep.ai_author

# with_author_and_message: action
- with_author_and_message: $metadata
+ with_author_and_message: metadata

# let: action (right-hand side)
- let: description = $event.task.description
+ let: description = event.task.description
```

**Both styles work**:
```yaml
# Bare name (cleaner for pure variable — resolved if defined, literal if not)
- with_author: prep.ai_author

# Explicit lookup (errors if variable is not defined)
- with_author: "{{prep.ai_author}}"

# Interpolation (needed for mixed content)
- with_author: "AI Agent {{prep.name}}"
```

**Bare names vs `{{var}}` in pure-variable fields** (`with_author`, `let` RHS, `set`):
```yaml
# Bare name: best-effort lookup. If the variable exists, its value is used.
# If not, the literal string is used as-is. No error.
- with_author: prep.ai_author    # → resolved value, or literal "prep.ai_author"
- set: version = v1.2.3          # → literal "v1.2.3" (not a defined variable)
- set: enabled = true            # → literal "true"

# {{var}}: mandatory lookup. If the variable doesn't exist, it's an error.
- with_author: "{{prep.ai_author}}"  # → resolved value, or ERROR if undefined
```

**Shell environment variables don't need changes**:
```yaml
# Shell env vars like $PATH, $USER, $HOME remain unchanged
- shell: echo "User: $USER, Path: $PATH"  # ✓ Already correct

# Only change aiki-specific variables
- shell: echo "$USER processing $event.file_paths"
+ shell: echo "$USER processing {{event.file_paths}}"
```

**Also update conditions** (via Rhai migration):

Remove `$` prefix from conditions (handled by `rhai-for-conditionals.md`):

```diff
- if: $event.task.type == "review"
+ if: event.task.type == "review"
```

**Common patterns**:
- Aiki string interpolation: `$event.*` → `{{event.*}}`
- Aiki string interpolation: `$cwd` → `{{cwd}}`
- Aiki string interpolation: `$session.*` → `{{session.*}}`
- Pure variable reference: `$metadata` → `metadata`
- Let right-hand side: `let: x = $var` → `let: x = var`
- Shell env vars: `$PATH`, `$USER`, `$HOME` → **unchanged** (passed to shell as-is)

**Check for typos**:
```bash
# After updating, test that hooks work
# Any typos in variable names will now be caught as errors:
aiki task start "Test task"

# Example error you might see:
# Error: Variable 'evnet.task.id' not found in event context
# Hint: Did you mean 'event.task.id'?
```

**If you see "variable not found" errors**:
1. Check for typos in variable names (`evnet` vs `event`, `taks` vs `task`)
2. Verify the variable is defined in the hook context (event.*, session.*, let bindings)
3. For environment variables, use `let:` to define them if needed

### For Template Authors

**No changes needed** - templates already use `{{var}}` syntax and already error on unknown variables.

## Breaking Changes

### Single Release (Hard Break)

**Breaking changes in this release**:

1. **`$var` syntax completely removed**
   - Hooks using `$var` for aiki variables will fail
   - String interpolation like `"$event.file_paths"` will not work
   - Let bindings with `$var` will not work
   - **Note**: Shell environment variables like `$PATH`, `$USER`, `$HOME` are unchanged and passed through to shell

2. **Unknown variables now error** (NEW)
   - Hooks with typos in variable names will fail: `{{evnet.task.id}}` → error
   - Previously: Unknown variables silently passed through (`{{unknown}}` → `{{unknown}}`)
   - Now: Unknown variables trigger helpful error messages
   - **Impact**: Latent typos will be discovered and must be fixed
   - **Benefit**: Prevents silent failures and runtime surprises

**What still works**:
- Templates (already use `{{var}}`)
- Conditions (migrated to Rhai, no prefix)
- Environment variables (optional by nature, won't error if not set)

**Migration required before upgrade**:
1. Manually update all hook files: aiki `$var` → `{{var}}` (preserve shell `$PATH`, `$USER`, etc.)
2. Update conditions: remove `$` prefix (use bare names)
3. **Fix any typos in variable names** (will now be caught as errors)
4. Test updated hooks

**Release includes**:
- `{{var}}` syntax implementation
- `$var` syntax completely removed (~200 LOC deleted)
- Error on unknown variables (typo prevention)
- Updated documentation
- Breaking change notice in release notes

## Coordination with Rhai Migration

This plan **coordinates with** `rhai-for-conditionals.md`:

| Feature | rhai-for-conditionals.md | variable-access-unification.md |
|---------|--------------------------|--------------------------------|
| **Conditions** | Migrates to Rhai: `event.task.type == "review"` (no `$`) | No action needed (Rhai handles it) |
| **String interpolation** | No change | Migrates to `{{var}}` syntax |
| **Variable resolution** | Rhai evaluates expressions | VariableResolver handles string interpolation |

**Execution order** (both done in same release):
1. `rhai-for-conditionals.md`: Replace hook conditions with Rhai (removes `$var` from conditions)
2. `variable-access-unification.md`: Replace VariableResolver with `{{var}}` (removes `$var` from actions)
3. Both breaking changes shipped together

**Result**: Unified syntax across aiki:
- Conditions: Rhai expressions (no prefix): `event.task.type == "review"`
- Interpolation: `{{var}}` (template-style): `{{event.file_paths}}`
- Templates: No changes needed (already use `{{var}}`)

## Success Metrics

**Phase 0 (Shared Core)**:
- [ ] `cli/src/interpolation.rs` created with `parse_template()` and `substitute_template()`
- [ ] Comprehensive tests for edge cases (unclosed braces, empty refs, whitespace)
- [ ] `VariableContext` refactored to use shared core
- [ ] All template tests pass with zero behavioral changes
- [ ] Net: +110 LOC (shared infrastructure)

**Phase 1 (Migration)**:
- [ ] `{{var}}` syntax works in all hook actions
- [ ] `$var` syntax completely removed (no backward compat code)
- [ ] `VariableResolver` uses shared interpolation core
- [ ] Zero regressions in hook or template behavior
- [ ] Documentation updated with new syntax only
- [ ] All repo hooks/templates updated to `{{var}}`
- [ ] All tests pass with new syntax
- [ ] Net: -200 LOC (VariableResolver simplification)

**Overall**:
- [ ] Single source of truth for `{{var}}` parsing
- [ ] Code reduction: +110 LOC (core), -200 LOC (hooks) = **net -90 LOC**
- [ ] Release notes include clear breaking change notice with migration steps
- [ ] Phase 2 (namespace alignment) will be easier due to shared core

## Design Decisions

### 1. Error Handling for Unknown Variables (DECISION: Error on unknown)

**Problem**: Two different behaviors exist for unknown variables today:
- **VariableResolver** (hooks, current): Silent passthrough (`$unknown` → `$unknown`)
- **VariableContext** (templates, current): Errors with helpful hint

**Decision**: **Error on unknown variables**, with env-var passthrough as the sole exception (see table below).

The shared core `substitute_template()` returns `Result<String, Vec<String>>` — it returns `Err` with the list of unresolved variable names. Callers add domain-specific error messages (hook hints vs template hints).

**Rationale**:
1. **Typo prevention**: Silent passthrough allows typos to slip through (`{{evnet.task.id}}` vs `{{event.task.id}}`)
2. **Fail fast**: Errors at hook/template parse time prevent runtime surprises
3. **Better UX**: Helpful error messages guide users to fix issues immediately
4. **Consistency**: Same behavior in hooks and templates
5. **Breaking change acceptable**: This is already a breaking migration (`$var` → `{{var}}`), so changing error behavior is acceptable

#### Unknown-Variable Behavior by Category

| Category | Example | Context | On unresolved | Rationale |
|----------|---------|---------|---------------|-----------|
| **Hook namespaced vars** | `event.task.id`, `session.cwd` | `{{event.task.id}}` in hook actions | **Error** with domain hint | Typos in known namespaces should fail fast |
| **Hook let-bound vars** | `message`, `description` | `{{message}}` in `resolve()` fields | **Error** with "use `let:` to define" hint | Let-bound vars are explicitly defined; missing = bug. In `resolve_or_lookup()` fields, bare `message` follows the bare-name rule (literal passthrough if undefined). |
| **Template vars** | `data.key`, `source.id`, `parent.id`, `item.text` | `{{data.key}}` in templates | **Error** with template-specific hint | Already the existing behavior; no change |
| **Env vars (via env lookup)** | `HOME`, `PATH`, `MY_CUSTOM_VAR` | `{{HOME}}` or bare `HOME` in hook actions | **Passthrough** (substitute empty string) — **consistent across both paths** | Env vars are external, optional, and set-dependent; erroring on unset env vars would break hooks across environments. Both `resolve()` (interpolation: `{{HOME}}`) and `resolve_or_lookup()` (bare name: `HOME`) substitute empty string for unset env-var candidates. This ensures `with_author: HOME` and `with_author: "{{HOME}}"` behave identically. |
| **Bare name (not a variable)** | `echo hello`, `true`, `some literal` | Bare string in hook YAML value | **Literal passthrough** (not treated as variable) | Strings that don't match `is_variable_name()` are literal text, not variable references |
| **Bare name (matches variable pattern)** | `prep.ai_author` | Bare string in `resolve_or_lookup` YAML field | **Literal passthrough** if not found in variable context | Bare names are best-effort: resolved if defined, literal if not. This avoids misclassifying values like `true`, `false`, `v1.2.3`, or `my.config.key` as broken variable references. Use `{{var}}` for mandatory lookup with error on undefined. |

**How env-var passthrough works in `get_variable()`**:

The env lookup is step 4 (last resort) in `get_variable()`. When a name reaches env lookup and the env var is not set, `get_variable()` returns `None`. Both callers then apply the env-var candidate check consistently:

- **`resolve()`** (interpolation path): `substitute_template()` reports the name as unknown. The error handler recognizes env-like names (matching `^[A-Z_][A-Z0-9_]*$`) and substitutes an empty string instead of erroring.
- **`resolve_or_lookup()`** (bare-name path): After `get_variable()` returns `None`, checks `is_env_var_candidate()` and substitutes an empty string if matched.

**Precise rule**: A name is treated as an env-var candidate when it matches `^[A-Z_][A-Z0-9_]*$` — i.e., starts with an uppercase letter or underscore, followed by uppercase letters, digits, or underscores. This covers common env var names like `HOME`, `HTTP2_PORT`, `PYTHON3_HOME`, etc. For these names, both `resolve()` and `resolve_or_lookup()` substitute an empty string and log a debug-level warning instead of returning `Err`. All other unresolved names are handled per their respective path (error in `resolve()`, literal passthrough in `resolve_or_lookup()`).

**Implementation**:

**Shared core** (`cli/src/interpolation.rs`):
- `substitute_template()` changes return type to `Result<String, Vec<String>>`
- Returns list of unknown variable names instead of silent passthrough
- Caller decides how to format error messages (domain-specific hints)

```rust
/// Substitute variables in text using a lookup function
///
/// Returns `Err` with list of unknown variable names if any variables couldn't be resolved.
pub fn substitute_template<F>(text: &str, lookup: F) -> Result<String, Vec<String>>
where
    F: Fn(&str) -> Option<String>,
{
    let refs = parse_template(text);
    
    if refs.is_empty() {
        return Ok(text.to_string());
    }

    let mut result = String::with_capacity(text.len());
    let mut last_pos = 0;
    let mut unknown_vars = Vec::new();

    for var_ref in refs {
        result.push_str(&text[last_pos..var_ref.start]);

        match lookup(&var_ref.name) {
            Some(value) => result.push_str(&value),
            None => {
                unknown_vars.push(var_ref.name.clone());
                // Still include the original text for error context
                result.push_str(&text[var_ref.start..var_ref.end]);
            }
        }

        last_pos = var_ref.end;
    }

    result.push_str(&text[last_pos..]);

    if unknown_vars.is_empty() {
        Ok(result)
    } else {
        Err(unknown_vars)
    }
}
```

**VariableResolver** (`cli/src/flows/variables.rs`):
- `resolve()` returns `Result<String, AikiError>` — interpolation only, no bare-name fallback
- `resolve_or_lookup()` returns `Result<String, AikiError>` — with bare-name fallback for specific fields
- Generates helpful error messages for unknown variables in hooks
- See Step 1.1 for the canonical code (not duplicated here to avoid drift)

```rust
// resolve() — interpolation only (for shell, log, jj, etc.)
pub fn resolve(&mut self, input: &str) -> Result<String, AikiError> {
    if !input.contains('{') {
        return Ok(input.to_string()); // No bare-name fallback
    }
    match substitute_template(input, |var_name| self.get_variable(var_name)) {
        Ok(result) => Ok(result),
        Err(unknown_vars) => {
            // Env-var candidates (ALL_CAPS_DIGITS_UNDERSCORES) get empty-string passthrough
            let non_env_unknowns: Vec<_> = unknown_vars.iter()
                .filter(|v| !Self::is_env_var_candidate(v))
                .collect();

            if non_env_unknowns.is_empty() {
                // All unknowns were env-var candidates — substitute empty
                for var in &unknown_vars {
                    debug!("Unset env var '{}' in template, substituting empty string", var);
                }
                // (the result string already has {{VAR}} placeholders;
                //  re-substitute with empty strings)
                let result = substitute_template(input, |var_name| {
                    self.get_variable(var_name)
                        .or_else(|| {
                            if Self::is_env_var_candidate(var_name) {
                                Some(String::new()) // empty for unset env vars
                            } else {
                                None
                            }
                        })
                }).unwrap(); // safe: all unknowns now resolve
                Ok(result)
            } else {
                let first_unknown = &non_env_unknowns[0];
                Err(AikiError::VariableNotFound {
                    variable: first_unknown.to_string(),
                    hint: self.generate_hook_error_hint(first_unknown),
                })
            }
        }
    }
}

/// Returns true if the name looks like an environment variable (ALL_CAPS_DIGITS_UNDERSCORES).
/// Matches `^[A-Z_][A-Z0-9_]*$` — e.g., HOME, HTTP2_PORT, PYTHON3_HOME.
fn is_env_var_candidate(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() || first == '_' => {
            chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        }
        _ => false,
    }
}

fn generate_hook_error_hint(&self, var_name: &str) -> String {
    if var_name.starts_with("event.") {
        "Variable not found in event context. Available: event.file_paths, event.task.id, etc.".to_string()
    } else if var_name.starts_with("session.") {
        "Variable not found in session context. Available: session.task.id, session.cwd, etc.".to_string()
    } else {
        format!("Variable '{}' not defined. Use 'let: {}=<value>' to define it.", var_name, var_name)
    }
}
```

**Environment variable handling** (see decision table above for full rules):

Env-var candidates (names matching `ALL_CAPS_WITH_UNDERSCORES`) resolve to empty string when unset, with a debug-level warning. This is the sole exception to the "error on unknown" rule. The rationale: env vars are external, optional, and vary by environment — erroring on unset env vars would make hooks non-portable across machines.

All other unresolved names produce `Err(AikiError::VariableNotFound { .. })` with a domain-specific hint.

**Migration impact**:
- **Breaking change**: Hooks with typos will now fail instead of silently passing through
- **User benefit**: Errors caught at hook execution time with helpful messages
- **Documentation**: Migration guide warns about this change

### 2. Bare-Name Disambiguation in `resolve_or_lookup` (DECISION: Best-effort lookup, literal fallback)

**Problem**: Fields that accept bare variable names (`with_author`, `let` RHS, `set` values) use `is_variable_name()` to detect whether the input looks like a variable. But `is_variable_name()` matches any identifier-like string — including values like `true`, `false`, `v1.2.3`, `foo.bar`, or `main` — that users may intend as literal values, not variable references.

If bare-name lookup errors on undefined variables, these literal values would produce confusing "variable not found" errors.

**Decision**: **Best-effort lookup with literal fallback.** In `resolve_or_lookup()`:
1. If the input matches `is_variable_name()` AND resolves to a defined variable → use the variable's value
2. If the input matches `is_variable_name()` but does NOT resolve → treat as literal string (return as-is, no error)
3. If the input contains `{{var}}` → use `resolve()` (mandatory lookup, errors on undefined)

**Rationale**:
1. **Avoids false positives**: `true`, `false`, `v1.2.3` are not erroneously treated as broken variables
2. **Predictable user experience**: bare names are a convenience shorthand; `{{var}}` is the explicit/strict form
3. **No quoting gymnastics**: users don't need to learn escape rules or special quoting to pass literal values
4. **Clear upgrade path**: if you want guaranteed resolution with error on missing, use `{{var}}`

**Trade-off**: A typo in a bare variable name (e.g., `prpe.ai_author` instead of `prep.ai_author`) silently falls through as a literal string. Users who want strict validation should use `{{var}}` syntax. This is documented in the migration guide.

**Examples** (all in `resolve_or_lookup` context):
| Input | Variable defined? | Result |
|-------|-------------------|--------|
| `prep.ai_author` | Yes (`"Claude"`) | `"Claude"` |
| `prep.ai_author` | No | `"prep.ai_author"` (literal) |
| `true` | No | `"true"` (literal) |
| `v1.2.3` | No | `"v1.2.3"` (literal, fails `is_variable_name` due to digits after dot) |
| `HOME` | Yes (env set) | `"/Users/me"` |
| `HOME` | No (env unset) | `""` (empty string — env-var candidate passthrough) |
| `"{{prep.ai_author}}"` | Yes | `"Claude"` |
| `"{{prep.ai_author}}"` | No | **Error**: Variable 'prep.ai_author' not found |
| `"{{HOME}}"` | No (env unset) | `""` (empty string — consistent with bare `HOME`) |

### 3. Escape Syntax (DECISION: Keep current behavior)

**Current behavior**: Single `{` is literal, `{{}}` is empty var (left unchanged)

**Decision**: Keep current behavior, document it.

Users can write `{` or `}` normally; `{{` is only special with matching `}}`.

### 4. Namespace Alignment (DECISION: Defer to Phase 2)

**Decision**: Defer to future work (domains differ enough to warrant separate namespaces).

- Hooks: focus on events, sessions, environment
- Templates: focus on task data, parent tasks, loop items

## References

- `rhai-for-conditionals.md` - Expression evaluation unification
- `cli/src/flows/variables.rs` - Hook variable resolver
- `cli/src/tasks/templates/variables.rs` - Template variable context
- `cli/src/flows/engine.rs` - Hook action execution
- `HOOKS.md` - Hook documentation
- `TEMPLATES.md` - Template documentation (if exists)
