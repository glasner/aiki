//! Tool classification for vendor-agnostic event routing
//!
//! This module provides shared types for classifying AI agent tools
//! into categories that map to the unified event model.

use serde::{Deserialize, Serialize};

/// Tool type classification for event routing
///
/// Represents the category of tool being used, which determines
/// which event type should be emitted. This enum is shared across
/// vendors; each vendor implements its own `classify_tool()` function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    /// File tools (Read, Edit, Write, Glob, Grep, LS, NotebookEdit)
    File,
    /// Shell command execution (Bash)
    Shell,
    /// Web access tools (WebFetch, WebSearch) - Phase 3
    Web,
    /// Internal orchestration tools (Task, TodoRead) - no event needed
    Internal,
    /// MCP server tools (anything else)
    Mcp,
}

/// File operation type
///
/// Represents the type of file operation being performed.
/// Used by flows to gate operations differently (e.g., allow reads, block deletes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOperation {
    /// Read operations: Read, LS, Glob, Grep
    Read,
    /// Write operations: Edit, Write, NotebookEdit, MultiEdit
    Write,
    /// Delete operations: rm, rmdir (parsed from shell commands)
    Delete,
}

impl std::fmt::Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOperation::Read => write!(f, "read"),
            FileOperation::Write => write!(f, "write"),
            FileOperation::Delete => write!(f, "delete"),
        }
    }
}
