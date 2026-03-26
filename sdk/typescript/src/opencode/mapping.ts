import type {
  AikiEventName,
  ChangeOperation,
  EditDetail,
  WebOperation,
} from "../types.js";

// ============================================================================
// OpenCode Tool → Aiki Event Mapping
// ============================================================================

/**
 * OpenCode tool classification for aiki event routing.
 *
 * Maps OpenCode tool names to the aiki event domain (change, read, shell, web, mcp).
 * Internal tools (todo, question, skill, task) are not mapped — they don't produce
 * aiki events.
 */
export type AikiEventDomain = "change" | "read" | "shell" | "web" | "mcp" | null;

/** OpenCode tool name → aiki event domain */
const TOOL_DOMAIN_MAP: Record<string, AikiEventDomain> = {
  // File mutation tools → change events
  edit: "change",
  edit_file: "change",
  write: "change",
  write_file: "change",
  patch: "change",

  // Read tools → read events
  read: "read",
  read_file: "read",
  glob: "read",
  grep: "read",
  list: "read",
  list_files: "read",

  // Shell → shell events
  bash: "shell",

  // Web → web events
  webSearch: "web",
  web_search: "web",

  // LSP operations are read-only
  lsp: "read",
};

/**
 * Classify an OpenCode tool name into an aiki event domain.
 *
 * Returns null for internal tools (todo, question, skill, task) or unknown tools.
 * MCP tools are detected by the `mcp__` prefix convention.
 */
export function classifyTool(toolName: string): AikiEventDomain {
  // MCP tools follow format: mcp__<server>__<tool>
  if (toolName.startsWith("mcp__")) {
    return "mcp";
  }

  return TOOL_DOMAIN_MAP[toolName] ?? null;
}

/**
 * Get the "before" event name (permission_asked) for a tool.
 * Returns null if the tool doesn't map to an aiki event.
 */
export function getBeforeEvent(toolName: string): AikiEventName | null {
  const domain = classifyTool(toolName);
  if (!domain) return null;
  return `${domain}.permission_asked` as AikiEventName;
}

/**
 * Get the "after" event name (completed) for a tool.
 * Returns null if the tool doesn't map to an aiki event.
 */
export function getAfterEvent(toolName: string): AikiEventName | null {
  const domain = classifyTool(toolName);
  if (!domain) return null;
  return `${domain}.completed` as AikiEventName;
}

// ============================================================================
// OpenCode Tool Input → Aiki Payload Builders
// ============================================================================

/**
 * Normalize an OpenCode tool name to the aiki tool name convention.
 *
 * OpenCode uses snake_case (edit_file, write_file, read_file, etc.),
 * but aiki expects PascalCase (Edit, Write, Read, etc.) to match Claude Code's
 * naming convention.
 */
export function normalizeToolName(opencodeTool: string): string {
  const map: Record<string, string> = {
    edit: "Edit",
    edit_file: "Edit",
    write: "Write",
    write_file: "Write",
    patch: "Patch",
    read: "Read",
    read_file: "Read",
    glob: "Glob",
    grep: "Grep",
    list: "LS",
    list_files: "LS",
    bash: "Bash",
    webSearch: "WebSearch",
    web_search: "WebSearch",
    lsp: "LSP",
  };

  // MCP tools keep their original name
  if (opencodeTool.startsWith("mcp__")) {
    return opencodeTool;
  }

  return map[opencodeTool] ?? opencodeTool;
}

/**
 * Build a change operation payload from OpenCode tool input.
 *
 * Extracts file_paths, edit_details, etc. from the OpenCode tool input
 * and formats them as an aiki ChangeOperation.
 */
export function buildChangeOperation(
  toolName: string,
  input: Record<string, unknown>,
): { operation: ChangeOperation; tool_name: string } {
  const normalized = normalizeToolName(toolName);

  // Edit tool: exact string replacement
  if (toolName === "edit" || toolName === "edit_file") {
    const filePath = String(input.file_path ?? input.path ?? "");
    const editDetails: EditDetail[] = [];

    if (input.old_string !== undefined && input.new_string !== undefined) {
      editDetails.push({
        file_path: filePath,
        old_string: String(input.old_string),
        new_string: String(input.new_string),
      });
    }

    return {
      tool_name: normalized,
      operation: {
        operation: "write",
        file_paths: [filePath],
        edit_details: editDetails,
      },
    };
  }

  // Write tool: create or overwrite file
  if (toolName === "write" || toolName === "write_file") {
    const filePath = String(input.file_path ?? input.path ?? "");
    return {
      tool_name: normalized,
      operation: {
        operation: "write",
        file_paths: [filePath],
      },
    };
  }

  // Patch tool: unified diff application
  if (toolName === "patch") {
    // Patch may affect multiple files; parse from diff if available
    const filePaths: string[] = [];
    if (input.file_path) filePaths.push(String(input.file_path));
    if (input.path) filePaths.push(String(input.path));

    return {
      tool_name: normalized,
      operation: {
        operation: "write",
        file_paths: filePaths.length > 0 ? filePaths : ["<patch>"],
      },
    };
  }

  // Fallback: generic write
  const filePath = String(input.file_path ?? input.path ?? "");
  return {
    tool_name: normalized,
    operation: {
      operation: "write",
      file_paths: filePath ? [filePath] : [],
    },
  };
}

/**
 * Build a read event payload from OpenCode tool input.
 *
 * Returns file_paths and optional pattern for Glob/Grep tools.
 */
export function buildReadPayload(
  toolName: string,
  input: Record<string, unknown>,
): { tool_name: string; file_paths: string[]; pattern?: string } {
  const normalized = normalizeToolName(toolName);

  // Read tool
  if (toolName === "read" || toolName === "read_file") {
    const filePath = String(input.file_path ?? input.path ?? "");
    return { tool_name: normalized, file_paths: [filePath] };
  }

  // Glob tool
  if (toolName === "glob") {
    const searchPath = String(input.path ?? input.directory ?? ".");
    const pattern = input.pattern ? String(input.pattern) : undefined;
    return { tool_name: normalized, file_paths: [searchPath], pattern };
  }

  // Grep tool
  if (toolName === "grep") {
    const searchPath = String(input.path ?? input.directory ?? ".");
    const pattern = input.pattern ? String(input.pattern) : undefined;
    return { tool_name: normalized, file_paths: [searchPath], pattern };
  }

  // List/LS tool
  if (toolName === "list" || toolName === "list_files") {
    const dirPath = String(input.path ?? input.directory ?? ".");
    return { tool_name: normalized, file_paths: [dirPath] };
  }

  // LSP operations
  if (toolName === "lsp") {
    const filePath = String(input.file_path ?? input.path ?? ".");
    return { tool_name: normalized, file_paths: [filePath] };
  }

  return { tool_name: normalized, file_paths: [] };
}

/**
 * Build a web event payload from OpenCode tool input.
 */
export function buildWebPayload(
  toolName: string,
  input: Record<string, unknown>,
): { operation: WebOperation; url?: string; query?: string } {
  if (toolName === "webSearch" || toolName === "web_search") {
    return {
      operation: "search",
      query: input.query ? String(input.query) : undefined,
    };
  }

  // WebFetch (if OpenCode adds it)
  return {
    operation: "fetch",
    url: input.url ? String(input.url) : undefined,
  };
}

/**
 * Parse MCP server name from tool name format: mcp__<server>__<tool>
 */
export function parseMcpServer(toolName: string): string | undefined {
  if (!toolName.startsWith("mcp__")) return undefined;
  const afterPrefix = toolName.slice(5);
  const idx = afterPrefix.indexOf("__");
  if (idx > 0) {
    return afterPrefix.slice(0, idx);
  }
  return undefined;
}
