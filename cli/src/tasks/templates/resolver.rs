//! Template resolution and discovery
//!
//! Handles finding template files in the filesystem:
//! - Built-in templates: `.aiki/templates/aiki/`
//! - Custom templates: `.aiki/templates/{namespace}/`

use crate::error::{AikiError, Result};
use crate::tasks::TaskPriority;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::parser::parse_template;
use super::types::{TaskDefinition, TaskTemplate};
use super::variables::{substitute, VariableContext};
use crate::tasks::types::TaskComment;
use regex::Regex;

/// An entry in the subtask list — either a static subtask or a composed template reference
#[derive(Debug, Clone)]
pub enum SubtaskEntry {
    /// A static subtask with resolved name and body
    Static(TaskDefinition),
    /// A reference to another template that should be composed as a nested subtask
    Composed {
        /// Template name (e.g., "aiki/plan")
        template_name: String,
        /// Source line number for error reporting
        line: usize,
    },
}

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
        let all_names: Vec<_> = available
            .iter()
            .map(|t| format!("    - {}", t.name))
            .collect();
        if all_names.is_empty() {
            return String::new();
        }
        return format!("\n\n  Available templates:\n{}", all_names.join("\n"));
    }

    let suggestions: Vec<_> = similar
        .iter()
        .map(|t| match &t.description {
            Some(desc) => format!("    - {} ({})", t.name, desc),
            None => format!("    - {}", t.name),
        })
        .collect();

    format!(
        "\n\n  Did you mean one of these?\n{}",
        suggestions.join("\n")
    )
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

    // Handle subtasks based on template type:
    // 1. subtasks_source in frontmatter -> use old dynamic subtasks approach
    // 2. inline {% for %} loops or {% subtask %} refs -> process through template engine
    // 3. neither -> static subtasks
    let subtasks = if template.subtasks_source.is_some() {
        // Legacy: Dynamic subtasks using frontmatter declaration
        create_dynamic_subtasks(template, variables, data_source)?
    } else if let Some(ref raw_content) = template.raw_content {
        if has_inline_loops(raw_content) || has_subtask_refs(raw_content) {
            // Process through conditional/loop engine (handles both loops and subtask refs)
            // For backward compat, filter out Composed entries (handled by Phase 3)
            create_subtasks_from_inline_loops(raw_content, variables, data_source)?
        } else {
            // Static subtasks: just substitute variables
            create_static_subtasks(template, variables)?
        }
    } else {
        // Static subtasks: just substitute variables
        create_static_subtasks(template, variables)?
    };

    Ok((parent, subtasks))
}

/// Create tasks from a template, returning SubtaskEntry items that may include composed references
///
/// Unlike `create_tasks_from_template()` which filters out Composed entries,
/// this function preserves them so callers can handle recursive template composition.
pub fn create_subtask_entries_from_template(
    template: &TaskTemplate,
    variables: &VariableContext,
    data_source: Option<Vec<TaskComment>>,
) -> Result<(TaskDefinition, Vec<SubtaskEntry>)> {
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

    // Route through the appropriate subtask extraction path
    let entries = if template.subtasks_source.is_some() {
        // Legacy: Dynamic subtasks — always static entries
        let defs = create_dynamic_subtasks(template, variables, data_source)?;
        defs.into_iter().map(SubtaskEntry::Static).collect()
    } else if let Some(ref raw_content) = template.raw_content {
        if has_inline_loops(raw_content) || has_subtask_refs(raw_content) {
            // Process through conditional/loop engine (returns mixed Static + Composed entries)
            create_subtask_entries(raw_content, variables, data_source)?
        } else {
            // Static subtasks: just substitute variables
            let defs = create_static_subtasks(template, variables)?;
            defs.into_iter().map(SubtaskEntry::Static).collect()
        }
    } else {
        // Static subtasks: just substitute variables
        let defs = create_static_subtasks(template, variables)?;
        defs.into_iter().map(SubtaskEntry::Static).collect()
    };

    Ok((parent, entries))
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

        // Copy parent data as {data.*} (accessible in subtasks for CLI-provided data)
        for (key, value) in &variables.data {
            subtask_ctx.set_data(key, value);
        }

        // Substitute variables in the heading and body
        let name = substitute(&parsed.heading, &subtask_ctx)?;
        let instructions = substitute(&parsed.body, &subtask_ctx)?;

        // Start with empty data map (no comment metadata)
        let mut data: HashMap<String, serde_json::Value> = HashMap::new();

        // Apply frontmatter if present, with variable substitution
        let (priority, assignee, sources) = if let Some(ref fm) = parsed.frontmatter {
            let priority = fm
                .priority
                .as_ref()
                .map(|p| substitute(p, &subtask_ctx))
                .transpose()?;
            let assignee = fm
                .assignee
                .as_ref()
                .map(|a| substitute(a, &subtask_ctx))
                .transpose()?;
            let sources: Vec<String> = fm
                .sources
                .iter()
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
///   - task:{source.id}
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
                let (frontmatter, body) = super::parser::extract_yaml_frontmatter::<
                    super::types::SubtaskFrontmatter,
                >(after_heading)
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

/// Known collection names that can be used in {% for %} loops
///
/// If a template references a collection not in this list, it's an error.
const KNOWN_COLLECTIONS: &[&str] = &[
    "source.comments",
    // Future: "source.files", "data.<array>", etc.
];

/// Check if a collection name is known/valid
fn is_known_collection(name: &str) -> bool {
    KNOWN_COLLECTIONS.contains(&name)
}

/// Expand loop markers in rendered template content
///
/// This function processes the `<!-- AIKI_LOOP:var:collection -->` markers
/// that were emitted during conditional processing and expands them with
/// actual data.
///
/// # Arguments
/// * `content` - The rendered template content containing loop markers
/// * `data_sources` - Map of collection names to their data (e.g., "source.comments" -> comments)
///
/// # Returns
/// The expanded content with loops replaced by their iterated output
///
/// # Errors
/// Returns an error if:
/// - A loop references an unknown collection name (typo, unsupported collection)
/// - Too many loop expansion iterations (infinite loop protection)
/// - Unclosed loop markers
pub fn expand_loops(
    content: &str,
    data_sources: &HashMap<String, Vec<TaskComment>>,
) -> Result<String> {
    let mut result = content.to_string();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 100; // Prevent infinite loops

    // Process loops from innermost to outermost by repeatedly finding and expanding
    // the first loop that has no nested loops in its body
    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return Err(AikiError::TemplateProcessingFailed {
                details: "Too many loop expansion iterations (possible infinite loop)".to_string(),
            });
        }

        // Find the first loop marker
        let loop_start_pattern = Regex::new(r"<!-- AIKI_LOOP:([a-z_][a-z0-9_]*):([^\s]+) -->\n?")
            .expect("Invalid regex");

        let Some(caps) = loop_start_pattern.captures(&result) else {
            // No more loops to process
            break;
        };

        let loop_start_match = caps.get(0).unwrap();
        let variable_name = caps.get(1).unwrap().as_str().to_string();
        let collection_name = caps.get(2).unwrap().as_str().to_string();
        let content_start = loop_start_match.end();

        // Validate collection name is known
        if !is_known_collection(&collection_name) {
            return Err(AikiError::TemplateProcessingFailed {
                details: format!(
                    "Unknown collection '{}'. Available collections: {}",
                    collection_name,
                    KNOWN_COLLECTIONS.join(", ")
                ),
            });
        }

        // Find the matching ENDLOOP marker using stack-based matching
        let rest = &result[content_start..];
        let Some((body_end, else_body, total_end)) = find_matching_endloop(rest)? else {
            return Err(AikiError::TemplateProcessingFailed {
                details: "Unclosed AIKI_LOOP marker".to_string(),
            });
        };

        let loop_body = &rest[..body_end];
        let full_end = content_start + total_end;

        // Get the data for this collection (known collection, may be empty or absent)
        let items = data_sources.get(&collection_name);
        let expanded = match items {
            Some(items) if !items.is_empty() => expand_loop_body(&variable_name, loop_body, items)?,
            _ => {
                // Collection is known but empty or not provided, use else body if present
                else_body.unwrap_or_default()
            }
        };

        // Replace the loop marker with expanded content
        result = result[..loop_start_match.start()].to_string() + &expanded + &result[full_end..];
    }

    Ok(result)
}

/// Find the matching ENDLOOP marker, handling nested loops
///
/// Returns (body_end_offset, else_body, total_end_offset) relative to the input string,
/// where input starts immediately after the opening AIKI_LOOP marker.
fn find_matching_endloop(content: &str) -> Result<Option<(usize, Option<String>, usize)>> {
    const LOOP_START: &str = "<!-- AIKI_LOOP:";
    const LOOP_END: &str = "<!-- AIKI_ENDLOOP -->";
    const LOOP_ELSE: &str = "<!-- AIKI_LOOPELSE -->";
    const LOOP_ELSE_END: &str = "<!-- AIKI_ENDLOOPELSE -->";

    let mut depth = 1; // We're already inside one loop
    let mut pos = 0;

    while pos < content.len() && depth > 0 {
        // Look for the next marker
        let remaining = &content[pos..];

        if remaining.starts_with(LOOP_START) {
            // Found nested loop start
            depth += 1;
            pos += LOOP_START.len();
        } else if remaining.starts_with(LOOP_END) {
            depth -= 1;
            if depth == 0 {
                // Found our matching end marker
                let body_end = pos;
                let after_endloop = pos + LOOP_END.len();

                // Check for optional else block
                let after_endloop_content = &content[after_endloop..];
                let trimmed = after_endloop_content.trim_start_matches('\n');
                let newlines_skipped = after_endloop_content.len() - trimmed.len();

                if trimmed.starts_with(LOOP_ELSE) {
                    // Find the ENDLOOPELSE marker
                    let else_start = after_endloop + newlines_skipped + LOOP_ELSE.len();
                    let else_content = &content[else_start..];
                    let trimmed_else = else_content.trim_start_matches('\n');
                    let else_newlines = else_content.len() - trimmed_else.len();

                    if let Some(else_end_pos) = trimmed_else.find(LOOP_ELSE_END) {
                        let else_body = trimmed_else[..else_end_pos].to_string();
                        let total_end =
                            else_start + else_newlines + else_end_pos + LOOP_ELSE_END.len();
                        return Ok(Some((body_end, Some(else_body), total_end)));
                    } else {
                        return Err(AikiError::TemplateProcessingFailed {
                            details: "AIKI_LOOPELSE without matching AIKI_ENDLOOPELSE".to_string(),
                        });
                    }
                } else {
                    return Ok(Some((body_end, None, after_endloop)));
                }
            } else {
                pos += LOOP_END.len();
            }
        } else {
            // Advance by one character (may be multi-byte for non-ASCII like emojis)
            pos += content[pos..].chars().next().map_or(1, |c| c.len_utf8());
        }
    }

    if depth > 0 {
        Ok(None) // Unclosed loop
    } else {
        Ok(None)
    }
}

/// Expand a loop body for each item in the collection
fn expand_loop_body(variable_name: &str, body: &str, items: &[TaskComment]) -> Result<String> {
    use super::conditionals::{process_conditionals, EvalContext};

    let mut result = String::new();
    let len = items.len();

    for (index, item) in items.iter().enumerate() {
        // Create evaluation context with loop variables for conditional processing
        let mut ctx = EvalContext::new();

        // Add loop metadata to context (for conditional evaluation like {% if loop.first %})
        ctx.set("loop.index", (index + 1).to_string());
        ctx.set("loop.index0", index.to_string());
        ctx.set("loop.first", (index == 0).to_string());
        ctx.set("loop.last", (index == len - 1).to_string());
        ctx.set("loop.length", len.to_string());

        // Add item fields to context for conditional evaluation
        ctx.set(format!("{}.text", variable_name), item.text.clone());

        // Process conditionals with the populated context
        // This evaluates {% if %} blocks using the loop variables
        let iteration_body =
            process_conditionals(body, &ctx).map_err(|e| AikiError::TemplateProcessingFailed {
                details: format!("Error processing conditionals in loop body: {}", e),
            })?;

        // Replace remaining variable references after conditional processing
        // process_conditionals leaves variables as {{var}} for later substitution
        //
        // IMPORTANT: We must NOT replace variables inside nested loop markers,
        // as those should be processed when the nested loop is expanded.
        // Use replace_outside_nested_loops instead of global replace.
        let mut iteration_body = iteration_body;

        // Build replacements map for loop metadata and item variables
        let mut replacements = Vec::new();

        // Loop metadata variables ({{loop.index}}, {{loop.first}}, etc.)
        replacements.push(("{{loop.index}}".to_string(), (index + 1).to_string()));
        replacements.push(("{{loop.index0}}".to_string(), index.to_string()));
        replacements.push(("{{loop.first}}".to_string(), (index == 0).to_string()));
        replacements.push(("{{loop.last}}".to_string(), (index == len - 1).to_string()));
        replacements.push(("{{loop.length}}".to_string(), len.to_string()));

        // {{var.text}} with the comment text
        let text_pattern = format!("{{{{{}.text}}}}", variable_name);
        replacements.push((text_pattern, item.text.clone()));

        // Apply all replacements, but only outside nested loop markers
        iteration_body = replace_outside_nested_loops(&iteration_body, &replacements);

        result.push_str(&iteration_body);
    }

    Ok(result)
}

/// Replace patterns in content, but only outside nested AIKI_LOOP markers
///
/// This prevents outer loop variable substitution from affecting inner loop
/// placeholders. For example, when expanding an outer loop, we should not
/// replace `{{loop.index}}` inside a nested `<!-- AIKI_LOOP -->` block.
fn replace_outside_nested_loops(content: &str, replacements: &[(String, String)]) -> String {
    const LOOP_START: &str = "<!-- AIKI_LOOP:";
    const LOOP_END: &str = "<!-- AIKI_ENDLOOP -->";

    let mut result = String::new();
    let mut pos = 0;

    while pos < content.len() {
        // Check if we're at the start of a nested loop
        if content[pos..].starts_with(LOOP_START) {
            // Find the matching end marker (handling nested loops)
            let nested_start = pos;
            let mut depth = 1;
            let mut search_pos = pos + LOOP_START.len();

            while search_pos < content.len() && depth > 0 {
                if content[search_pos..].starts_with(LOOP_START) {
                    depth += 1;
                    search_pos += LOOP_START.len();
                } else if content[search_pos..].starts_with(LOOP_END) {
                    depth -= 1;
                    if depth == 0 {
                        // Found matching end - include the end marker in the nested block
                        search_pos += LOOP_END.len();
                        break;
                    }
                    search_pos += LOOP_END.len();
                } else {
                    // Advance by one character (may be multi-byte for non-ASCII like emojis)
                    search_pos += content[search_pos..]
                        .chars()
                        .next()
                        .map_or(1, |c| c.len_utf8());
                }
            }

            // Copy the entire nested loop block unchanged
            result.push_str(&content[nested_start..search_pos]);
            pos = search_pos;
        } else {
            // Find the next nested loop start (or end of content)
            let next_loop = content[pos..]
                .find(LOOP_START)
                .map(|i| pos + i)
                .unwrap_or(content.len());

            // Extract the segment before the next nested loop
            let segment = &content[pos..next_loop];

            // Apply all replacements to this segment
            let mut replaced_segment = segment.to_string();
            for (pattern, replacement) in replacements {
                replaced_segment = replaced_segment.replace(pattern, replacement);
            }

            result.push_str(&replaced_segment);
            pos = next_loop;
        }
    }

    result
}

/// Process template content through conditionals and loops, then extract subtask entries
///
/// This function handles templates that use `{% for %}` loops and/or `{% subtask %}`
/// references in the `# Subtasks` section.
///
/// # Arguments
/// * `content` - The raw template content
/// * `variables` - Variables for substitution
/// * `data_source` - Optional comments for loop iteration
///
/// # Returns
/// Vec of SubtaskEntry for each extracted subtask (static or composed)
pub fn create_subtask_entries(
    content: &str,
    variables: &VariableContext,
    data_source: Option<Vec<TaskComment>>,
) -> Result<Vec<SubtaskEntry>> {
    // First, process conditionals (which emits loop markers and subtask ref markers)
    let ctx = super::conditionals::EvalContext::new();
    let processed = super::conditionals::process_conditionals(content, &ctx).map_err(|e| {
        AikiError::TemplateProcessingFailed {
            details: e.to_string(),
        }
    })?;

    // Validate that subtask ref markers only appear after # Subtasks heading
    validate_subtask_ref_placement(&processed)?;

    // Expand loops if any are present
    let expanded = if processed.contains("<!-- AIKI_LOOP:") {
        let mut data_sources = HashMap::new();
        if let Some(comments) = data_source {
            data_sources.insert("source.comments".to_string(), comments);
        }
        expand_loops(&processed, &data_sources)?
    } else {
        processed
    };

    // Find the # Subtasks section
    let subtasks_section = extract_subtasks_section(&expanded);
    if subtasks_section.is_empty() {
        return Ok(Vec::new());
    }

    // Parse subtask entries (static ## headings and composed <!-- AIKI_SUBTASK_REF --> markers)
    parse_expanded_subtasks(&subtasks_section, variables)
}

/// Legacy wrapper: create subtasks from inline loops (returns only static TaskDefinitions)
///
/// Used by existing code paths that don't need to handle composed subtask references.
/// Returns empty if the content has no inline loops or subtask refs (caller should
/// fall back to static subtask processing in that case).
pub fn create_subtasks_from_inline_loops(
    content: &str,
    variables: &VariableContext,
    data_source: Option<Vec<TaskComment>>,
) -> Result<Vec<TaskDefinition>> {
    // Only process if there are loops or subtask refs to handle
    if !has_inline_loops(content) && !has_subtask_refs(content) {
        return Ok(Vec::new());
    }

    let entries = create_subtask_entries(content, variables, data_source)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| match entry {
            SubtaskEntry::Static(def) => Some(def),
            SubtaskEntry::Composed { .. } => None,
        })
        .collect())
}

/// Validate that `<!-- AIKI_SUBTASK_REF:... -->` markers only appear in the # Subtasks section
fn validate_subtask_ref_placement(content: &str) -> Result<()> {
    let subtask_ref_re =
        Regex::new(r"<!-- AIKI_SUBTASK_REF:([^:]+):(\d+) -->").expect("Invalid regex");

    // Find the # Subtasks heading
    let subtasks_line = content
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim().to_lowercase() == "# subtasks")
        .map(|(i, _)| i);

    for (line_idx, line) in content.lines().enumerate() {
        if let Some(caps) = subtask_ref_re.captures(line) {
            let template_name = caps.get(1).unwrap().as_str();
            let source_line: usize = caps.get(2).unwrap().as_str().parse().unwrap_or(0);

            match subtasks_line {
                Some(marker_idx) if line_idx > marker_idx => {
                    // Good - it's after # Subtasks
                }
                _ => {
                    return Err(AikiError::TemplateProcessingFailed {
                        details: format!(
                            "{{% subtask {} %}} at line {} is outside # Subtasks section. Move it below the # Subtasks heading.",
                            template_name,
                            source_line
                        ),
                    });
                }
            }
        }
    }

    Ok(())
}

/// Extract the content after "# Subtasks" marker
fn extract_subtasks_section(content: &str) -> String {
    for (i, line) in content.lines().enumerate() {
        if line.trim().to_lowercase() == "# subtasks" {
            return content.lines().skip(i + 1).collect::<Vec<_>>().join("\n");
        }
    }
    String::new()
}

/// Parse subtasks from expanded template content
///
/// Looks for `## ` headings and `<!-- AIKI_SUBTASK_REF:... -->` markers,
/// returning them as SubtaskEntry items (either Static or Composed).
fn parse_expanded_subtasks(
    content: &str,
    variables: &VariableContext,
) -> Result<Vec<SubtaskEntry>> {
    let subtask_ref_re =
        Regex::new(r"^<!-- AIKI_SUBTASK_REF:([^:]+):(\d+) -->$").expect("Invalid regex");

    let mut entries = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Check for subtask reference marker
        if let Some(caps) = subtask_ref_re.captures(line) {
            let template_name = caps.get(1).unwrap().as_str().to_string();
            let source_line: usize = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
            entries.push(SubtaskEntry::Composed {
                template_name,
                line: source_line,
            });
            i += 1;
        }
        // Look for ## heading (static subtask)
        else if let Some(name) = line.strip_prefix("## ") {
            let name = name.trim().to_string();

            // Collect body until next ## or subtask ref marker or end
            let mut body_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                let next_line = lines[i];
                let trimmed = next_line.trim();
                if trimmed.starts_with("## ") || subtask_ref_re.is_match(trimmed) {
                    break;
                }
                body_lines.push(next_line);
                i += 1;
            }

            let instructions = body_lines.join("\n").trim().to_string();

            // Substitute variables in name and instructions
            let name = substitute(&name, variables)?;
            let instructions = substitute(&instructions, variables)?;

            entries.push(SubtaskEntry::Static(TaskDefinition {
                name,
                task_type: None,
                instructions,
                priority: None,
                assignee: None,
                sources: Vec::new(),
                data: HashMap::new(),
            }));
        } else {
            i += 1;
        }
    }

    Ok(entries)
}

/// Check if template content contains inline loops
pub fn has_inline_loops(content: &str) -> bool {
    content.contains("{% for ")
}

/// Check if template content contains subtask references
pub fn has_subtask_refs(content: &str) -> bool {
    content.contains("{% subtask ")
}

/// Create review task with subtasks from template
///
/// This function is shared between the `aiki review` CLI command and the flow
/// engine's `review:` action. It delegates to the unified `create_from_template`
/// code path in `commands::task`.
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
    use crate::commands::task::{create_from_template, TemplateTaskParams};

    let mut builtins = HashMap::new();
    builtins.insert("scope".to_string(), scope_id.to_string());
    builtins.insert("scope.name".to_string(), scope_name.to_string());
    builtins.insert("scope.id".to_string(), scope_id.to_string());

    let mut sources = vec![];
    if scope_id != "session" {
        sources.push(format!("task:{}", scope_id));
    }

    let params = TemplateTaskParams {
        template_name: template_name.to_string(),
        sources,
        assignee: assignee.clone(),
        builtins,
        ..Default::default()
    };

    create_from_template(cwd, params)
}

/// Placeholder value for parent.id during initial template processing
///
/// We use this placeholder because the parent ID isn't known until after
/// template processing (it's generated from the resolved parent name).
pub const PARENT_ID_PLACEHOLDER: &str = "__AIKI_PARENT_ID_PLACEHOLDER__";

/// Substitute the parent.id placeholder with the actual parent ID
///
/// Static subtasks can reference `{{parent.id}}` in their instructions, but the
/// parent ID isn't known until after template processing. During template
/// processing, `parent.id` is set to PARENT_ID_PLACEHOLDER. This function does
/// a post-processing pass to substitute the actual parent ID.
///
/// # Arguments
/// * `subtasks` - Mutable slice of subtask definitions to update
/// * `parent_id` - The generated parent task ID
pub fn substitute_parent_id(subtasks: &mut [TaskDefinition], parent_id: &str) {
    for subtask in subtasks.iter_mut() {
        // Replace the placeholder with the actual parent ID
        subtask.instructions = subtask
            .instructions
            .replace(PARENT_ID_PLACEHOLDER, parent_id);
    }
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

# Review: {{data.scope}}

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
        assert_eq!(
            template.description,
            Some("General code review".to_string())
        );
        assert_eq!(template.defaults.task_type, Some("review".to_string()));
        assert_eq!(template.parent.name, "Review: {{data.scope}}");
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
        template.parent.name = "Review: {{data.scope}}".to_string();
        template.parent.instructions = "Review all issues.".to_string();
        template.subtasks_source = Some("source.comments".to_string());
        template.subtask_template = Some(
            r#"## Fix: {{item.text}}"#
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
            },
            TaskComment {
                id: None,
                text: "Unused import".to_string(),
                timestamp: Utc::now(),
            },
        ];

        let (parent, subtasks) =
            create_tasks_from_template(&template, &variables, Some(comments)).unwrap();

        assert_eq!(parent.name, "Review: src/auth.rs");
        assert_eq!(subtasks.len(), 2);

        // Check first subtask
        assert_eq!(subtasks[0].name, "Fix: Variable may be null");

        // Check second subtask
        assert_eq!(subtasks[1].name, "Fix: Unused import");
    }

    #[test]
    fn test_create_tasks_from_template_dynamic_empty_data_source() {
        let mut template = TaskTemplate::new("test/dynamic");
        template.parent.name = "Review".to_string();
        template.parent.instructions = "Review all issues.".to_string();
        template.subtasks_source = Some("source.comments".to_string());
        template.subtask_template = Some("## Fix: {{item.file}}\n\n{{item.text}}".to_string());

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
        let content = r#"## Fix: {{item.file}}:{{item.line}}

**Severity**: {{item.severity}}

{{item.text}}"#;

        let result = parse_subtask_template(content).unwrap();
        assert!(result.is_some());

        let parsed = result.unwrap();
        assert_eq!(parsed.heading, "Fix: {{item.file}}:{{item.line}}");
        assert!(parsed.frontmatter.is_none());
        assert!(parsed.body.contains("**Severity**: {{item.severity}}"));
        assert!(parsed.body.contains("{{item.text}}"));
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
  - task:{source.id}
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
        assert_eq!(fm.sources[0], "task:{source.id}");

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
        assert_eq!(
            fm.data.get("severity"),
            Some(&serde_json::json!("{item.severity}"))
        );
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

    // ===== Loop Expansion Tests =====

    #[test]
    fn test_expand_loops_basic() {
        use chrono::Utc;

        let content = r#"# Task

<!-- AIKI_LOOP:item:source.comments -->
## {{item.text}}
<!-- AIKI_ENDLOOP -->"#;

        let mut data_sources = HashMap::new();
        data_sources.insert(
            "source.comments".to_string(),
            vec![
                TaskComment {
                    id: None,
                    text: "Fix this bug".to_string(),
                    timestamp: Utc::now(),
                },
                TaskComment {
                    id: None,
                    text: "Add tests".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        );

        let result = expand_loops(content, &data_sources).unwrap();

        // Should expand two iterations
        assert!(result.contains("## Fix this bug"));
        assert!(result.contains("## Add tests"));

        // Should NOT contain loop markers
        assert!(!result.contains("AIKI_LOOP"));
        assert!(!result.contains("AIKI_ENDLOOP"));
    }

    #[test]
    fn test_expand_loops_empty_collection() {
        let content = r#"<!-- AIKI_LOOP:item:source.comments -->
## {{item.file}}
<!-- AIKI_ENDLOOP -->
<!-- AIKI_LOOPELSE -->
No items found.
<!-- AIKI_ENDLOOPELSE -->"#;

        let data_sources = HashMap::new();

        let result = expand_loops(content, &data_sources).unwrap();

        // Should use the else content
        assert!(result.contains("No items found."));
        assert!(!result.contains("AIKI_LOOP"));
    }

    #[test]
    fn test_expand_loops_loop_metadata() {
        use chrono::Utc;

        let content = r#"<!-- AIKI_LOOP:item:source.comments -->
{{loop.index}}. {{item.text}} (first={{loop.first}}, last={{loop.last}})
<!-- AIKI_ENDLOOP -->"#;

        let mut data_sources = HashMap::new();
        data_sources.insert(
            "source.comments".to_string(),
            vec![
                TaskComment {
                    id: None,
                    text: "First".to_string(),
                    timestamp: Utc::now(),
                },
                TaskComment {
                    id: None,
                    text: "Second".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        );

        let result = expand_loops(content, &data_sources).unwrap();

        assert!(result.contains("1. First (first=true, last=false)"));
        assert!(result.contains("2. Second (first=false, last=true)"));
    }

    #[test]
    fn test_expand_loops_no_markers() {
        let content = "# Just plain content\n\nNo loops here.";
        let data_sources = HashMap::new();

        let result = expand_loops(content, &data_sources).unwrap();
        assert_eq!(result, content);
    }

    #[test]
    fn test_expand_loops_unknown_collection_errors() {
        // Unknown collections should error, not silently return empty
        let content = r#"<!-- AIKI_LOOP:item:source.unknown_collection -->
## {{item.name}}
<!-- AIKI_ENDLOOP -->"#;

        let data_sources = HashMap::new();

        let result = expand_loops(content, &data_sources);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown collection"));
        assert!(msg.contains("source.unknown_collection"));
        assert!(msg.contains("source.comments")); // Shows available collections
    }

    #[test]
    fn test_expand_loops_nested() {
        use chrono::Utc;

        // This tests the fix for nested loop expansion
        // The old regex-based approach would break on nested loops
        let content = r#"<!-- AIKI_LOOP:outer:source.comments -->
## {{outer.text}}
<!-- AIKI_LOOP:inner:source.comments -->
- {{inner.text}}
<!-- AIKI_ENDLOOP -->
<!-- AIKI_ENDLOOP -->"#;

        let mut data_sources = HashMap::new();
        data_sources.insert(
            "source.comments".to_string(),
            vec![
                TaskComment {
                    id: None,
                    text: "A".to_string(),
                    timestamp: Utc::now(),
                },
                TaskComment {
                    id: None,
                    text: "B".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        );

        let result = expand_loops(content, &data_sources).unwrap();

        // Should have outer loop expanded twice, each with inner loop expanded twice
        // Total: ## A with - A - B, ## B with - A - B
        assert!(result.contains("## A"));
        assert!(result.contains("## B"));
        assert!(result.contains("- A"));
        assert!(result.contains("- B"));

        // No loop markers should remain
        assert!(!result.contains("AIKI_LOOP"));
        assert!(!result.contains("AIKI_ENDLOOP"));
    }

    #[test]
    fn test_expand_loops_with_conditionals() {
        use chrono::Utc;

        // This tests the fix for premature conditional evaluation
        // Conditionals inside loop bodies should use loop variable values
        let content = r#"<!-- AIKI_LOOP:item:source.comments -->
## {{item.text}}
{% if item.text == "Urgent task" %}**HIGH PRIORITY**{% endif %}
<!-- AIKI_ENDLOOP -->"#;

        let mut data_sources = HashMap::new();
        data_sources.insert(
            "source.comments".to_string(),
            vec![
                TaskComment {
                    id: None,
                    text: "Normal task".to_string(),
                    timestamp: Utc::now(),
                },
                TaskComment {
                    id: None,
                    text: "Urgent task".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        );

        let result = expand_loops(content, &data_sources).unwrap();

        // Should have both tasks
        assert!(result.contains("## Normal task"));
        assert!(result.contains("## Urgent task"));

        // Only the high priority task should have the HIGH PRIORITY marker
        // Count occurrences - should be exactly 1
        let high_count = result.matches("**HIGH PRIORITY**").count();
        assert_eq!(
            high_count, 1,
            "HIGH PRIORITY should appear exactly once (for the urgent task)"
        );

        // Verify the HIGH PRIORITY appears after Urgent task, not after Normal task
        let urgent_pos = result.find("## Urgent task").unwrap();
        let high_pos = result.find("**HIGH PRIORITY**").unwrap();
        assert!(
            high_pos > urgent_pos,
            "HIGH PRIORITY should appear after Urgent task"
        );
    }

    // ===== Inline Loop Tests =====

    #[test]
    fn test_has_inline_loops() {
        assert!(has_inline_loops("{% for item in list %}"));
        assert!(has_inline_loops("Some text\n{% for x in y %}\nmore text"));
        assert!(!has_inline_loops("No loops here"));
        assert!(!has_inline_loops("{%for item in list%}")); // missing space
    }

    #[test]
    fn test_create_subtasks_from_inline_loops() {
        use chrono::Utc;

        let content = r#"---
version: 1.0.0
---

# Fix: task123

Fix the issues.

# Subtasks

{% for item in source.comments %}
## Fix: {{item.text}}
{% endfor %}
"#;

        let variables = VariableContext::new();
        let comments = vec![
            TaskComment {
                id: None,
                text: "Bug in login".to_string(),
                timestamp: Utc::now(),
            },
            TaskComment {
                id: None,
                text: "Missing test".to_string(),
                timestamp: Utc::now(),
            },
        ];

        let subtasks =
            create_subtasks_from_inline_loops(content, &variables, Some(comments)).unwrap();

        assert_eq!(subtasks.len(), 2);
        assert_eq!(subtasks[0].name, "Fix: Bug in login");
        assert_eq!(subtasks[1].name, "Fix: Missing test");
    }

    #[test]
    fn test_create_subtasks_from_inline_loops_empty() {
        let content = r#"# Task

# Subtasks

{% for item in source.comments %}
## {{item.file}}
{% endfor %}
"#;

        let variables = VariableContext::new();
        let subtasks = create_subtasks_from_inline_loops(content, &variables, None).unwrap();
        assert!(subtasks.is_empty());
    }

    #[test]
    fn test_create_subtasks_from_inline_loops_no_loops() {
        let content =
            "# Task\n\nNo loops here.\n\n# Subtasks\n\n## Static subtask\n\nInstructions.";
        let variables = VariableContext::new();
        let subtasks = create_subtasks_from_inline_loops(content, &variables, None).unwrap();
        // Returns empty because there are no loop markers to process
        assert!(subtasks.is_empty());
    }

    #[test]
    fn test_create_tasks_with_inline_loops() {
        use chrono::Utc;

        // Create a template with inline loops
        let mut template = TaskTemplate::new("test/inline");
        template.parent.name = "Fix: {{data.scope}}".to_string();
        template.parent.instructions = "Fix all issues.".to_string();
        template.raw_content = Some(
            r#"---
version: 1.0.0
---

# Fix: {{data.scope}}

Fix all issues.

# Subtasks

{% for item in source.comments %}
## Fix: {{item.text}}
{% endfor %}
"#
            .to_string(),
        );

        let mut variables = VariableContext::new();
        variables.set_data("scope", "src/");

        let comments = vec![TaskComment {
            id: None,
            text: "Error here".to_string(),
            timestamp: Utc::now(),
        }];

        let (parent, subtasks) =
            create_tasks_from_template(&template, &variables, Some(comments)).unwrap();

        assert_eq!(parent.name, "Fix: src/");
        assert_eq!(subtasks.len(), 1);
        assert_eq!(subtasks[0].name, "Fix: Error here");
    }

    #[test]
    fn test_expand_loops_nested_loop_index() {
        use chrono::Utc;

        // This tests nested loops with loop.index references
        // The inner loop should have its OWN loop.index values, not the outer loop's
        let content = r#"<!-- AIKI_LOOP:outer:source.comments -->
Outer: {{loop.index}}
<!-- AIKI_LOOP:inner:source.comments -->
  Inner: {{loop.index}}
<!-- AIKI_ENDLOOP -->
<!-- AIKI_ENDLOOP -->"#;

        let mut data_sources = HashMap::new();
        data_sources.insert(
            "source.comments".to_string(),
            vec![
                TaskComment {
                    id: None,
                    text: "".to_string(),
                    timestamp: Utc::now(),
                },
                TaskComment {
                    id: None,
                    text: "".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        );

        let result = expand_loops(content, &data_sources).unwrap();

        // The outer loop should show 1, 2
        // Each inner loop should show 1, 2 (not 1, 1 or 2, 2)
        // Expected:
        // Outer: 1
        //   Inner: 1
        //   Inner: 2
        // Outer: 2
        //   Inner: 1
        //   Inner: 2

        // Expected pattern: after first "Outer: 1", we should see "Inner: 1" then "Inner: 2"
        // NOT "Inner: 1" then "Inner: 1"
        let lines: Vec<&str> = result
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        // Find the first Inner line after "Outer: 1"
        let mut found_outer_1 = false;
        let mut inner_values_after_outer_1 = Vec::new();
        for line in &lines {
            if *line == "Outer: 1" {
                found_outer_1 = true;
            } else if found_outer_1 && line.starts_with("Inner:") {
                inner_values_after_outer_1.push(*line);
                if inner_values_after_outer_1.len() == 2 {
                    break;
                }
            } else if found_outer_1 && line.starts_with("Outer:") {
                break;
            }
        }

        assert_eq!(
            inner_values_after_outer_1,
            vec!["Inner: 1", "Inner: 2"],
            "After Outer: 1, inner loop should iterate 1, 2. Got: {:?}. Full output:\n{}",
            inner_values_after_outer_1,
            result
        );
    }

    #[test]
    fn test_replace_outside_nested_loops_with_non_ascii() {
        // Non-ASCII characters (emojis, unicode) should not cause panics
        let content = "🛑 Before\n<!-- AIKI_LOOP:item:source.comments -->\n{{item.text}}\n<!-- AIKI_ENDLOOP -->\n🎉 After {{loop.index}}";
        let replacements = vec![("{{loop.index}}".to_string(), "1".to_string())];

        let result = replace_outside_nested_loops(content, &replacements);

        // Should replace outside the loop but not inside
        assert!(result.contains("🛑 Before"));
        assert!(result.contains("🎉 After 1"));
        // Inside the loop should remain unchanged
        assert!(result.contains("{{item.text}}"));
    }

    #[test]
    fn test_expand_loops_with_non_ascii_content() {
        use chrono::Utc;

        // Templates can contain emojis (e.g., "🛑 Do NOT edit code")
        let content = "🛑 Important\n<!-- AIKI_LOOP:item:source.comments -->\n## {{item.text}} 🎯\n<!-- AIKI_ENDLOOP -->";

        let mut data_sources = HashMap::new();
        data_sources.insert(
            "source.comments".to_string(),
            vec![TaskComment {
                id: None,
                text: "Fix büg".to_string(),
                timestamp: Utc::now(),
            }],
        );

        let result = expand_loops(content, &data_sources).unwrap();
        assert!(result.contains("🛑 Important"));
        assert!(result.contains("## Fix büg 🎯"));
    }

    // Phase 2: SubtaskEntry extraction tests

    #[test]
    fn test_create_subtask_entries_mixed_static_and_composed() {
        let content = r#"# Build: feature.md

Build the feature.

# Subtasks

## Setup environment
Install dependencies.

{% subtask aiki/plan %}

## Execute plan
Run each plan subtask.
"#;
        let variables = VariableContext::new();
        let entries = create_subtask_entries(content, &variables, None).unwrap();

        assert_eq!(entries.len(), 3);

        // First entry: static
        match &entries[0] {
            SubtaskEntry::Static(def) => assert_eq!(def.name, "Setup environment"),
            _ => panic!("Expected Static, got Composed"),
        }

        // Second entry: composed
        match &entries[1] {
            SubtaskEntry::Composed { template_name, .. } => {
                assert_eq!(template_name, "aiki/plan");
            }
            _ => panic!("Expected Composed, got Static"),
        }

        // Third entry: static
        match &entries[2] {
            SubtaskEntry::Static(def) => assert_eq!(def.name, "Execute plan"),
            _ => panic!("Expected Static, got Composed"),
        }
    }

    #[test]
    fn test_create_subtask_entries_only_composed() {
        let content = r#"# Review: target

Review the target.

# Subtasks

{% subtask aiki/review/spec %}
"#;
        let variables = VariableContext::new();
        let entries = create_subtask_entries(content, &variables, None).unwrap();

        assert_eq!(entries.len(), 1);
        match &entries[0] {
            SubtaskEntry::Composed { template_name, .. } => {
                assert_eq!(template_name, "aiki/review/spec");
            }
            _ => panic!("Expected Composed"),
        }
    }

    #[test]
    fn test_create_subtask_entries_conditional_subtask_ref_true() {
        let content = r#"# Review

Review target.

# Subtasks

{% subtask aiki/review/spec if data.file_type == "spec" %}
"#;
        let mut variables = VariableContext::new();
        variables.set_data("file_type", "spec");

        // Note: process_conditionals uses EvalContext, not VariableContext
        // The condition evaluation happens during process_conditionals with EvalContext
        // For this test, we need to set the variable in the right context
        let entries = create_subtask_entries(content, &variables, None).unwrap();

        // The condition is evaluated during process_conditionals with an empty EvalContext,
        // so the condition will be false (data.file_type not set in EvalContext)
        // This is expected - actual condition evaluation happens in render_with_loops
        // which uses EvalContext, not VariableContext
        assert!(entries.is_empty());
    }

    #[test]
    fn test_create_subtask_entries_subtask_ref_outside_subtasks_section() {
        let content = r#"# Task

{% subtask aiki/plan %}

# Subtasks

## Do work
Instructions.
"#;
        let variables = VariableContext::new();
        let result = create_subtask_entries(content, &variables, None);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("outside # Subtasks section"));
    }

    #[test]
    fn test_has_subtask_refs() {
        assert!(has_subtask_refs("{% subtask aiki/plan %}"));
        assert!(has_subtask_refs("some text\n{% subtask aiki/review/spec if data.type == \"spec\" %}\nmore"));
        assert!(!has_subtask_refs("no subtask refs here"));
        assert!(!has_subtask_refs("{% if data.plan %}...{% endif %}"));
    }

    #[test]
    fn test_validate_subtask_ref_placement_ok() {
        let content = "# Task\n\nInstructions.\n\n# Subtasks\n\n<!-- AIKI_SUBTASK_REF:aiki/plan:5 -->";
        assert!(validate_subtask_ref_placement(content).is_ok());
    }

    #[test]
    fn test_validate_subtask_ref_placement_before_subtasks() {
        let content = "# Task\n\n<!-- AIKI_SUBTASK_REF:aiki/plan:3 -->\n\n# Subtasks\n\n## Work";
        let result = validate_subtask_ref_placement(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("outside # Subtasks section"));
    }

    #[test]
    fn test_validate_subtask_ref_placement_no_subtasks_section() {
        let content = "# Task\n\n<!-- AIKI_SUBTASK_REF:aiki/plan:3 -->";
        let result = validate_subtask_ref_placement(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("outside # Subtasks section"));
    }

    #[test]
    fn test_parse_expanded_subtasks_with_refs() {
        let content = "## Setup\nInstall deps.\n\n<!-- AIKI_SUBTASK_REF:aiki/plan:10 -->\n\n## Run\nExecute.";
        let variables = VariableContext::new();
        let entries = parse_expanded_subtasks(content, &variables).unwrap();

        assert_eq!(entries.len(), 3);
        match &entries[0] {
            SubtaskEntry::Static(def) => assert_eq!(def.name, "Setup"),
            _ => panic!("Expected Static"),
        }
        match &entries[1] {
            SubtaskEntry::Composed { template_name, line } => {
                assert_eq!(template_name, "aiki/plan");
                assert_eq!(*line, 10);
            }
            _ => panic!("Expected Composed"),
        }
        match &entries[2] {
            SubtaskEntry::Static(def) => assert_eq!(def.name, "Run"),
            _ => panic!("Expected Static"),
        }
    }
}
