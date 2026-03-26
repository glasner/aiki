import { AikiHooksClient } from "../client.js";
import type { AikiSession, HookResult } from "../types.js";
import {
  buildChangeOperation,
  buildReadPayload,
  buildWebPayload,
  classifyTool,
  normalizeToolName,
  parseMcpServer,
} from "./mapping.js";

// ============================================================================
// OpenCode Plugin Types
// ============================================================================

/** OpenCode plugin context (passed to the plugin's default export) */
export interface OpenCodePluginContext {
  /** Project root directory */
  directory: string;
  /** Server URL */
  url?: string;
  [key: string]: unknown;
}

/** OpenCode session object */
export interface OpenCodeSession {
  id: string;
  [key: string]: unknown;
}

/** OpenCode tool object */
export interface OpenCodeTool {
  name: string;
  [key: string]: unknown;
}

/** OpenCode message object */
export interface OpenCodeMessage {
  content: string;
  [key: string]: unknown;
}

/** Options for creating the aiki plugin */
export interface AikiPluginOptions {
  /** Override the aiki binary path */
  binary?: string;
  /** Timeout for aiki commands (ms) */
  timeout?: number;
  /** Enable debug logging */
  debug?: boolean;
}

// ============================================================================
// Plugin Factory
// ============================================================================

/**
 * Create an OpenCode plugin that integrates with aiki.
 *
 * Returns an object matching OpenCode's plugin hook interface. Register this
 * as your plugin's default export:
 *
 * @example
 * ```ts
 * // .opencode/plugins/aiki.ts
 * import { createAikiPlugin } from "@aiki/sdk/opencode";
 *
 * export default function(ctx) {
 *   return createAikiPlugin(ctx);
 * }
 * ```
 *
 * Or with options:
 *
 * @example
 * ```ts
 * export default function(ctx) {
 *   return createAikiPlugin(ctx, { debug: true });
 * }
 * ```
 */
export function createAikiPlugin(
  ctx: OpenCodePluginContext,
  options?: AikiPluginOptions,
) {
  const client = new AikiHooksClient({
    agent: "opencode",
    cwd: ctx.directory,
    binary: options?.binary,
    timeout: options?.timeout,
  });

  // Track the aiki session per OpenCode session
  const sessions = new Map<string, AikiSession>();

  function getOrCreateSession(openCodeSession: OpenCodeSession): AikiSession {
    const existing = sessions.get(openCodeSession.id);
    if (existing) return existing;

    const session = client.buildSession({
      externalId: openCodeSession.id,
      parentPid: typeof process !== "undefined" ? process.ppid : undefined,
    });
    sessions.set(openCodeSession.id, session);
    return session;
  }

  function log(msg: string): void {
    if (options?.debug || process.env.AIKI_DEBUG) {
      console.error(`[aiki] ${msg}`);
    }
  }

  return {
    // ==================================================================
    // Session Lifecycle
    // ==================================================================

    "session.created": ({ session }: { session: OpenCodeSession }) => {
      const aikiSession = getOrCreateSession(session);
      const result = client.sessionStarted(aikiSession);
      log(`session.started → ${result.decision}`);
    },

    "session.deleted": ({ session }: { session: OpenCodeSession }) => {
      const aikiSession = getOrCreateSession(session);
      client.sessionEnded(aikiSession, "clear");
      sessions.delete(session.id);
      log("session.ended (deleted)");
    },

    "session.error": ({ session }: { session: OpenCodeSession }) => {
      const aikiSession = getOrCreateSession(session);
      client.sessionEnded(aikiSession, "error");
      sessions.delete(session.id);
      log("session.ended (error)");
    },

    "session.idle": ({ session }: { session: OpenCodeSession }) => {
      // session.idle is informational — no aiki event needed
      log(`session.idle: ${session.id}`);
    },

    // ==================================================================
    // Message Lifecycle (turn tracking)
    // ==================================================================

    "chat.message": ({
      session,
      message,
    }: {
      session: OpenCodeSession;
      message: OpenCodeMessage;
    }) => {
      const aikiSession = getOrCreateSession(session);
      const result = client.turnStarted(aikiSession, message.content);

      // If aiki injected context, prepend it to the message
      if (result.context) {
        message.content = `${result.context}\n\n${message.content}`;
      }

      log(`turn.started → context: ${result.context ? "yes" : "no"}`);
    },

    // ==================================================================
    // Tool Lifecycle
    // ==================================================================

    "tool.execute.before": ({
      session,
      tool,
      input,
    }: {
      session: OpenCodeSession;
      tool: OpenCodeTool;
      input: Record<string, unknown>;
    }) => {
      const domain = classifyTool(tool.name);
      if (!domain) return; // Internal tool, no aiki event

      const aikiSession = getOrCreateSession(session);

      try {
        switch (domain) {
          case "change": {
            const { tool_name, operation } = buildChangeOperation(tool.name, input);
            client.changePermissionAsked(aikiSession, tool_name, operation as unknown as Record<string, unknown>);
            break;
          }
          case "read": {
            const { tool_name, file_paths, pattern } = buildReadPayload(tool.name, input);
            client.readPermissionAsked(aikiSession, tool_name, file_paths, pattern);
            break;
          }
          case "shell": {
            const command = String(input.command ?? input.cmd ?? "");
            client.shellPermissionAsked(aikiSession, command);
            break;
          }
          case "web": {
            const { operation, url, query } = buildWebPayload(tool.name, input);
            client.webPermissionAsked(aikiSession, operation, { url, query });
            break;
          }
          case "mcp": {
            // Note: OpenCode doesn't currently fire plugin hooks for MCP tools
            // (see issue #2319). This is here for future compatibility.
            log(`mcp.permission_asked (${tool.name}) — may not fire in OpenCode`);
            break;
          }
        }
      } catch (error: unknown) {
        // If aiki blocked the operation, throw to prevent execution
        const message = error instanceof Error ? error.message : String(error);
        log(`tool.execute.before BLOCKED: ${message}`);
        throw error;
      }
    },

    "tool.execute.after": ({
      session,
      tool,
      input,
      output,
    }: {
      session: OpenCodeSession;
      tool: OpenCodeTool;
      input: Record<string, unknown>;
      output: { text?: string; [key: string]: unknown };
    }) => {
      const domain = classifyTool(tool.name);
      if (!domain) return;

      const aikiSession = getOrCreateSession(session);

      switch (domain) {
        case "change": {
          const { tool_name, operation } = buildChangeOperation(tool.name, input);
          client.changeCompleted(aikiSession, tool_name, true, operation as unknown as Record<string, unknown>);
          break;
        }
        case "read": {
          const { tool_name, file_paths } = buildReadPayload(tool.name, input);
          client.readCompleted(aikiSession, tool_name, file_paths);
          break;
        }
        case "shell": {
          const command = String(input.command ?? input.cmd ?? "");
          // Parse exit code from output if available
          const exitCode = typeof output.exitCode === "number" ? output.exitCode : undefined;
          const success = exitCode === undefined || exitCode === 0;
          client.shellCompleted(aikiSession, command, success, {
            exit_code: exitCode,
            stdout: typeof output.stdout === "string" ? output.stdout : undefined,
            stderr: typeof output.stderr === "string" ? output.stderr : undefined,
          });
          break;
        }
        case "web": {
          const { operation, url, query } = buildWebPayload(tool.name, input);
          client.webCompleted(aikiSession, operation, { url, query });
          break;
        }
        case "mcp": {
          log(`mcp.completed (${tool.name}) — may not fire in OpenCode`);
          break;
        }
      }
    },

    // ==================================================================
    // Context Compaction
    // ==================================================================

    "experimental.session.compacting": ({
      session,
    }: {
      session: OpenCodeSession;
    }) => {
      const aikiSession = getOrCreateSession(session);

      // Fire will_compact then compacted
      client.fire("session.will_compact", {
        session: aikiSession,
        cwd: ctx.directory,
        timestamp: new Date().toISOString(),
      });

      const result = client.fire("session.compacted", {
        session: aikiSession,
        cwd: ctx.directory,
        timestamp: new Date().toISOString(),
      });

      log(`session.compacted → context: ${result.context ? "yes" : "no"}`);

      // Return context to be re-injected after compaction
      if (result.context) {
        return result.context;
      }
    },
  };
}
