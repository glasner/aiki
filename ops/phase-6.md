# Plan: Phase 6 - ACP Support via Bidirectional Proxy

## Key Use Cases Requiring Bidirectional Control

1. **Provenance tracking** - Observe agent → Zed `tool_call` notifications
2. **Prompt manipulation** - Inject context into Zed → Agent prompts
3. **Autonomous review** - Add review feedback to prompts
4. **Policy enforcement** - Block or modify dangerous operations
5. **Context enhancement** - Pass previous session data to agents

---

## Architecture: Full Bidirectional Proxy

```
IDE (Zed/Neovim/etc) ←→ aiki acp ←→ Agent (claude-code/gemini/etc)
                            ↓              ↓
                       Modify prompts  Observe tool_call
                       Inject context  Record provenance
                       Auto-detect IDE from InitializeRequest
```

---

## Command Structure

```bash
# Aiki subcommand (NOT a separate binary)
# Generic ACP proxy - auto-detects client from InitializeRequest.clientInfo
aiki acp <agent-type> [--bin <path>] [-- <agent-args>...]

# Examples (executable derived from agent-type):
aiki acp claude-code
aiki acp gemini
aiki acp cursor

# Examples (custom executable):
aiki acp claude-code --bin /custom/path/to/claude-code
aiki acp gemini --bin gemini-cli-beta

# Examples (passing args to agent):
aiki acp gemini -- --verbose --model gemini-2.0
aiki acp cursor --bin /usr/local/bin/cursor -- --debug
```

**Terminology:**
- **Client/IDE**: The editor making requests (Zed, Neovim, etc.) - auto-detected from `InitializeRequest.clientInfo.name`
- **Agent Type**: Your `AgentType` enum value (claude-code, cursor, gemini, etc.) - specified for provenance tracking
- **--bin**: Optional flag to specify custom binary path - if not specified, derived from agent-type (e.g., "claude-code" → "claude-code")
- **-- <agent-args>**: Optional args to pass to the agent executable (after `--` separator)

---

## IDE Configuration

### Zed (~/.config/zed/settings.json):
```json
{
  "agent_servers": {
    "claude": {
      "env": {
        "CLAUDE_CODE_EXECUTABLE": "aiki"
      },
      "args": ["acp", "claude-code"]
    },
    "gemini": {
      "command": "aiki",
      "args": ["acp", "gemini"]
    },
    "cursor": {
      "command": "aiki",
      "args": ["acp", "cursor"]
    }
  }
}
```

**Note:** The executable is derived from the agent type (e.g., "claude-code" → "claude-code" binary). For custom paths, use the `--bin` flag: `["acp", "claude-code", "--bin", "/custom/path/claude-code"]`

### Other ACP-Compatible IDEs:
The same pattern works for any IDE that supports ACP - Neovim, VSCode (future), JetBrains (future), etc. The client name is auto-detected from the `InitializeRequest`.

---

## Implementation

### CLI Structure (`cli/src/main.rs`):

```rust
#[derive(Parser)]
enum Commands {
    // ... existing commands
    
    /// ACP proxy for IDE-agent communication (auto-detects IDE)
    Acp {
        /// Agent type for provenance (e.g., "claude-code", "cursor", "gemini")
        agent_type: String,
        
        /// Optional custom binary path (defaults to derived from agent_type)
        #[arg(short, long)]
        bin: Option<String>,
        
        /// Optional arguments to pass to the agent executable
        #[arg(last = true)]
        agent_args: Vec<String>,
    },
}

fn run() -> Result<()> {
    match cli.command {
        // ... existing commands
        
        Commands::Acp { agent_type, bin, agent_args } => {
            commands::acp::run(agent_type, bin, agent_args)
        }
    }
}
```

### Proxy Implementation (`cli/src/commands/acp.rs`):

```rust
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;
use anyhow::Result;
use crate::acp::protocol::{JsonRpcMessage, InitializeRequest};
use crate::error::AikiError;

pub fn run(agent_type: String, bin: Option<String>, agent_args: Vec<String>) -> Result<()> {
    // Validate agent_type matches our enum
    let _ = parse_agent_type(&agent_type)?;
    
    // Determine executable: use --bin flag if provided, otherwise derive from agent_type
    let executable = bin.unwrap_or_else(|| derive_executable(&agent_type));
    
    let mut client_name: Option<String> = None;
    
    // Launch agent with piped stdin/stdout
    let mut agent = Command::new(&executable)
        .args(&agent_args)
        .env("AIKI_ENABLED", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    
    let mut agent_stdin = agent.stdin.take().unwrap();
    let agent_stdout = agent.stdout.take().unwrap();
    
    // Thread 1: IDE → Agent (intercept and modify)
    let client_name_clone = client_name.clone();
    thread::spawn(move || -> Result<()> {
        for line in io::stdin().lock().lines() {
            let line = line?;
            
            // Parse message from IDE
            let mut msg = serde_json::from_str::<JsonRpcMessage>(&line)?;
            
            // Capture client info from initialize request
            if let Some(method) = &msg.method {
                if method == "initialize" {
                    if let Ok(init_req) = serde_json::from_value::<InitializeRequest>(msg.params.clone()) {
                        if let Some(client_info) = init_req.client_info {
                            client_name_clone = Some(client_info.name);
                        }
                    }
                }
                
                // Future: Modify messages before sending to agent
                match method.as_str() {
                    "session/send_message" => {
                        // msg = modify_user_prompt(msg, &client_name_clone)?;
                    }
                    _ => {}
                }
            }
            
            // Forward to agent
            writeln!(agent_stdin, "{}", serde_json::to_string(&msg)?)?;
            agent_stdin.flush()?;
        }
        Ok(())
    });
    
    // Thread 2: Agent → IDE (observe and record)
    for line in BufReader::new(agent_stdout).lines() {
        let line = line?;
        
        // Parse message from agent
        if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&line) {
            if let Some(method) = &msg.method {
                if method == "session/update" {
                    handle_session_update(&msg, &agent_type, &client_name)?;
                }
            }
        }
        
        // Forward to IDE
        println!("{}", line);
    }
    
    let status = agent.wait()?;
    std::process::exit(status.code().unwrap_or(1));
}

fn parse_agent_type(agent: &str) -> Result<()> {
    // Validate against AgentType enum
    match agent {
        "claude-code" | "cursor" | "gemini" | "windsurf" | "aider" | "cline" => Ok(()),
        _ => Err(AikiError::UnknownAgentType(agent.to_string()).into()),
    }
}

fn derive_executable(agent_type: &str) -> String {
    // Map agent type to default executable
    // Most agent types use their name as the executable
    match agent_type {
        "gemini" => "gemini-cli".to_string(),
        other => other.to_string(),
    }
}

fn handle_session_update(msg: &JsonRpcMessage, agent_type: &str, client_name: &Option<String>) -> Result<()> {
    // Extract tool_call notifications and record provenance
    // agent_type: from AgentType enum (e.g., "claude-code", "cursor")
    // client_name: from InitializeRequest (e.g., "zed", "neovim")
    // (non-blocking, async dispatch)
    Ok(())
}
```

---

## Files to Create

1. `cli/src/commands/acp.rs` - **NEW:** Bidirectional ACP proxy command
2. `cli/src/acp/` - **NEW:** ACP protocol support
   - `acp/mod.rs`
   - `acp/protocol.rs` - JSON-RPC and ACP types (InitializeRequest, ClientInfo, etc.)
   - `acp/modifier.rs` - Message modification (future)
   - `acp/context.rs` - Provenance context loading (future)
3. `ops/phase-6.md` - **NEW:** Phase documentation

---

## Files to Modify

1. `ops/ROADMAP.md` - Insert Phase 6 section
2. `cli/src/main.rs` - Add `Acp` command variant
3. `cli/src/commands/mod.rs` - Export `acp` module
4. `cli/src/provenance.rs` - Add `client_name` (IDE), keep `agent_type` for agent
5. `cli/src/events.rs` - Add `client_name` to `AikiPostChangeEvent`
6. `cli/src/commands/hooks.rs` - Update IDE settings (Zed settings.json, etc.)
7. `cli/src/commands/doctor.rs` - Validate ACP configuration for all IDEs
8. `cli/src/authors.rs` - Display "agent-name (client-name)" format (e.g., "claude-code (zed)")
9. `cli/src/blame.rs` - Show both agent and client in output

---

## Installation Flow

```bash
$ aiki hooks install
✓ Git hooks installed
✓ Claude Code hooks configured
✓ Cursor hooks configured
✓ ACP proxy configured for supported IDEs

IDE Configuration:
  Updated ~/.config/zed/settings.json:
    - claude: Uses 'aiki acp claude-code'
    - gemini: Uses 'aiki acp gemini'
    - cursor: Uses 'aiki acp cursor'

Restart your IDE to activate.

$ aiki doctor
ACP Proxy:
  ✓ 'aiki' command found in PATH
  ✓ Zed: Claude configured to use 'aiki acp claude-code'
  ✓ Zed: Gemini configured to use 'aiki acp gemini'
  ✓ Zed: Cursor configured to use 'aiki acp cursor'
```

---

## Phase 6 Scope

**Must Have:**
- ✅ `aiki acp <agent-type> [--bin <path>]` subcommand (bidirectional proxy)
- ✅ Validate `agent-type` against `AgentType` enum
- ✅ Auto-detect client (IDE) from `InitializeRequest.clientInfo.name`
- ✅ Observe Agent → IDE messages (tool_call notifications)
- ✅ Intercept IDE → Agent messages (foundation for modification)
- ✅ Record provenance with both `client_name` (IDE) and `agent_type` (from enum)
- ✅ Auto-configure IDE settings via `aiki hooks install`
- ✅ Works with any ACP-compatible IDE (Zed, Neovim, etc.)
- ✅ Works with any agent in `AgentType` enum

**Future Phases:**
- Advanced prompt modification
- Context injection
- Policy enforcement

---

## Success Criteria

- ✅ `aiki acp <agent-type> [--bin <path>]` command exists and works
- ✅ Validates `agent-type` against `AgentType` enum (errors on invalid types)
- ✅ Bidirectional message forwarding (transparent)
- ✅ Auto-detects client (IDE) from `InitializeRequest.clientInfo.name`
- ✅ Detects tool_call notifications from agents
- ✅ Records provenance with `client_name` (IDE) and `agent_type` (from enum)
- ✅ 100% attribution accuracy
- ✅ `aiki hooks install` configures IDEs automatically
- ✅ `aiki doctor` validates ACP setup
- ✅ Non-blocking provenance recording
- ✅ Works with all ACP-compatible IDEs
- ✅ Works with all agents in `AgentType` enum

---

## Timeline

- ACP protocol types: 1 day
- `aiki acp` command + bidirectional proxy: 2-3 days
- Provenance integration (client_name, agent_type): 1 day
- Zed auto-configuration: 1 day
- Doctor validation: 1 day
- Testing: 2 days

**Total: 2 weeks**

---

## Why This Works

1. ✅ **Consistent with Aiki CLI** - Uses subcommand pattern (not separate binary)
2. ✅ **Single binary** - No wrapper script to install
3. ✅ **PATH handling** - As long as `aiki` is in PATH, works everywhere
4. ✅ **Full control** - Can modify prompts, observe all messages
5. ✅ **Future-proof** - Foundation for autonomous review, policy enforcement
6. ✅ **Enables dogfooding** - Use Aiki in Zed immediately
