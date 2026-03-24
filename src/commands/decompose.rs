//! Decompose command — break a plan into subtasks under a target task.
//!
//! Provides both a CLI entry point (`aiki decompose <plan> --target <id>`)
//! and a public `run_decompose()` function reusable from `epic.rs` and `fix.rs`.

use std::env;
use std::path::Path;

use super::task::{create_from_template, TemplateTaskParams};
use super::OutputFormat;
use crate::agents::AgentType;
use crate::error::{AikiError, Result};
use crate::tasks::runner::{handle_session_result, task_run, task_run_on_session, TaskRunOptions};
use crate::tasks::{find_task, materialize_graph, read_events, write_link_event};

/// Options for `run_decompose` that callers can customize.
pub struct DecomposeOptions {
    pub template: Option<String>,
    pub agent: Option<AgentType>,
}

/// Arguments for the `aiki decompose` CLI command.
#[derive(clap::Args)]
pub struct DecomposeArgs {
    /// Path to plan file (e.g., ops/now/my-feature.md)
    pub plan_path: String,

    /// Target task ID to decompose into (subtasks are created under this task)
    #[arg(long)]
    pub target: String,

    /// Decompose template to use (default: decompose)
    #[arg(long)]
    pub template: Option<String>,

    /// Agent for decomposition (default: claude-code)
    #[arg(long)]
    pub agent: Option<String>,

    /// Output format (e.g., `id` for bare task ID)
    #[arg(long, short = 'o', value_name = "FORMAT")]
    pub output: Option<OutputFormat>,
}

/// CLI entry point for `aiki decompose`.
pub fn run(args: DecomposeArgs) -> Result<()> {
    let cwd = env::current_dir()
        .map_err(|_| AikiError::InvalidArgument("Failed to get current directory".to_string()))?;

    let agent_type = if let Some(ref agent_str) = args.agent {
        Some(
            AgentType::from_str(agent_str)
                .ok_or_else(|| AikiError::UnknownAgentType(agent_str.clone()))?,
        )
    } else {
        None
    };

    let options = DecomposeOptions {
        template: args.template,
        agent: agent_type,
    };

    let decompose_task_id = run_decompose(&cwd, &args.plan_path, &args.target, options, false)?;

    match args.output {
        Some(OutputFormat::Id) => println!("{}", decompose_task_id),
        None => eprintln!("Decomposed: {}", decompose_task_id),
    }

    Ok(())
}

/// Decompose a plan into subtasks under `target_id`.
///
/// Steps:
/// 1. Write `implements-plan` link: target → `file:<plan_path>`
/// 2. Create decompose task from template with `data.target` and `data.plan`
/// 3. Write `decomposes-plan` link: decompose task → `file:<plan_path>`
/// 4. Write `populated-by` link: target → decompose task
/// 5. `task_run(decompose_task)` with agent options
/// 6. Return decompose task ID
pub fn run_decompose(
    cwd: &Path,
    plan_path: &str,
    target_id: &str,
    options: DecomposeOptions,
    show_tui: bool,
) -> Result<String> {
    let spec_target = make_spec_target(plan_path);

    // 0. Validate target exists before emitting any links/events
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    find_task(&graph.tasks, target_id)?;

    // 1. Write implements-plan link: target → file:<plan_path>
    write_link_event(cwd, &graph, "implements-plan", target_id, &spec_target)?;

    // 2. Create decompose task from template with data.target and data.plan
    let params = build_decompose_params(plan_path, target_id, &spec_target, &options);

    let decompose_task_id = create_from_template(cwd, params)?;

    // 3. Write decomposes-plan link: decompose task → file:<plan_path>
    let events = read_events(cwd)?;
    let graph = materialize_graph(&events);
    write_link_event(
        cwd,
        &graph,
        "decomposes-plan",
        &decompose_task_id,
        &spec_target,
    )?;

    // 4. Write populated-by link: target → decompose task
    write_link_event(cwd, &graph, "populated-by", target_id, &decompose_task_id)?;

    // 5. task_run(decompose_task) with agent options
    let run_options = if let Some(agent) = options.agent {
        TaskRunOptions::new().with_agent(agent)
    } else {
        TaskRunOptions::new()
    };
    if show_tui {
        let result = task_run_on_session(cwd, &decompose_task_id, run_options, true)?;
        handle_session_result(cwd, &decompose_task_id, result, true)?;
    } else {
        task_run(cwd, &decompose_task_id, run_options)?;
    }

    // 6. Return decompose task ID
    Ok(decompose_task_id)
}

/// Normalize a plan path into a `file:` spec target for link events.
fn make_spec_target(plan_path: &str) -> String {
    if plan_path.starts_with("file:") {
        plan_path.to_string()
    } else {
        format!("file:{}", plan_path)
    }
}

/// Build the `TemplateTaskParams` for the decompose task.
fn build_decompose_params(
    plan_path: &str,
    target_id: &str,
    spec_target: &str,
    options: &DecomposeOptions,
) -> TemplateTaskParams {
    let template = options.template.as_deref().unwrap_or("decompose");

    let assignee = options
        .agent
        .as_ref()
        .map(|a| a.as_str().to_string())
        .or_else(|| Some("claude-code".to_string()));

    let mut data = std::collections::HashMap::new();
    data.insert("plan".to_string(), plan_path.to_string());
    data.insert("target".to_string(), target_id.to_string());

    TemplateTaskParams {
        template_name: template.to_string(),
        data,
        sources: vec![spec_target.to_string()],
        assignee,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── make_spec_target ──────────────────────────────────────────

    #[test]
    fn test_make_spec_target_adds_file_prefix() {
        assert_eq!(
            make_spec_target("ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn test_make_spec_target_preserves_existing_prefix() {
        assert_eq!(
            make_spec_target("file:ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    // ── build_decompose_params ────────────────────────────────────

    #[test]
    fn test_build_decompose_params_defaults() {
        let options = DecomposeOptions {
            template: None,
            agent: None,
        };
        let params = build_decompose_params(
            "ops/now/feat.md",
            "target123",
            "file:ops/now/feat.md",
            &options,
        );

        assert_eq!(params.template_name, "decompose");
        assert_eq!(params.assignee, Some("claude-code".to_string()));
        assert_eq!(params.data.get("plan").unwrap(), "ops/now/feat.md");
        assert_eq!(params.data.get("target").unwrap(), "target123");
        assert_eq!(params.sources, vec!["file:ops/now/feat.md"]);
    }

    #[test]
    fn test_build_decompose_params_custom_template() {
        let options = DecomposeOptions {
            template: Some("my/custom-decompose".to_string()),
            agent: None,
        };
        let params = build_decompose_params("plan.md", "t1", "file:plan.md", &options);

        assert_eq!(params.template_name, "my/custom-decompose");
    }

    #[test]
    fn test_build_decompose_params_custom_agent() {
        let options = DecomposeOptions {
            template: None,
            agent: Some(AgentType::Codex),
        };
        let params = build_decompose_params("plan.md", "t1", "file:plan.md", &options);

        assert_eq!(params.assignee, Some("codex".to_string()));
    }

    #[test]
    fn test_build_decompose_params_data_uses_target_not_epic() {
        let options = DecomposeOptions {
            template: None,
            agent: None,
        };
        let params = build_decompose_params("plan.md", "target_id", "file:plan.md", &options);

        // data.target should exist (not data.epic)
        assert!(params.data.contains_key("target"));
        assert!(!params.data.contains_key("epic"));
        assert_eq!(params.data.get("target").unwrap(), "target_id");
        assert_eq!(params.data.get("plan").unwrap(), "plan.md");
    }

    // ── DecomposeArgs clap parsing ────────────────────────────────

    #[test]
    fn test_decompose_args_required_fields() {
        use clap::Parser;

        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            args: DecomposeArgs,
        }

        let cli = Cli::parse_from(["test", "ops/now/feat.md", "--target", "abc123"]);
        assert_eq!(cli.args.plan_path, "ops/now/feat.md");
        assert_eq!(cli.args.target, "abc123");
        assert!(cli.args.template.is_none());
        assert!(cli.args.agent.is_none());
        assert!(cli.args.output.is_none());
    }

    #[test]
    fn test_decompose_args_with_optional_fields() {
        use clap::Parser;

        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            args: DecomposeArgs,
        }

        let cli = Cli::parse_from([
            "test",
            "plan.md",
            "--target",
            "t1",
            "--template",
            "my/tmpl",
            "--agent",
            "codex",
            "-o",
            "id",
        ]);
        assert_eq!(cli.args.plan_path, "plan.md");
        assert_eq!(cli.args.target, "t1");
        assert_eq!(cli.args.template, Some("my/tmpl".to_string()));
        assert_eq!(cli.args.agent, Some("codex".to_string()));
        assert!(matches!(cli.args.output, Some(OutputFormat::Id)));
    }

    // ── decompose template uses data.target ───────────────────────

    #[test]
    fn test_decompose_template_uses_data_target_not_data_epic() {
        // Canonical path: BUILTIN_TEMPLATES_SOURCE (see templates/mod.rs). Macro requires literal.
        let template_content = include_str!("../tasks/templates/core/decompose.md");
        assert!(
            template_content.contains("{{data.target}}"),
            "Decompose template must use {{{{data.target}}}}"
        );
        assert!(
            !template_content.contains("{{data.epic}}"),
            "Decompose template must NOT use {{{{data.epic}}}}"
        );
    }
}
