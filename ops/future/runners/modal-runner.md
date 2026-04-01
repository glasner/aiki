# Modal Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [Modal](https://modal.com/) SDK to run agents in gVisor-isolated sandboxes — the only runner with native GPU support for agent tasks.

## What is Modal?

[Modal](https://modal.com/) is a serverless compute platform with a first-class Sandbox API for running arbitrary code in isolated environments.

- **Sub-second cold starts** — Production-grade at massive scale
- **GPU + CPU sandboxes** — Only runner that can provide GPU for agent tasks
- **gVisor isolation** — User-space kernel (stronger than containers, lighter than full VMs). SOC 2 Type 2
- **Python SDK** — `Sandbox.create()`, `sandbox.exec()` with streaming stdout/stderr
- **Dynamic image composition** — Build images at runtime without Dockerfiles

## `provision`

```python
import json, sys
import modal

request = json.load(sys.stdin)
config = request.get("config", {})
context = request.get("context", {})

image = modal.Image.debian_slim()
for cmd in config.get("setup", []):
    image = image.run_commands(cmd)

sandbox = modal.Sandbox.create(
    image=image,
    cpu=config.get("cpu", 2),
    memory=config.get("memory", 4096),
    gpu=config.get("gpu"),
    timeout=config.get("timeout", 7200),
)

if context.get("repo_url"):
    sandbox.exec("git", "clone", context["repo_url"], "/workspace")
    if context.get("branch"):
        sandbox.exec("bash", "-c", f"cd /workspace && git checkout {context['branch']}")

json.dump({
    "status": "ok",
    "environment": {"id": sandbox.object_id, "cwd": "/workspace"}
}, sys.stdout)
```

## `exec`

```python
import json, sys
import modal

request = json.load(sys.stdin)
env = request["environment"]
sandbox = modal.Sandbox.from_id(env["id"])

process = sandbox.exec(*request["command"], *(request.get("args", [])))
stdout = process.stdout.read()
stderr = process.stderr.read()
process.wait()

json.dump({
    "status": "ok", "exit_code": process.returncode,
    "stdout": stdout, "stderr": stderr,
}, sys.stdout)
```

## `cleanup`

```python
import json, sys
import modal

request = json.load(sys.stdin)
sandbox = modal.Sandbox.from_id(request["environment"]["id"])
sandbox.terminate()
json.dump({"status": "ok"}, sys.stdout)
```

## Configuration

```yaml
runner: modal
runners:
  modal:
    cpu: 2
    memory: 4096             # MB
    gpu: null                # null, "T4", "A100", "H100"
    timeout: 7200            # seconds
    setup:
      - npm install -g @anthropic-ai/claude-code
    # Auth: reads MODAL_TOKEN_ID + MODAL_TOKEN_SECRET from env
```

## Limitations

- **No self-hosting** — SaaS only.
- **Python-first SDK** — TypeScript/Go SDKs exist but are newer.
- **No persistence** — Sandboxes are ephemeral.
- **GPU costs** — GPU sandboxes are significantly more expensive than CPU-only.
