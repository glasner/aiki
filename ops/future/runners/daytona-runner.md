# Daytona Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [Daytona](https://www.daytona.io/) SDK or CLI to run agents in ephemeral sandboxes — purpose-built infrastructure for AI-generated code execution.

## What is Daytona?

[Daytona](https://www.daytona.io/) is a secure, elastic infrastructure platform for running AI-generated code. Originally an open-source development environment manager, it pivoted in early 2025 to become purpose-built infrastructure for AI agent sandboxes.

- **Sub-90ms provisioning** — Warm sandbox pool enables near-instant starts
- **Full SDK** — Python, TypeScript, Ruby, Go SDKs for programmatic sandbox management
- **Native git support** — `sandbox.git.clone()` built into the SDK
- **Shell exec with stdout/stderr** — `sandbox.process.exec()` returns exit code, stdout, stderr
- **Stateful sandboxes** — Stop, archive, resume indefinitely
- **Volumes** — FUSE-based persistent storage shared across sandboxes
- **Snapshots** — Pre-built images for instant-start sandboxes
- **Open source** — Apache 2.0. Self-hostable via Docker Compose or Kubernetes
- **Pricing** — $0.0504/vCPU-hr, $0.0162/GiB-hr memory. $200 free credits on signup

## Why Daytona for aiki?

- **Agent-native** — Designed for AI agents, not retrofitted from dev environments
- **Fastest SDK-driven provisioning** — Sub-90ms from warm pool
- **Rich SDK** — `process.exec()`, `git.clone()`, `fs.upload_file()` — all the primitives the runner needs
- **Ephemeral + persistent** — Auto-delete on stop for one-shot tasks or persist for long-lived environments
- **Open source** — Can self-host for full control, or use the managed SaaS

## `provision`

Using the Python SDK (recommended):

```python
import json, sys, os
from daytona import Daytona, CreateSandboxFromImageParams, Image, Resources

request = json.load(sys.stdin)
config = request.get("config", {})
context = request.get("context", {})

daytona = Daytona()  # Reads DAYTONA_API_KEY from env

sandbox = daytona.create(
    CreateSandboxFromImageParams(
        image=config.get("image", Image.debian_slim("3.12")),
        resources=Resources(
            cpu=config.get("cpu", 2),
            memory=config.get("memory", 4),
            disk=config.get("disk", 8),
        ),
        env_vars={k: os.environ.get(k, "") for k in config.get("forward_env", [])},
        auto_stop_interval=0,
        ephemeral=config.get("ephemeral", True),
    )
)

if context.get("repo_url"):
    sandbox.git.clone(context["repo_url"], "/home/daytona/workspace")
    if context.get("branch"):
        sandbox.process.exec(f"git checkout {context['branch']}", cwd="/home/daytona/workspace")

for cmd in config.get("setup", []):
    sandbox.process.exec(cmd, cwd="/home/daytona/workspace")

json.dump({
    "status": "ok",
    "environment": {"id": sandbox.id, "cwd": "/home/daytona/workspace"}
}, sys.stdout)
```

## `exec`

```python
import json, sys
from daytona import Daytona

request = json.load(sys.stdin)
env = request["environment"]

daytona = Daytona()
sandbox = daytona.get(env["id"])

cmd = " ".join(request["command"])
if request.get("args"):
    cmd += " " + " ".join(request["args"])

response = sandbox.process.exec(
    cmd, cwd=env.get("cwd", "/home/daytona/workspace"),
    timeout=request.get("timeout", 7200), env=request.get("env", {}),
)

json.dump({
    "status": "ok", "exit_code": response.exit_code,
    "stdout": response.result, "stderr": "",
}, sys.stdout)
```

## `cleanup`

```python
import json, sys
from daytona import Daytona

request = json.load(sys.stdin)
env = request["environment"]
config = request.get("config", {})

daytona = Daytona()
sandbox = daytona.get(env["id"])

if config.get("ephemeral", True):
    sandbox.delete()
else:
    sandbox.stop()

json.dump({"status": "ok"}, sys.stdout)
```

## Configuration

```yaml
runner: daytona
runners:
  daytona:
    cpu: 2               # vCPUs (max 4 default)
    memory: 4            # GiB (max 8 default)
    disk: 8              # GiB (max 10 default)
    ephemeral: true
    image: null          # Default (Debian slim) or any Docker/OCI image
    snapshot: null       # Pre-built snapshot for instant starts
    setup:
      - npm install -g @anthropic-ai/claude-code
      - pip install codex-cli
    forward_env:
      - ANTHROPIC_API_KEY
      - OPENAI_API_KEY
    # Auth: reads DAYTONA_API_KEY from env
    # Self-hosted: set DAYTONA_API_URL to your instance
```

## Limitations

- **Docker-level isolation** — Shared kernel with host. Not as strong as Firecracker microVMs.
- **SDK is Alpha** — API surface may change.
- **Default resource limits** — Max 4 vCPUs / 8 GB RAM per sandbox.
- **No streaming stdout** — Must wait for completion.
