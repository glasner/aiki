import { execFileSync, type ExecFileSyncOptions } from "node:child_process";
import type {
  AikiEventName,
  AikiSession,
  AgentType,
  Decision,
  DetectionMethod,
  HookResult,
  SessionMode,
} from "./types.js";

// ============================================================================
// Client Options
// ============================================================================

export interface AikiClientOptions {
  /** Agent type identifier (e.g. "opencode", "claude-code") */
  agent: AgentType;
  /** Working directory for aiki commands (defaults to process.cwd()) */
  cwd?: string;
  /** Path to the aiki binary (defaults to "aiki") */
  binary?: string;
  /** Timeout in milliseconds for aiki commands (defaults to 10000) */
  timeout?: number;
}

// ============================================================================
// Session Builder
// ============================================================================

export interface SessionOptions {
  /** External session ID from the agent */
  externalId: string;
  /** Agent version string */
  agentVersion?: string;
  /** Detection method */
  detectionMethod?: DetectionMethod;
  /** Session mode */
  mode?: SessionMode;
  /** Parent process ID */
  parentPid?: number;
  /** Task ID (from AIKI_TASK env var) */
  task?: string;
}

// ============================================================================
// Aiki Hooks Client
// ============================================================================

/**
 * Client for communicating with aiki via the `aiki hooks stdin` protocol.
 *
 * Each method fires an aiki event by piping JSON to `aiki hooks stdin`
 * and parsing the JSON response. This replaces raw `execSync` calls
 * with type-safe, validated interactions.
 *
 * @example
 * ```ts
 * const client = new AikiHooksClient({ agent: "opencode" });
 * const session = client.buildSession({ externalId: "sess-123" });
 *
 * // Fire session.started
 * const result = await client.fire("session.started", {
 *   session,
 *   cwd: process.cwd(),
 *   timestamp: new Date().toISOString(),
 * });
 *
 * if (result.decision === "block") {
 *   throw new Error("Blocked by aiki");
 * }
 * ```
 */
export class AikiHooksClient {
  private readonly agent: AgentType;
  private readonly cwd: string;
  private readonly binary: string;
  private readonly timeout: number;

  constructor(options: AikiClientOptions) {
    this.agent = options.agent;
    this.cwd = options.cwd ?? process.cwd();
    this.binary = options.binary ?? "aiki";
    this.timeout = options.timeout ?? 10_000;
  }

  /**
   * Build an AikiSession object from session options.
   *
   * The uuid is computed server-side by aiki (deterministic UUID v5).
   * We pass an empty string here; aiki populates it from the external_id.
   */
  buildSession(options: SessionOptions): AikiSession {
    return {
      uuid: "", // Computed by aiki server-side
      agent_type: this.agent,
      agent_version: options.agentVersion,
      external_id: options.externalId,
      detection_method: options.detectionMethod ?? "Hook",
      mode: options.mode ?? "interactive",
      parent_pid: options.parentPid ?? (typeof process !== "undefined" ? process.ppid : undefined),
      task: options.task ?? process.env.AIKI_TASK,
    };
  }

  /**
   * Fire an aiki event by piping a payload to `aiki hooks stdin`.
   *
   * Returns the parsed HookResult from aiki's stdout.
   * If aiki is not installed or the command fails, returns a default
   * "allow" result so the agent can continue gracefully.
   */
  fire(event: AikiEventName, payload: Record<string, unknown>): HookResult {
    const fullPayload = { event, ...payload };
    const json = JSON.stringify(fullPayload);

    try {
      const execOptions: ExecFileSyncOptions = {
        cwd: this.cwd,
        input: json,
        timeout: this.timeout,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      };

      const stdout = execFileSync(
        this.binary,
        ["hooks", "stdin", "--agent", this.agent, "--event", event],
        execOptions,
      ) as string;

      return this.parseResult(stdout);
    } catch (error: unknown) {
      // Graceful degradation: if aiki isn't installed or errors out,
      // return allow so the agent can continue working
      const message = error instanceof Error ? error.message : String(error);
      if (process.env.AIKI_DEBUG) {
        console.error(`[aiki-sdk] Hook ${event} failed: ${message}`);
      }
      return { decision: "allow", failures: [] };
    }
  }

  /**
   * Fire an event and throw if the decision is "block".
   *
   * Convenience wrapper for gateable events (permission_asked).
   * The thrown error message includes the failure reasons from aiki.
   */
  fireOrThrow(event: AikiEventName, payload: Record<string, unknown>): HookResult {
    const result = this.fire(event, payload);
    if (result.decision === "block") {
      const reason = result.failures.length > 0
        ? result.failures.join("; ")
        : "Blocked by aiki";
      throw new Error(reason);
    }
    return result;
  }

  // --- Convenience methods for common events ---

  /** Fire session.started */
  sessionStarted(session: AikiSession): HookResult {
    return this.fire("session.started", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
    });
  }

  /** Fire session.resumed */
  sessionResumed(session: AikiSession): HookResult {
    return this.fire("session.resumed", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
    });
  }

  /** Fire session.ended */
  sessionEnded(session: AikiSession, reason?: string): HookResult {
    return this.fire("session.ended", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      reason: reason ?? "other",
    });
  }

  /** Fire turn.started */
  turnStarted(session: AikiSession, prompt: string): HookResult {
    return this.fire("turn.started", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      prompt,
    });
  }

  /** Fire turn.completed */
  turnCompleted(session: AikiSession, response: string, modifiedFiles?: string[]): HookResult {
    return this.fire("turn.completed", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      response,
      modified_files: modifiedFiles ?? [],
    });
  }

  /** Fire change.permission_asked — throws if blocked */
  changePermissionAsked(
    session: AikiSession,
    toolName: string,
    operation: Record<string, unknown>,
  ): HookResult {
    return this.fireOrThrow("change.permission_asked", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      tool_name: toolName,
      ...operation,
    });
  }

  /** Fire change.completed */
  changeCompleted(
    session: AikiSession,
    toolName: string,
    success: boolean,
    operation: Record<string, unknown>,
  ): HookResult {
    return this.fire("change.completed", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      tool_name: toolName,
      success,
      ...operation,
    });
  }

  /** Fire shell.permission_asked — throws if blocked */
  shellPermissionAsked(session: AikiSession, command: string): HookResult {
    return this.fireOrThrow("shell.permission_asked", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      command,
    });
  }

  /** Fire shell.completed */
  shellCompleted(
    session: AikiSession,
    command: string,
    success: boolean,
    output?: { exit_code?: number; stdout?: string; stderr?: string },
  ): HookResult {
    return this.fire("shell.completed", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      command,
      success,
      ...output,
    });
  }

  /** Fire read.permission_asked — throws if blocked */
  readPermissionAsked(
    session: AikiSession,
    toolName: string,
    filePaths: string[],
    pattern?: string,
  ): HookResult {
    return this.fireOrThrow("read.permission_asked", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      tool_name: toolName,
      file_paths: filePaths,
      pattern,
    });
  }

  /** Fire read.completed */
  readCompleted(
    session: AikiSession,
    toolName: string,
    filePaths: string[],
  ): HookResult {
    return this.fire("read.completed", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      tool_name: toolName,
      file_paths: filePaths,
      success: true,
    });
  }

  /** Fire web.permission_asked — throws if blocked */
  webPermissionAsked(
    session: AikiSession,
    operation: "fetch" | "search",
    target: { url?: string; query?: string },
  ): HookResult {
    return this.fireOrThrow("web.permission_asked", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      operation,
      ...target,
    });
  }

  /** Fire web.completed */
  webCompleted(
    session: AikiSession,
    operation: "fetch" | "search",
    target: { url?: string; query?: string },
  ): HookResult {
    return this.fire("web.completed", {
      session,
      cwd: this.cwd,
      timestamp: new Date().toISOString(),
      operation,
      success: true,
      ...target,
    });
  }

  // --- Internal helpers ---

  private parseResult(stdout: string): HookResult {
    const trimmed = stdout.trim();
    if (!trimmed || trimmed === "{}") {
      return { decision: "allow", failures: [] };
    }

    try {
      const parsed = JSON.parse(trimmed);

      // Parse decision from various vendor formats
      let decision: Decision = "allow";
      if (parsed.decision === "block" || parsed.decision === "deny") {
        decision = "block";
      }
      if (parsed.hookSpecificOutput?.permissionDecision === "deny") {
        decision = "block";
      }

      // Extract context
      const context =
        parsed.hookSpecificOutput?.additionalContext ??
        parsed.systemMessage ??
        undefined;

      // Extract failure reasons
      const failures: string[] = [];
      if (parsed.reason) {
        failures.push(parsed.reason);
      }
      if (parsed.hookSpecificOutput?.permissionDecisionReason) {
        failures.push(parsed.hookSpecificOutput.permissionDecisionReason);
      }

      return { decision, context, failures };
    } catch {
      // If we can't parse the output, allow by default
      return { decision: "allow", failures: [] };
    }
  }
}
