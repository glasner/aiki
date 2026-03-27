//! Plan command for interactive plan authoring
//!
//! This module provides the `aiki plan` command which:
//! - Creates and refines plan documents collaboratively with an AI agent
//! - Supports subcommands: `epic` (default) and `fix`
//! - `aiki plan` / `aiki plan epic [args]` → interactive plan authoring
//! - `aiki plan fix <review-id>` → create fix plan from review issues
//! - Always runs interactively (no --async or --start flags)

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::output_utils;
use crate::session::find_active_session;
use crate::tasks::md::MdBuilder;
use crate::tasks::runner::{task_run, TaskRunOptions};
use crate::tasks::templates::{
    create_tasks_from_template, find_templates_dir, load_template, substitute_parent_id,
    VariableContext, PARENT_ID_PLACEHOLDER,
};
use crate::tasks::{
    generate_task_id, get_current_scope_set, get_in_progress, get_ready_queue_for_scope_set,
    materialize_graph, read_events, reassign_task, start_task_core, write_event, Task, TaskEvent,
    TaskPriority, TaskStatus,
};

/// Plan mode determined from input arguments
#[derive(Debug, Clone)]
pub enum PlanMode {
    /// Edit an existing plan file
    Edit { path: PathBuf, text: String },
    /// Create a new plan at the specified path
    CreateAtPath {
        path: PathBuf,
        initial_idea: String,
        text: String,
    },
    /// Create a new plan with an auto-generated filename
    Autogen { description: String, slug: String },
}

/// Run the plan command
///
/// Dispatches to subcommands:
/// - `aiki plan` (no args) → defaults to epic behavior
/// - `aiki plan epic [args]` → epic plan authoring
/// - `aiki plan fix <review-id>` → create fix plan from review issues
/// - `aiki plan <anything-else>` → epic plan authoring (backward compat)
pub fn run(
    args: Vec<String>,
    template: Option<String>,
    agent: Option<String>,
    output_format: Option<super::OutputFormat>,
) -> Result<()> {
    let cwd = env::current_dir()
        .map_err(|_| AikiError::InvalidArgument("Failed to get current directory".to_string()))?;

    let output_id = matches!(output_format, Some(super::OutputFormat::Id));

    // Dispatch based on first argument
    match args.first().map(|s| s.as_str()) {
        Some("epic") => {
            // `aiki plan epic [args...]` → strip "epic" and run epic plan
            let epic_args = args[1..].to_vec();
            let mode = determine_mode(&cwd, &epic_args)?;
            run_epic(&cwd, mode, template, agent, output_id)
        }
        Some("fix") => {
            // `aiki plan fix <review-id>` → create fix plan from review
            let review_id = args.get(1).ok_or_else(|| {
                AikiError::InvalidArgument(
                    "Missing review ID. Usage: aiki plan fix <review-id>".to_string(),
                )
            })?;
            run_fix(&cwd, review_id, template, agent, output_id)
        }
        _ => {
            // No subcommand or unrecognized first arg → default to epic behavior
            let mode = determine_mode(&cwd, &args)?;
            run_epic(&cwd, mode, template, agent, output_id)
        }
    }
}

/// Determine plan mode from command arguments
fn determine_mode(cwd: &Path, args: &[String]) -> Result<PlanMode> {
    if args.is_empty() {
        // Interactive mode - would prompt for input
        // For now, return an error since we need input
        return Err(AikiError::InvalidArgument(
            "No plan path or description provided. Usage: aiki plan <path-or-text...>".to_string(),
        ));
    }

    let first_arg = &args[0];

    // Check if first arg ends with .md
    if first_arg.ends_with(".md") {
        let path = if first_arg.starts_with('/') {
            PathBuf::from(first_arg)
        } else {
            cwd.join(first_arg)
        };

        // Validate path is inside repo
        validate_path_in_repo(cwd, &path)?;

        // Remaining args after the .md path become free-form guidance text
        let text = if args.len() > 1 {
            args[1..].join(" ")
        } else {
            String::new()
        };

        if path.exists() {
            // Check it's a file, not a directory
            if !path.is_file() {
                return Err(AikiError::InvalidArgument(format!(
                    "Not a markdown file: {}",
                    path.display()
                )));
            }
            // Edit mode
            Ok(PlanMode::Edit { path, text })
        } else {
            // Create at path mode - parse initial idea from filename
            let initial_idea = parse_idea_from_filename(&path);
            Ok(PlanMode::CreateAtPath {
                path,
                initial_idea,
                text,
            })
        }
    } else {
        // Autogen mode - join all args as description
        let description = args.join(" ");
        let slug = generate_slug(&description);

        // Find a unique filename
        let base_path = cwd.join("ops/now");
        let path = find_unique_path(&base_path, &slug)?;

        Ok(PlanMode::Autogen {
            description,
            slug: path.file_stem().unwrap().to_string_lossy().to_string(),
        })
    }
}

/// Validate that a path is inside the repository
fn validate_path_in_repo(cwd: &Path, path: &Path) -> Result<()> {
    // Canonicalize both paths for comparison
    let cwd_canonical = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());

    // For paths that don't exist yet, canonicalize the parent
    let path_for_check = if path.exists() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else if let Some(parent) = path.parent() {
        if parent.exists() {
            parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf())
        } else {
            // Parent doesn't exist, use the path as-is
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    if !path_for_check.starts_with(&cwd_canonical) {
        return Err(AikiError::InvalidArgument(
            "Path must be inside repository".to_string(),
        ));
    }

    Ok(())
}

/// Parse initial idea from filename
/// e.g., "dark-mode.md" -> "Dark Mode"
fn parse_idea_from_filename(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Convert hyphens to spaces and capitalize words
    stem.split('-')
        .map(|word| {
            let mut chars: Vec<char> = word.chars().collect();
            if let Some(first) = chars.first_mut() {
                *first = first.to_uppercase().next().unwrap_or(*first);
            }
            chars.into_iter().collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Generate a slug from a description
/// e.g., "Add User Auth" -> "add-user-auth"
fn generate_slug(description: &str) -> String {
    description
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                // Skip special characters
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect::<String>()
        // Collapse multiple hyphens
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Whether the interactive prompt should fire.
///
/// Interactive prompt fires when:
/// - Creating a new file (not editing existing)
/// - Not autogen mode (description IS the idea)
/// - No trailing CLI text was provided (args_text was empty)
///
/// When editing an existing file, skip the prompt and go straight to the agent,
/// which will digest the plan and ask clarifying questions.
fn initial_idea_needs_input(mode: &PlanMode, has_cli_text: bool, is_new: bool) -> bool {
    is_new && !matches!(mode, PlanMode::Autogen { .. }) && !has_cli_text
}

/// Prompt for multi-line text input using crossterm raw mode.
///
/// - Enter: submit text
/// - Shift+Enter: insert newline
/// - Esc: skip (return None)
/// - Ctrl+C: skip (return None)
/// - Backspace: delete character
///
/// Returns None if skipped, Some(text) if submitted.
fn prompt_multiline_input(header: &str) -> Result<Option<String>> {
    let mut stderr = io::stderr();

    // Print the prompt header
    eprintln!("\x1b[1m{}\x1b[0m", header);
    eprintln!("\x1b[2m(Enter to submit, Shift+Enter for newline, Esc to skip)\x1b[0m");
    eprint!("> ");
    stderr.flush().ok();

    enable_raw_mode()
        .map_err(|e| AikiError::InvalidArgument(format!("Failed to enable raw mode: {}", e)))?;

    let mut lines: Vec<String> = vec![String::new()];

    loop {
        let ev = event::read().map_err(|e| {
            disable_raw_mode().ok();
            AikiError::InvalidArgument(format!("Failed to read input: {}", e))
        })?;

        if let Event::Key(key_event) = ev {
            // Only react to Press events (not Release/Repeat)
            if key_event.kind != crossterm::event::KeyEventKind::Press {
                continue;
            }

            match (key_event.code, key_event.modifiers) {
                // Esc: skip
                (KeyCode::Esc, _) => {
                    disable_raw_mode().ok();
                    eprintln!();
                    return Ok(None);
                }
                // Ctrl+C: skip
                (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                    disable_raw_mode().ok();
                    eprintln!();
                    return Ok(None);
                }
                // Shift+Enter: new line
                (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
                    lines.push(String::new());
                    eprint!("\r\n> ");
                    stderr.flush().ok();
                }
                // Enter (without shift): submit
                (KeyCode::Enter, _) => {
                    disable_raw_mode().ok();
                    eprintln!();
                    let text = lines.join("\n").trim().to_string();
                    return Ok(if text.is_empty() { None } else { Some(text) });
                }
                // Backspace
                (KeyCode::Backspace, _) => {
                    if let Some(current_line) = lines.last_mut() {
                        if !current_line.is_empty() {
                            current_line.pop();
                            eprint!("\x08 \x08");
                            stderr.flush().ok();
                        } else if lines.len() > 1 {
                            lines.pop();
                            let prev = lines.last().unwrap();
                            eprint!("\x1b[A\r> {}\x1b[K", prev);
                            stderr.flush().ok();
                        }
                    }
                }
                // Regular character
                (KeyCode::Char(c), _) => {
                    if let Some(current_line) = lines.last_mut() {
                        current_line.push(c);
                        eprint!("{}", c);
                        stderr.flush().ok();
                    }
                }
                _ => {}
            }
        }
    }
}

/// Find a unique path for a plan file, incrementing suffix if needed
fn find_unique_path(base_dir: &Path, slug: &str) -> Result<PathBuf> {
    // Ensure base directory exists
    fs::create_dir_all(base_dir).map_err(|e| {
        AikiError::InvalidArgument(format!(
            "Cannot create directory {}: {}",
            base_dir.display(),
            e
        ))
    })?;

    let base_path = base_dir.join(format!("{}.md", slug));

    if !base_path.exists() {
        return Ok(base_path);
    }

    // Try incrementing suffixes
    for i in 2..=100 {
        let path = base_dir.join(format!("{}-{}.md", slug, i));
        if !path.exists() {
            return Ok(path);
        }
    }

    Err(AikiError::InvalidArgument(format!(
        "Could not find unique filename for slug: {}",
        slug
    )))
}

/// Core epic plan implementation
fn run_epic(
    cwd: &Path,
    mode: PlanMode,
    template_name: Option<String>,
    agent: Option<String>,
    output_id: bool,
) -> Result<()> {
    let timestamp = chrono::Utc::now();

    // Determine plan file path, initial idea, and args-provided text
    let (plan_path, is_new, args_idea, args_text) = match &mode {
        PlanMode::Edit { path, text } => (path.clone(), false, String::new(), text.clone()),
        PlanMode::CreateAtPath {
            path,
            initial_idea,
            text,
        } => (path.clone(), true, initial_idea.clone(), text.clone()),
        PlanMode::Autogen { description, slug } => {
            let path = cwd.join("ops/now").join(format!("{}.md", slug));
            (path, true, description.clone(), String::new())
        }
    };

    // Build initial_idea by merging filename-derived idea + CLI text
    let has_cli_text = !args_text.is_empty();
    let mut initial_idea = if has_cli_text {
        if args_idea.is_empty() {
            args_text
        } else {
            format!("{}: {}", args_idea, args_text)
        }
    } else {
        args_idea
    };

    // Interactive prompt fires when no CLI text was provided (except autogen mode)
    if initial_idea_needs_input(&mode, has_cli_text, is_new) && io::stdin().is_terminal() {
        let header = if initial_idea.is_empty() {
            format!(
                "What would you like to accomplish with this plan?\nPlan file: {}",
                plan_path.display()
            )
        } else {
            format!(
                "What would you like to accomplish with this plan?\nPlan file: {} ({})\n",
                plan_path.display(),
                initial_idea
            )
        };
        if let Some(text) = prompt_multiline_input(&header)? {
            if initial_idea.is_empty() {
                initial_idea = text;
            } else {
                initial_idea = format!("{}: {}", initial_idea, text);
            }
        }
    }

    let agent_type = match agent.as_deref() {
        Some(agent_str) => Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.to_string()))?,
        ),
        None => None,
    };
    let (launch_agent, launch_binary) = resolve_plan_launch_agent(agent.as_deref())?;
    if !launch_agent.is_installed() {
        return Err(AikiError::InvalidArgument(format!(
            "Agent '{}' is not installed. {}",
            launch_agent.as_str(),
            launch_agent.install_hint()
        )));
    }

    // Check for existing plan task with source: file:<path>
    let events = read_events(cwd)?;
    let tasks = materialize_graph(&events).tasks;

    let source_key = format!("file:{}", plan_path.display());
    let existing_task = tasks.values().find(|t| {
        t.task_type.as_deref() == Some("plan")
            && t.status != TaskStatus::Closed
            && t.sources.iter().any(|s| s == &source_key)
    });

    let plan_task_id = if let Some(task) = existing_task {
        // Resume existing task
        output_utils::emit(|| format!("Resuming existing plan task: {}", task.id));

        // If agent differs, update assignee
        if let Some(agent) = agent_type {
            if task.assignee.as_deref() != Some(agent.as_str()) {
                reassign_task(cwd, &task.id, agent.as_str())?;
            }
        }

        task.id.clone()
    } else {
        // Create new plan task
        create_plan_task(
            cwd,
            &plan_path,
            &initial_idea,
            is_new,
            template_name.as_deref().unwrap_or("plan"),
            agent_type.as_ref().map(|a| a.as_str().to_string()),
            timestamp,
        )?
    };

    // If this is a new file, create it
    if is_new && !plan_path.exists() {
        // Ensure parent directory exists
        if let Some(parent) = plan_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AikiError::InvalidArgument(format!(
                    "Cannot create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Create file with draft frontmatter
        fs::write(&plan_path, "---\ndraft: true\n---\n\n").map_err(|e| {
            AikiError::InvalidArgument(format!("Cannot write to {}: {}", plan_path.display(), e))
        })?;
    }

    // Re-read tasks to include newly created plan task
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    let tasks = &graph.tasks;
    let scope_set = get_current_scope_set(&graph);
    let in_progress: Vec<&Task> = get_in_progress(tasks).into_iter().collect();
    let ready = get_ready_queue_for_scope_set(&graph, &scope_set);

    // Reassign task to current agent if we're in an agent session
    if let Some(session) = find_active_session(cwd) {
        if agent.is_none() {
            // Use the current session's agent
            reassign_task(cwd, &plan_task_id, session.agent_type.as_str())?;
        }
    }

    // Start the task
    start_task_core(cwd, &[plan_task_id.clone()])?;

    // Output started message
    output_plan_started(&plan_task_id, &plan_path, is_new, &in_progress, &ready)?;

    // Spawn Claude interactively (not using task_run which is for autonomous execution)
    // The prompt includes the user's context so Claude sees it immediately,
    // rather than requiring it to discover the instructions via `aiki task show`
    let prompt = if initial_idea.is_empty() {
        format!(
            "Run `aiki task start {}` to begin working on this plan task.",
            plan_task_id
        )
    } else {
        format!(
            "Run `aiki task start {}` to begin working on this plan task.\n\nUser's guidance: {}",
            plan_task_id, initial_idea
        )
    };

    if !output_id {
        output_utils::emit(|| {
            format!(
                "Spawning {} agent session for task {}...",
                launch_agent.display_name(),
                plan_task_id
            )
        });
    }

    // Spawn the selected agent interactively - inherits stdin/stdout/stderr for user interaction
    // Note: We don't use --print or --dangerously-skip-permissions here because
    // plan sessions are interactive and the user can approve actions themselves
    // AIKI_THREAD is set so the session tracks which thread is driving it, enabling
    // auto-end when the thread's tail task closes
    let thread = crate::tasks::lanes::ThreadId::single(plan_task_id.clone());
    let status = Command::new(launch_binary)
        .current_dir(cwd)
        .env("AIKI_THREAD", &thread.serialize())
        .arg(&prompt)
        .status();

    match status {
        Ok(exit_status) => {
            if exit_status.success() {
                if !output_id {
                    output_plan_completed(&plan_task_id, &plan_path)?;
                }
            } else {
                // Claude exited with non-zero - could be user cancelled, graceful termination, or error
                let code = exit_status.code().unwrap_or(-1);
                if code == 130 {
                    // SIGINT (Ctrl+C) - user cancelled, not an error
                    if !output_id {
                        output_utils::emit(|| "Plan session cancelled by user.".to_string());
                    }
                } else if code == 143 {
                    // SIGTERM - graceful termination (e.g., via `claude --exit` when task closes)
                    // This is expected behavior, not an error
                    if !output_id {
                        output_plan_completed(&plan_task_id, &plan_path)?;
                    }
                } else {
                    output_plan_error(
                        &plan_task_id,
                        &format!("{} exited with code {}", launch_agent.display_name(), code),
                    )?;
                }
            }
        }
        Err(e) => {
            output_plan_error(
                &plan_task_id,
                &format!(
                    "Failed to spawn {}: {}",
                    launch_agent.display_name().to_lowercase(),
                    e
                ),
            )?;
            return Err(AikiError::AgentSpawnFailed(e.to_string()));
        }
    }

    if output_id {
        println!("{}", plan_task_id);
    }

    Ok(())
}

/// Create a plan task from template
fn create_plan_task(
    cwd: &Path,
    plan_path: &Path,
    initial_idea: &str,
    is_new: bool,
    template_name: &str,
    assignee: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<String> {
    use crate::jj::get_working_copy_change_id;

    let working_copy = get_working_copy_change_id(cwd);

    // Load the template
    let templates_dir = find_templates_dir(cwd)?;
    let template = load_template(template_name, &templates_dir)?;

    // Set up variable context
    let mut variables = VariableContext::new();
    variables.set_data("plan_path", &plan_path.display().to_string());
    variables.set_data("is_new", if is_new { "true" } else { "false" });
    variables.set_data("initial_idea", initial_idea);

    // Set parent.id placeholder - it will be replaced after we generate the actual parent ID
    variables.set_parent("id", PARENT_ID_PLACEHOLDER);

    // Create tasks from template
    let (parent_def, mut subtask_defs) = create_tasks_from_template(&template, &variables, None)?;

    // Generate parent task ID
    let parent_id = generate_task_id(&parent_def.name);

    // Substitute {{parent.id}} in subtask instructions now that we have the parent ID
    substitute_parent_id(&mut subtask_defs, &parent_id);

    // Determine task type
    let task_type = parent_def
        .task_type
        .or_else(|| template.defaults.task_type.clone())
        .or_else(|| Some("plan".to_string()));

    // Determine priority
    let priority = parent_def
        .priority
        .as_ref()
        .and_then(|p| crate::tasks::templates::parse_priority(p))
        .or_else(|| {
            template
                .defaults
                .priority
                .as_ref()
                .and_then(|p| crate::tasks::templates::parse_priority(p))
        })
        .unwrap_or(TaskPriority::P2);

    // Build sources
    let mut sources = parent_def.sources.clone();
    sources.push(format!("file:{}", plan_path.display()));

    // Create parent task event
    let parent_event = TaskEvent::Created {
        task_id: parent_id.clone(),
        name: parent_def.name.clone(),
        slug: None,
        task_type,
        priority,
        assignee: assignee
            .or_else(|| template.defaults.assignee.clone())
            .or_else(|| Some("claude-code".to_string())),
        sources,
        template: Some(template.template_id()),
        working_copy: working_copy.clone(),
        instructions: Some(parent_def.instructions.clone()),
        data: crate::tasks::templates::convert_data(&parent_def.data),
        timestamp,
    };
    write_event(cwd, &parent_event)?;

    // Create subtasks
    for (i, subtask_def) in subtask_defs.iter().enumerate() {
        let subtask_id = format!("{}.{}", parent_id, i + 1);

        let subtask_priority = subtask_def
            .priority
            .as_ref()
            .and_then(|p| crate::tasks::templates::parse_priority(p))
            .unwrap_or(priority);

        let mut subtask_sources = subtask_def.sources.clone();
        if !subtask_sources.iter().any(|s| s.starts_with("task:")) {
            subtask_sources.push(format!("task:{}", parent_id));
        }

        let subtask_event = TaskEvent::Created {
            task_id: subtask_id,
            name: subtask_def.name.clone(),
            slug: None,
            task_type: Some("plan".to_string()),
            priority: subtask_priority,
            assignee: subtask_def.assignee.clone(),
            sources: subtask_sources,
            template: None,
            working_copy: working_copy.clone(),
            instructions: Some(subtask_def.instructions.clone()),
            data: crate::tasks::templates::convert_data(&subtask_def.data),
            timestamp,
        };
        write_event(cwd, &subtask_event)?;
    }

    Ok(parent_id)
}

/// Run `aiki plan fix <review-id>`: create and run a plan-fix task from review issues
fn run_fix(
    cwd: &Path,
    review_id: &str,
    template_name: Option<String>,
    agent: Option<String>,
    output_id: bool,
) -> Result<()> {
    use super::task::{create_from_template, TemplateTaskParams};

    let template = template_name.as_deref().unwrap_or("fix");

    // Build data for the template
    let mut data = HashMap::new();
    data.insert("review".to_string(), review_id.to_string());
    data.insert("target".to_string(), review_id.to_string());

    let params = TemplateTaskParams {
        template_name: template.to_string(),
        data,
        sources: vec![format!("task:{}", review_id)],
        assignee: agent,
        ..Default::default()
    };

    let task_id = create_from_template(cwd, params)?;

    // Ensure plans directory exists
    let plans_dir = PathBuf::from("/tmp/aiki/plans");
    fs::create_dir_all(&plans_dir)
        .map_err(|e| AikiError::InvalidArgument(format!("Cannot create plans directory: {}", e)))?;

    let plan_path = plans_dir.join(format!("{}.md", task_id));

    // Run the task to completion
    let options = TaskRunOptions::new();
    task_run(cwd, &task_id, options)?;

    // Output results
    if output_id {
        println!("{}", task_id);
    } else {
        output_utils::emit(|| {
            format!(
                "## Plan Fix Completed\n- **Task:** {}\n- **Review:** {}\n- **Plan:** {}\n",
                task_id,
                review_id,
                plan_path.display()
            )
        });
    }

    Ok(())
}

/// Output plan started message
fn output_plan_started(
    plan_id: &str,
    plan_path: &Path,
    is_new: bool,
    _in_progress: &[&Task],
    _ready: &[&Task],
) -> Result<()> {
    let action = if is_new { "Creating" } else { "Editing" };
    output_utils::emit(|| {
        let content = format!(
            "## Plan Started\n- **Task:** {}\n- **File:** {}\n- {} plan at {}.\n",
            plan_id,
            plan_path.display(),
            action,
            plan_path.display()
        );
        MdBuilder::new().build(&content)
    });
    Ok(())
}

/// Output plan completed message
fn output_plan_completed(plan_id: &str, plan_path: &Path) -> Result<()> {
    output_utils::emit(|| {
        let content = format!(
            "## Plan Completed\n- **Task:** {}\n- **File:** {}\n- Created: {}\n\n---\nRun `aiki build {}` to build.\n",
            plan_id,
            plan_path.display(),
            plan_path.display(),
            plan_path.display()
        );
        MdBuilder::new().build(&content)
    });
    Ok(())
}

/// Output plan error message
fn output_plan_error(plan_id: &str, error: &str) -> Result<()> {
    let content = format!("Plan task {}: {}", plan_id, error);
    let md = MdBuilder::new().build_error(&content);
    eprintln!("{}", md);
    Ok(())
}

fn resolve_plan_launch_agent(agent: Option<&str>) -> Result<(AgentType, &'static str)> {
    let agent_type = match agent {
        Some(agent_str) => AgentType::from_str(agent_str)
            .ok_or_else(|| AikiError::UnknownAgentType(agent_str.to_string()))?,
        None => AgentType::ClaudeCode,
    };

    let binary = agent_type.cli_binary().ok_or_else(|| {
        AikiError::InvalidArgument(format!(
            "Agent '{}' does not support interactive `aiki plan` sessions. {}",
            agent_type.as_str(),
            agent_type.install_hint()
        ))
    })?;

    Ok((agent_type, binary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_idea_from_filename() {
        assert_eq!(
            parse_idea_from_filename(Path::new("dark-mode.md")),
            "Dark Mode"
        );
        assert_eq!(
            parse_idea_from_filename(Path::new("user-auth-v2.md")),
            "User Auth V2"
        );
        assert_eq!(parse_idea_from_filename(Path::new("simple.md")), "Simple");
        assert_eq!(
            parse_idea_from_filename(Path::new("add-user-authentication.md")),
            "Add User Authentication"
        );
    }

    #[test]
    fn test_generate_slug() {
        assert_eq!(
            generate_slug("Add user authentication"),
            "add-user-authentication"
        );
        assert_eq!(generate_slug("Fix the login bug"), "fix-the-login-bug");
        assert_eq!(generate_slug("Add User Auth"), "add-user-auth");
        assert_eq!(generate_slug("Simple"), "simple");
        assert_eq!(generate_slug("  Multiple   Spaces  "), "multiple-spaces");
        assert_eq!(
            generate_slug("Special! @#$ Characters"),
            "special-characters"
        );
    }

    #[test]
    fn test_determine_mode_edit() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let plan_path = temp_dir.path().join("existing.md");
        fs::write(&plan_path, "# Existing Plan").unwrap();

        let mode = determine_mode(temp_dir.path(), &["existing.md".to_string()]).unwrap();

        match mode {
            PlanMode::Edit { path, text } => {
                assert_eq!(path, plan_path);
                assert_eq!(text, "");
            }
            _ => panic!("Expected Edit mode"),
        }
    }

    #[test]
    fn test_determine_mode_edit_with_text() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let plan_path = temp_dir.path().join("existing.md");
        fs::write(&plan_path, "# Existing Plan").unwrap();

        let mode = determine_mode(
            temp_dir.path(),
            &[
                "existing.md".to_string(),
                "add".to_string(),
                "rate".to_string(),
                "limiting".to_string(),
            ],
        )
        .unwrap();

        match mode {
            PlanMode::Edit { path, text } => {
                assert_eq!(path, plan_path);
                assert_eq!(text, "add rate limiting");
            }
            _ => panic!("Expected Edit mode"),
        }
    }

    #[test]
    fn test_determine_mode_create_at_path() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        let mode = determine_mode(temp_dir.path(), &["new-feature.md".to_string()]).unwrap();

        match mode {
            PlanMode::CreateAtPath {
                path,
                initial_idea,
                text,
            } => {
                assert_eq!(path, temp_dir.path().join("new-feature.md"));
                assert_eq!(initial_idea, "New Feature");
                assert_eq!(text, "");
            }
            _ => panic!("Expected CreateAtPath mode"),
        }
    }

    #[test]
    fn test_determine_mode_create_at_path_with_text() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        let mode = determine_mode(
            temp_dir.path(),
            &[
                "jwt-auth.md".to_string(),
                "JWT".to_string(),
                "auth".to_string(),
                "with".to_string(),
                "refresh".to_string(),
                "tokens".to_string(),
            ],
        )
        .unwrap();

        match mode {
            PlanMode::CreateAtPath {
                path,
                initial_idea,
                text,
            } => {
                assert_eq!(path, temp_dir.path().join("jwt-auth.md"));
                assert_eq!(initial_idea, "Jwt Auth");
                assert_eq!(text, "JWT auth with refresh tokens");
            }
            _ => panic!("Expected CreateAtPath mode"),
        }
    }

    #[test]
    fn test_determine_mode_autogen() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        // Create ops/now directory
        fs::create_dir_all(temp_dir.path().join("ops/now")).unwrap();

        let mode = determine_mode(
            temp_dir.path(),
            &[
                "Add".to_string(),
                "user".to_string(),
                "authentication".to_string(),
            ],
        )
        .unwrap();

        match mode {
            PlanMode::Autogen { description, slug } => {
                assert_eq!(description, "Add user authentication");
                assert_eq!(slug, "add-user-authentication");
            }
            _ => panic!("Expected Autogen mode"),
        }
    }

    #[test]
    fn test_determine_mode_empty_args() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        let result = determine_mode(temp_dir.path(), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_initial_idea_needs_input_autogen_no_text() {
        let mode = PlanMode::Autogen {
            description: "Add auth".to_string(),
            slug: "add-auth".to_string(),
        };
        // Autogen never prompts — the description IS the idea
        assert!(!initial_idea_needs_input(&mode, false, true));
    }

    #[test]
    fn test_initial_idea_needs_input_autogen_with_text() {
        let mode = PlanMode::Autogen {
            description: "Add auth".to_string(),
            slug: "add-auth".to_string(),
        };
        assert!(!initial_idea_needs_input(&mode, true, true));
    }

    #[test]
    fn test_initial_idea_needs_input_create_no_text() {
        let mode = PlanMode::CreateAtPath {
            path: PathBuf::from("dark-mode.md"),
            initial_idea: "Dark Mode".to_string(),
            text: String::new(),
        };
        // No CLI text — should prompt for guidance
        assert!(initial_idea_needs_input(&mode, false, true));
    }

    #[test]
    fn test_initial_idea_needs_input_create_with_text() {
        let mode = PlanMode::CreateAtPath {
            path: PathBuf::from("dark-mode.md"),
            initial_idea: "Dark Mode".to_string(),
            text: "add JWT auth".to_string(),
        };
        // CLI text provided — skip prompt
        assert!(!initial_idea_needs_input(&mode, true, true));
    }

    #[test]
    fn test_initial_idea_needs_input_edit_no_text() {
        let mode = PlanMode::Edit {
            path: PathBuf::from("existing.md"),
            text: String::new(),
        };
        // Edit mode with no CLI text — should NOT prompt (go straight to agent)
        assert!(!initial_idea_needs_input(&mode, false, false));
    }

    #[test]
    fn test_initial_idea_needs_input_edit_with_text() {
        let mode = PlanMode::Edit {
            path: PathBuf::from("existing.md"),
            text: "add rate limiting".to_string(),
        };
        // Edit mode with CLI text — skip prompt
        assert!(!initial_idea_needs_input(&mode, true, false));
    }

    #[test]
    fn test_resolve_plan_launch_agent_defaults_to_claude() {
        let (agent, binary) = resolve_plan_launch_agent(None).unwrap();
        assert_eq!(agent, AgentType::ClaudeCode);
        assert_eq!(binary, "claude");
    }

    #[test]
    fn test_resolve_plan_launch_agent_supports_codex() {
        let (agent, binary) = resolve_plan_launch_agent(Some("codex")).unwrap();
        assert_eq!(agent, AgentType::Codex);
        assert_eq!(binary, "codex");
    }

    #[test]
    fn test_resolve_plan_launch_agent_rejects_non_spawnable_agent() {
        let err = resolve_plan_launch_agent(Some("cursor")).unwrap_err();
        assert!(format!("{}", err).contains("does not support interactive `aiki plan` sessions"));
    }
}
