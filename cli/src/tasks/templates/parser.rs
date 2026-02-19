//! Template parser for markdown files with YAML frontmatter
//!
//! Parses template files in the format:
//! ```markdown
//! ---
//! version: "1.0.0"
//! description: Template description
//! type: review
//! ---
//!
//! # Task Name
//!
//! Task instructions...
//!
//! # Subtasks
//!
//! ## Subtask 1
//!
//! Subtask 1 instructions...
//!
//! ## Subtask 2
//!
//! Subtask 2 instructions...
//! ```

use crate::error::{AikiError, Result};

use super::types::{
    SubtaskFrontmatter, TaskDefaults, TaskDefinition, TaskTemplate, TemplateFrontmatter,
};

/// Errors that can occur when extracting YAML frontmatter
#[derive(Debug)]
pub enum FrontmatterError {
    /// YAML parsing failed
    Yaml(serde_yaml::Error),
    /// Frontmatter started with `---` but no closing `---` found
    Unterminated,
}

impl std::fmt::Display for FrontmatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrontmatterError::Yaml(e) => write!(f, "{}", e),
            FrontmatterError::Unterminated => {
                write!(f, "Unterminated frontmatter: found opening '---' but no closing '---'")
            }
        }
    }
}

impl std::error::Error for FrontmatterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FrontmatterError::Yaml(e) => Some(e),
            FrontmatterError::Unterminated => None,
        }
    }
}

impl From<serde_yaml::Error> for FrontmatterError {
    fn from(e: serde_yaml::Error) -> Self {
        FrontmatterError::Yaml(e)
    }
}

/// Parse a template from markdown content
///
/// # Arguments
/// * `content` - The raw markdown content
/// * `name` - The template name (from filename)
/// * `file_path` - The file path (for error messages)
pub fn parse_template(content: &str, name: &str, file_path: &str) -> Result<TaskTemplate> {
    // Extract frontmatter if present
    let (frontmatter, body) = extract_frontmatter(content, file_path)?;

    // Check if subtasks should be dynamically generated from a data source
    let has_subtasks_source = frontmatter.subtasks.is_some();

    // Parse the markdown body
    let (parent, subtasks, subtask_template_content) =
        parse_markdown_body_with_mode(&body, file_path, has_subtasks_source)?;

    // Build the template
    let mut template = TaskTemplate::new(name);
    template.version = frontmatter.version;
    template.description = frontmatter.description;
    template.defaults = TaskDefaults {
        task_type: frontmatter.task_type,
        assignee: frontmatter.assignee,
        priority: frontmatter.priority,
        data: frontmatter.data,
    };
    template.parent = parent;
    // Propagate slug from template-level frontmatter to the parent definition.
    // This allows composed templates (loaded via {% subtask %}) to declare their slug.
    template.parent.slug = frontmatter.slug;
    template.subtasks = subtasks;
    template.subtasks_source = frontmatter.subtasks;
    template.subtask_template = subtask_template_content;
    template.spawns = frontmatter.spawns;

    // Validate spawn entries: each must have exactly one of task/subtask
    for (i, entry) in template.spawns.iter().enumerate() {
        match (&entry.task, &entry.subtask) {
            (Some(_), Some(_)) => {
                return Err(AikiError::TemplateProcessingFailed {
                    details: format!(
                        "spawn entry {} in template '{}' has both 'task' and 'subtask' — exactly one is required",
                        i, name
                    ),
                });
            }
            (None, None) => {
                return Err(AikiError::TemplateProcessingFailed {
                    details: format!(
                        "spawn entry {} in template '{}' has neither 'task' nor 'subtask' — exactly one is required",
                        i, name
                    ),
                });
            }
            _ => {} // Exactly one is set — valid
        }
    }

    Ok(template)
}

/// Extract YAML frontmatter from content (generic over frontmatter type)
///
/// Returns `Ok((Some(frontmatter), body))` if frontmatter is present and valid.
/// Returns `Ok((None, content))` if no frontmatter delimiters are found.
/// Returns `Err(FrontmatterError::Yaml)` if YAML is malformed.
/// Returns `Err(FrontmatterError::Unterminated)` if opening `---` found but no closing `---`.
///
/// This ensures users get clear error messages when their frontmatter is invalid,
/// rather than silently treating it as no frontmatter.
pub fn extract_yaml_frontmatter<T>(
    content: &str,
) -> std::result::Result<(Option<T>, String), FrontmatterError>
where
    T: serde::de::DeserializeOwned,
{
    let content = content.trim_start();

    // Check for frontmatter delimiter
    if !content.starts_with("---") {
        return Ok((None, content.to_string()));
    }

    // Find the closing delimiter
    let after_first = &content[3..];
    let after_first = after_first.trim_start_matches(['\n', '\r']);
    let end_idx = after_first
        .find("\n---")
        .or_else(|| after_first.find("\r\n---"));

    match end_idx {
        Some(idx) => {
            let yaml_content = &after_first[..idx];
            let body_start = idx + 4; // Skip "\n---"
            let body = after_first[body_start..].trim_start_matches(['\n', '\r']);

            // Parse YAML - return error if malformed
            let fm = serde_yaml::from_str::<T>(yaml_content)?;
            Ok((Some(fm), body.to_string()))
        }
        None => {
            // No closing delimiter found - error, not silent ignore
            Err(FrontmatterError::Unterminated)
        }
    }
}

/// Extract YAML frontmatter from markdown content (with error reporting)
fn extract_frontmatter(content: &str, file_path: &str) -> Result<(TemplateFrontmatter, String)> {
    let content = content.trim_start();

    // Check for frontmatter delimiter
    if !content.starts_with("---") {
        return Ok((TemplateFrontmatter::default(), content.to_string()));
    }

    // Find the closing delimiter
    let after_first = &content[3..];
    let end_idx = after_first
        .find("\n---")
        .or_else(|| after_first.find("\r\n---"));

    match end_idx {
        Some(idx) => {
            let yaml_content = &after_first[..idx];
            let body_start = idx + 4; // Skip "\n---"
            let body = after_first[body_start..].trim_start_matches(['\n', '\r']);

            // Parse YAML with error reporting
            let frontmatter: TemplateFrontmatter =
                serde_yaml::from_str(yaml_content).map_err(|e| {
                    AikiError::TemplateFrontmatterInvalid {
                        file: file_path.to_string(),
                        details: e.to_string(),
                    }
                })?;

            Ok((frontmatter, body.to_string()))
        }
        None => {
            // No closing delimiter found - error
            Err(AikiError::TemplateFrontmatterInvalid {
                file: file_path.to_string(),
                details: "Unterminated frontmatter: found opening '---' but no closing '---'".to_string(),
            })
        }
    }
}

/// Parse the markdown body into parent task and subtasks
fn parse_markdown_body(
    body: &str,
    file_path: &str,
) -> Result<(TaskDefinition, Vec<TaskDefinition>)> {
    let (parent, subtasks, _) = parse_markdown_body_with_mode(body, file_path, false)?;
    Ok((parent, subtasks))
}

/// Parse the markdown body with optional subtask template extraction mode
///
/// When `extract_template` is true (frontmatter has `subtasks` field), the `# Subtasks` section
/// is extracted as a raw template string instead of being parsed into static subtask definitions.
fn parse_markdown_body_with_mode(
    body: &str,
    file_path: &str,
    extract_template: bool,
) -> Result<(TaskDefinition, Vec<TaskDefinition>, Option<String>)> {
    let lines: Vec<&str> = body.lines().collect();

    // Find the first h1 heading (# Task Name)
    let (task_name, task_start_idx) = find_first_h1(&lines, file_path)?;

    // Find the "# Subtasks" marker
    let subtasks_marker_idx = find_subtasks_marker(&lines, task_start_idx);

    // Extract parent task instructions (between first h1 and # Subtasks or end)
    let parent_end = subtasks_marker_idx.unwrap_or(lines.len());
    let parent_instructions = extract_instructions(&lines, task_start_idx + 1, parent_end);

    let parent = TaskDefinition {
        name: task_name,
        slug: None, // Parent tasks don't have slugs from templates
        task_type: None, // Set from frontmatter by resolver
        instructions: parent_instructions,
        priority: None,
        assignee: None,
        sources: Vec::new(),
        data: Default::default(),
    };

    // Handle subtasks based on mode
    if let Some(marker_idx) = subtasks_marker_idx {
        if extract_template {
            // Extract the raw subtask template content (everything after "# Subtasks")
            let template_content = extract_subtask_template_section(&lines, marker_idx + 1);
            Ok((parent, Vec::new(), Some(template_content)))
        } else {
            // Parse static subtasks normally
            let subtasks = parse_subtasks(&lines, marker_idx + 1, file_path)?;
            Ok((parent, subtasks, None))
        }
    } else {
        Ok((parent, Vec::new(), None))
    }
}

/// Find the first h1 heading in the markdown
fn find_first_h1(lines: &[&str], file_path: &str) -> Result<(String, usize)> {
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") && !trimmed.starts_with("## ") {
            let name = trimmed[2..].trim().to_string();
            return Ok((name, idx));
        }
    }

    Err(AikiError::TemplateStructureInvalid {
        file: file_path.to_string(),
        details: "Missing required '# ' heading for task name".to_string(),
    })
}

/// Find the "# Subtasks" marker line
fn find_subtasks_marker(lines: &[&str], start_idx: usize) -> Option<usize> {
    for (idx, line) in lines.iter().enumerate().skip(start_idx) {
        let trimmed = line.trim().to_lowercase();
        if trimmed == "# subtasks" {
            return Some(idx);
        }
    }
    None
}

/// Extract instructions from a range of lines
fn extract_instructions(lines: &[&str], start: usize, end: usize) -> String {
    if start >= end || start >= lines.len() {
        return String::new();
    }

    let end = end.min(lines.len());
    lines[start..end].join("\n").trim().to_string()
}

/// Extract the entire subtask template section as raw content
///
/// When `subtasks` frontmatter is present, the `# Subtasks` section contains a TEMPLATE
/// for each item, not static subtask definitions. This function extracts the entire
/// section (including the h2 heading template) as raw content for later template expansion.
fn extract_subtask_template_section(lines: &[&str], start_idx: usize) -> String {
    if start_idx >= lines.len() {
        return String::new();
    }

    lines[start_idx..].join("\n").trim().to_string()
}

/// Parse subtasks from lines after the # Subtasks marker
fn parse_subtasks(
    lines: &[&str],
    start_idx: usize,
    file_path: &str,
) -> Result<Vec<TaskDefinition>> {
    let mut subtasks = Vec::new();
    let mut current_subtask: Option<(String, usize)> = None; // (name, start_line)

    for (idx, line) in lines.iter().enumerate().skip(start_idx) {
        let trimmed = line.trim();

        // Check for h2 heading (## Subtask Name)
        if trimmed.starts_with("## ") {
            // Finish previous subtask if any
            if let Some((name, start)) = current_subtask.take() {
                let subtask = parse_single_subtask(&name, &lines[start..idx], file_path)?;
                subtasks.push(subtask);
            }

            // Start new subtask
            let name = trimmed[3..].trim().to_string();
            current_subtask = Some((name, idx + 1));
        }
    }

    // Finish last subtask
    if let Some((name, start)) = current_subtask {
        let subtask = parse_single_subtask(&name, &lines[start..], file_path)?;
        subtasks.push(subtask);
    }

    Ok(subtasks)
}

/// Parse a single subtask from its content lines
fn parse_single_subtask(name: &str, lines: &[&str], file_path: &str) -> Result<TaskDefinition> {
    // Check for subtask frontmatter
    let (frontmatter, instructions) = extract_subtask_frontmatter(lines, file_path)?;

    Ok(TaskDefinition {
        name: name.to_string(),
        slug: frontmatter.slug,
        task_type: None, // Subtasks inherit type from parent
        instructions,
        priority: frontmatter.priority,
        assignee: frontmatter.assignee,
        sources: frontmatter.sources,
        data: frontmatter.data,
    })
}

/// Extract optional frontmatter from subtask content
fn extract_subtask_frontmatter(
    lines: &[&str],
    file_path: &str,
) -> Result<(SubtaskFrontmatter, String)> {
    let content = lines.join("\n");
    let (frontmatter, body) = extract_yaml_frontmatter::<SubtaskFrontmatter>(&content)
        .map_err(|e| AikiError::TemplateFrontmatterInvalid {
            file: file_path.to_string(),
            details: e.to_string(),
        })?;
    Ok((frontmatter.unwrap_or_default(), body.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_template() {
        let content = r#"
# Minimal task

Do something.

# Subtasks

## Do the work

Instructions here.
"#;

        let template = parse_template(content, "minimal", "minimal.md").unwrap();
        assert_eq!(template.name, "minimal");
        assert!(template.version.is_none());
        assert_eq!(template.parent.name, "Minimal task");
        assert!(template.parent.instructions.contains("Do something"));
        assert_eq!(template.subtasks.len(), 1);
        assert_eq!(template.subtasks[0].name, "Do the work");
        assert!(template.subtasks[0]
            .instructions
            .contains("Instructions here"));
    }

    #[test]
    fn test_parse_with_frontmatter() {
        let content = r#"---
version: "1.2.0"
description: Test template
type: review
assignee: claude-code
priority: p1
data:
  scope: "@"
---

# Review: {{data.scope}}

Review the changes.

# Subtasks

## Digest code

Examine the code.

## Review code

Review for issues.
"#;

        let template = parse_template(content, "aiki/review", "review.md").unwrap();
        assert_eq!(template.name, "aiki/review");
        assert_eq!(template.version, Some("1.2.0".to_string()));
        assert_eq!(template.description, Some("Test template".to_string()));
        assert_eq!(template.defaults.task_type, Some("review".to_string()));
        assert_eq!(template.defaults.assignee, Some("claude-code".to_string()));
        assert_eq!(template.defaults.priority, Some("p1".to_string()));
        assert_eq!(template.parent.name, "Review: {{data.scope}}");
        assert_eq!(template.subtasks.len(), 2);
        assert_eq!(template.subtasks[0].name, "Digest code");
        assert_eq!(template.subtasks[1].name, "Review code");
    }

    #[test]
    fn test_parse_no_subtasks() {
        let content = r#"
# Simple task

Just do it.
"#;

        let template = parse_template(content, "simple", "simple.md").unwrap();
        assert_eq!(template.parent.name, "Simple task");
        assert!(template.subtasks.is_empty());
    }

    #[test]
    fn test_parse_subtask_frontmatter() {
        let content = r#"
# Parent task

Instructions.

# Subtasks

## Urgent subtask
---
priority: p0
assignee: security-specialist
---

Do this urgently.

## Normal subtask

Do this normally.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();
        assert_eq!(template.subtasks.len(), 2);

        let urgent = &template.subtasks[0];
        assert_eq!(urgent.name, "Urgent subtask");
        assert_eq!(urgent.priority, Some("p0".to_string()));
        assert_eq!(urgent.assignee, Some("security-specialist".to_string()));
        assert!(urgent.instructions.contains("Do this urgently"));

        let normal = &template.subtasks[1];
        assert_eq!(normal.name, "Normal subtask");
        assert!(normal.priority.is_none());
        assert!(normal.instructions.contains("Do this normally"));
    }

    #[test]
    fn test_parse_missing_h1() {
        let content = r#"
## Only h2 heading

Some content.
"#;

        let result = parse_template(content, "test", "test.md");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Missing required '# ' heading"));
    }

    #[test]
    fn test_parse_invalid_frontmatter() {
        let content = r#"---
invalid: yaml: :
---

# Task

Content.
"#;

        let result = parse_template(content, "test", "test.md");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_frontmatter_no_closing() {
        let content = "---\nversion: 1.0\n\n# Task\n\nContent.";
        let result = extract_frontmatter(content, "test.md");
        // When no closing delimiter, error instead of silently ignoring
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unterminated frontmatter"));
    }

    #[test]
    fn test_extract_yaml_frontmatter_no_closing() {
        let content = "---\nkey: value\n\n# Body content";
        let result = extract_yaml_frontmatter::<SubtaskFrontmatter>(content);
        // When no closing delimiter, error instead of silently ignoring
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, FrontmatterError::Unterminated));
        assert!(err.to_string().contains("Unterminated frontmatter"));
    }

    #[test]
    fn test_template_id() {
        let mut template = TaskTemplate::new("aiki/review");
        assert_eq!(template.template_id(), "aiki/review");

        template.version = Some("1.0.0".to_string());
        assert_eq!(template.template_id(), "aiki/review@1.0.0");
    }

    #[test]
    fn test_h2_before_subtasks_marker() {
        let content = r#"
# Parent task

## Section in parent

This h2 is part of the parent instructions.

More parent content.

# Subtasks

## Real subtask

Subtask instructions.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();
        // The h2 before # Subtasks should be part of parent instructions
        assert!(template.parent.instructions.contains("Section in parent"));
        assert!(template
            .parent
            .instructions
            .contains("This h2 is part of the parent"));

        // Only one subtask should exist
        assert_eq!(template.subtasks.len(), 1);
        assert_eq!(template.subtasks[0].name, "Real subtask");
    }

    #[test]
    fn test_parse_with_subtasks_frontmatter_extracts_template() {
        let content = r#"---
version: 1.0.0
subtasks: source.comments
---

# Followup: {{source.name}}

Fix all issues identified in review.

# Subtasks

## Fix: {{data.file}}:{{data.line}}

**Severity**: {{data.severity}}
**Category**: {{data.category}}

{{text}}
"#;

        let template = parse_template(content, "followup", "followup.md").unwrap();

        // Should have subtasks_source set
        assert_eq!(
            template.subtasks_source,
            Some("source.comments".to_string())
        );

        // Should NOT have static subtasks parsed
        assert!(
            template.subtasks.is_empty(),
            "subtasks should be empty when subtasks_source is set"
        );

        // Should have raw subtask template content
        assert!(template.subtask_template.is_some());
        let subtask_template = template.subtask_template.unwrap();

        // Should include the h2 heading template
        assert!(
            subtask_template.contains("## Fix: {{data.file}}:{{data.line}}"),
            "subtask_template should include the h2 heading"
        );

        // Should include the body content
        assert!(
            subtask_template.contains("**Severity**: {{data.severity}}"),
            "subtask_template should include the body"
        );
        assert!(
            subtask_template.contains("{{text}}"),
            "subtask_template should include variable placeholders"
        );

        // Parent task should still be parsed normally
        assert_eq!(template.parent.name, "Followup: {{source.name}}");
        assert!(template
            .parent
            .instructions
            .contains("Fix all issues identified"));
    }

    #[test]
    fn test_parse_without_subtasks_frontmatter_parses_static_subtasks() {
        let content = r#"---
version: 1.0.0
---

# Review task

Review the code.

# Subtasks

## Check formatting

Verify formatting is correct.

## Check logic

Verify logic is correct.
"#;

        let template = parse_template(content, "review", "review.md").unwrap();

        // Should NOT have subtasks_source set
        assert!(
            template.subtasks_source.is_none(),
            "subtasks_source should be None when not in frontmatter"
        );

        // Should NOT have subtask template
        assert!(
            template.subtask_template.is_none(),
            "subtask_template should be None when subtasks_source is not set"
        );

        // Should have static subtasks parsed
        assert_eq!(template.subtasks.len(), 2);
        assert_eq!(template.subtasks[0].name, "Check formatting");
        assert_eq!(template.subtasks[1].name, "Check logic");
        assert!(template.subtasks[0]
            .instructions
            .contains("Verify formatting"));
        assert!(template.subtasks[1].instructions.contains("Verify logic"));
    }

    #[test]
    fn test_subtasks_source_stored_from_frontmatter() {
        let content = r#"---
subtasks: review.findings
---

# Task

Instructions.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();
        assert_eq!(
            template.subtasks_source,
            Some("review.findings".to_string())
        );
    }

    #[test]
    fn test_subtask_template_no_subtasks_section() {
        // When subtasks frontmatter is set but there's no # Subtasks section
        let content = r#"---
subtasks: source.comments
---

# Task

Just instructions, no subtasks section.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();

        // subtasks_source should still be set
        assert_eq!(
            template.subtasks_source,
            Some("source.comments".to_string())
        );

        // But subtask_template should be None (no section to extract)
        assert!(
            template.subtask_template.is_none(),
            "subtask_template should be None when no # Subtasks section exists"
        );
    }

    #[test]
    fn test_subtask_with_slug_frontmatter() {
        let content = r#"
# Build task

Parent instructions.

# Subtasks

## Build binary

---
slug: build
priority: p0
---

Build the binary.

## Run tests

---
slug: run-tests
---

Run all tests.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();
        assert_eq!(template.subtasks.len(), 2);
        assert_eq!(template.subtasks[0].slug, Some("build".to_string()));
        assert_eq!(template.subtasks[0].priority, Some("p0".to_string()));
        assert_eq!(template.subtasks[1].slug, Some("run-tests".to_string()));
    }

    #[test]
    fn test_subtask_without_slug_frontmatter() {
        let content = r#"
# Build task

Parent instructions.

# Subtasks

## Step one

Do the first step.

## Step two

Do the second step.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();
        assert_eq!(template.subtasks.len(), 2);
        assert!(template.subtasks[0].slug.is_none());
        assert!(template.subtasks[1].slug.is_none());
    }

    #[test]
    fn test_parse_template_with_spawns() {
        let content = r#"---
version: "1.0.0"
type: review
spawns:
  - when: not approved
    task:
      template: aiki/fix
      priority: p0
      data:
        max_iterations: 3
  - when: data.needs_analysis
    subtask:
      template: aiki/analysis
      assignee: claude-code
---

# Review task

Review the changes.
"#;

        let template = parse_template(content, "aiki/review", "review.md").unwrap();
        assert_eq!(template.spawns.len(), 2);

        // First spawn: standalone task
        assert_eq!(template.spawns[0].when, "not approved");
        assert!(template.spawns[0].task.is_some());
        assert!(template.spawns[0].subtask.is_none());
        let task_cfg = template.spawns[0].task.as_ref().unwrap();
        assert_eq!(task_cfg.template, "aiki/fix");
        assert_eq!(task_cfg.priority, Some("p0".to_string()));

        // Second spawn: subtask
        assert_eq!(template.spawns[1].when, "data.needs_analysis");
        assert!(template.spawns[1].task.is_none());
        assert!(template.spawns[1].subtask.is_some());
        let subtask_cfg = template.spawns[1].subtask.as_ref().unwrap();
        assert_eq!(subtask_cfg.template, "aiki/analysis");
        assert_eq!(subtask_cfg.assignee, Some("claude-code".to_string()));
    }

    #[test]
    fn test_parse_template_without_spawns() {
        let content = r#"---
version: "1.0.0"
---

# Simple task

Do something.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();
        assert!(template.spawns.is_empty());
    }

    #[test]
    fn test_parse_template_spawn_both_task_and_subtask_rejected() {
        let content = r#"---
version: "1.0.0"
spawns:
  - when: not approved
    task:
      template: aiki/fix
    subtask:
      template: aiki/analysis
---

# Review task

Review the changes.
"#;

        let result = parse_template(content, "aiki/review", "review.md");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("both 'task' and 'subtask'"), "Error: {}", err);
    }

    #[test]
    fn test_parse_template_spawn_neither_task_nor_subtask_rejected() {
        let content = r#"---
version: "1.0.0"
spawns:
  - when: not approved
---

# Review task

Review the changes.
"#;

        let result = parse_template(content, "aiki/review", "review.md");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("neither 'task' nor 'subtask'"), "Error: {}", err);
    }

    #[test]
    fn test_template_level_slug_propagates_to_parent() {
        let content = r#"---
slug: criteria
---

# Understand Criteria: Code

Evaluate the implementation.
"#;

        let template = parse_template(content, "aiki/review/criteria/code", "code.md").unwrap();
        assert_eq!(template.parent.slug, Some("criteria".to_string()));
        assert_eq!(template.parent.name, "Understand Criteria: Code");
    }

    #[test]
    fn test_template_without_slug_has_none() {
        let content = r#"---
version: "1.0.0"
---

# Simple task

Do something.
"#;

        let template = parse_template(content, "test", "test.md").unwrap();
        assert!(template.parent.slug.is_none());
    }

    #[test]
    fn test_frontmatter_slug_deserialize() {
        let yaml = r#"
slug: criteria
version: "1.0.0"
"#;
        let fm: TemplateFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.slug, Some("criteria".to_string()));
        assert_eq!(fm.version, Some("1.0.0".to_string()));
    }
}
