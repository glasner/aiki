//! XML output generation for task commands
//!
//! All task commands return XML with a consistent structure including
//! a `<context>` element showing the current state.

use super::types::Task;

/// Escape special characters for XML
#[must_use]
pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// XML builder for task command responses
pub struct XmlBuilder {
    cmd: String,
    status: String,
    scopes: Vec<String>,
}

impl XmlBuilder {
    /// Create a new XML builder for a command
    #[must_use]
    pub fn new(cmd: &str) -> Self {
        Self {
            cmd: cmd.to_string(),
            status: "ok".to_string(),
            scopes: Vec::new(),
        }
    }

    /// Mark this response as an error
    #[must_use]
    pub fn error(mut self) -> Self {
        self.status = "error".to_string();
        self
    }

    /// Set a single scope (parent task ID when working within a parent's subtasks)
    #[must_use]
    #[allow(dead_code)] // Part of XmlBuilder API
    pub fn with_scope(mut self, scope: &str) -> Self {
        self.scopes = vec![scope.to_string()];
        self
    }

    /// Set multiple scopes (when working on subtasks from multiple parents)
    #[must_use]
    pub fn with_scopes(mut self, scopes: &[String]) -> Self {
        self.scopes = scopes.to_vec();
        self
    }

    /// Build the full XML response with context
    #[must_use]
    pub fn build(&self, content: &str, in_progress: &[&Task], ready_queue: &[&Task]) -> String {
        let mut xml = String::new();

        // Root element with attributes
        xml.push_str("<aiki_task");
        xml.push_str(&format!(r#" cmd="{}""#, self.cmd));
        xml.push_str(&format!(r#" status="{}""#, self.status));
        if !self.scopes.is_empty() {
            // Output scopes as comma-separated list
            xml.push_str(&format!(r#" scope="{}""#, self.scopes.join(",")));
        }
        xml.push_str(">\n");

        // Command-specific content
        if !content.is_empty() {
            xml.push_str(content);
            xml.push('\n');
        }

        // Context element (always present)
        xml.push_str(&Self::build_context(in_progress, ready_queue));
        xml.push('\n');

        xml.push_str("</aiki_task>");
        xml
    }

    /// Build an error response
    #[must_use]
    pub fn build_error(&self, message: &str) -> String {
        format!(
            r#"<aiki_task cmd="{}" status="error">
  <error>{}</error>
</aiki_task>"#,
            self.cmd,
            escape_xml(message)
        )
    }

    /// Build the context element showing current state
    fn build_context(in_progress: &[&Task], ready_queue: &[&Task]) -> String {
        let mut ctx = String::from("  <context>\n");

        // In-progress tasks
        if in_progress.is_empty() {
            ctx.push_str("    <in_progress/>\n");
        } else {
            ctx.push_str("    <in_progress>\n");
            for task in in_progress {
                ctx.push_str(&format!(
                    r#"      <task id="{}" name="{}"/>"#,
                    task.id,
                    escape_xml(&task.name)
                ));
                ctx.push('\n');
            }
            ctx.push_str("    </in_progress>\n");
        }

        // Ready queue (limited to 5 tasks shown, with total count)
        let ready_count = ready_queue.len();
        ctx.push_str(&format!(r#"    <list ready="{}">"#, ready_count));
        ctx.push('\n');

        for task in ready_queue.iter().take(5) {
            ctx.push_str(&format!(
                r#"      <task id="{}" name="{}" priority="{}"/>"#,
                task.id,
                escape_xml(&task.name),
                task.priority
            ));
            ctx.push('\n');
        }
        ctx.push_str("    </list>\n");
        ctx.push_str("  </context>");

        ctx
    }
}

/// Format a task element for output
#[must_use]
#[allow(dead_code)] // Part of XML formatting API
pub fn format_task(task: &Task, include_body: bool) -> String {
    let mut xml = format!(
        r#"<task id="{}" name="{}" priority="{}""#,
        task.id,
        escape_xml(&task.name),
        task.priority
    );

    if let Some(ref assignee) = task.assignee {
        xml.push_str(&format!(r#" assignee="{}""#, escape_xml(assignee)));
    }

    if include_body {
        // For now, tasks don't have a body in Phase 1
        xml.push_str("/>");
    } else {
        xml.push_str("/>");
    }

    xml
}

/// Format a task element with body content
#[must_use]
pub fn format_task_with_body(task: &Task, body: Option<&str>) -> String {
    let mut xml = format!(
        r#"    <task id="{}" priority="{}" name="{}""#,
        task.id,
        task.priority,
        escape_xml(&task.name),
    );

    if let Some(body) = body {
        xml.push_str(">\n");
        // Body is embedded as text content, indented
        for line in body.lines() {
            xml.push_str("  ");
            xml.push_str(line);
            xml.push('\n');
        }
        xml.push_str("</task>");
    } else {
        xml.push_str("/>");
    }

    xml
}

/// Format a list of tasks for the main list output
#[must_use]
pub fn format_task_list(tasks: &[&Task]) -> String {
    let mut xml = format!("  <list total=\"{}\">\n", tasks.len());

    for task in tasks {
        let mut task_xml = format!(
            r#"    <task id="{}" name="{}" priority="{}""#,
            task.id,
            escape_xml(&task.name),
            task.priority
        );
        if let Some(ref assignee) = task.assignee {
            task_xml.push_str(&format!(r#" assignee="{}""#, escape_xml(assignee)));
        }
        task_xml.push_str("/>");
        xml.push_str(&task_xml);
        xml.push('\n');
    }

    xml.push_str("  </list>");
    xml
}

/// Format added tasks output
#[must_use]
pub fn format_added(tasks: &[&Task]) -> String {
    let mut xml = String::from("  <added>\n");

    for task in tasks {
        let mut task_xml = format!(
            r#"    <task id="{}" name="{}" priority="{}""#,
            task.id,
            escape_xml(&task.name),
            task.priority
        );
        if let Some(ref assignee) = task.assignee {
            task_xml.push_str(&format!(r#" assignee="{}""#, escape_xml(assignee)));
        }
        task_xml.push_str("/>");
        xml.push_str(&task_xml);
        xml.push('\n');
    }

    xml.push_str("  </added>");
    xml
}

/// Format started tasks output
#[must_use]
pub fn format_started(tasks: &[&Task]) -> String {
    let mut xml = String::from("  <started>\n");

    for task in tasks {
        xml.push_str(&format_task_with_body(task, None));
        xml.push('\n');
    }

    xml.push_str("  </started>");
    xml
}

/// Format stopped tasks output
#[must_use]
pub fn format_stopped(tasks: &[&Task], reason: Option<&str>) -> String {
    let mut xml = String::from("  <stopped");
    if let Some(reason) = reason {
        xml.push_str(&format!(r#" reason="{}""#, escape_xml(reason)));
    }
    xml.push_str(">\n");

    for task in tasks {
        xml.push_str(&format!(
            r#"    <task id="{}" name="{}"/>"#,
            task.id,
            escape_xml(&task.name)
        ));
        xml.push('\n');
    }

    xml.push_str("  </stopped>");
    xml
}

/// Format closed tasks output
#[must_use]
pub fn format_closed(tasks: &[&Task], outcome: &str) -> String {
    let mut xml = format!(r#"  <closed outcome="{}">"#, outcome);
    xml.push('\n');

    for task in tasks {
        xml.push_str(&format!(
            r#"    <task id="{}" name="{}"/>"#,
            task.id,
            escape_xml(&task.name)
        ));
        xml.push('\n');
    }

    xml.push_str("  </closed>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::types::{TaskPriority, TaskStatus};
    use chrono::Utc;

    fn make_task(id: &str, name: &str, priority: TaskPriority, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            name: name.to_string(),
            status,
            priority,
            assignee: None,
            sources: Vec::new(),
            created_at: Utc::now(),
            started_at: None,
            claimed_by_session: None,
            stopped_reason: None,
            closed_outcome: None,
            comments: Vec::new(),
        }
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("hello"), "hello");
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
        assert_eq!(escape_xml("a & b"), "a &amp; b");
        assert_eq!(escape_xml(r#""quoted""#), "&quot;quoted&quot;");
    }

    #[test]
    fn test_xml_builder_basic() {
        let task = make_task("a1b2", "Test task", TaskPriority::P2, TaskStatus::Open);
        let xml = XmlBuilder::new("list").build("", &[], &[&task]);

        assert!(xml.contains(r#"cmd="list""#));
        assert!(xml.contains(r#"status="ok""#));
        assert!(xml.contains("<context>"));
        assert!(xml.contains("<in_progress/>"));
        assert!(xml.contains(r#"<list ready="1">"#));
    }

    #[test]
    fn test_xml_builder_error() {
        let xml = XmlBuilder::new("start")
            .error()
            .build_error("Task not found");

        assert!(xml.contains(r#"status="error""#));
        assert!(xml.contains("<error>Task not found</error>"));
    }

    #[test]
    fn test_xml_builder_with_scope() {
        let xml = XmlBuilder::new("list")
            .with_scope("a1b2")
            .build("", &[], &[]);

        assert!(xml.contains(r#"scope="a1b2""#));
    }

    #[test]
    fn test_format_task_list() {
        let task1 = make_task("a1b2", "Task 1", TaskPriority::P0, TaskStatus::Open);
        let task2 = make_task("c3d4", "Task 2", TaskPriority::P2, TaskStatus::Open);

        let xml = format_task_list(&[&task1, &task2]);

        assert!(xml.contains(r#"<list total="2">"#));
        assert!(xml.contains(r#"id="a1b2""#));
        assert!(xml.contains(r#"id="c3d4""#));
        assert!(xml.contains(r#"priority="p0""#));
        assert!(xml.contains(r#"priority="p2""#));
    }

    #[test]
    fn test_format_added() {
        let task = make_task("a1b2", "New task", TaskPriority::P2, TaskStatus::Open);
        let xml = format_added(&[&task]);

        assert!(xml.contains("<added>"));
        assert!(xml.contains(r#"id="a1b2""#));
        assert!(xml.contains("</added>"));
    }

    #[test]
    fn test_format_stopped_with_reason() {
        let task = make_task(
            "a1b2",
            "Stopped task",
            TaskPriority::P2,
            TaskStatus::Stopped,
        );
        let xml = format_stopped(&[&task], Some("Need info"));

        assert!(xml.contains(r#"<stopped reason="Need info">"#));
        assert!(xml.contains(r#"id="a1b2""#));
    }

    #[test]
    fn test_format_closed() {
        let task = make_task("a1b2", "Done task", TaskPriority::P2, TaskStatus::Closed);
        let xml = format_closed(&[&task], "done");

        assert!(xml.contains(r#"<closed outcome="done">"#));
        assert!(xml.contains(r#"id="a1b2""#));
    }

    #[test]
    fn test_context_with_in_progress() {
        let in_progress = make_task("a1b2", "Working", TaskPriority::P2, TaskStatus::InProgress);
        let ready = make_task("c3d4", "Ready", TaskPriority::P2, TaskStatus::Open);

        let xml = XmlBuilder::new("list").build("", &[&in_progress], &[&ready]);

        assert!(xml.contains("<in_progress>"));
        assert!(xml.contains(r#"<task id="a1b2" name="Working"/>"#));
        assert!(xml.contains("</in_progress>"));
    }
}
