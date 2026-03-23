# OpenCode Integration Plan

Make OpenCode a first-class agent in Aiki — on par with Claude Code and Codex.

## Background

### What is OpenCode?

[OpenCode](https://opencode.ai/) is an open-source, provider-agnostic AI coding agent built in Go by the SST team. With 120K+ GitHub stars and 5M+ monthly developers, it's one of the largest open-source coding agents. It ships as a terminal TUI (built with Bubble Tea), a desktop app, and an IDE extension.

- **GitHub**: [github.com/opencode-ai/opencode](https://github.com/opencode-ai/opencode) (originally `sst/opencode`, now `anomalyco/opencode`)
- **Docs**: [opencode.ai/docs](https://opencode.ai/docs/)
- **Go SDK**: [github.com/sst/opencode-sdk-go](https://github.com/sst/opencode-sdk-go)
- **JS/TS SDK**: `@opencode-ai/sdk`

### Key characteristics

| Feature | Detail |
|---------|--------|
| **Language** | Go (Bun/Node for plugins) |
| **Architecture** | Client/server — TUI is a client talking to an HTTP server |
| **LLM providers** | 75+ providers including Anthropic, OpenAI, Google, Groq, AWS Bedrock, local (Ollama) |
| **Config** | `opencode.json` (project) + `~/.config/opencode/opencode.json` (global) |
| **Built-in agents** | `build` (full-access, default), `plan` (read-only analysis) |
| **Built-in subagents** | `general` (full tools, parallel work), `explore` (read-only codebase search) |
| **Tools** | Shell exec, file edit (exact string replace), grep/glob/list (ripgrep), LSP ops, patch apply, todo lists |
| **MCP support** | Full — local and remote MCP servers, auto OAuth for remote |
| **Plugin system** | JS/TS plugins in `.opencode/plugins/` with 25+ lifecycle hooks |
| **Session storage** | SQLite (persistent across restarts) |
| **Headless mode** | `opencode run "prompt"` — non-interactive, auto-approves all permissions |
| **Server mode** | `opencode serve` — headless HTTP server with OpenAPI 3.1 spec at `/doc` |
| **ACP support** | `opencode acp` — Agent Client Protocol via stdin/stdout nd-JSON |
| **SSE events** | Real-time streaming via `/sse` endpoint |
| **Instructions** | `AGENTS.md` file + `instructions` array in config + `.opencode/skills/` |
| **Permissions** | Per-tool `allow`/`ask`/`deny` rules, configurable per agent |

### OpenCode's execution modes

```
┌─────────────────────────────────────────────────────────┐
│  opencode              → Interactive TUI (default)       │
│  opencode run "..."    → Non-interactive, stdout result  │
│  opencode serve        → Headless HTTP server (:4096)    │
│  opencode acp          → ACP over stdin/stdout (nd-JSON) │
│  opencode web          → HTTP server + browser UI        │
│  opencode attach       → TUI client → remote server     │
│  opencode run --attach → Non-interactive → remote server │
└─────────────────────────────────────────────────────────┘
```

### OpenCode's plugin hook events

OpenCode plugins can intercept these lifecycle events:

| Hook | When | Can modify? |
|------|------|-------------|
| `chat.message` | After user submit, before agent processing | Prompt text |
| `tool.execute.before` | Before a tool runs | Can block (throw error) |
| `tool.execute.after` | After a tool completes | Output text |
| `session.*` | Session create, delete, error, idle | Session state |
| `system.transform` | Before message history sent to LLM | Full message array |
| `experimental.session.compacting` | Before context compaction | Compaction prompt |

**Known gap**: MCP tool calls do NOT trigger `tool.execute.before`/`tool.execute.after` hooks (see [issue #2319](https://github.com/sst/opencode/issues/2319)).

### OpenCode's server API

The `opencode serve` HTTP server exposes an OpenAPI 3.1 spec with 80+ endpoints:

| Route group | Purpose |
|-------------|---------|
| `/session` | CRUD sessions, send messages, list messages |
| `/config` | Read/write configuration |
| `/provider` | LLM provider management |
| `/permission` | Tool permission management |
| `/pty` | WebSocket pseudo-terminal |
| `/mcp` | MCP server management |
| `/global` | Global state |
| `/doc` | OpenAPI spec |
| SSE `/sse` | Real-time event stream (`message.part.updated`, etc.) |

---

## Integration strategy

OpenCode offers **three integration surfaces** for Aiki. We should use them based on the interaction mode:

| Aiki mode | OpenCode surface | Why |
|-----------|-----------------|-----|
| **Hook mode** (OpenCode calls aiki) | OpenCode plugin | Richest lifecycle access — intercept tool calls, inject context, track sessions |
| **Runtime mode** (aiki spawns OpenCode) | `opencode run` CLI | Simple, proven pattern matching Codex runtime |
| **ACP proxy mode** (IDE↔OpenCode via aiki) | `opencode acp` | Already have ACP proxy infra in `src/editors/acp/` |

### Phase 1: Core agent type + runtime + process detection

Add OpenCode as a spawnable agent alongside Claude Code and Codex. This is the minimum to make `aiki task run --agent opencode` work.

#### 1.1 Add `OpenCode` variant to `AgentType`

**File**: `src/agents/types.rs`

```rust
pub enum AgentType {
    ClaudeCode,
    Codex,
    OpenCode,    // ← new
    Cursor,
    Gemini,
    Unknown,
}
```

Implement all required methods:

| Method | Value |
|--------|-------|
| `from_str` | `"opencode"` |
| `as_str` | `"opencode"` |
| `to_metadata_string` | `"opencode"` |
| `email` | `"noreply@opencode.ai"` |
| `display_name` | `"OpenCode"` |
| `cli_binary` | `Some("opencode")` |
| `install_hint` | `"Install: curl -fsSL https://opencode.ai/install | bash (or: npm i -g opencode)"` |
| `git_author` | `"OpenCode <noreply@opencode.ai>"` |

#### 1.2 Add process detection

**File**: `src/agents/detect.rs`

Add to `match_agent()`:

```rust
// OpenCode
if name.contains("opencode") {
    return Some(AgentType::OpenCode);
}
```

#### 1.3 Create OpenCode runtime

**File**: `src/agents/runtime/opencode.rs` (new)

Follow the Codex runtime pattern. OpenCode's `opencode run` is the equivalent of `codex exec`:

```
opencode run "prompt text"
```

Key flags:
- `--quiet` / `-q` — suppress spinner (for scripting)
- `--format json` — structured output (if available)
- `--model <provider/model>` — override model
- `--continue` — resume previous session

The runtime spawns `opencode run` with the task prompt. Three modes:

| Spawn mode | Implementation |
|------------|---------------|
| `spawn_blocking` | `opencode run -q "prompt"`, wait for completion, parse stdout |
| `spawn_background` | Same but detached stdin/stdout/stderr, AIKI env vars set |
| `spawn_monitored` | Same but stderr piped for error capture |

Environment variables to pass:
- `AIKI_TASK` — task ID
- `AIKI_SESSION_MODE` — `"background"` or `"monitored"`
- `AIKI_PARENT_SESSION_UUID` — for workspace isolation chaining

**No JJ flags needed** — unlike Codex, OpenCode doesn't have a sandbox that needs `--skip-git-repo-check` or `--add-dir`. OpenCode doesn't manage its own git sandbox.

#### 1.4 Register in runtime lookup

**File**: `src/agents/runtime/mod.rs`

```rust
mod opencode;
pub use opencode::OpenCodeRuntime;

pub fn get_runtime(agent_type: AgentType) -> Option<Box<dyn AgentRuntime>> {
    match agent_type {
        AgentType::ClaudeCode => Some(Box::new(ClaudeCodeRuntime::new())),
        AgentType::Codex => Some(Box::new(CodexRuntime::new())),
        AgentType::OpenCode => Some(Box::new(OpenCodeRuntime::new())),
        _ => None,
    }
}
```

#### 1.5 Update Assignee parsing

**File**: `src/agents/types.rs`

The `Assignee::from_str` already delegates to `AgentType::from_str`, so adding `"opencode"` there covers task assignment automatically.

---

### Phase 2: Hook mode — OpenCode plugin for aiki

Create an aiki plugin for OpenCode that mirrors what we do with Claude Code hooks. This lets OpenCode-as-the-driver fire aiki events.

#### 2.1 Create aiki OpenCode plugin

**File**: `.opencode/plugins/aiki.ts` (shipped in repo, users copy or npm-install)

The plugin hooks into OpenCode's lifecycle and calls `aiki hooks stdin` just like Claude Code hooks do:

```typescript
import { execSync } from "child_process";

export default async function (ctx) {
  return {
    // Session lifecycle
    "session.created": async ({ session }) => {
      execSync(
        `echo '${JSON.stringify({ session_id: session.id })}' | aiki hooks stdin --agent opencode --event SessionStart`,
        { cwd: ctx.directory }
      );
    },

    // Tool execution — maps to change.completed, shell.completed, etc.
    "tool.execute.before": async ({ tool, input }) => {
      // Map OpenCode tool names to aiki event types:
      //   edit_file → change.permission_asked
      //   bash      → shell.permission_asked
      //   glob/grep → read.permission_asked
      const event = mapToolToAikiEvent(tool.name, "permission_asked");
      if (event) {
        const result = execSync(
          `echo '${JSON.stringify({ tool: tool.name, input })}' | aiki hooks stdin --agent opencode --event ${event}`,
          { cwd: ctx.directory }
        );
        // If aiki blocks, throw to prevent execution
        const parsed = JSON.parse(result.toString());
        if (parsed.decision === "block") {
          throw new Error(parsed.reason || "Blocked by aiki");
        }
      }
    },

    "tool.execute.after": async ({ tool, input, output }) => {
      const event = mapToolToAikiEvent(tool.name, "completed");
      if (event) {
        execSync(
          `echo '${JSON.stringify({ tool: tool.name, input, output: output.text })}' | aiki hooks stdin --agent opencode --event ${event}`,
          { cwd: ctx.directory }
        );
      }
    },
  };
}

function mapToolToAikiEvent(toolName: string, suffix: string): string | null {
  const mapping: Record<string, string> = {
    edit_file: "change",
    write_file: "change",
    bash: "shell",
    glob: "read",
    grep: "read",
    list_files: "read",
    read_file: "read",
  };
  const prefix = mapping[toolName];
  return prefix ? `${prefix}.${suffix}` : null;
}
```

#### 2.2 Create aiki editor handler for OpenCode

**Directory**: `src/editors/opencode/` (new)

Follow the Claude Code editor pattern with these files:

| File | Purpose |
|------|---------|
| `mod.rs` | Entry point, route incoming hook events |
| `events.rs` | Map OpenCode plugin JSON → `AikiEvent` |
| `session.rs` | Create `AikiSession` from OpenCode session data |
| `output.rs` | Format `HookResult` back to OpenCode plugin format |

The plugin communicates with aiki via `aiki hooks stdin --agent opencode --event <name>`. The editor handler parses the JSON payload and maps it to `AikiEvent` variants.

#### 2.3 OpenCode tool → aiki event mapping

| OpenCode tool | Aiki event |
|---------------|------------|
| `edit_file` | `ChangeCompleted` / `ChangePermissionAsked` |
| `write_file` | `ChangeCompleted` / `ChangePermissionAsked` |
| `bash` | `ShellCompleted` / `ShellPermissionAsked` |
| `glob`, `grep`, `list_files`, `read_file` | `ReadCompleted` / `ReadPermissionAsked` |
| `todo` | (no aiki event — internal to OpenCode) |
| `lsp_*` | `ReadCompleted` (LSP operations are read-only) |
| MCP tools | **Not mapped** — OpenCode doesn't fire plugin hooks for MCP calls |

#### 2.4 Session detection

OpenCode sessions need special handling for PID-based detection:

- **TUI mode**: The `opencode` process is the parent. Process detection works via `match_agent("opencode", ...)`.
- **Server mode**: `opencode serve` runs as a daemon. The plugin should pass the server PID or session ID explicitly.
- **`opencode run` mode**: Short-lived process. Aiki runtime handles this directly (Phase 1).

The plugin should include `parent_pid: process.pid` in its SessionStart payload so the session file system can track it.

---

### Phase 3: ACP proxy mode

OpenCode supports ACP via `opencode acp`. Our existing ACP proxy infrastructure (`src/editors/acp/`) can work with OpenCode as the agent backend.

#### 3.1 Register OpenCode as an ACP-capable agent

The ACP proxy in `src/commands/acp.rs` needs to know how to spawn the agent subprocess. Add OpenCode:

```rust
// When --agent opencode is passed, spawn: opencode acp
fn agent_acp_command(agent: AgentType) -> Option<Command> {
    match agent {
        AgentType::ClaudeCode => { /* existing */ },
        AgentType::OpenCode => {
            let mut cmd = Command::new("opencode");
            cmd.arg("acp");
            Some(cmd)
        },
        _ => None,
    }
}
```

This lets IDEs like Zed use `aiki acp --agent opencode` to get an OpenCode backend with full aiki provenance tracking.

---

### Phase 4: Configuration + instructions injection

#### 4.1 Auto-generate `opencode.json` during `aiki init`

When `aiki init` detects or is told about OpenCode, generate a project-level `opencode.json`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "instructions": [
    "AGENTS.md",
    ".aiki/instructions/**/*.md"
  ],
  "mcp": {},
  "agent": {
    "build": {
      "tools": {
        "permission": {
          "bash": "allow",
          "edit_file": "allow",
          "write_file": "allow"
        }
      }
    }
  }
}
```

#### 4.2 Plugin installation

Add an `aiki init --agent opencode` flow that:

1. Creates `.opencode/plugins/aiki.ts` with the plugin from Phase 2
2. Adds `"plugin": ["aiki"]` to `opencode.json` (or uses local file detection)
3. Generates `AGENTS.md` with aiki-specific instructions (task workflow, `aiki task start/close` usage)

#### 4.3 Context injection via hooks

The aiki plugin's `chat.message` hook can inject task context before the LLM sees the prompt:

```typescript
"chat.message": async ({ message }) => {
  // Inject current task context if AIKI_TASK is set
  const taskId = process.env.AIKI_TASK;
  if (taskId) {
    const context = execSync(`aiki task show ${taskId} --format=context`, { cwd: ctx.directory });
    message.content = `${context}\n\n${message.content}`;
  }
}
```

---

### Phase 5: Advanced — server mode integration

For long-running workflows, Aiki can talk directly to OpenCode's HTTP server instead of spawning `opencode run` for each task.

#### 5.1 Server-backed runtime (optional, future)

Instead of spawning a new `opencode run` per task, start `opencode serve` once and use the REST API:

```
POST /session          → Create session
POST /session/{id}/message → Send task prompt
GET  /sse              → Stream events
```

Benefits:
- **No cold start** — MCP servers stay warm across tasks
- **Session continuity** — can continue previous sessions
- **Real-time monitoring** — SSE events for progress tracking

This requires the Go or JS SDK, or direct HTTP calls from Rust. Could use `reqwest` for HTTP and `eventsource` for SSE.

#### 5.2 Model configuration passthrough

When spawning OpenCode for a task, Aiki should be able to configure which model to use:

```rust
// In AgentSpawnOptions or a new OpenCodeSpawnOptions
pub struct OpenCodeSpawnOptions {
    pub model: Option<String>,      // e.g. "anthropic/claude-sonnet-4-20250514"
    pub provider: Option<String>,   // e.g. "anthropic"
}
```

This maps to `--model` on `opencode run` or the `model` field in `opencode.json`.

---

## Implementation order

| Phase | Scope | Effort | Depends on |
|-------|-------|--------|------------|
| **Phase 1** | Agent type + runtime + detection | Small | Nothing |
| **Phase 2** | Plugin + editor handler | Medium | Phase 1 |
| **Phase 3** | ACP proxy registration | Small | Phase 1 |
| **Phase 4** | Config generation + init flow | Medium | Phase 2 |
| **Phase 5** | Server mode runtime | Large | Phase 1, exploratory |

Phase 1 is the critical path — it unlocks `aiki task run --agent opencode` immediately. Phases 2-4 add the observability and provenance tracking that make it a true first-class citizen. Phase 5 is an optimization for heavy multi-task workflows.

---

## Open questions

1. **OpenCode's `opencode run` output format** — Does `--format json` exist and what's the schema? Need to test to confirm structured output parsing for `AgentSessionResult`.

2. **Plugin distribution** — Should the aiki plugin be an npm package (`aiki-opencode-plugin`) or always a local `.opencode/plugins/aiki.ts` file? npm is cleaner for updates, local is simpler for adoption.

3. **MCP tool hook gap** — OpenCode doesn't fire plugin hooks for MCP tool calls ([#2319](https://github.com/sst/opencode/issues/2319)). This means aiki can't track MCP-based file changes via the plugin. Workaround: use `system.transform` to scan conversation for MCP results, or wait for upstream fix.

4. **Session resume gap** — OpenCode doesn't fire a session event on `--continue`/`--session` resume ([#5409](https://github.com/sst/opencode/issues/5409)). Aiki's session tracking may miss resumed sessions. Workaround: detect via PID-based matching on subsequent tool calls.

5. **Server mode auth** — When using `opencode serve`, authentication is via `OPENCODE_SERVER_PASSWORD`. Should aiki manage this automatically (generate random password) or require user config?

6. **Model selection** — OpenCode supports 75+ providers. Should aiki expose model selection in task config, or delegate entirely to OpenCode's `opencode.json`?

---

## References

- [OpenCode docs](https://opencode.ai/docs/)
- [OpenCode GitHub](https://github.com/opencode-ai/opencode)
- [OpenCode Go SDK](https://pkg.go.dev/github.com/sst/opencode-sdk-go)
- [OpenCode JS/TS SDK](https://opencode.ai/docs/sdk/)
- [OpenCode server docs](https://opencode.ai/docs/server/)
- [OpenCode plugin guide](https://opencode.ai/docs/plugins/)
- [OpenCode agents docs](https://opencode.ai/docs/agents/)
- [OpenCode CLI docs](https://opencode.ai/docs/cli/)
- [OpenCode config docs](https://opencode.ai/docs/config/)
- [OpenCode MCP docs](https://opencode.ai/docs/mcp-servers/)
- [OpenCode deep dive (internals)](https://cefboud.com/posts/coding-agents-internals-opencode-deepdive/)
- [ACP Protocol Spec](https://agentclientprotocol.com/protocol/schema)
