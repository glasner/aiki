# E2B Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [E2B](https://e2b.dev/) SDK to run agents in Firecracker microVM sandboxes — the strongest isolation of any sandbox-style runner.

## What is E2B?

[E2B](https://e2b.dev/) is an open-source (Apache 2.0) platform for running AI-generated code in Firecracker microVM sandboxes. Each sandbox has its own kernel — hardware-level isolation, not shared-kernel containers.

- **Sub-200ms provisioning** — Firecracker snapshot restore
- **Firecracker microVM isolation** — Each sandbox has its own kernel
- **Python and TypeScript SDKs** — `sandbox.commands.run()` returns stdout/stderr/exit_code directly
- **Custom templates** — Build from Dockerfiles, snapshot for instant starts
- **Streaming stdout/stderr** — Real-time output streaming during command execution
- **24-hour max session** — Pro plan extends but sessions are time-limited
- **Open source** — Apache 2.0. Self-hostable (Terraform for GCP/AWS)

## `provision`

```python
import json, sys, os
from e2b_code_interpreter import Sandbox

request = json.load(sys.stdin)
config = request.get("config", {})
context = request.get("context", {})

sandbox = Sandbox(
    template=config.get("template", "base"),
    timeout=config.get("timeout", 3600),
    env_vars={k: os.environ.get(k, "") for k in config.get("forward_env", [])},
)

if context.get("repo_url"):
    sandbox.commands.run(f"git clone {context['repo_url']} /home/user/workspace")
    if context.get("branch"):
        sandbox.commands.run(f"cd /home/user/workspace && git checkout {context['branch']}")

for cmd in config.get("setup", []):
    sandbox.commands.run(cmd, cwd="/home/user/workspace")

json.dump({
    "status": "ok",
    "environment": {"id": sandbox.sandbox_id, "cwd": "/home/user/workspace"}
}, sys.stdout)
```

## `exec`

```python
import json, sys
from e2b_code_interpreter import Sandbox

request = json.load(sys.stdin)
env = request["environment"]
sandbox = Sandbox.reconnect(env["id"])

cmd = " ".join(request["command"])
if request.get("args"):
    cmd += " " + " ".join(request["args"])

result = sandbox.commands.run(
    cmd, cwd=env.get("cwd", "/home/user/workspace"),
    env_vars=request.get("env", {}), timeout=request.get("timeout", 7200),
)

json.dump({
    "status": "ok", "exit_code": result.exit_code,
    "stdout": result.stdout, "stderr": result.stderr,
}, sys.stdout)
```

## `cleanup`

```python
import json, sys
from e2b_code_interpreter import Sandbox

request = json.load(sys.stdin)
sandbox = Sandbox.reconnect(request["environment"]["id"])
sandbox.close()
json.dump({"status": "ok"}, sys.stdout)
```

## Configuration

```yaml
runner: e2b
runners:
  e2b:
    template: base           # Custom template name (build from Dockerfile)
    timeout: 3600            # Sandbox max lifetime in seconds
    setup:
      - npm install -g @anthropic-ai/claude-code
    forward_env:
      - ANTHROPIC_API_KEY
      - OPENAI_API_KEY
    # Auth: reads E2B_API_KEY from env
```

## Limitations

- **24-hour max session** — Sessions expire. Long-running agents must checkpoint and resume.
- **No persistence** — All state lost when sandbox closes.
- **Template build required** — Custom environments require building and pushing a Dockerfile template.
- **SaaS dependency** — Self-hosting is possible but less mature than Daytona's Docker Compose path.
