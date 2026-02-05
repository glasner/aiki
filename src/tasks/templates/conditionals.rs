//! Conditional logic for task templates
//!
//! Provides Tera-style conditional syntax for templates:
//! - `{% if condition %}...{% endif %}`
//! - `{% if condition %}...{% else %}...{% endif %}`
//! - `{% if condition %}...{% elif condition %}...{% else %}...{% endif %}`
//!
//! Supports:
//! - Comparison operators: `==`, `!=`, `>`, `<`, `>=`, `<=`
//! - Boolean operators: `and`, `or`, `not`
//! - Parentheses for grouping
//! - Truthy checks (variable existence)

use std::collections::HashMap;
use std::iter::Peekable;

/// Errors that can occur during conditional parsing or evaluation
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionalError {
    /// Unclosed conditional block
    UnclosedBlock { line: usize },
    /// Unclosed loop block
    UnclosedLoop { line: usize },
    /// Unexpected token (e.g., {% else %} without {% if %})
    UnexpectedToken { token: String, line: usize },
    /// Invalid condition syntax
    InvalidCondition { condition: String, line: usize },
    /// Mismatched delimiters
    MismatchedDelimiters { expected: String, found: String, line: usize },
    /// Single-brace syntax detected (migration needed)
    SingleBraceSyntax { variable: String, line: usize },
    /// Invalid loop syntax
    InvalidLoopSyntax { details: String, line: usize },
    /// Invalid loop variable name
    InvalidLoopVariable { name: String, line: usize },
    /// Unknown collection in loop
    UnknownCollection { name: String, line: usize },
}

impl std::fmt::Display for ConditionalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConditionalError::UnclosedBlock { line } => {
                write!(f, "Unclosed conditional block starting at line {}", line)
            }
            ConditionalError::UnclosedLoop { line } => {
                write!(
                    f,
                    "Unclosed loop starting at line {}. Expected {{% endfor %}}",
                    line
                )
            }
            ConditionalError::UnexpectedToken { token, line } => {
                write!(
                    f,
                    "Unexpected {{%% {} %}} at line {} without matching {{%% if %}}",
                    token, line
                )
            }
            ConditionalError::InvalidCondition { condition, line } => {
                write!(
                    f,
                    "Invalid condition at line {}: '{}'. Expected: 'var == \"value\"', 'var', or 'not var'",
                    line, condition
                )
            }
            ConditionalError::MismatchedDelimiters { expected, found, line } => {
                write!(
                    f,
                    "Mismatched delimiters at line {}: expected '{}', found '{}'",
                    line, expected, found
                )
            }
            ConditionalError::SingleBraceSyntax { variable, line } => {
                write!(
                    f,
                    "Invalid template syntax at line {}: '{{{{{}}}}}'\n\n  Single-brace variable syntax is no longer supported.\n  Please update to double-brace syntax: '{{{{{{{}}}}}}}'",
                    line, variable, variable
                )
            }
            ConditionalError::InvalidLoopSyntax { details, line } => {
                write!(
                    f,
                    "Invalid loop syntax at line {}: {}. Expected: {{% for var in collection %}}",
                    line, details
                )
            }
            ConditionalError::InvalidLoopVariable { name, line } => {
                write!(
                    f,
                    "Invalid loop variable '{}' at line {}. Must match [a-z_][a-z0-9_]*",
                    name, line
                )
            }
            ConditionalError::UnknownCollection { name, line } => {
                write!(
                    f,
                    "Unknown collection '{}' at line {}. Available: source.comments",
                    name, line
                )
            }
        }
    }
}

impl std::error::Error for ConditionalError {}

/// Token types from the tokenizer
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Plain text content
    Text(String),
    /// Variable reference: {{ var }}
    Variable(String),
    /// Control block: {% if/elif/else/endif %}
    ControlBlock(String),
}

/// Parsed template AST node
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateNode {
    /// Plain text (to be output as-is)
    Text(String),
    /// Variable reference (to be substituted)
    Variable(String),
    /// Conditional block with branches
    Conditional {
        /// The if condition and its content
        if_branch: (Condition, Vec<TemplateNode>),
        /// Optional elif branches
        elif_branches: Vec<(Condition, Vec<TemplateNode>)>,
        /// Optional else branch
        else_branch: Option<Vec<TemplateNode>>,
    },
    /// Loop block for iteration
    Loop {
        /// Loop variable name (e.g., "item")
        variable: String,
        /// Collection to iterate over (e.g., "source.comments")
        collection: String,
        /// Body to repeat for each item
        body: Vec<TemplateNode>,
        /// Optional else body for empty collections
        else_body: Option<Vec<TemplateNode>>,
    },
}

/// Serialize template nodes back to template syntax
///
/// This preserves conditionals as `{% if %}` blocks instead of evaluating them,
/// allowing deferred evaluation (e.g., inside loop bodies that need iteration context).
pub fn nodes_to_template(nodes: &[TemplateNode]) -> String {
    let mut result = String::new();
    for node in nodes {
        result.push_str(&node_to_template(node));
    }
    result
}

/// Serialize a single template node to template syntax
fn node_to_template(node: &TemplateNode) -> String {
    match node {
        TemplateNode::Text(t) => t.clone(),
        TemplateNode::Variable(v) => format!("{{{{{}}}}}", v),
        TemplateNode::Conditional {
            if_branch,
            elif_branches,
            else_branch,
        } => {
            let mut result = String::new();
            result.push_str(&format!("{{% if {} %}}", condition_to_string(&if_branch.0)));
            result.push_str(&nodes_to_template(&if_branch.1));
            for (cond, content) in elif_branches {
                result.push_str(&format!("{{% elif {} %}}", condition_to_string(cond)));
                result.push_str(&nodes_to_template(content));
            }
            if let Some(else_content) = else_branch {
                result.push_str("{% else %}");
                result.push_str(&nodes_to_template(else_content));
            }
            result.push_str("{% endif %}");
            result
        }
        TemplateNode::Loop {
            variable,
            collection,
            body,
            else_body,
        } => {
            let mut result = String::new();
            result.push_str(&format!("{{% for {} in {} %}}", variable, collection));
            result.push_str(&nodes_to_template(body));
            if let Some(else_content) = else_body {
                result.push_str("{% else %}");
                result.push_str(&nodes_to_template(else_content));
            }
            result.push_str("{% endfor %}");
            result
        }
    }
}

/// Convert a Condition back to its string representation
fn condition_to_string(cond: &Condition) -> String {
    match cond {
        Condition::Exists(var) => var.clone(),
        Condition::Not(inner) => format!("not {}", condition_to_string(inner)),
        Condition::And(left, right) => {
            format!("{} and {}", condition_to_string(left), condition_to_string(right))
        }
        Condition::Or(left, right) => {
            format!("{} or {}", condition_to_string(left), condition_to_string(right))
        }
        Condition::Equals { left, right } => {
            format!("{} == {}", left, value_to_string(right))
        }
        Condition::NotEquals { left, right } => {
            format!("{} != {}", left, value_to_string(right))
        }
        Condition::GreaterThan { left, right } => {
            format!("{} > {}", left, value_to_string(right))
        }
        Condition::LessThan { left, right } => {
            format!("{} < {}", left, value_to_string(right))
        }
        Condition::GreaterOrEqual { left, right } => {
            format!("{} >= {}", left, value_to_string(right))
        }
        Condition::LessOrEqual { left, right } => {
            format!("{} <= {}", left, value_to_string(right))
        }
    }
}

/// Convert a Value back to its string representation
fn value_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => format!("\"{}\"", s),
        Value::Number(n) => n.to_string(),
        Value::Variable(v) => v.clone(),
    }
}

/// Condition expression
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// Equality comparison: var == "value"
    Equals { left: String, right: Value },
    /// Inequality: var != "value"
    NotEquals { left: String, right: Value },
    /// Greater than: var > value
    GreaterThan { left: String, right: Value },
    /// Less than: var < value
    LessThan { left: String, right: Value },
    /// Greater or equal: var >= value
    GreaterOrEqual { left: String, right: Value },
    /// Less or equal: var <= value
    LessOrEqual { left: String, right: Value },
    /// Truthy check: var (exists and is truthy)
    Exists(String),
    /// Negation: not condition
    Not(Box<Condition>),
    /// Logical AND: cond1 and cond2
    And(Box<Condition>, Box<Condition>),
    /// Logical OR: cond1 or cond2
    Or(Box<Condition>, Box<Condition>),
}

/// Value in a condition (right-hand side)
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// String literal: "value"
    String(String),
    /// Numeric literal: 42, 3.14
    Number(f64),
    /// Variable reference: data.other
    Variable(String),
}

/// Context for evaluating conditions
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    /// All variables available for evaluation
    pub variables: HashMap<String, String>,
    /// Strictness level for undefined variables
    pub strict: bool,
    /// Warnings collected during evaluation
    pub warnings: Vec<String>,
}

impl EvalContext {
    /// Create a new evaluation context
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable value
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(key.into(), value.into());
    }

    /// Get a variable value
    pub fn get(&self, key: &str) -> Option<&String> {
        self.variables.get(key)
    }

    /// Check if a variable is truthy
    ///
    /// A value is truthy if it exists and is not:
    /// - empty string
    /// - "false" (case-insensitive)
    /// - "0"
    /// - "null"
    pub fn is_truthy(&self, key: &str) -> bool {
        match self.variables.get(key) {
            None => false,
            Some(v) => {
                let v_lower = v.to_lowercase();
                !v.is_empty() && v_lower != "false" && v != "0" && v_lower != "null"
            }
        }
    }

    /// Resolve a value for comparison
    fn resolve_value(&self, value: &Value) -> Option<String> {
        match value {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Variable(var) => self.variables.get(var).cloned(),
        }
    }

    /// Compare two values numerically if possible
    fn compare_numeric(&self, left: &str, right: &str) -> Option<std::cmp::Ordering> {
        let left_num = left.parse::<f64>().ok()?;
        let right_num = right.parse::<f64>().ok()?;
        left_num.partial_cmp(&right_num)
    }
}

/// Tokenize a template string into tokens
///
/// Recognizes:
/// - `{{` ... `}}` as Variable tokens
/// - `{%` ... `%}` as ControlBlock tokens
/// - Everything else as Text tokens
pub fn tokenize(input: &str) -> Result<Vec<Token>, ConditionalError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current_text = String::new();
    let mut line = 1;

    while let Some(c) = chars.next() {
        if c == '\n' {
            current_text.push(c);
            line += 1;
            continue;
        }

        if c == '{' {
            match chars.peek() {
                Some(&'{') => {
                    // Start of variable: {{
                    chars.next();

                    // Flush any accumulated text
                    if !current_text.is_empty() {
                        tokens.push(Token::Text(std::mem::take(&mut current_text)));
                    }

                    // Read until }}
                    let mut var_content = String::new();
                    let start_line = line;
                    let mut found_close = false;

                    while let Some(c2) = chars.next() {
                        if c2 == '\n' {
                            line += 1;
                        }
                        if c2 == '}' {
                            if chars.peek() == Some(&'}') {
                                chars.next();
                                found_close = true;
                                break;
                            }
                        }
                        var_content.push(c2);
                    }

                    if !found_close {
                        return Err(ConditionalError::MismatchedDelimiters {
                            expected: "}}".to_string(),
                            found: "end of input".to_string(),
                            line: start_line,
                        });
                    }

                    tokens.push(Token::Variable(var_content.trim().to_string()));
                }
                Some(&'%') => {
                    // Start of control block: {%
                    chars.next();

                    // Flush any accumulated text
                    if !current_text.is_empty() {
                        tokens.push(Token::Text(std::mem::take(&mut current_text)));
                    }

                    // Read until %}
                    let mut block_content = String::new();
                    let start_line = line;
                    let mut found_close = false;

                    while let Some(c2) = chars.next() {
                        if c2 == '\n' {
                            line += 1;
                        }
                        if c2 == '%' {
                            if chars.peek() == Some(&'}') {
                                chars.next();
                                found_close = true;
                                break;
                            }
                        }
                        block_content.push(c2);
                    }

                    if !found_close {
                        return Err(ConditionalError::MismatchedDelimiters {
                            expected: "%}".to_string(),
                            found: "end of input".to_string(),
                            line: start_line,
                        });
                    }

                    tokens.push(Token::ControlBlock(block_content.trim().to_string()));
                }
                _ => {
                    // Check for single-brace variable syntax (deprecated)
                    // Look ahead to see if this might be {var}
                    let mut lookahead = String::new();
                    let mut temp_chars: Vec<char> = vec![c];
                    let mut found_single_close = false;

                    // Save current position by collecting chars until we see } or {{
                    for c2 in chars.by_ref() {
                        temp_chars.push(c2);
                        if c2 == '}' {
                            found_single_close = true;
                            break;
                        }
                        if c2 == '{' {
                            // Nested brace, not single-brace syntax
                            break;
                        }
                        if c2 == '\n' || c2 == ' ' || c2 == '\t' {
                            // Whitespace/newline means not a variable
                            break;
                        }
                        lookahead.push(c2);
                    }

                    // Check if this looks like a single-brace variable (e.g., {data.foo})
                    if found_single_close && !lookahead.is_empty() && lookahead.chars().all(|ch| ch.is_alphanumeric() || ch == '.' || ch == '_') {
                        return Err(ConditionalError::SingleBraceSyntax {
                            variable: lookahead,
                            line,
                        });
                    }

                    // Not a single-brace variable, add all chars to text
                    for tc in temp_chars {
                        if tc == '\n' {
                            line += 1;
                        }
                        current_text.push(tc);
                    }
                }
            }
        } else {
            current_text.push(c);
        }
    }

    // Flush remaining text
    if !current_text.is_empty() {
        tokens.push(Token::Text(current_text));
    }

    Ok(tokens)
}

/// Parse tokens into a template AST
pub fn parse(tokens: &[Token]) -> Result<Vec<TemplateNode>, ConditionalError> {
    let mut result = Vec::new();
    let mut iter = tokens.iter().peekable();
    let mut line = 1;

    while let Some(token) = iter.next() {
        // Track line numbers in text
        if let Token::Text(t) = token {
            line += t.chars().filter(|&c| c == '\n').count();
        }

        match token {
            Token::Text(t) => {
                result.push(TemplateNode::Text(t.clone()));
            }
            Token::Variable(v) => {
                result.push(TemplateNode::Variable(v.clone()));
            }
            Token::ControlBlock(block) => {
                let block_lower = block.to_lowercase();

                if block_lower.starts_with("if ") || block_lower == "if" {
                    // Parse if block
                    let condition_str = if block_lower == "if" {
                        ""
                    } else {
                        block[3..].trim()
                    };

                    let condition = parse_condition(condition_str, line)?;
                    let (if_content, elif_branches, else_branch) =
                        parse_if_body(&mut iter, &mut line)?;

                    result.push(TemplateNode::Conditional {
                        if_branch: (condition, if_content),
                        elif_branches,
                        else_branch,
                    });
                } else if block_lower.starts_with("for ") {
                    // Parse for loop: {% for var in collection %}
                    let (variable, collection) = parse_for_header(&block[4..], line)?;
                    let (body, else_body) = parse_for_body(&mut iter, &mut line)?;

                    result.push(TemplateNode::Loop {
                        variable,
                        collection,
                        body,
                        else_body,
                    });
                } else if block_lower.starts_with("elif ") || block_lower == "elif" {
                    return Err(ConditionalError::UnexpectedToken {
                        token: "elif".to_string(),
                        line,
                    });
                } else if block_lower == "else" {
                    return Err(ConditionalError::UnexpectedToken {
                        token: "else".to_string(),
                        line,
                    });
                } else if block_lower == "endif" {
                    return Err(ConditionalError::UnexpectedToken {
                        token: "endif".to_string(),
                        line,
                    });
                } else if block_lower == "endfor" {
                    return Err(ConditionalError::UnexpectedToken {
                        token: "endfor".to_string(),
                        line,
                    });
                }
                // Unknown control blocks are ignored (for future extensibility)
            }
        }
    }

    Ok(result)
}

/// Parse the body of an if block, collecting elif and else branches
fn parse_if_body<'a, I>(
    iter: &mut Peekable<I>,
    line: &mut usize,
) -> Result<(Vec<TemplateNode>, Vec<(Condition, Vec<TemplateNode>)>, Option<Vec<TemplateNode>>), ConditionalError>
where
    I: Iterator<Item = &'a Token>,
{
    let mut if_content: Vec<TemplateNode> = Vec::new();
    let mut elif_branches: Vec<(Condition, Vec<TemplateNode>)> = Vec::new();
    let mut else_branch: Option<Vec<TemplateNode>> = None;
    let start_line = *line;

    loop {
        let token = iter.next().ok_or(ConditionalError::UnclosedBlock { line: start_line })?;

        // Track line numbers
        if let Token::Text(t) = token {
            *line += t.chars().filter(|&c| c == '\n').count();
        }

        match token {
            Token::Text(t) => {
                if else_branch.is_some() {
                    else_branch.as_mut().unwrap().push(TemplateNode::Text(t.clone()));
                } else if !elif_branches.is_empty() {
                    elif_branches.last_mut().unwrap().1.push(TemplateNode::Text(t.clone()));
                } else {
                    if_content.push(TemplateNode::Text(t.clone()));
                }
            }
            Token::Variable(v) => {
                let node = TemplateNode::Variable(v.clone());
                if else_branch.is_some() {
                    else_branch.as_mut().unwrap().push(node);
                } else if !elif_branches.is_empty() {
                    elif_branches.last_mut().unwrap().1.push(node);
                } else {
                    if_content.push(node);
                }
            }
            Token::ControlBlock(block) => {
                let block_lower = block.to_lowercase();

                if block_lower == "endif" {
                    return Ok((if_content, elif_branches, else_branch));
                } else if block_lower.starts_with("elif ") || block_lower == "elif" {
                    if else_branch.is_some() {
                        return Err(ConditionalError::UnexpectedToken {
                            token: "elif after else".to_string(),
                            line: *line,
                        });
                    }

                    let condition_str = if block_lower == "elif" {
                        ""
                    } else {
                        block[5..].trim()
                    };
                    let condition = parse_condition(condition_str, *line)?;
                    elif_branches.push((condition, Vec::new()));
                } else if block_lower == "else" {
                    if else_branch.is_some() {
                        return Err(ConditionalError::UnexpectedToken {
                            token: "multiple else".to_string(),
                            line: *line,
                        });
                    }
                    else_branch = Some(Vec::new());
                } else if block_lower.starts_with("if ") || block_lower == "if" {
                    // Nested if block
                    let condition_str = if block_lower == "if" {
                        ""
                    } else {
                        block[3..].trim()
                    };
                    let condition = parse_condition(condition_str, *line)?;
                    let (nested_if, nested_elif, nested_else) = parse_if_body(iter, line)?;

                    let nested_node = TemplateNode::Conditional {
                        if_branch: (condition, nested_if),
                        elif_branches: nested_elif,
                        else_branch: nested_else,
                    };

                    if else_branch.is_some() {
                        else_branch.as_mut().unwrap().push(nested_node);
                    } else if !elif_branches.is_empty() {
                        elif_branches.last_mut().unwrap().1.push(nested_node);
                    } else {
                        if_content.push(nested_node);
                    }
                }
            }
        }
    }
}

/// Parse the header of a for loop: "var in collection"
///
/// Returns (variable_name, collection_path)
fn parse_for_header(header: &str, line: usize) -> Result<(String, String), ConditionalError> {
    let header = header.trim();

    // Find " in " to split variable and collection
    let in_pos = header.find(" in ");
    let (variable, collection) = match in_pos {
        Some(pos) => {
            let var = header[..pos].trim();
            let coll = header[pos + 4..].trim();
            (var, coll)
        }
        None => {
            return Err(ConditionalError::InvalidLoopSyntax {
                details: format!("missing 'in' keyword in '{}'", header),
                line,
            });
        }
    };

    // Validate variable name using centralized validation
    if !crate::validation::is_valid_template_identifier(variable) {
        return Err(ConditionalError::InvalidLoopVariable {
            name: variable.to_string(),
            line,
        });
    }

    // Validate collection is not empty
    if collection.is_empty() {
        return Err(ConditionalError::InvalidLoopSyntax {
            details: "collection cannot be empty".to_string(),
            line,
        });
    }

    Ok((variable.to_string(), collection.to_string()))
}

/// Parse the body of a for loop, collecting nodes until {% endfor %}
///
/// Also handles the optional {% else %} block for empty collections.
fn parse_for_body<'a, I>(
    iter: &mut Peekable<I>,
    line: &mut usize,
) -> Result<(Vec<TemplateNode>, Option<Vec<TemplateNode>>), ConditionalError>
where
    I: Iterator<Item = &'a Token>,
{
    let mut body: Vec<TemplateNode> = Vec::new();
    let mut else_body: Option<Vec<TemplateNode>> = None;
    let start_line = *line;
    let mut in_else = false;

    loop {
        let token = iter
            .next()
            .ok_or(ConditionalError::UnclosedLoop { line: start_line })?;

        // Track line numbers
        if let Token::Text(t) = token {
            *line += t.chars().filter(|&c| c == '\n').count();
        }

        match token {
            Token::Text(t) => {
                let node = TemplateNode::Text(t.clone());
                if in_else {
                    else_body.as_mut().unwrap().push(node);
                } else {
                    body.push(node);
                }
            }
            Token::Variable(v) => {
                let node = TemplateNode::Variable(v.clone());
                if in_else {
                    else_body.as_mut().unwrap().push(node);
                } else {
                    body.push(node);
                }
            }
            Token::ControlBlock(block) => {
                let block_lower = block.to_lowercase();

                if block_lower == "endfor" {
                    return Ok((body, else_body));
                } else if block_lower == "else" {
                    if in_else {
                        return Err(ConditionalError::UnexpectedToken {
                            token: "multiple else in for loop".to_string(),
                            line: *line,
                        });
                    }
                    in_else = true;
                    else_body = Some(Vec::new());
                } else if block_lower.starts_with("if ") || block_lower == "if" {
                    // Nested if block
                    let condition_str = if block_lower == "if" {
                        ""
                    } else {
                        block[3..].trim()
                    };
                    let condition = parse_condition(condition_str, *line)?;
                    let (if_content, elif_branches, else_branch) = parse_if_body(iter, line)?;

                    let nested_node = TemplateNode::Conditional {
                        if_branch: (condition, if_content),
                        elif_branches,
                        else_branch,
                    };

                    if in_else {
                        else_body.as_mut().unwrap().push(nested_node);
                    } else {
                        body.push(nested_node);
                    }
                } else if block_lower.starts_with("for ") {
                    // Nested for loop
                    let (variable, collection) = parse_for_header(&block[4..], *line)?;
                    let (nested_body, nested_else) = parse_for_body(iter, line)?;

                    let nested_node = TemplateNode::Loop {
                        variable,
                        collection,
                        body: nested_body,
                        else_body: nested_else,
                    };

                    if in_else {
                        else_body.as_mut().unwrap().push(nested_node);
                    } else {
                        body.push(nested_node);
                    }
                } else if block_lower == "endif" {
                    return Err(ConditionalError::UnexpectedToken {
                        token: "endif".to_string(),
                        line: *line,
                    });
                } else if block_lower.starts_with("elif ") || block_lower == "elif" {
                    return Err(ConditionalError::UnexpectedToken {
                        token: "elif".to_string(),
                        line: *line,
                    });
                }
                // Unknown control blocks are passed through
            }
        }
    }
}

/// Parse a condition string into a Condition AST
fn parse_condition(condition_str: &str, line: usize) -> Result<Condition, ConditionalError> {
    let condition_str = condition_str.trim();

    if condition_str.is_empty() {
        return Err(ConditionalError::InvalidCondition {
            condition: "(empty)".to_string(),
            line,
        });
    }

    // Parse with operator precedence: or < and < not < comparisons
    parse_or_expr(condition_str, line)
}

/// Parse OR expressions (lowest precedence)
fn parse_or_expr(s: &str, line: usize) -> Result<Condition, ConditionalError> {
    // Find " or " that's not inside parentheses
    if let Some(idx) = find_operator_outside_parens(s, " or ") {
        let left = parse_or_expr(&s[..idx], line)?;
        let right = parse_and_expr(&s[idx + 4..], line)?;
        return Ok(Condition::Or(Box::new(left), Box::new(right)));
    }
    parse_and_expr(s, line)
}

/// Parse AND expressions
fn parse_and_expr(s: &str, line: usize) -> Result<Condition, ConditionalError> {
    if let Some(idx) = find_operator_outside_parens(s, " and ") {
        let left = parse_and_expr(&s[..idx], line)?;
        let right = parse_not_expr(&s[idx + 5..], line)?;
        return Ok(Condition::And(Box::new(left), Box::new(right)));
    }
    parse_not_expr(s, line)
}

/// Parse NOT expressions
fn parse_not_expr(s: &str, line: usize) -> Result<Condition, ConditionalError> {
    let s = s.trim();
    if s.starts_with("not ") {
        let inner = parse_not_expr(&s[4..], line)?;
        return Ok(Condition::Not(Box::new(inner)));
    }
    parse_primary_expr(s, line)
}

/// Parse primary expressions (comparisons, parenthesized expressions, truthy checks)
fn parse_primary_expr(s: &str, line: usize) -> Result<Condition, ConditionalError> {
    let s = s.trim();

    // Handle parenthesized expressions
    if s.starts_with('(') && s.ends_with(')') {
        // Find matching closing paren
        let inner = &s[1..s.len()-1];
        // Verify it's actually matched (handle nested parens)
        let mut depth = 0;
        for (i, c) in s.chars().enumerate() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && i < s.len() - 1 {
                        // This closing paren doesn't match the opening one
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth == 0 {
            return parse_or_expr(inner, line);
        }
    }

    // Try comparison operators (order matters: >= before >, etc.)
    // Check for >= and <= before > and <
    if let Some(idx) = s.find(">=") {
        let left = s[..idx].trim().to_string();
        let right_str = s[idx + 2..].trim();
        let right = parse_value(right_str, line)?;
        return Ok(Condition::GreaterOrEqual { left, right });
    }
    if let Some(idx) = s.find("<=") {
        let left = s[..idx].trim().to_string();
        let right_str = s[idx + 2..].trim();
        let right = parse_value(right_str, line)?;
        return Ok(Condition::LessOrEqual { left, right });
    }
    if let Some(idx) = s.find("==") {
        let left = s[..idx].trim().to_string();
        let right_str = s[idx + 2..].trim();
        let right = parse_value(right_str, line)?;
        return Ok(Condition::Equals { left, right });
    }
    if let Some(idx) = s.find("!=") {
        let left = s[..idx].trim().to_string();
        let right_str = s[idx + 2..].trim();
        let right = parse_value(right_str, line)?;
        return Ok(Condition::NotEquals { left, right });
    }
    if let Some(idx) = s.find('>') {
        let left = s[..idx].trim().to_string();
        let right_str = s[idx + 1..].trim();
        let right = parse_value(right_str, line)?;
        return Ok(Condition::GreaterThan { left, right });
    }
    if let Some(idx) = s.find('<') {
        let left = s[..idx].trim().to_string();
        let right_str = s[idx + 1..].trim();
        let right = parse_value(right_str, line)?;
        return Ok(Condition::LessThan { left, right });
    }

    // Must be a truthy check (variable exists and is truthy)
    if s.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_') {
        return Ok(Condition::Exists(s.to_string()));
    }

    Err(ConditionalError::InvalidCondition {
        condition: s.to_string(),
        line,
    })
}

/// Parse a value (string literal, number, or variable reference)
fn parse_value(s: &str, line: usize) -> Result<Value, ConditionalError> {
    let s = s.trim();

    // String literal: "value" or 'value'
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return Ok(Value::String(s[1..s.len()-1].to_string()));
    }

    // Number literal
    if let Ok(n) = s.parse::<f64>() {
        return Ok(Value::Number(n));
    }

    // Variable reference
    if s.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_') {
        return Ok(Value::Variable(s.to_string()));
    }

    Err(ConditionalError::InvalidCondition {
        condition: format!("invalid value: {}", s),
        line,
    })
}

/// Find an operator that's not inside parentheses
fn find_operator_outside_parens(s: &str, op: &str) -> Option<usize> {
    let mut depth = 0;
    let op_len = op.len();
    let s_bytes = s.as_bytes();
    let op_bytes = op.as_bytes();

    for i in 0..s.len() {
        match s_bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {
                if depth == 0 && i + op_len <= s.len() && &s_bytes[i..i + op_len] == op_bytes {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Evaluate a condition against a context
pub fn evaluate_condition(condition: &Condition, ctx: &EvalContext) -> bool {
    match condition {
        Condition::Exists(var) => ctx.is_truthy(var),

        Condition::Not(inner) => !evaluate_condition(inner, ctx),

        Condition::And(left, right) => {
            evaluate_condition(left, ctx) && evaluate_condition(right, ctx)
        }

        Condition::Or(left, right) => {
            evaluate_condition(left, ctx) || evaluate_condition(right, ctx)
        }

        Condition::Equals { left, right } => {
            let left_val = ctx.variables.get(left).map(String::as_str).unwrap_or("");
            let right_val = ctx.resolve_value(right).unwrap_or_default();
            left_val == right_val
        }

        Condition::NotEquals { left, right } => {
            let left_val = ctx.variables.get(left).map(String::as_str).unwrap_or("");
            let right_val = ctx.resolve_value(right).unwrap_or_default();
            left_val != right_val
        }

        Condition::GreaterThan { left, right } => {
            let left_val = ctx.variables.get(left).map(String::as_str).unwrap_or("0");
            let right_val = ctx.resolve_value(right).unwrap_or_default();
            ctx.compare_numeric(left_val, &right_val)
                .map(|o| o == std::cmp::Ordering::Greater)
                .unwrap_or(false)
        }

        Condition::LessThan { left, right } => {
            let left_val = ctx.variables.get(left).map(String::as_str).unwrap_or("0");
            let right_val = ctx.resolve_value(right).unwrap_or_default();
            ctx.compare_numeric(left_val, &right_val)
                .map(|o| o == std::cmp::Ordering::Less)
                .unwrap_or(false)
        }

        Condition::GreaterOrEqual { left, right } => {
            let left_val = ctx.variables.get(left).map(String::as_str).unwrap_or("0");
            let right_val = ctx.resolve_value(right).unwrap_or_default();
            ctx.compare_numeric(left_val, &right_val)
                .map(|o| o != std::cmp::Ordering::Less)
                .unwrap_or(false)
        }

        Condition::LessOrEqual { left, right } => {
            let left_val = ctx.variables.get(left).map(String::as_str).unwrap_or("0");
            let right_val = ctx.resolve_value(right).unwrap_or_default();
            ctx.compare_numeric(left_val, &right_val)
                .map(|o| o != std::cmp::Ordering::Greater)
                .unwrap_or(false)
        }
    }
}

/// Render a parsed template with the given context
///
/// Returns the rendered string with conditionals evaluated and variables left as-is
/// (for later substitution by the variable system)
pub fn render(nodes: &[TemplateNode], ctx: &EvalContext) -> String {
    render_with_loops(nodes, ctx, &HashMap::new())
}

/// Internal render function that supports loop iteration context
fn render_with_loops(
    nodes: &[TemplateNode],
    ctx: &EvalContext,
    loop_vars: &HashMap<String, LoopItem>,
) -> String {
    let mut result = String::new();

    for node in nodes {
        match node {
            TemplateNode::Text(t) => {
                result.push_str(t);
            }
            TemplateNode::Variable(v) => {
                // Check if variable is a loop variable or loop metadata
                if let Some(value) = resolve_loop_variable(v, loop_vars) {
                    result.push_str(&value);
                } else {
                    // Leave variables as {{ var }} for later substitution
                    result.push_str("{{");
                    result.push_str(v);
                    result.push_str("}}");
                }
            }
            TemplateNode::Conditional {
                if_branch,
                elif_branches,
                else_branch,
            } => {
                // Evaluate conditions in order, considering loop variables
                let eval_ctx = make_loop_aware_context(ctx, loop_vars);
                if evaluate_condition(&if_branch.0, &eval_ctx) {
                    result.push_str(&render_with_loops(&if_branch.1, ctx, loop_vars));
                } else {
                    let mut matched = false;
                    for (cond, content) in elif_branches {
                        if evaluate_condition(cond, &eval_ctx) {
                            result.push_str(&render_with_loops(content, ctx, loop_vars));
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        if let Some(else_content) = else_branch {
                            result.push_str(&render_with_loops(else_content, ctx, loop_vars));
                        }
                    }
                }
            }
            TemplateNode::Loop {
                variable,
                collection,
                body,
                else_body,
            } => {
                // Get the collection items - for now we just pass through
                // The actual iteration is handled by the template resolver
                // Here we just emit the loop as markers for later processing
                result.push_str(&render_loop(variable, collection, body, else_body, ctx, loop_vars));
            }
        }
    }

    // Apply whitespace cleanup rules
    cleanup_whitespace(&result)
}

/// Loop item containing the item data and loop metadata
#[derive(Debug, Clone)]
pub struct LoopItem {
    /// The current item's data (field -> value)
    pub data: HashMap<String, String>,
    /// Loop metadata
    pub index: usize,      // 1-based index
    pub index0: usize,     // 0-based index
    pub first: bool,       // Is this the first item?
    pub last: bool,        // Is this the last item?
    pub length: usize,     // Total number of items
}

/// Resolve a variable that might be a loop variable or loop metadata
fn resolve_loop_variable(var: &str, loop_vars: &HashMap<String, LoopItem>) -> Option<String> {
    // Check for loop metadata: loop.index, loop.first, etc.
    if let Some(meta_key) = var.strip_prefix("loop.") {
        // Find the innermost loop (any key works, there should only be one active "loop")
        // Actually, loop.* always refers to the innermost loop
        if let Some((_, item)) = loop_vars.iter().next() {
            return match meta_key {
                "index" => Some(item.index.to_string()),
                "index0" => Some(item.index0.to_string()),
                "first" => Some(item.first.to_string()),
                "last" => Some(item.last.to_string()),
                "length" => Some(item.length.to_string()),
                _ => None,
            };
        }
        return None;
    }

    // Check for loop variable fields: item.field, comment.text, etc.
    let parts: Vec<&str> = var.splitn(2, '.').collect();
    if parts.len() == 2 {
        let var_name = parts[0];
        let field_name = parts[1];

        if let Some(item) = loop_vars.get(var_name) {
            return item.data.get(field_name).cloned();
        }
    }

    // Check if it's just the loop variable name (shouldn't happen in normal use)
    if loop_vars.contains_key(var) {
        // Return some representation - this case is unusual
        return None;
    }

    None
}

/// Create an EvalContext that includes loop variable values for condition evaluation
fn make_loop_aware_context(ctx: &EvalContext, loop_vars: &HashMap<String, LoopItem>) -> EvalContext {
    let mut new_ctx = ctx.clone();

    // Add loop metadata
    if let Some((_, item)) = loop_vars.iter().next() {
        new_ctx.set("loop.index", item.index.to_string());
        new_ctx.set("loop.index0", item.index0.to_string());
        new_ctx.set("loop.first", item.first.to_string());
        new_ctx.set("loop.last", item.last.to_string());
        new_ctx.set("loop.length", item.length.to_string());
    }

    // Add loop variable fields
    for (var_name, item) in loop_vars {
        for (field, value) in &item.data {
            new_ctx.set(format!("{}.{}", var_name, field), value.clone());
        }
    }

    new_ctx
}

/// Render a loop - outputs markers that can be processed by the template resolver
///
/// For Phase 1, we emit the loop as structured markers. The template resolver
/// will handle actual iteration when processing templates.
///
/// IMPORTANT: We use `nodes_to_template` instead of `render_with_loops` for the body
/// to preserve conditionals as `{% if %}` syntax rather than evaluating them. This
/// allows conditionals inside loop bodies to be evaluated per-iteration in the resolver
/// when loop variables are available.
fn render_loop(
    variable: &str,
    collection: &str,
    body: &[TemplateNode],
    else_body: &Option<Vec<TemplateNode>>,
    _ctx: &EvalContext,
    _loop_vars: &HashMap<String, LoopItem>,
) -> String {
    // Emit a special marker that the resolver can process
    // Format: <!-- AIKI_LOOP:var:collection --> body <!-- AIKI_ENDLOOP --> [<!-- AIKI_LOOPELSE --> else <!-- AIKI_ENDLOOPELSE -->]
    let mut result = String::new();

    result.push_str(&format!(
        "<!-- AIKI_LOOP:{}:{} -->\n",
        variable, collection
    ));

    // Serialize the body back to template syntax, preserving conditionals
    // for deferred evaluation during loop expansion
    result.push_str(&nodes_to_template(body));

    result.push_str("<!-- AIKI_ENDLOOP -->");

    // Include else body if present
    if let Some(else_nodes) = else_body {
        result.push_str("\n<!-- AIKI_LOOPELSE -->\n");
        result.push_str(&nodes_to_template(else_nodes));
        result.push_str("<!-- AIKI_ENDLOOPELSE -->");
    }

    result
}

/// Clean up whitespace introduced by conditional blocks
///
/// Rules (v1 - simple automatic strategy):
/// 1. Remove lines that contain only whitespace after conditional evaluation
/// 2. Collapse multiple consecutive blank lines to one
fn cleanup_whitespace(s: &str) -> String {
    let mut result = String::new();
    let mut prev_was_blank = false;

    for line in s.lines() {
        let is_blank = line.trim().is_empty();

        if is_blank {
            if !prev_was_blank {
                result.push('\n');
            }
            prev_was_blank = true;
        } else {
            if !result.is_empty() && !prev_was_blank {
                result.push('\n');
            }
            result.push_str(line);
            prev_was_blank = false;
        }
    }

    // Preserve trailing newline if original had one
    if s.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Process a template string: tokenize, parse, and render
///
/// This is the main entry point for conditional processing.
/// Variables are left as `{{var}}` for later substitution.
pub fn process_conditionals(
    template: &str,
    ctx: &EvalContext,
) -> Result<String, ConditionalError> {
    let tokens = tokenize(template)?;
    let ast = parse(&tokens)?;
    Ok(render(&ast, ctx))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Tokenizer Tests =====

    #[test]
    fn test_tokenize_plain_text() {
        let tokens = tokenize("Hello world").unwrap();
        assert_eq!(tokens, vec![Token::Text("Hello world".to_string())]);
    }

    #[test]
    fn test_tokenize_variable() {
        let tokens = tokenize("Hello {{name}}!").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Text("Hello ".to_string()),
                Token::Variable("name".to_string()),
                Token::Text("!".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_control_block() {
        let tokens = tokenize("{% if x %}yes{% endif %}").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::ControlBlock("if x".to_string()),
                Token::Text("yes".to_string()),
                Token::ControlBlock("endif".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_mixed() {
        let tokens = tokenize("Hello {% if show %}{{name}}{% endif %}!").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Text("Hello ".to_string()),
                Token::ControlBlock("if show".to_string()),
                Token::Variable("name".to_string()),
                Token::ControlBlock("endif".to_string()),
                Token::Text("!".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_single_brace_error() {
        let result = tokenize("Hello {name}!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConditionalError::SingleBraceSyntax { .. }));
    }

    #[test]
    fn test_tokenize_unclosed_variable() {
        let result = tokenize("Hello {{name");
        assert!(result.is_err());
    }

    // ===== Parser Tests =====

    #[test]
    fn test_parse_simple_if() {
        let tokens = tokenize("{% if x %}yes{% endif %}").unwrap();
        let ast = parse(&tokens).unwrap();
        assert_eq!(ast.len(), 1);
        match &ast[0] {
            TemplateNode::Conditional { if_branch, elif_branches, else_branch } => {
                assert!(matches!(&if_branch.0, Condition::Exists(v) if v == "x"));
                assert_eq!(if_branch.1.len(), 1);
                assert!(elif_branches.is_empty());
                assert!(else_branch.is_none());
            }
            _ => panic!("Expected conditional"),
        }
    }

    #[test]
    fn test_parse_if_else() {
        let tokens = tokenize("{% if x %}yes{% else %}no{% endif %}").unwrap();
        let ast = parse(&tokens).unwrap();
        match &ast[0] {
            TemplateNode::Conditional { else_branch, .. } => {
                assert!(else_branch.is_some());
            }
            _ => panic!("Expected conditional"),
        }
    }

    #[test]
    fn test_parse_if_elif_else() {
        let tokens = tokenize("{% if a %}A{% elif b %}B{% else %}C{% endif %}").unwrap();
        let ast = parse(&tokens).unwrap();
        match &ast[0] {
            TemplateNode::Conditional { elif_branches, else_branch, .. } => {
                assert_eq!(elif_branches.len(), 1);
                assert!(else_branch.is_some());
            }
            _ => panic!("Expected conditional"),
        }
    }

    #[test]
    fn test_parse_equality() {
        let cond = parse_condition("data.type == \"file\"", 1).unwrap();
        match cond {
            Condition::Equals { left, right } => {
                assert_eq!(left, "data.type");
                assert_eq!(right, Value::String("file".to_string()));
            }
            _ => panic!("Expected Equals"),
        }
    }

    #[test]
    fn test_parse_numeric_comparison() {
        let cond = parse_condition("data.count > 10", 1).unwrap();
        match cond {
            Condition::GreaterThan { left, right } => {
                assert_eq!(left, "data.count");
                assert_eq!(right, Value::Number(10.0));
            }
            _ => panic!("Expected GreaterThan"),
        }
    }

    #[test]
    fn test_parse_and_or() {
        let cond = parse_condition("a and b or c", 1).unwrap();
        // Should be: (a and b) or c  due to precedence
        match cond {
            Condition::Or(left, _) => {
                assert!(matches!(*left, Condition::And(_, _)));
            }
            _ => panic!("Expected Or"),
        }
    }

    #[test]
    fn test_parse_parentheses() {
        let cond = parse_condition("a and (b or c)", 1).unwrap();
        match cond {
            Condition::And(_, right) => {
                assert!(matches!(*right, Condition::Or(_, _)));
            }
            _ => panic!("Expected And with nested Or"),
        }
    }

    #[test]
    fn test_parse_not() {
        let cond = parse_condition("not data.skip", 1).unwrap();
        match cond {
            Condition::Not(inner) => {
                assert!(matches!(*inner, Condition::Exists(v) if v == "data.skip"));
            }
            _ => panic!("Expected Not"),
        }
    }

    // ===== Evaluator Tests =====

    #[test]
    fn test_eval_exists_true() {
        let mut ctx = EvalContext::new();
        ctx.set("data.show", "true");

        let cond = Condition::Exists("data.show".to_string());
        assert!(evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_eval_exists_false() {
        let ctx = EvalContext::new();
        let cond = Condition::Exists("data.missing".to_string());
        assert!(!evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_eval_equals() {
        let mut ctx = EvalContext::new();
        ctx.set("data.type", "file");

        let cond = Condition::Equals {
            left: "data.type".to_string(),
            right: Value::String("file".to_string()),
        };
        assert!(evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_eval_numeric_gt() {
        let mut ctx = EvalContext::new();
        ctx.set("data.count", "15");

        let cond = Condition::GreaterThan {
            left: "data.count".to_string(),
            right: Value::Number(10.0),
        };
        assert!(evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_eval_and() {
        let mut ctx = EvalContext::new();
        ctx.set("a", "true");
        ctx.set("b", "true");

        let cond = Condition::And(
            Box::new(Condition::Exists("a".to_string())),
            Box::new(Condition::Exists("b".to_string())),
        );
        assert!(evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_eval_or() {
        let mut ctx = EvalContext::new();
        ctx.set("a", "true");

        let cond = Condition::Or(
            Box::new(Condition::Exists("a".to_string())),
            Box::new(Condition::Exists("b".to_string())),
        );
        assert!(evaluate_condition(&cond, &ctx));
    }

    // ===== Integration Tests =====

    #[test]
    fn test_process_simple_if_true() {
        let mut ctx = EvalContext::new();
        ctx.set("show", "true");

        let result = process_conditionals("{% if show %}Hello{% endif %}", &ctx).unwrap();
        assert_eq!(result.trim(), "Hello");
    }

    #[test]
    fn test_process_simple_if_false() {
        let ctx = EvalContext::new();

        let result = process_conditionals("{% if show %}Hello{% endif %}", &ctx).unwrap();
        assert_eq!(result.trim(), "");
    }

    #[test]
    fn test_process_if_else() {
        let ctx = EvalContext::new();

        let result = process_conditionals(
            "{% if show %}Yes{% else %}No{% endif %}",
            &ctx,
        ).unwrap();
        assert_eq!(result.trim(), "No");
    }

    #[test]
    fn test_process_preserves_variables() {
        let mut ctx = EvalContext::new();
        ctx.set("show", "true");

        let result = process_conditionals(
            "{% if show %}Hello {{name}}{% endif %}",
            &ctx,
        ).unwrap();
        assert!(result.contains("{{name}}"));
    }

    #[test]
    fn test_process_equality_comparison() {
        let mut ctx = EvalContext::new();
        ctx.set("data.type", "file");

        let result = process_conditionals(
            "{% if data.type == \"file\" %}File review{% else %}Code review{% endif %}",
            &ctx,
        ).unwrap();
        assert_eq!(result.trim(), "File review");
    }

    #[test]
    fn test_process_nested_if() {
        let mut ctx = EvalContext::new();
        ctx.set("a", "true");
        ctx.set("b", "true");

        let template = r#"{% if a %}
outer
{% if b %}
inner
{% endif %}
{% endif %}"#;

        let result = process_conditionals(template, &ctx).unwrap();
        assert!(result.contains("outer"));
        assert!(result.contains("inner"));
    }

    #[test]
    fn test_truthy_values() {
        let mut ctx = EvalContext::new();

        // Empty string is falsy
        ctx.set("empty", "");
        assert!(!ctx.is_truthy("empty"));

        // "false" is falsy
        ctx.set("false_str", "false");
        assert!(!ctx.is_truthy("false_str"));

        // "0" is falsy
        ctx.set("zero", "0");
        assert!(!ctx.is_truthy("zero"));

        // "null" is falsy
        ctx.set("null_str", "null");
        assert!(!ctx.is_truthy("null_str"));

        // Other values are truthy
        ctx.set("truthy", "yes");
        assert!(ctx.is_truthy("truthy"));
    }

    #[test]
    fn test_multiline_template() {
        let mut ctx = EvalContext::new();
        ctx.set("data.target_type", "file");

        let template = r#"# Review: {{data.target_name}}

{% if data.target_type == "file" %}
Read the file at `{{data.path}}` to understand:
- What is this document about?
{% else %}
Examine the code changes:
1. View the diff
{% endif %}

## Quality check"#;

        let result = process_conditionals(template, &ctx).unwrap();
        assert!(result.contains("Read the file"));
        assert!(!result.contains("Examine the code"));
        assert!(result.contains("Quality check"));
        assert!(result.contains("{{data.target_name}}")); // Variables preserved
        assert!(result.contains("{{data.path}}")); // Variables preserved
    }

    // ===== ## Heading Preservation Tests =====
    // These tests verify that ## headings in various contexts are preserved as text

    #[test]
    fn test_hash_headings_in_block_quotes_preserved() {
        let ctx = EvalContext::new();

        let template = r#"> This is a block quote
> ## This is a heading inside the quote
> More quoted text"#;

        let result = process_conditionals(template, &ctx).unwrap();
        // ## inside block quote should be preserved as-is
        assert!(result.contains("## This is a heading inside the quote"));
    }

    #[test]
    fn test_hash_headings_in_indented_lists_preserved() {
        let ctx = EvalContext::new();

        let template = r#"- List item 1
  ## Indented heading (not a real heading)
- List item 2"#;

        let result = process_conditionals(template, &ctx).unwrap();
        // ## in indented context should be preserved
        assert!(result.contains("## Indented heading"));
    }

    #[test]
    fn test_hash_headings_in_fenced_code_preserved() {
        let ctx = EvalContext::new();

        let template = r#"```markdown
## This is inside a code block
Some code here
```"#;

        let result = process_conditionals(template, &ctx).unwrap();
        // ## inside fenced code block should be preserved
        assert!(result.contains("## This is inside a code block"));
    }

    #[test]
    fn test_hash_headings_with_conditionals() {
        let mut ctx = EvalContext::new();
        ctx.set("show", "true");

        let template = r#"{% if show %}
## Conditional heading

Content under heading
{% endif %}"#;

        let result = process_conditionals(template, &ctx).unwrap();
        assert!(result.contains("## Conditional heading"));
        assert!(result.contains("Content under heading"));
    }

    // ===== Error Line Number Tests =====

    #[test]
    fn test_error_line_number_unclosed_variable() {
        let template = "Line 1\nLine 2\n{{unclosed";
        let result = tokenize(template);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConditionalError::MismatchedDelimiters { line, .. } => {
                assert_eq!(line, 3, "Error should report line 3");
            }
            other => panic!("Expected MismatchedDelimiters, got {:?}", other),
        }
    }

    #[test]
    fn test_error_line_number_unclosed_control_block() {
        let template = "Line 1\nLine 2\nLine 3\n{% if x";
        let result = tokenize(template);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConditionalError::MismatchedDelimiters { line, .. } => {
                assert_eq!(line, 4, "Error should report line 4");
            }
            other => panic!("Expected MismatchedDelimiters, got {:?}", other),
        }
    }

    #[test]
    fn test_error_line_number_single_brace_syntax() {
        let template = "Line 1\n{old_syntax}\nLine 3";
        let result = tokenize(template);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConditionalError::SingleBraceSyntax { line, variable } => {
                assert_eq!(line, 2, "Error should report line 2");
                assert_eq!(variable, "old_syntax");
            }
            other => panic!("Expected SingleBraceSyntax, got {:?}", other),
        }
    }

    #[test]
    fn test_error_line_number_unclosed_if() {
        let tokens = tokenize("Line 1\n{% if x %}\nContent\n").unwrap();
        let result = parse(&tokens);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConditionalError::UnclosedBlock { line } => {
                // The error reports where the unclosed block started
                assert_eq!(line, 2, "Error should report line 2 where if block started");
            }
            other => panic!("Expected UnclosedBlock, got {:?}", other),
        }
    }

    #[test]
    fn test_error_line_number_unexpected_else() {
        let tokens = tokenize("{% else %}content{% endif %}").unwrap();
        let result = parse(&tokens);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConditionalError::UnexpectedToken { token, .. } => {
                assert_eq!(token, "else");
            }
            other => panic!("Expected UnexpectedToken, got {:?}", other),
        }
    }

    #[test]
    fn test_complex_boolean_expression_line_tracking() {
        // For complex expressions like "{% if a and b or c %}", warnings about
        // undefined variables should reference the control block start line
        let template = "Line 1\nLine 2\n{% if undefined_a and undefined_b %}\nContent\n{% endif %}";

        // This should parse and render successfully (undefined vars are falsy, not errors)
        let ctx = EvalContext::new();
        let result = process_conditionals(template, &ctx);
        assert!(result.is_ok());
        // The conditional should evaluate to false (undefined vars are falsy)
        assert!(!result.unwrap().contains("Content"));
    }

    // ===== Loop Parser Tests =====

    #[test]
    fn test_tokenize_for_loop() {
        let tokens = tokenize("{% for item in source.comments %}{{item.text}}{% endfor %}").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::ControlBlock("for item in source.comments".to_string()),
                Token::Variable("item.text".to_string()),
                Token::ControlBlock("endfor".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_simple_for_loop() {
        let tokens = tokenize("{% for item in source.comments %}text{% endfor %}").unwrap();
        let ast = parse(&tokens).unwrap();
        assert_eq!(ast.len(), 1);
        match &ast[0] {
            TemplateNode::Loop { variable, collection, body, else_body } => {
                assert_eq!(variable, "item");
                assert_eq!(collection, "source.comments");
                assert_eq!(body.len(), 1);
                assert!(else_body.is_none());
            }
            _ => panic!("Expected Loop"),
        }
    }

    #[test]
    fn test_parse_for_loop_with_else() {
        let tokens = tokenize("{% for item in source.comments %}content{% else %}empty{% endfor %}").unwrap();
        let ast = parse(&tokens).unwrap();
        match &ast[0] {
            TemplateNode::Loop { variable, collection, body, else_body } => {
                assert_eq!(variable, "item");
                assert_eq!(collection, "source.comments");
                assert!(!body.is_empty());
                assert!(else_body.is_some());
                let else_nodes = else_body.as_ref().unwrap();
                assert!(matches!(&else_nodes[0], TemplateNode::Text(t) if t == "empty"));
            }
            _ => panic!("Expected Loop"),
        }
    }

    #[test]
    fn test_parse_nested_for_loops() {
        let tokens = tokenize("{% for a in x %}{% for b in y %}inner{% endfor %}{% endfor %}").unwrap();
        let ast = parse(&tokens).unwrap();
        assert_eq!(ast.len(), 1);
        match &ast[0] {
            TemplateNode::Loop { variable, body, .. } => {
                assert_eq!(variable, "a");
                // Inner loop should be in body
                assert!(body.iter().any(|n| matches!(n, TemplateNode::Loop { variable, .. } if variable == "b")));
            }
            _ => panic!("Expected Loop"),
        }
    }

    #[test]
    fn test_parse_for_with_if_inside() {
        let tokens = tokenize("{% for item in list %}{% if item.show %}yes{% endif %}{% endfor %}").unwrap();
        let ast = parse(&tokens).unwrap();
        match &ast[0] {
            TemplateNode::Loop { body, .. } => {
                assert!(body.iter().any(|n| matches!(n, TemplateNode::Conditional { .. })));
            }
            _ => panic!("Expected Loop"),
        }
    }

    #[test]
    fn test_parse_for_header_valid() {
        let (var, coll) = parse_for_header("item in source.comments", 1).unwrap();
        assert_eq!(var, "item");
        assert_eq!(coll, "source.comments");

        let (var, coll) = parse_for_header("c in data.items", 1).unwrap();
        assert_eq!(var, "c");
        assert_eq!(coll, "data.items");

        let (var, coll) = parse_for_header("my_var in things", 1).unwrap();
        assert_eq!(var, "my_var");
        assert_eq!(coll, "things");
    }

    #[test]
    fn test_parse_for_header_invalid_no_in() {
        let result = parse_for_header("item source.comments", 1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConditionalError::InvalidLoopSyntax { .. }));
    }

    #[test]
    fn test_parse_for_header_invalid_variable() {
        // Uppercase not allowed
        let result = parse_for_header("Item in source.comments", 1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConditionalError::InvalidLoopVariable { .. }));

        // Starts with digit
        let result = parse_for_header("1item in source.comments", 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_for_header_empty_collection() {
        let result = parse_for_header("item in ", 1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConditionalError::InvalidLoopSyntax { .. }));
    }

    #[test]
    fn test_unclosed_for_loop() {
        let tokens = tokenize("{% for item in list %}content").unwrap();
        let result = parse(&tokens);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConditionalError::UnclosedLoop { .. }));
    }

    #[test]
    fn test_unexpected_endfor() {
        let tokens = tokenize("{% endfor %}").unwrap();
        let result = parse(&tokens);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConditionalError::UnexpectedToken { token, .. } if token == "endfor"));
    }

    #[test]
    fn test_process_for_loop_output() {
        let template = "{% for item in source.comments %}## {{item.text}}{% endfor %}";
        let ctx = EvalContext::new();
        let result = process_conditionals(template, &ctx).unwrap();

        // Should contain loop markers for resolver processing
        assert!(result.contains("AIKI_LOOP:item:source.comments"));
        assert!(result.contains("AIKI_ENDLOOP"));
        assert!(result.contains("{{item.text}}")); // Variable preserved
    }

    #[test]
    fn test_process_for_loop_with_else_output() {
        let template = "{% for item in list %}content{% else %}no items{% endfor %}";
        let ctx = EvalContext::new();
        let result = process_conditionals(template, &ctx).unwrap();

        assert!(result.contains("AIKI_LOOP"));
        assert!(result.contains("AIKI_LOOPELSE"));
        assert!(result.contains("no items"));
        assert!(result.contains("AIKI_ENDLOOPELSE"));
    }

    #[test]
    fn test_multiline_for_loop() {
        let template = r#"# Subtasks

{% for item in source.comments %}
## Fix: {{item.file}}

{{item.text}}
{% endfor %}"#;

        let ctx = EvalContext::new();
        let result = process_conditionals(template, &ctx).unwrap();

        assert!(result.contains("# Subtasks"));
        assert!(result.contains("AIKI_LOOP:item:source.comments"));
        assert!(result.contains("## Fix: {{item.file}}"));
        assert!(result.contains("{{item.text}}"));
        assert!(result.contains("AIKI_ENDLOOP"));
    }
}
