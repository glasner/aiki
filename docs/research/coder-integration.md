# Coder.com Integration Research

## Overview

This document captures deep research on [Coder](https://coder.com/) — an open-source, self-hosted platform for governed Cloud Development Environments (CDEs) — and identifies integration points with Aiki's AI-native task tracking and agent orchestration system.

---

## Coder Platform Summary

### What Coder Does

Coder provisions isolated, reproducible development workspaces on your infrastructure (cloud, on-prem, or air-gapped) using **Terraform templates**. It moves development off local machines into standardized remote environments.

### Architecture

```
Users/IDEs ──→ coderd (API + Dashboard) ──→ PostgreSQL
                     │
               Provisioners (Terraform) ──→ Cloud APIs (AWS/Azure/GCP/K8s/Docker)
                     │
               Workspace Agent (inside workspace) ←──→ User's IDE via SSH/HTTPS
```

**Components:**
- **Control Plane (`coderd`)**: Central service providing REST API, dashboard UI, and built-in Terraform provisioner daemons. Requires external PostgreSQL.
- **Workspace Agent**: Service running inside each workspace providing SSH, port forwarding, workspace app serving.
- **Provisioners**: Terraform runners (built-in or external) executing `terraform apply/destroy` for workspace builds.
- **Workspace Proxies** (Premium): Relay connections to reduce latency for distributed teams.

### Key Features (as of 2025-2026)

| Feature | Description |
|---------|-------------|
| **Workspace Management** | Create, start, stop, delete workspaces via dashboard, CLI, or REST API |
| **Terraform Templates** | Define workspace infrastructure as code (Docker, K8s, VMs, cloud instances) |
| **Coder Tasks** (v2.27+) | Headless AI agent execution with lifecycle management, pause/resume, task reporting |
| **AI Bridge** | Centralized LLM gateway with auth, model access control, usage logging across providers |
| **Agent Boundaries** | Process-level firewalls restricting agent network access, tool permissions, audit logging |
| **External Auth** | OAuth2 integration with Git providers (GitHub, GitLab, Bitbucket) with automatic token injection |
| **Coder Registry** | Pre-built modules for Claude Code, Goose, Codex, Gemini CLI, and more |
| **Prebuilt Workspaces** | Zero-wait workspace starts via pre-provisioned environments |
| **Dynamic Parameters** | Smart workspace creation forms that adapt to user input and enforce policy |
| **Coder Desktop** | Local companion app connecting remote workspaces to native tools |

---

## API & Integration Surface

### REST API

Full CRUD at `/api/v2/`, authenticated via `Coder-Session-Token` header.

**Key endpoint categories:** Agents, Applications, Audit, Authentication, Authorization, Builds, Files, Git, Insights, Members, Organizations, Templates, Users, WorkspaceProxies, Workspaces.

**Workspace lifecycle endpoints:**
```
POST   /api/v2/organizations/{org}/members/{user}/workspaces  # Create
POST   /api/v2/workspaces/{id}/builds  (transition: "start"|"stop")  # Start/Stop
GET    /api/v2/workspaces/{id}  # Status
DELETE /api/v2/workspaces/{id}  # Cleanup
```

**Tasks API (experimental):**
```
/api/experimental/tasks/
```

### Go SDK

- **`codersdk`** (`github.com/coder/coder/v2/codersdk`): Full API coverage with rich `Client` type
- **`toolsdk`** (`github.com/coder/coder/v2/codersdk/toolsdk`): For building AI tool integrations
- **`acp-go-sdk`** (`github.com/coder/acp-go-sdk`): Agent Client Protocol SDK

### CLI

The `coder` CLI supports non-interactive operation with session tokens (`CODER_SESSION_TOKEN` env var), suitable for CI/CD and scripting.

Key commands:
- `coder create/start/stop/delete` — workspace management
- `coder ssh` — SSH into workspace
- `coder tasks` — manage AI agent tasks
- `coder templates init/push/pull` — manage templates
- `coder external-auth access-token` — retrieve OAuth tokens

### Terraform Providers

1. **`coder/coder`** (in-template): Resources for workspace definition (`coder_agent`, `coder_app`, `coder_script`, `coder_ai_task`, `coder_env`, `coder_metadata`)
2. **`coder/coderd`** (deployment management): Manage templates, users, groups, organizations via GitOps/CI/CD

### Webhooks

Webhook-based notifications via `CODER_NOTIFICATIONS_WEBHOOK_ENDPOINT`. Covers workspace events, template updates, system notifications.

### Workspace Lifecycle Hooks

Via `coder_script` Terraform resource:
- `run_on_start` — install dependencies, clone repos, start services
- `run_on_stop` — cleanup tasks
- `cron` — scheduled maintenance
- `start_blocks_login` — wait for critical services before allowing user login

### External Auth / OAuth

OAuth2.0 for any provider. Automatic Git credential injection via `GIT_ASKPASS`. Token retrieval from within workspaces:
```bash
curl "${CODER_AGENT_URL}api/v2/workspaceagents/me/external-auth?id=<provider-id>" \
  -H "Coder-Session-Token: ${CODER_AGENT_TOKEN}"
```

---

## Coder Tasks & AI Agent Support

### Coder Tasks Architecture

Each task runs inside its own Coder workspace for isolation. A template becomes task-capable by defining a `coder_ai_task` resource.

**Key capabilities:**
- Headless agent execution with full lifecycle control
- Pause/resume on idle timeout (saves compute)
- Status reporting via AgentAPI
- CLI + REST API management (v2.27+)
- Any MCP-compatible agent can be integrated

### Claude Code Module

Available at [registry.coder.com/modules/coder/claude-code](https://registry.coder.com/modules/coder/claude-code):

```hcl
module "claude-code" {
  source        = "registry.coder.com/coder/claude-code/coder"
  version       = "4.8.1"
  agent_id      = coder_agent.main.id
  workdir       = "/home/coder/project"
  claude_api_key = var.anthropic_api_key
}
```

Features:
- AgentAPI integration for task reporting in Coder UI
- AI Bridge support (`enable_aibridge = true`) — auto-injects `ANTHROPIC_BASE_URL` and `CLAUDE_API_KEY`
- Agent Boundary support (`enable_boundary = true`) — network-level access control
- State persistence across workspace restarts (agentapi >= v0.12.0)
- Uses `--dangerously-skip-permissions` flag (trusted environments only)

### AI Bridge

Centralized LLM gateway:
- Authentication and model access control
- Usage logging across providers (Anthropic, OpenAI, Bedrock, Ollama)
- Auto-injects MCP server tools into LLM calls
- Full audit logging of prompts, tool calls, and token usage

### Agent Boundaries

Process-level firewalls for agents:
- Network policy enforcement (block domains, HTTP verbs)
- Tool restrictions
- Audit logging to workspace
- Configuration via inline module config or external `config.yaml`

### AgentAPI

Open-source Go HTTP server ([github.com/coder/agentapi](https://github.com/coder/agentapi)) that controls coding agents through terminal emulation:
- `POST /message` — send messages to agent
- `GET /status` — check agent status (stable/running)
- `GET /events` — SSE stream for real-time updates
- Built-in chat UI at `http://localhost:3284/chat`
- Works by running an in-memory terminal emulator, translating API calls to terminal keystrokes

### Tool SDK (`codersdk/toolsdk`)

Pre-built Go tool definitions for AI/agent integration:
- `ToolNameReportTask` — report task status back to Coder
- `ToolNameGetWorkspace` / `ToolNameCreateWorkspace` / `ToolNameListWorkspaces` — workspace management
- `ToolNameListTemplates` — template discovery
- Designed for building custom AI tool integrations with Coder

### GitHub Actions Integration

Coder provides a workflow for issue-to-PR automation:
1. Label a GitHub issue with "coder"
2. GitHub Action triggers, launches a Coder Task with Claude Code
3. Agent reads issue description, comments, and context from GitHub
4. Agent works on the issue and opens a PR
5. Full lifecycle managed via Coder Tasks API

---

## Integration Points with Aiki

### 1. Workspace as Agent Session Host (High Priority)

**What:** Aiki sessions currently use local JJ workspaces at `/tmp/aiki/{repo-id}/{session-id}/`. Coder workspaces could serve as the execution environment.

**How:**
- Use Coder REST API to spin up a workspace per Aiki session
- Map Aiki `session_id` → Coder workspace ID for lifecycle management
- Use `coder_script` with `run_on_start` to bootstrap JJ + Aiki in each workspace
- Aiki's session isolation model maps directly to Coder's workspace isolation

**Integration surface:**
- `src/session/isolation.rs` — Add `CoderWorkspace` isolation backend
- `src/agents/runtime/` — Add `CoderRuntime` for spawning agents inside Coder workspaces

### 2. Coder Tasks ↔ Aiki Tasks Bridge (High Priority)

**What:** Bridge Aiki's rich task system (DAGs, lanes, templates, status) with Coder Tasks' headless execution.

**How:**
- When `aiki task run` executes, create a corresponding Coder Task
- Use `coder_ai_task` Terraform resource in Aiki-provisioned templates
- Report Aiki task status back to Coder dashboard via AgentAPI
- Leverage Coder's task pause/resume for long-running `aiki build` pipelines

**Integration surface:**
- `src/tasks/runner.rs` / `src/tasks/spawner.rs` — Add Coder Tasks execution backend
- `src/commands/build.rs` — Option to distribute build lanes across Coder workspaces

### 3. Headless Lane Orchestration via Coder API (High Priority)

**What:** Aiki's `aiki build` decomposes plans into parallel task lanes. Coder's API enables programmatic workspace management.

**How:**
- `aiki build --backend coder` creates N Coder workspaces (one per lane)
- Each workspace runs an agent with Aiki pre-installed
- Aiki's lane orchestrator (`src/tasks/lanes.rs`) manages dependencies across workspaces
- Lifecycle: create workspace → run task → collect results → delete workspace

### 4. Pre-built Coder Template with Aiki (High Priority, Low Effort)

**What:** A Coder template that includes Aiki, JJ, and agent runtimes pre-configured.

```hcl
resource "coder_agent" "main" {
  os   = "linux"
  arch = "amd64"
}

resource "coder_script" "install_aiki" {
  agent_id           = coder_agent.main.id
  display_name       = "Install Aiki"
  script             = <<-EOF
    brew install glasner/tap/aiki
    aiki init
  EOF
  run_on_start       = true
  start_blocks_login = true
}

module "claude-code" {
  source   = "registry.coder.com/coder/claude-code/coder"
  version  = "4.8.1"
  agent_id = coder_agent.main.id
  workdir  = "/home/coder/project"
}

resource "coder_ai_task" "aiki" {
  agent_id = coder_agent.main.id
}
```

### 5. AI Bridge Auto-Detection in Flows (Medium Priority, Low Effort)

**What:** When running inside a Coder workspace, Aiki agents automatically get LLM credentials from AI Bridge.

**How:**
- Detect Coder environment via `CODER_AGENT_URL` / `CODER_AGENT_TOKEN` env vars
- Aiki's flow engine (`src/flows/engine.rs`) adapts behavior when in Coder context
- AI Bridge audit logging complements Aiki's provenance tracking

### 6. Agent Boundaries + Aiki Provenance (Medium Priority, Low Effort)

**What:** Combine Coder's network-level restrictions with Aiki's change-level attribution.

**How:**
- Coder logs boundary violations (what agent *tried*)
- Aiki tracks actual code changes (what agent *did*)
- Together: complete governance story

### 7. External Auth Passthrough (Medium Priority, Trivial Effort)

**What:** Inside Coder workspaces, Git auth is automatic via `coder_external_auth` and `GIT_ASKPASS`.

**How:** Aiki's JJ operations inherit Git auth transparently. Token refresh handled by Coder agent process.

### 8. Coder Registry Module for Aiki (Medium Priority)

**What:** Publish an `aiki` module to [registry.coder.com](https://registry.coder.com).

```hcl
module "aiki" {
  source   = "registry.coder.com/glasner/aiki/coder"
  agent_id = coder_agent.main.id
  workdir  = "/home/coder/project"
}
```

### 9. MCP Server for Aiki Tasks (Lower Priority, High Effort)

**What:** Expose Aiki task operations (create, status, link, run) as MCP tools.

**How:**
- Create `aiki-mcp-server` exposing task management operations
- AI agents in Coder workspaces call Aiki task management via MCP
- Enables agents to self-organize work using Aiki's task DAG

---

## Integration Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Aiki CLI (orchestrator)                                     │
│  ├─ aiki build --backend coder                             │
│  │   ├─ Creates Coder workspaces per lane                  │
│  │   ├─ Each workspace: JJ + Aiki + Agent                 │
│  │   └─ Collects results, merges changes                   │
│  ├─ aiki task run --on coder                               │
│  │   ├─ Maps to Coder Task                                 │
│  │   └─ Reports status via AgentAPI                        │
│  └─ aiki session --provider coder                          │
│      ├─ Workspace = session isolation                       │
│      └─ Auto-cleanup on session end                        │
├─────────────────────────────────────────────────────────────┤
│ Coder Platform                                              │
│  ├─ Workspace provisioning (Terraform)                     │
│  ├─ AI Bridge (LLM routing + governance)                  │
│  ├─ Agent Boundaries (network policies)                    │
│  ├─ External Auth (Git tokens)                             │
│  └─ Task UI (status, chat, pause/resume)                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Priority Ranking

| # | Integration | Value | Effort | Quick Win? |
|---|------------|-------|--------|------------|
| 1 | Coder Tasks ↔ Aiki Tasks bridge | Very High | Medium | No |
| 2 | Workspace-per-session isolation backend | Very High | High | No |
| 3 | Headless lane orchestration via Coder API | High | Medium | No |
| 4 | Pre-built Coder template with Aiki | High | Low | **Yes** |
| 5 | AI Bridge auto-detection in flows | Medium | Low | **Yes** |
| 6 | Agent Boundaries + Provenance combo | Medium | Low | **Yes** |
| 7 | External Auth passthrough | Medium | Trivial | **Yes** |
| 8 | Coder Registry module | Medium | Medium | No |
| 9 | MCP server for Aiki tasks | Lower | High | No |

---

## Sources

- [Coder Homepage](https://coder.com/)
- [Coder Docs](https://coder.com/docs)
- [Coder Architecture](https://coder.com/docs/admin/infrastructure/architecture)
- [Coder Tasks Documentation](https://coder.com/docs/ai-coder/tasks)
- [Agent Boundaries Documentation](https://coder.com/docs/ai-coder/agent-boundary)
- [Workspace Lifecycle](https://coder.com/docs/user-guides/workspace-lifecycle)
- [Workspace Management](https://coder.com/docs/user-guides/workspace-management)
- [Automate Coder Tasks via CLI and API](https://coder.com/blog/automate-coder-tasks-via-cli-and-api)
- [Coder Automation Guide](https://coder.com/docs/admin/automation)
- [Workspaces API Reference](https://coder.com/docs/reference/api/workspaces)
- [Extending Templates](https://coder.com/docs/admin/templates/extending-templates)
- [Terraform Modules](https://coder.com/docs/admin/templates/extending-templates/modules)
- [Claude Code Module - Coder Registry](https://registry.coder.com/modules/coder/claude-code)
- [External Auth for Git Providers](https://coder.com/docs/admin/external-auth)
- [Git API Reference](https://coder.com/docs/reference/api/git)
- [Coder Launch Week Recap](https://coder.com/blog/launch-dec-recap)
- [GitHub Issue to PR: Coder Tasks + Claude Code](https://coder.com/blog/launch-dec-2025-coder-tasks)
- [Coder Terraform Provider](https://registry.terraform.io/providers/coder/coder/latest/docs)
- [Coder coderd Terraform Provider](https://registry.terraform.io/providers/coder/coderd/latest/docs)
- [Go SDK: codersdk](https://pkg.go.dev/github.com/coder/coder/v2@v2.10.2/codersdk)
- [Go SDK: toolsdk](https://pkg.go.dev/github.com/coder/coder/v2/codersdk/toolsdk)
- [GitHub: coder/coder](https://github.com/coder/coder)
- [Coder 2.30 Changelog](https://coder.com/changelog/coder-2-30)
- [Governed Workspaces for AI Coding Agents](https://coder.com/products/workspaces)
