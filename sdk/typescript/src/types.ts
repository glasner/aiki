// ============================================================================
// Core Types — mirrors Rust types in src/session/mod.rs, src/events/mod.rs
// ============================================================================

/** Agent type identifier */
export type AgentType =
  | "claude-code"
  | "codex"
  | "opencode"
  | "cursor"
  | "gemini"
  | "unknown";

/** How Aiki detected/integrated with the agent */
export type DetectionMethod = "Hook" | "ACP" | "Unknown";

/** Session mode — background (task runner) vs interactive (user-driven) */
export type SessionMode = "background" | "interactive";

/** Aiki session metadata, shared by all event payloads */
export interface AikiSession {
  /** Deterministic UUID v5, first 8 hex chars */
  uuid: string;
  /** Agent type (claude-code, opencode, etc.) */
  agent_type: AgentType;
  /** Agent version string, e.g. "0.10.6" */
  agent_version?: string;
  /** External session ID provided by the agent */
  external_id: string;
  /** Client (IDE) name, e.g. "zed", "neovim" (ACP only) */
  client_name?: string;
  /** Client (IDE) version */
  client_version?: string;
  /** Integration type — how Aiki is integrated with the agent */
  detection_method: DetectionMethod;
  /** Session mode */
  mode: SessionMode;
  /** Parent process ID of the agent */
  parent_pid?: number;
  /** Task ID driving this session (from AIKI_TASK env var) */
  task?: string;
}

// ============================================================================
// Turn Info
// ============================================================================

/** Turn metadata for events */
export interface Turn {
  /** Sequential turn number within session (starts at 1, 0 if unknown) */
  number: number;
  /** Deterministic turn identifier: uuid_v5(session_uuid, turn.to_string()) */
  id: string;
  /** Source of this turn: "user" or "autoreply" */
  source: string;
}

// ============================================================================
// Change Operations (tagged union)
// ============================================================================

/** Edit detail — old_string -> new_string replacement */
export interface EditDetail {
  file_path: string;
  old_string: string;
  new_string: string;
}

/** Write operation — file content created or modified */
export interface WriteOperation {
  operation: "write";
  file_paths: string[];
  edit_details?: EditDetail[];
}

/** Delete operation — file removed */
export interface DeleteOperation {
  operation: "delete";
  file_paths: string[];
}

/** Move operation — file relocated */
export interface MoveOperation {
  operation: "move";
  file_paths: string[];
  source_paths: string[];
  destination_paths: string[];
}

/** Discriminated union: the type of file mutation */
export type ChangeOperation = WriteOperation | DeleteOperation | MoveOperation;

// ============================================================================
// Web & File Operation Enums
// ============================================================================

/** Web operation type */
export type WebOperation = "fetch" | "search";

/** File operation type */
export type FileOperation = "read" | "write" | "delete" | "move";

// ============================================================================
// Hook Result Types
// ============================================================================

/** Decision about whether to allow or block an operation */
export type Decision = "allow" | "block";

/** Generic hook result returned from aiki hooks stdin */
export interface HookResult {
  /** Context string for PrePrompt (modified prompt) or PostResponse (autoreply) */
  context?: string;
  /** Decision about whether to allow or block the operation */
  decision: Decision;
  /** Failure messages */
  failures: string[];
}

// ============================================================================
// Event Payloads — mirrors Rust types in src/events/*.rs
// ============================================================================

// --- Session Lifecycle ---

export interface SessionStartedPayload {
  event: "session.started";
  session: AikiSession;
  cwd: string;
  timestamp: string;
}

export interface SessionResumedPayload {
  event: "session.resumed";
  session: AikiSession;
  cwd: string;
  timestamp: string;
}

export interface SessionWillCompactPayload {
  event: "session.will_compact";
  session: AikiSession;
  cwd: string;
  timestamp: string;
}

export interface SessionCompactedPayload {
  event: "session.compacted";
  session: AikiSession;
  cwd: string;
  timestamp: string;
}

export interface SessionClearedPayload {
  event: "session.cleared";
  session: AikiSession;
  cwd: string;
  timestamp: string;
}

export interface SessionEndedPayload {
  event: "session.ended";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  /** Reason for session termination */
  reason?: string;
}

// --- Turn Lifecycle ---

export interface TurnStartedPayload {
  event: "turn.started";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  turn?: Turn;
  /** The prompt text (user input or autoreply context) */
  prompt: string;
  /** References to files injected as context */
  injected_refs?: string[];
}

export interface TurnCompletedPayload {
  event: "turn.completed";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  turn?: Turn;
  /** The agent's response text for this turn */
  response: string;
  /** Files modified during this turn */
  modified_files?: string[];
}

// --- Read Operations ---

export interface ReadPermissionAskedPayload {
  event: "read.permission_asked";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  /** Tool requesting the read (e.g. "Read", "Glob", "Grep") */
  tool_name: string;
  /** Files/directories about to be read or searched */
  file_paths: string[];
  /** Search pattern for Glob/Grep tools */
  pattern?: string;
}

export interface ReadCompletedPayload {
  event: "read.completed";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  tool_name: string;
  file_paths: string[];
  success: boolean;
}

// --- Change Operations (Unified mutations: write, delete, move) ---

export interface ChangePermissionAskedPayload {
  event: "change.permission_asked";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  /** Tool requesting permission (e.g. "Edit", "Write", "Delete", "Move", "Bash") */
  tool_name: string;
  /** The specific operation being requested (flattened) */
  operation: ChangeOperation["operation"];
  file_paths: string[];
  edit_details?: EditDetail[];
  source_paths?: string[];
  destination_paths?: string[];
}

export interface ChangeCompletedPayload {
  event: "change.completed";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  tool_name: string;
  success: boolean;
  turn?: Turn;
  /** The specific operation that occurred (flattened) */
  operation: ChangeOperation["operation"];
  file_paths: string[];
  edit_details?: EditDetail[];
  source_paths?: string[];
  destination_paths?: string[];
}

// --- Shell Commands ---

export interface ShellPermissionAskedPayload {
  event: "shell.permission_asked";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  /** The shell command about to be executed */
  command: string;
}

export interface ShellCompletedPayload {
  event: "shell.completed";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  command: string;
  success: boolean;
  exit_code?: number;
  stdout?: string;
  stderr?: string;
}

// --- Web Access ---

export interface WebPermissionAskedPayload {
  event: "web.permission_asked";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  operation: WebOperation;
  url?: string;
  query?: string;
}

export interface WebCompletedPayload {
  event: "web.completed";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  operation: WebOperation;
  url?: string;
  query?: string;
  success: boolean;
}

// --- MCP Tools ---

export interface McpPermissionAskedPayload {
  event: "mcp.permission_asked";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  /** MCP server name (parsed from tool_name format: mcp__<server>__<tool>) */
  server?: string;
  tool_name: string;
  parameters?: Record<string, unknown>;
}

export interface McpCompletedPayload {
  event: "mcp.completed";
  session: AikiSession;
  cwd: string;
  timestamp: string;
  server?: string;
  tool_name: string;
  success: boolean;
  result?: string;
}

// ============================================================================
// Event Names & Discriminated Union
// ============================================================================

/** All event names */
export type AikiEventName =
  | "session.started"
  | "session.resumed"
  | "session.will_compact"
  | "session.compacted"
  | "session.cleared"
  | "session.ended"
  | "turn.started"
  | "turn.completed"
  | "read.permission_asked"
  | "read.completed"
  | "change.permission_asked"
  | "change.completed"
  | "shell.permission_asked"
  | "shell.completed"
  | "web.permission_asked"
  | "web.completed"
  | "mcp.permission_asked"
  | "mcp.completed";

/** Discriminated union of all event payloads (tagged by `event` field) */
export type AikiEvent =
  | SessionStartedPayload
  | SessionResumedPayload
  | SessionWillCompactPayload
  | SessionCompactedPayload
  | SessionClearedPayload
  | SessionEndedPayload
  | TurnStartedPayload
  | TurnCompletedPayload
  | ReadPermissionAskedPayload
  | ReadCompletedPayload
  | ChangePermissionAskedPayload
  | ChangeCompletedPayload
  | ShellPermissionAskedPayload
  | ShellCompletedPayload
  | WebPermissionAskedPayload
  | WebCompletedPayload
  | McpPermissionAskedPayload
  | McpCompletedPayload;

/** Map event names to their payload types */
export interface AikiEventMap {
  "session.started": SessionStartedPayload;
  "session.resumed": SessionResumedPayload;
  "session.will_compact": SessionWillCompactPayload;
  "session.compacted": SessionCompactedPayload;
  "session.cleared": SessionClearedPayload;
  "session.ended": SessionEndedPayload;
  "turn.started": TurnStartedPayload;
  "turn.completed": TurnCompletedPayload;
  "read.permission_asked": ReadPermissionAskedPayload;
  "read.completed": ReadCompletedPayload;
  "change.permission_asked": ChangePermissionAskedPayload;
  "change.completed": ChangeCompletedPayload;
  "shell.permission_asked": ShellPermissionAskedPayload;
  "shell.completed": ShellCompletedPayload;
  "web.permission_asked": WebPermissionAskedPayload;
  "web.completed": WebCompletedPayload;
  "mcp.permission_asked": McpPermissionAskedPayload;
  "mcp.completed": McpCompletedPayload;
}
