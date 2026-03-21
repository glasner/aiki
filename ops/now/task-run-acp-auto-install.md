# task run ACP Agent Auto-Install

## Problem

When a user runs `aiki task run`, if the ACP agent binary isn't installed (no Zed, no PATH binary), aiki fails with `AcpBinaryNotFound` / `AgentNotInstalled` and tells the user to install it manually. Users without Zed have no automated path.

## Solution

Add an auto-install step to the agent binary resolution chain. When neither Zed nor PATH provides the binary, aiki fetches the agent's metadata from the ACP registry, downloads/installs it to `$AIKI_HOME/agents/{id}/{version}/`, and uses it.

## Design

### Storage Layout

```
$AIKI_HOME/agents/
├── registry.json              # Cached registry (TTL: 1 hour)
├── claude-acp/
│   └── 0.22.2/
│       └── node_modules/...   # npx-installed package
├── codex-acp/
│   └── 0.10.0/
│       ├── codex-acp          # Downloaded native binary
│       └── ...
└── gemini/
    └── 0.34.0/
        └── node_modules/...   # npx-installed package
```

### Registry Integration

The ACP registry at `github.com/agentclientprotocol/registry` defines three distribution types per agent:

| Type | How to install | Runtime dep |
|------|---------------|-------------|
| `binary` | Download archive, extract | None |
| `npx` | `npm install --prefix <dir> <package>` | Node.js |
| `uvx` | `uv pip install <package>` | Python/uv |

Each agent has an `agent.json` like:

```json
{
  "id": "claude-acp",
  "version": "0.22.2",
  "distribution": {
    "npx": { "package": "@zed-industries/claude-agent-acp@0.22.2" }
  }
}
```

```json
{
  "id": "codex-acp",
  "version": "0.10.0",
  "distribution": {
    "binary": {
      "linux-x86_64": {
        "archive": "https://github.com/.../codex-acp-0.10.0-x86_64-unknown-linux-gnu.tar.gz",
        "cmd": "./codex-acp"
      }
    },
    "npx": { "package": "@zed-industries/codex-acp@0.10.0" }
  }
}
```

**Strategy**: Prefer `binary` when available for the current platform (no runtime deps). Fall back to `npx` if no binary exists. `uvx` as last resort.

### Agent-to-Registry ID Mapping

Add a method to `AgentType`:

```rust
impl AgentType {
    /// ACP registry ID for this agent
    fn registry_id(&self) -> Option<&'static str> {
        match self {
            AgentType::ClaudeCode => Some("claude-acp"),
            AgentType::Codex => Some("codex-acp"),
            AgentType::Gemini => Some("gemini"),
            _ => None,
        }
    }
}
```

### Resolution Chain Update

Current chain in `resolve_agent_binary()`:
1. Zed installation
2. System PATH
3. **Error** ← change this

New chain:
1. Zed installation
2. System PATH
3. **Aiki-managed installation** (`$AIKI_HOME/agents/{id}/{version}/`)
4. **Auto-install from registry** (download + install, then return path)

### Implementation Steps

#### Step 1: New module `src/agents/registry.rs`

Structs mirroring the ACP registry format:

```rust
/// ACP registry agent entry
struct RegistryAgent {
    id: String,
    name: String,
    version: String,
    distribution: Distribution,
}

struct Distribution {
    binary: Option<HashMap<String, BinaryTarget>>,
    npx: Option<NpxDistribution>,
    uvx: Option<UvxDistribution>,
}

struct BinaryTarget {
    archive: String,
    cmd: String,
    args: Option<Vec<String>>,
}

struct NpxDistribution {
    package: String,
    args: Option<Vec<String>>,
}
```

Functions:
- `fetch_agent_metadata(agent_id: &str) -> Result<RegistryAgent>` — fetches individual `agent.json` from GitHub raw content
- `current_platform() -> &str` — returns the ACP platform string (e.g., `linux-x86_64`)

#### Step 2: New module `src/agents/install.rs`

The actual installation logic:

```rust
/// Install an ACP agent to $AIKI_HOME/agents/{id}/{version}/
pub fn install_agent(agent: &RegistryAgent) -> Result<InstalledAgent> {
    let install_dir = global_aiki_dir()
        .join("agents")
        .join(&agent.id)
        .join(&agent.version);

    // Already installed?
    if install_dir.exists() {
        return resolve_installed(&install_dir, agent);
    }

    // Try binary first (no runtime deps)
    if let Some(binary) = &agent.distribution.binary {
        if let Some(target) = binary.get(current_platform()) {
            return install_binary(&install_dir, target);
        }
    }

    // Fall back to npx
    if let Some(npx) = &agent.distribution.npx {
        return install_npx(&install_dir, npx);
    }

    Err(...)
}
```

`install_binary()`:
1. Create temp dir
2. Download archive URL via `curl` or `reqwest`
3. Extract based on extension (`.tar.gz`, `.zip`)
4. Move to `install_dir`
5. Return `InstalledAgent { cmd, args }`

`install_npx()`:
1. Verify Node.js is installed (error if not — but this is the fallback path, binary is preferred)
2. Run `npm install --prefix {install_dir} {package}`
3. Find the entry point in `node_modules/.bin/` or `dist/index.js`
4. Return `InstalledAgent { cmd: "node", args: [entry_point] }`

#### Step 3: Update `resolve_agent_binary()` in `zed_detection.rs`

Add step 3 (aiki-managed) and step 4 (auto-install) before the `AcpBinaryNotFound` error:

```rust
pub fn resolve_agent_binary(agent_type: &str) -> Result<ResolvedBinary> {
    // 1. Zed installation (existing)
    // 2. PATH (existing)

    // 3. Aiki-managed installation
    if let Some(installed) = find_aiki_installed(agent_type)? {
        return Ok(installed);
    }

    // 4. Auto-install from registry
    eprintln!("  Installing {} agent...", agent_type);
    let registry_id = agent_registry_id(agent_type);
    let metadata = registry::fetch_agent_metadata(&registry_id)?;
    let installed = install::install_agent(&metadata)?;
    Ok(installed.into_resolved_binary())
}
```

#### Step 4: Update `is_installed()` in `AgentType`

Currently only checks `which`. Needs to also check `$AIKI_HOME/agents/`:

```rust
pub fn is_installed(&self) -> bool {
    // Check PATH
    if let Some(binary) = self.cli_binary() {
        if which::which(binary).is_ok() {
            return true;
        }
    }
    // Check aiki-managed
    if let Some(id) = self.registry_id() {
        if find_aiki_installed(id).ok().flatten().is_some() {
            return true;
        }
    }
    false
}
```

#### Step 5: Add `ResolvedBinary::AikiManaged` variant

```rust
pub enum ResolvedBinary {
    ZedNodeJs(PathBuf),
    ZedNative(PathBuf),
    InPath(String),
    AikiManaged { cmd: String, args: Vec<String> },
}
```

#### Step 6: User-facing output

During auto-install:
```
  Agent 'claude-acp' not found locally. Installing v0.22.2...
  Downloading @zed-industries/claude-agent-acp@0.22.2...
  Installed to ~/.aiki/agents/claude-acp/0.22.2/
```

#### Step 7: `aiki doctor` integration

Add a check: "ACP agents installed" that shows which agents are available and from which source (Zed, PATH, aiki-managed, not installed).

### What NOT to build (yet)

- Agent update mechanism (just re-install with new version)
- `aiki agent` subcommand (can add later)
- Registry caching with TTL (fetch fresh each time, it's small)
- uvx support (no major agent uses it yet)

### Dependencies

For binary downloads, use one of:
- `curl` via `Command::new("curl")` — no new Rust deps, available everywhere
- Add `reqwest` + `flate2` + `tar` to Cargo.toml — more robust but heavier

Recommendation: Use `curl` for downloads and `tar` CLI for extraction. Keeps deps minimal and these tools are universally available on dev machines.

### Error Handling

New error variants:
```rust
#[error("Failed to fetch agent registry for '{agent_id}': {reason}")]
RegistryFetchFailed { agent_id: String, reason: String },

#[error("Failed to install agent '{agent_id}': {reason}")]
AgentInstallFailed { agent_id: String, reason: String },

#[error("Node.js is required to install agent '{agent_id}' but was not found. Install Node.js or install the agent binary manually")]
NodeJsRequiredForAgent { agent_id: String },
```
