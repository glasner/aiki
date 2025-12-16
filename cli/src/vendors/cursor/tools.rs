use crate::tools::ToolType;

/// Classify a Cursor MCP tool by name into its type
///
/// Note: Cursor's tool names may differ from Claude Code's.
/// This covers known file-modifying tools and treats everything else as MCP.
///
/// Cursor only emits beforeMCPExecution/afterMCPExecution for MCP tools,
/// so we only classify between File (for file-modifying MCP tools) and Mcp.
/// Shell commands have their own beforeShellExecution/afterShellExecution hooks.
pub fn classify_mcp_tool(tool_name: &str) -> ToolType {
    match tool_name {
        // File-modifying tools (various naming conventions)
        "Edit" | "Write" | "NotebookEdit" | "edit" | "write" | "file_edit" => ToolType::File,
        // Everything else is treated as MCP tool
        _ => ToolType::Mcp,
    }
}
