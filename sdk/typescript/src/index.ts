// Core types
export type {
  AgentType,
  DetectionMethod,
  SessionMode,
  AikiSession,
  Turn,
  EditDetail,
  WriteOperation,
  DeleteOperation,
  MoveOperation,
  ChangeOperation,
  WebOperation,
  FileOperation,
  Decision,
  HookResult,
  AikiEventName,
  AikiEvent,
  AikiEventMap,
  // Individual payload types
  SessionStartedPayload,
  SessionResumedPayload,
  SessionWillCompactPayload,
  SessionCompactedPayload,
  SessionClearedPayload,
  SessionEndedPayload,
  TurnStartedPayload,
  TurnCompletedPayload,
  ReadPermissionAskedPayload,
  ReadCompletedPayload,
  ChangePermissionAskedPayload,
  ChangeCompletedPayload,
  ShellPermissionAskedPayload,
  ShellCompletedPayload,
  WebPermissionAskedPayload,
  WebCompletedPayload,
  McpPermissionAskedPayload,
  McpCompletedPayload,
} from "./types.js";

// Client
export { AikiHooksClient } from "./client.js";
export type { AikiClientOptions, SessionOptions } from "./client.js";

// OpenCode tool mapping utilities
export {
  classifyTool,
  getBeforeEvent,
  getAfterEvent,
  normalizeToolName,
  buildChangeOperation,
  buildReadPayload,
  buildWebPayload,
  parseMcpServer,
} from "./opencode-mapping.js";
export type { AikiEventDomain } from "./opencode-mapping.js";
