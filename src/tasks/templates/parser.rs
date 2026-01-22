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

/// Parse a template from markdown content
///
/// # Arguments
/// * `content` - The raw markdown content
/// * `name` - The template name (from filename)
/// * `file_path` - The file path (for error messages)
pub fn parse_template(content: &str, name: &str, file_path: &str) -> Result<TaskTemplate> {
    // Extract frontmatter if present
    let (frontmatter, body) = extract_frontmatter(content, file_path)?;

    // Parse the markdown body
    let (parent, subtasks) = parse_markdown_body(&body, file_path)?;

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
    template.subtasks = subtasks;

    Ok(template)
}

/// Extract YAML frontmatter from markdown content
fn extract_frontmatter(content: &str, file_path: &str) -> Result<(TemplateFrontmatter, String)> {
    let content = content.trim_start();

    // Check for frontmatter delimiter
    if !content.starts_with("---") {
        // No frontmatter, return defaults and full content
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

            // Parse YAML
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
            // No closing delimiter found, treat as no frontmatter
            Ok((TemplateFrontmatter::default(), content.to_string()))
        }
    }
}

/// Parse the markdown body into parent task and subtasks
fn parse_markdown_body(body: &str, file_path: &str) -> Result<(TaskDefinition, Vec<TaskDefinition>)> {
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
        instructions: parent_instructions,
        priority: None,
        assignee: None,
        data: Default::default(),
    };

    // Parse subtasks if # Subtasks marker exists
    let subtasks = if let Some(marker_idx) = subtasks_marker_idx {
        parse_subtasks(&lines, marker_idx + 1, file_path)?
    } else {
        Vec::new()
    };

    Ok((parent, subtasks))
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

/// Parse subtasks from lines after the # Subtasks marker
fn parse_subtasks(lines: &[&str], start_idx: usize, file_path: &str) -> Result<Vec<TaskDefinition>> {
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
        instructions,
        priority: frontmatter.priority,
        assignee: frontmatter.assignee,
        data: frontmatter.data,
    })
}

/// Extract optional frontmatter from subtask content
fn extract_subtask_frontmatter(lines: &[&str], file_path: &str) -> Result<(SubtaskFrontmatter, String)> {
    if lines.is_empty() {
        return Ok((SubtaskFrontmatter::default(), String::new()));
    }

    // Find first non-blank line
    let first_content_idx = lines.iter().position(|l| !l.trim().is_empty());
    let first_content_idx = match first_content_idx {
        Some(idx) => idx,
        None => return Ok((SubtaskFrontmatter::default(), String::new())),
    };

    // Check if first content line is frontmatter delimiter
    if lines[first_content_idx].trim() != "---" {
        // No frontmatter, return all lines as instructions
        return Ok((SubtaskFrontmatter::default(), lines.join("\n").trim().to_string()));
    }

    // Find closing delimiter
    let after_first = &lines[first_content_idx + 1..];
    let close_idx = after_first.iter().position(|l| l.trim() == "---");

    match close_idx {
        Some(idx) => {
            let yaml_lines = &after_first[..idx];
            let yaml_content = yaml_lines.join("\n");
            let instructions_lines = &after_first[idx + 1..];

            let frontmatter: SubtaskFrontmatter =
                serde_yaml::from_str(&yaml_content).map_err(|e| {
                    AikiError::TemplateFrontmatterInvalid {
                        file: file_path.to_string(),
                        details: format!("Subtask frontmatter error: {}", e),
                    }
                })?;

            Ok((frontmatter, instructions_lines.join("\n").trim().to_string()))
        }
        None => {
            // No closing delimiter, treat all as instructions
            Ok((SubtaskFrontmatter::default(), lines.join("\n").trim().to_string()))
        }
    }
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
        assert!(template.subtasks[0].instructions.contains("Instructions here"));
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

# Review: {data.scope}

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
        assert_eq!(template.parent.name, "Review: {data.scope}");
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
        let (fm, body) = extract_frontmatter(content, "test.md").unwrap();
        // When no closing delimiter, treat as no frontmatter
        assert!(fm.version.is_none());
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
        assert!(template.parent.instructions.contains("This h2 is part of the parent"));

        // Only one subtask should exist
        assert_eq!(template.subtasks.len(), 1);
        assert_eq!(template.subtasks[0].name, "Real subtask");
    }
}
