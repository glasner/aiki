//! Task template system for defining reusable workflows
//!
//! Templates are markdown files with YAML frontmatter that define:
//! - Task structure: parent task + subtasks
//! - Instructions: what the agent should do in each step
//! - Defaults: type, assignee, priority, custom data
//! - Variables: placeholders filled at runtime
//! - Conditionals: dynamic content based on context
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
//! # Task Name with {{variables}}
//!
//! Task instructions...
//!
//! {% if data.target_type == "file" %}
//! Review the document for completeness.
//! {% else %}
//! Review the code for bugs.
//! {% endif %}
//!
//! # Subtasks
//!
//! ## Subtask Name
//!
//! Subtask instructions...
//! ```
//!
//! # Syntax
//!
//! ## Variables (Tera-style)
//!
//! - `{{data.key}}` - Data variable substitution
//! - `{{item.field}}` - Iteration item fields
//! - `{{source.task_id}}` - Source reference fields
//!
//! ## Conditionals
//!
//! - `{% if condition %}...{% endif %}`
//! - `{% if condition %}...{% else %}...{% endif %}`
//! - `{% if condition %}...{% elif condition %}...{% else %}...{% endif %}`
//!
//! Supported conditions:
//! - Equality: `data.type == "file"`, `priority != "p0"`
//! - Numeric: `data.count > 10`, `data.score >= 80`
//! - Truthy: `data.flag` (exists and is truthy)
//! - Negation: `not data.skip`
//! - Boolean: `a and b`, `a or b`, `(a or b) and c`
//!
//! ## Loops
//!
//! - `{% for item in collection %}...{% endfor %}`
//! - `{% for item in collection %}...{% else %}...{% endfor %}` (else for empty collections)
//!
//! Loop metadata variables:
//! - `loop.index` - 1-based index (1, 2, 3, ...)
//! - `loop.index0` - 0-based index (0, 1, 2, ...)
//! - `loop.first` - true for first iteration
//! - `loop.last` - true for last iteration
//! - `loop.length` - total number of items
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

pub mod conditionals;
pub mod data_source;
pub mod parser;
pub mod resolver;
pub mod types;
pub mod variables;

pub use conditionals::{
    process_conditionals, tokenize as tokenize_conditionals, Condition, ConditionalError,
    EvalContext, LoopItem, TemplateNode, Token, Value,
};
pub use data_source::{parse_data_source, resolve_data_source, DataSource};
pub use parser::{extract_yaml_frontmatter, parse_template, FrontmatterError};
pub use resolver::{
    convert_data, create_review_task_from_template, create_subtasks_from_inline_loops,
    create_tasks_from_template, expand_loops, find_templates_dir, get_working_copy_change_id,
    has_inline_loops, list_templates, load_template, load_template_file, parse_priority,
    substitute_parent_id, TemplateInfo, PARENT_ID_PLACEHOLDER,
};
pub use types::{TaskDefaults, TaskDefinition, TaskTemplate, TemplateFrontmatter};
pub use variables::{
    coerce_to_string, coerce_value, find_variables, substitute, substitute_with_template_name,
    VariableContext,
};
