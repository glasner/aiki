//! Template resolution and discovery
//!
//! Handles finding template files in the filesystem:
//! - Built-in templates: `.aiki/templates/aiki/`
//! - Custom templates: `.aiki/templates/{namespace}/`

use crate::error::{AikiError, Result};
use crate::tasks::{generate_task_id, write_event, TaskEvent, TaskPriority};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::parser::parse_template;
use super::types::{TaskDefinition, TaskTemplate};
use super::variables::{substitute, VariableContext};
use crate::tasks::types::TaskComment;

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

    let mut template = parse_template(&content, name, &file_path.display().to_string())?;

    // Store the source path and raw content for display purposes
    template.source_path = Some(file_path.display().to_string());
    template.raw_content = Some(content);

    Ok(template)
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

/// Create tasks from a template with variable substitution
///
/// # Arguments
/// * `template` - The loaded TaskTemplate
/// * `variables` - Variables for substitution in the template
/// * `data_source` - Optional comments for subtask iteration (will be generalized when other sources are added)
///
/// # Returns
/// A tuple of (parent_task_definition, subtask_definitions) with all variables resolved
pub fn create_tasks_from_template(
    template: &TaskTemplate,
    variables: &VariableContext,
    data_source: Option<Vec<TaskComment>>,
) -> Result<(TaskDefinition, Vec<TaskDefinition>)> {
    // Resolve parent task variables
    let parent_name = substitute(&template.parent.name, variables)?;
    let parent_instructions = substitute(&template.parent.instructions, variables)?;

    let parent = TaskDefinition {
        name: parent_name,
        task_type: template.defaults.task_type.clone(),
        instructions: parent_instructions,
        priority: template.parent.priority.clone(),
        assignee: template.parent.assignee.clone(),
        sources: template.parent.sources.clone(),
        data: template.parent.data.clone(),
    };

    // Handle subtasks based on whether we have a dynamic or static template
    let subtasks = if template.subtasks_source.is_some() {
        // Dynamic subtasks: iterate over data source
        create_dynamic_subtasks(template, variables, data_source)?
    } else {
        // Static subtasks: just substitute variables
        create_static_subtasks(template, variables)?
    };

    Ok((parent, subtasks))
}

/// Create static subtasks by substituting variables in each predefined subtask
fn create_static_subtasks(
    template: &TaskTemplate,
    variables: &VariableContext,
) -> Result<Vec<TaskDefinition>> {
    let mut subtasks = Vec::new();

    for subtask in &template.subtasks {
        let name = substitute(&subtask.name, variables)?;
        let instructions = substitute(&subtask.instructions, variables)?;

        subtasks.push(TaskDefinition {
            name,
            task_type: None, // Subtasks inherit type from parent
            instructions,
            priority: subtask.priority.clone(),
            assignee: subtask.assignee.clone(),
            sources: subtask.sources.clone(),
            data: subtask.data.clone(),
        });
    }

    Ok(subtasks)
}

/// Create dynamic subtasks by iterating over the data source
fn create_dynamic_subtasks(
    template: &TaskTemplate,
    variables: &VariableContext,
    data_source: Option<Vec<TaskComment>>,
) -> Result<Vec<TaskDefinition>> {
    // If no data source or empty, return empty subtasks
    let comments = match data_source {
        Some(comments) if !comments.is_empty() => comments,
        _ => return Ok(Vec::new()),
    };

    // Get the subtask template content
    let subtask_template_content = match &template.subtask_template {
        Some(content) => content,
        None => return Ok(Vec::new()),
    };

    // Parse the subtask template to get heading, frontmatter, and body
    let parsed = match parse_subtask_template(subtask_template_content)? {
        Some(parsed) => parsed,
        None => return Ok(Vec::new()),
    };

    let mut subtasks = Vec::new();

    for comment in comments {
        // Create a new VariableContext with parent.* namespace populated from parent variables
        let mut subtask_ctx = VariableContext::new();

        // Copy parent data into parent.* namespace (accessible as {parent.data.key})
        // We use the prefix "data." so {parent.data.scope} maps to parent["data.scope"]
        for (key, value) in &variables.data {
            subtask_ctx.set_parent(&format!("data.{}", key), value);
        }

        // Copy parent builtins into parent.* namespace (accessible as {parent.key})
        for (key, value) in &variables.builtins {
            subtask_ctx.set_parent(key, value);
        }

        // Copy parent source info (accessible as {parent.source} and {parent.source.*})
        if let Some(ref source) = variables.source {
            subtask_ctx.set_parent("source", source);
            for (key, value) in &variables.source_data {
                subtask_ctx.set_parent(&format!("source.{}", key), value);
            }
            // Also set source for this subtask (inherits parent source)
            subtask_ctx.set_source(source);
        }

        // Add {item.text} variable from comment.text
        subtask_ctx.set_item("text", &comment.text);

        // Add structured data as {item.*} variables from the comment
        for (key, value) in &comment.data {
            subtask_ctx.set_item(key, value);
        }

        // Copy parent data as {data.*} (accessible in subtasks for CLI-provided data)
        for (key, value) in &variables.data {
            subtask_ctx.set_data(key, value);
        }

        // Substitute variables in the heading and body
        let name = substitute(&parsed.heading, &subtask_ctx)?;
        let instructions = substitute(&parsed.body, &subtask_ctx)?;

        // Start with comment metadata as base data (persisted for later use)
        let mut data: HashMap<String, serde_json::Value> = comment.data.iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();

        // Apply frontmatter if present, with variable substitution
        let (priority, assignee, sources) = if let Some(ref fm) = parsed.frontmatter {
            let priority = fm.priority.as_ref()
                .map(|p| substitute(p, &subtask_ctx))
                .transpose()?;
            let assignee = fm.assignee.as_ref()
                .map(|a| substitute(a, &subtask_ctx))
                .transpose()?;
            let sources: Vec<String> = fm.sources.iter()
                .map(|s| substitute(s, &subtask_ctx))
                .collect::<Result<Vec<_>>>()?;
            // Substitute variables in frontmatter data values and merge into data
            // Frontmatter data takes precedence over comment data
            for (k, v) in &fm.data {
                let substituted = match v {
                    serde_json::Value::String(s) => {
                        substitute(s, &subtask_ctx).map(serde_json::Value::String)?
                    }
                    other => other.clone(),
                };
                data.insert(k.clone(), substituted);
            }
            (priority, assignee, sources)
        } else {
            (None, None, Vec::new())
        };

        subtasks.push(TaskDefinition {
            name,
            task_type: None, // Subtasks inherit type from parent
            instructions,
            priority,
            assignee,
            sources,
            data,
        });
    }

    Ok(subtasks)
}

/// Parsed subtask template with optional frontmatter
#[derive(Debug)]
struct ParsedSubtaskTemplate {
    /// The heading template (e.g., "Fix: {data.file}:{data.line}")
    heading: String,
    /// Optional frontmatter (priority, assignee, sources, data)
    frontmatter: Option<super::types::SubtaskFrontmatter>,
    /// The body/instructions template
    body: String,
}

/// Parse a subtask template section into heading, optional frontmatter, and body
///
/// The subtask template should contain an h2 heading (## ...) optionally followed
/// by YAML frontmatter, then body content.
///
/// # Example without frontmatter
/// ```ignore
/// ## Fix: {data.file}:{data.line}
///
/// {text}
/// ```
///
/// # Example with frontmatter
/// ```ignore
/// ## Fix: {data.file}:{data.line}
/// ---
/// sources:
///   - task:{source.task_id}
/// priority: p1
/// ---
///
/// {text}
/// ```
fn parse_subtask_template(content: &str) -> Result<Option<ParsedSubtaskTemplate>> {
    let content = content.trim();

    // Find the h2 heading line
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading_text) = trimmed.strip_prefix("## ") {
            let heading = heading_text.trim().to_string();

            // Find where the heading line ends
            if let Some(heading_end_pos) = content.find(line) {
                let after_heading = &content[heading_end_pos + line.len()..];
                let after_heading = after_heading.trim_start_matches('\n');

                // Check if there's frontmatter after the heading
                let (frontmatter, body) = super::parser::extract_yaml_frontmatter::<super::types::SubtaskFrontmatter>(after_heading)
                    .map_err(|e| AikiError::TemplateFrontmatterInvalid {
                        file: "(subtask template)".to_string(),
                        details: e.to_string(),
                    })?;

                return Ok(Some(ParsedSubtaskTemplate {
                    heading,
                    frontmatter,
                    body,
                }));
            }
        }
    }

    Ok(None)
}

/// Create review task with subtasks from template
///
/// This function is shared between the `aiki review` CLI command and the flow
/// engine's `review:` action. It:
/// 1. Loads the specified template
/// 2. Sets up the variable context with scope information
/// 3. Creates the parent task and subtasks from the template
/// 4. Writes all task events to storage
///
/// # Arguments
/// * `cwd` - The current working directory
/// * `scope_name` - Human-readable scope description (e.g., "task abc123" or "current session")
/// * `scope_id` - The scope identifier (task ID or "session")
/// * `assignee` - Optional assignee for the review task
/// * `template_name` - Template name (e.g., "aiki/review")
///
/// # Returns
/// The task ID of the created review parent task
pub fn create_review_task_from_template(
    cwd: &Path,
    scope_name: &str,
    scope_id: &str,
    assignee: &Option<String>,
    template_name: &str,
) -> Result<String> {
    let timestamp = chrono::Utc::now();
    let working_copy = get_working_copy_change_id(cwd);

    // Load the template
    let templates_dir = find_templates_dir(cwd)?;
    let template = load_template(template_name, &templates_dir)?;

    // Set up variable context for template substitution
    let mut variables = VariableContext::new();

    // Set scope variables as builtins (template uses {scope}, {scope.name}, {scope.id})
    variables.set_builtin("scope", scope_id);
    variables.set_builtin("scope.name", scope_name);
    variables.set_builtin("scope.id", scope_id);

    // Create tasks from template (no data source for review - static subtasks)
    let (parent_def, subtask_defs) = create_tasks_from_template(&template, &variables, None)?;

    // Generate parent task ID from the resolved name
    let parent_id = generate_task_id(&parent_def.name);

    // Determine task type from template defaults or definition
    let task_type = parent_def.task_type.or(template.defaults.task_type.clone());

    // Determine priority from template
    let priority = parent_def
        .priority
        .as_ref()
        .and_then(|p| parse_priority(p))
        .or(template.defaults.priority.as_ref().and_then(|p| parse_priority(p)))
        .unwrap_or(TaskPriority::P2);

    // Build sources list
    let mut sources = parent_def.sources.clone();
    if scope_id != "session" && !sources.iter().any(|s| s.starts_with("task:")) {
        sources.push(format!("task:{}", scope_id));
    }

    // Create parent task event
    let parent_event = TaskEvent::Created {
        task_id: parent_id.clone(),
        name: parent_def.name.clone(),
        task_type,
        priority,
        assignee: assignee.clone(),
        sources,
        template: Some(template.template_id()),
        working_copy: working_copy.clone(),
        instructions: Some(parent_def.instructions.clone()),
        data: convert_data(&parent_def.data),
        timestamp,
    };
    write_event(cwd, &parent_event)?;

    // Create subtasks
    for (i, subtask_def) in subtask_defs.iter().enumerate() {
        let subtask_id = format!("{}.{}", parent_id, i + 1);

        let subtask_priority = subtask_def
            .priority
            .as_ref()
            .and_then(|p| parse_priority(p))
            .unwrap_or(priority);

        // Subtask sources: link to parent
        let mut subtask_sources = subtask_def.sources.clone();
        if !subtask_sources.iter().any(|s| s.starts_with("task:")) {
            subtask_sources.push(format!("task:{}", parent_id));
        }

        let subtask_event = TaskEvent::Created {
            task_id: subtask_id,
            name: subtask_def.name.clone(),
            task_type: Some("review".to_string()), // Subtasks inherit review type
            priority: subtask_priority,
            assignee: subtask_def.assignee.clone().or_else(|| assignee.clone()),
            sources: subtask_sources,
            template: None,
            working_copy: working_copy.clone(),
            instructions: Some(subtask_def.instructions.clone()),
            data: convert_data(&subtask_def.data),
            timestamp,
        };
        write_event(cwd, &subtask_event)?;
    }

    Ok(parent_id)
}

/// Parse priority string to TaskPriority
pub fn parse_priority(s: &str) -> Option<TaskPriority> {
    match s.to_lowercase().as_str() {
        "p0" => Some(TaskPriority::P0),
        "p1" => Some(TaskPriority::P1),
        "p2" => Some(TaskPriority::P2),
        "p3" => Some(TaskPriority::P3),
        _ => None,
    }
}

/// Convert serde_json::Value HashMap to String HashMap for TaskEvent
pub fn convert_data(data: &HashMap<String, serde_json::Value>) -> HashMap<String, String> {
    data.iter()
        .map(|(k, v)| {
            let value_str = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), value_str)
        })
        .collect()
}

/// Returns the change_id of the current working copy (`@` in jj terms).
pub fn get_working_copy_change_id(cwd: &Path) -> Option<String> {
    use crate::jj::jj_cmd;

    let output = jj_cmd()
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if change_id.is_empty() {
        None
    } else {
        Some(change_id)
    }
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

    #[test]
    fn test_create_tasks_from_template_static_subtasks() {
        let temp_dir = TempDir::new().unwrap();
        create_test_templates(temp_dir.path());

        let template = load_template("aiki/review", temp_dir.path()).unwrap();

        let mut variables = VariableContext::new();
        variables.set_data("scope", "@");

        let (parent, subtasks) = create_tasks_from_template(&template, &variables, None).unwrap();

        assert_eq!(parent.name, "Review: @");
        assert!(parent.instructions.contains("Review the changes."));
        assert_eq!(subtasks.len(), 1);
        assert_eq!(subtasks[0].name, "Digest");
        assert!(subtasks[0].instructions.contains("Examine code."));
    }

    #[test]
    fn test_create_tasks_from_template_dynamic_subtasks() {
        use crate::tasks::types::TaskComment;
        use chrono::Utc;

        // Create a template with dynamic subtasks
        let mut template = TaskTemplate::new("test/dynamic");
        template.parent.name = "Review: {data.scope}".to_string();
        template.parent.instructions = "Review all issues.".to_string();
        template.subtasks_source = Some("source.comments".to_string());
        template.subtask_template = Some(
            r#"## Fix: {item.file}:{item.line}

**Severity**: {item.severity}

{item.text}"#
                .to_string(),
        );

        let mut variables = VariableContext::new();
        variables.set_data("scope", "src/auth.rs");

        // Create comments directly
        let comments = vec![
            TaskComment {
                id: None,
                text: "Variable may be null".to_string(),
                timestamp: Utc::now(),
                data: {
                    let mut d = HashMap::new();
                    d.insert("file".to_string(), "src/auth.ts".to_string());
                    d.insert("line".to_string(), "42".to_string());
                    d.insert("severity".to_string(), "error".to_string());
                    d
                },
            },
            TaskComment {
                id: None,
                text: "Unused import".to_string(),
                timestamp: Utc::now(),
                data: {
                    let mut d = HashMap::new();
                    d.insert("file".to_string(), "src/utils.ts".to_string());
                    d.insert("line".to_string(), "7".to_string());
                    d.insert("severity".to_string(), "warning".to_string());
                    d
                },
            },
        ];

        let (parent, subtasks) =
            create_tasks_from_template(&template, &variables, Some(comments)).unwrap();

        assert_eq!(parent.name, "Review: src/auth.rs");
        assert_eq!(subtasks.len(), 2);

        // Check first subtask
        assert_eq!(subtasks[0].name, "Fix: src/auth.ts:42");
        assert!(subtasks[0].instructions.contains("**Severity**: error"));
        assert!(subtasks[0].instructions.contains("Variable may be null"));

        // Verify comment metadata is persisted to TaskDefinition.data
        assert_eq!(subtasks[0].data.get("file"), Some(&serde_json::json!("src/auth.ts")));
        assert_eq!(subtasks[0].data.get("line"), Some(&serde_json::json!("42")));
        assert_eq!(subtasks[0].data.get("severity"), Some(&serde_json::json!("error")));

        // Check second subtask
        assert_eq!(subtasks[1].name, "Fix: src/utils.ts:7");
        assert!(subtasks[1].instructions.contains("**Severity**: warning"));
        assert!(subtasks[1].instructions.contains("Unused import"));

        // Verify comment metadata is persisted to TaskDefinition.data
        assert_eq!(subtasks[1].data.get("file"), Some(&serde_json::json!("src/utils.ts")));
        assert_eq!(subtasks[1].data.get("line"), Some(&serde_json::json!("7")));
        assert_eq!(subtasks[1].data.get("severity"), Some(&serde_json::json!("warning")));
    }

    #[test]
    fn test_create_tasks_from_template_dynamic_empty_data_source() {
        let mut template = TaskTemplate::new("test/dynamic");
        template.parent.name = "Review".to_string();
        template.parent.instructions = "Review all issues.".to_string();
        template.subtasks_source = Some("source.comments".to_string());
        template.subtask_template = Some("## Fix: {item.file}\n\n{item.text}".to_string());

        let variables = VariableContext::new();

        // Test with None data source
        let (_, subtasks) = create_tasks_from_template(&template, &variables, None).unwrap();
        assert!(subtasks.is_empty());

        // Test with empty Vec
        let (_, subtasks) =
            create_tasks_from_template(&template, &variables, Some(Vec::new())).unwrap();
        assert!(subtasks.is_empty());
    }

    #[test]
    fn test_parse_subtask_template() {
        let content = r#"## Fix: {item.file}:{item.line}

**Severity**: {item.severity}

{item.text}"#;

        let result = parse_subtask_template(content).unwrap();
        assert!(result.is_some());

        let parsed = result.unwrap();
        assert_eq!(parsed.heading, "Fix: {item.file}:{item.line}");
        assert!(parsed.frontmatter.is_none());
        assert!(parsed.body.contains("**Severity**: {item.severity}"));
        assert!(parsed.body.contains("{item.text}"));
    }

    #[test]
    fn test_parse_subtask_template_simple() {
        let content = "## Task Name\n\nTask body.";
        let result = parse_subtask_template(content).unwrap();
        assert!(result.is_some());

        let parsed = result.unwrap();
        assert_eq!(parsed.heading, "Task Name");
        assert!(parsed.frontmatter.is_none());
        assert_eq!(parsed.body, "Task body.");
    }

    #[test]
    fn test_parse_subtask_template_no_heading() {
        let content = "Just some text without a heading.";
        let result = parse_subtask_template(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_subtask_template_h1_not_h2() {
        let content = "# H1 Heading\n\nBody text.";
        let result = parse_subtask_template(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_subtask_template_with_frontmatter() {
        let content = r#"## Fix: {item.file}
---
sources:
  - task:{source.task_id}
priority: p1
assignee: claude-code
---

Fix the issue described above."#;

        let result = parse_subtask_template(content).unwrap();
        assert!(result.is_some());

        let parsed = result.unwrap();
        assert_eq!(parsed.heading, "Fix: {item.file}");
        assert!(parsed.frontmatter.is_some());

        let fm = parsed.frontmatter.unwrap();
        assert_eq!(fm.priority, Some("p1".to_string()));
        assert_eq!(fm.assignee, Some("claude-code".to_string()));
        assert_eq!(fm.sources.len(), 1);
        assert_eq!(fm.sources[0], "task:{source.task_id}");

        assert!(parsed.body.contains("Fix the issue"));
    }

    #[test]
    fn test_parse_subtask_template_frontmatter_with_data() {
        let content = r#"## Task
---
data:
  severity: "{item.severity}"
  file: "{item.file}"
---

Body text."#;

        let result = parse_subtask_template(content).unwrap();
        assert!(result.is_some());

        let parsed = result.unwrap();
        assert!(parsed.frontmatter.is_some());

        let fm = parsed.frontmatter.unwrap();
        assert_eq!(fm.data.get("severity"), Some(&serde_json::json!("{item.severity}")));
        assert_eq!(fm.data.get("file"), Some(&serde_json::json!("{item.file}")));
    }

    #[test]
    fn test_parse_subtask_template_invalid_yaml() {
        let content = r#"## Fix: {item.file}
---
invalid: yaml: : :
priority: p1
---

Body text."#;

        let result = parse_subtask_template(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid template frontmatter"));
    }
}
