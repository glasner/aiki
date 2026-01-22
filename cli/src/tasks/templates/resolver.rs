//! Template resolution and discovery
//!
//! Handles finding template files in the filesystem:
//! - Built-in templates: `.aiki/templates/aiki/`
//! - Custom templates: `.aiki/templates/{namespace}/`

use crate::error::{AikiError, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::parser::parse_template;
use super::types::TaskTemplate;

/// Information about a discovered template
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    /// Template name (e.g., "aiki/review")
    pub name: String,
    /// Full path to the template file
    pub path: PathBuf,
    /// Description from frontmatter (if available)
    pub description: Option<String>,
}

/// Find the templates directory for a project
///
/// Searches upward from the current directory for `.aiki/templates/`
pub fn find_templates_dir(start_path: &Path) -> Result<PathBuf> {
    let mut current = start_path;

    loop {
        let templates_dir = current.join(".aiki").join("templates");
        if templates_dir.is_dir() {
            return Ok(templates_dir);
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(AikiError::TemplatesDirectoryNotFound {
                    path: start_path.join(".aiki/templates").display().to_string(),
                })
            }
        }
    }
}

/// Load a template by name
///
/// # Arguments
/// * `name` - Template name (e.g., "aiki/review", "myorg/refactor-cleanup")
/// * `templates_dir` - The templates directory path
pub fn load_template(name: &str, templates_dir: &Path) -> Result<TaskTemplate> {
    let file_path = resolve_template_path(name, templates_dir)?;
    load_template_file(&file_path, name)
}

/// Resolve a template name to its file path
fn resolve_template_path(name: &str, templates_dir: &Path) -> Result<PathBuf> {
    // Template name is the path within .aiki/templates/
    // e.g., "aiki/review" -> .aiki/templates/aiki/review.md
    let relative_path = format!("{}.md", name);
    let full_path = templates_dir.join(&relative_path);

    if full_path.is_file() {
        return Ok(full_path);
    }

    // Template not found, provide helpful error
    let suggestions = suggest_similar_templates(name, templates_dir);
    Err(AikiError::TemplateNotFound {
        name: name.to_string(),
        expected_path: full_path.display().to_string(),
        suggestions,
    })
}

/// Load a template from a specific file path
pub fn load_template_file(file_path: &Path, name: &str) -> Result<TaskTemplate> {
    let content = fs::read_to_string(file_path).map_err(|e| AikiError::TemplateNotFound {
        name: name.to_string(),
        expected_path: file_path.display().to_string(),
        suggestions: format!("\n  Error reading file: {}", e),
    })?;

    parse_template(&content, name, &file_path.display().to_string())
}

/// Suggest similar template names
fn suggest_similar_templates(name: &str, templates_dir: &Path) -> String {
    let available = list_templates(templates_dir).unwrap_or_default();
    if available.is_empty() {
        return String::new();
    }

    // Find templates that might be similar
    let name_lower = name.to_lowercase();
    let similar: Vec<_> = available
        .iter()
        .filter(|t| {
            let t_lower = t.name.to_lowercase();
            // Check if any part matches
            t_lower.contains(&name_lower)
                || name_lower.contains(&t_lower)
                || t_lower.split('/').any(|p| name_lower.contains(p))
        })
        .take(3)
        .collect();

    if similar.is_empty() {
        let all_names: Vec<_> = available.iter().map(|t| format!("    - {}", t.name)).collect();
        if all_names.is_empty() {
            return String::new();
        }
        return format!(
            "\n\n  Available templates:\n{}",
            all_names.join("\n")
        );
    }

    let suggestions: Vec<_> = similar
        .iter()
        .map(|t| match &t.description {
            Some(desc) => format!("    - {} ({})", t.name, desc),
            None => format!("    - {}", t.name),
        })
        .collect();

    format!("\n\n  Did you mean one of these?\n{}", suggestions.join("\n"))
}

/// List all available templates
pub fn list_templates(templates_dir: &Path) -> Result<Vec<TemplateInfo>> {
    let mut templates = Vec::new();

    if !templates_dir.is_dir() {
        return Ok(templates);
    }

    // Walk the templates directory
    collect_templates(templates_dir, templates_dir, &mut templates)?;

    // Sort by name
    templates.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(templates)
}

/// Recursively collect templates from a directory
fn collect_templates(
    base_dir: &Path,
    current_dir: &Path,
    templates: &mut Vec<TemplateInfo>,
) -> Result<()> {
    let entries = match fs::read_dir(current_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Recurse into subdirectory
            collect_templates(base_dir, &path, templates)?;
        } else if path.is_file() && path.extension().map_or(false, |e| e == "md") {
            // Found a template file
            let relative = path.strip_prefix(base_dir).unwrap_or(&path);
            let name = relative
                .with_extension("")
                .display()
                .to_string()
                .replace('\\', "/"); // Normalize path separators

            // Try to extract description from frontmatter (quick parse)
            let description = extract_description(&path);

            templates.push(TemplateInfo {
                name,
                path: path.clone(),
                description,
            });
        }
    }

    Ok(())
}

/// Quick extraction of description from template file
fn extract_description(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let content = content.trim_start();

    if !content.starts_with("---") {
        return None;
    }

    let after_first = &content[3..];
    let end_idx = after_first.find("\n---")?;
    let yaml_content = &after_first[..end_idx];

    // Simple extraction without full YAML parse
    for line in yaml_content.lines() {
        let line = line.trim();
        if line.starts_with("description:") {
            let value = line[12..].trim();
            // Remove quotes if present
            let value = value.trim_matches('"').trim_matches('\'');
            return Some(value.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_templates(dir: &Path) {
        // Create aiki/review.md
        let aiki_dir = dir.join("aiki");
        fs::create_dir_all(&aiki_dir).unwrap();
        fs::write(
            aiki_dir.join("review.md"),
            r#"---
description: General code review
type: review
---

# Review: {data.scope}

Review the changes.

# Subtasks

## Digest

Examine code.
"#,
        )
        .unwrap();

        // Create myorg/refactor.md
        let myorg_dir = dir.join("myorg");
        fs::create_dir_all(&myorg_dir).unwrap();
        fs::write(
            myorg_dir.join("refactor.md"),
            r#"---
description: Code refactoring workflow
---

# Refactor: {data.scope}

Refactor the code.

# Subtasks

## Identify

Find opportunities.
"#,
        )
        .unwrap();
    }

    #[test]
    fn test_list_templates() {
        let temp_dir = TempDir::new().unwrap();
        create_test_templates(temp_dir.path());

        let templates = list_templates(temp_dir.path()).unwrap();
        assert_eq!(templates.len(), 2);

        let names: Vec<_> = templates.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"aiki/review"));
        assert!(names.contains(&"myorg/refactor"));
    }

    #[test]
    fn test_load_template() {
        let temp_dir = TempDir::new().unwrap();
        create_test_templates(temp_dir.path());

        let template = load_template("aiki/review", temp_dir.path()).unwrap();
        assert_eq!(template.name, "aiki/review");
        assert_eq!(template.description, Some("General code review".to_string()));
        assert_eq!(template.defaults.task_type, Some("review".to_string()));
        assert_eq!(template.parent.name, "Review: {data.scope}");
        assert_eq!(template.subtasks.len(), 1);
    }

    #[test]
    fn test_load_template_not_found() {
        let temp_dir = TempDir::new().unwrap();
        create_test_templates(temp_dir.path());

        let result = load_template("nonexistent/template", temp_dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Template not found"));
        assert!(msg.contains("nonexistent/template"));
    }

    #[test]
    fn test_template_suggestions() {
        let temp_dir = TempDir::new().unwrap();
        create_test_templates(temp_dir.path());

        let result = load_template("review", temp_dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        // Should suggest aiki/review
        assert!(msg.contains("aiki/review"));
    }

    #[test]
    fn test_extract_description() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(
            &file_path,
            r#"---
description: Test description
type: test
---

# Test

Content.
"#,
        )
        .unwrap();

        let desc = extract_description(&file_path);
        assert_eq!(desc, Some("Test description".to_string()));
    }

    #[test]
    fn test_extract_description_quoted() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(
            &file_path,
            r#"---
description: "Quoted description"
---

# Test
"#,
        )
        .unwrap();

        let desc = extract_description(&file_path);
        assert_eq!(desc, Some("Quoted description".to_string()));
    }

    #[test]
    fn test_extract_description_no_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(
            &file_path,
            r#"# Test

No frontmatter here.
"#,
        )
        .unwrap();

        let desc = extract_description(&file_path);
        assert!(desc.is_none());
    }
}
