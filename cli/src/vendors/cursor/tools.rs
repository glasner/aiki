/// Tool type classification for event routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    /// File-modifying tools
    FileChange,
    /// MCP tools (non-file)
    Mcp,
}

/// Classify a Cursor MCP tool by name into its type
///
/// Note: Cursor's tool names may differ from Claude Code's.
/// This covers known file-modifying tools and treats everything else as MCP.
pub fn classify_mcp_tool(tool_name: &str) -> ToolType {
    match tool_name {
        // File-modifying tools (various naming conventions)
        "Edit" | "Write" | "NotebookEdit" | "edit" | "write" | "file_edit" => ToolType::FileChange,
        // Everything else is treated as MCP tool
        _ => ToolType::Mcp,
    }
}
