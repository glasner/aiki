//! Task template system for defining reusable workflows
//!
//! Templates are markdown files with YAML frontmatter that define:
//! - Task structure: parent task + subtasks
//! - Instructions: what the agent should do in each step
//! - Defaults: type, assignee, priority, custom data
//! - Variables: placeholders filled at runtime
//!
//! # Template Format
//!
//! ```markdown
//! ---
//! version: "1.0.0"
//! description: Template description
//! type: review
//! assignee: claude-code
//! priority: p2
//! data:
//!   custom_key: value
//! ---
//!
//! # Task Name with {variables}
//!
//! Task instructions...
//!
//! # Subtasks
//!
//! ## Subtask Name
//!
//! Subtask instructions...
//! ```
//!
//! # Usage
//!
//! ```bash
//! # Create task from template
//! aiki task create --template aiki/review --data scope="@"
//!
//! # Create and start in one command
//! aiki task start --template myorg/refactor --data scope="src/auth.rs"
//!
//! # List available templates
//! aiki task template list
//! ```

pub mod data_source;
pub mod parser;
pub mod resolver;
pub mod types;
pub mod variables;

pub use data_source::{parse_data_source, resolve_data_source, DataSource};
pub use parser::{extract_yaml_frontmatter, parse_template, FrontmatterError};
pub use resolver::{
    convert_data, create_review_task_from_template, create_tasks_from_template,
    find_templates_dir, get_working_copy_change_id, list_templates, load_template,
    load_template_file, parse_priority, TemplateInfo,
};
pub use types::{TaskDefaults, TaskDefinition, TaskTemplate, TemplateFrontmatter};
pub use variables::{coerce_to_string, coerce_value, find_variables, substitute, substitute_with_template_name, VariableContext};
