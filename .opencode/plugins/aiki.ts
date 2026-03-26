/**
 * Aiki plugin for OpenCode
 *
 * Bridges OpenCode's plugin lifecycle hooks to aiki's event system,
 * enabling provenance tracking, session management, and workflow
 * automation when using OpenCode as the AI coding agent.
 *
 * Uses @aiki/sdk for typed communication with `aiki hooks stdin`.
 *
 * Install: place this file in `.opencode/plugins/aiki.ts` or add
 * `"plugin": ["aiki"]` to your `opencode.json`.
 */

import {
  AikiHooksClient,
  classifyTool,
  buildChangeOperation,
  buildReadPayload,
  buildWebPayload,
  type AikiSession,
} from "@aiki/sdk";

// ============================================================================
// OpenCode plugin types (from OpenCode's plugin API)
// ============================================================================

interface PluginContext {
  directory: string;
  url?: string;
  [key: string]: unknown;
}

interface OpenCodeSession {
  id: string;
  [key: string]: unknown;
}

interface OpenCodeTool {
  name: string;
  [key: string]: unknown;
}

interface OpenCodeMessage {
  content: string;
  [key: string]: unknown;
}

// ============================================================================
// Plugin entry point
// ============================================================================

export default function aikiPlugin(ctx: PluginContext) {
  const client = new AikiHooksClient({
    agent: "opencode",
    cwd: ctx.directory,
  });

  // Track aiki sessions keyed by OpenCode session ID
  const sessions = new Map<string, AikiSession>();

  function getOrCreateSession(ocSession: OpenCodeSession): AikiSession {
    const existing = sessions.get(ocSession.id);
    if (existing) return existing;

    const session = client.buildSession({
      externalId: ocSession.id,
      parentPid: typeof process !== "undefined" ? process.ppid : undefined,
    });
    sessions.set(ocSession.id, session);
    return session;
  }

  function log(msg: string): void {
    if (process.env.AIKI_DEBUG) {
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

      // Prepend injected context (workspace path, tasks, etc.)
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
      if (!domain) return;

      const aikiSession = getOrCreateSession(session);

      try {
        switch (domain) {
          case "change": {
            const { tool_name, operation } = buildChangeOperation(tool.name, input);
            client.changePermissionAsked(
              aikiSession,
              tool_name,
              operation as unknown as Record<string, unknown>,
            );
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
            // OpenCode doesn't fire plugin hooks for MCP tools yet (issue #2319)
            log(`mcp.permission_asked (${tool.name}) — not supported by OpenCode`);
            break;
          }
        }
      } catch (error: unknown) {
        // Aiki blocked the operation — propagate to prevent execution
        const message = error instanceof Error ? error.message : String(error);
        log(`BLOCKED: ${message}`);
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
      output: { text?: string; exitCode?: number; stdout?: string; stderr?: string; [key: string]: unknown };
    }) => {
      const domain = classifyTool(tool.name);
      if (!domain) return;

      const aikiSession = getOrCreateSession(session);

      switch (domain) {
        case "change": {
          const { tool_name, operation } = buildChangeOperation(tool.name, input);
          client.changeCompleted(
            aikiSession,
            tool_name,
            true,
            operation as unknown as Record<string, unknown>,
          );
          break;
        }
        case "read": {
          const { tool_name, file_paths } = buildReadPayload(tool.name, input);
          client.readCompleted(aikiSession, tool_name, file_paths);
          break;
        }
        case "shell": {
          const command = String(input.command ?? input.cmd ?? "");
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
          log(`mcp.completed (${tool.name}) — not supported by OpenCode`);
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

      // Re-inject critical state after compaction
      if (result.context) {
        return result.context;
      }
    },
  };
}
