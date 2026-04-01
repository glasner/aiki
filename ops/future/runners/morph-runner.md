# Morph Cloud Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [Morph Cloud](https://www.morphcloud.io/) SDK to run agents on snapshot-capable VMs with instant branching — enabling parallel agent exploration.

## What is Morph Cloud?

[Morph Cloud](https://www.morphcloud.io/) provides VMs with instant snapshot and branch capabilities. Fork an entire VM state in <250ms, enabling tree-of-thought execution patterns where multiple agent approaches run in parallel from the same checkpoint.

- **Sub-250ms from snapshot** — Restore entire VM state almost instantly
- **Infinibranch** — Fork a running VM into multiple parallel copies in milliseconds
- **Python and TypeScript SDKs** — `instance.exec()` returns exit_code/stdout/stderr directly
- **SSH access** — Full SSH via `instance.ssh()`

## `provision`

```python
import json, sys, os
from morphcloud.api import MorphCloudClient

request = json.load(sys.stdin)
config = request.get("config", {})
context = request.get("context", {})

client = MorphCloudClient()  # Reads MORPH_API_KEY from env

instance = client.instances.start(
    snapshot_id=config.get("snapshot", "base-ubuntu-22.04"),
    vcpus=config.get("vcpus", 2),
    memory=config.get("memory", 4096),
)
instance.wait_until_ready()

if context.get("repo_url"):
    instance.exec(f"git clone {context['repo_url']} /workspace")
    if context.get("branch"):
        instance.exec(f"cd /workspace && git checkout {context['branch']}")

for cmd in config.get("setup", []):
    instance.exec(cmd)

json.dump({
    "status": "ok",
    "environment": {"id": instance.id, "cwd": "/workspace"}
}, sys.stdout)
```

## `exec`

```python
import json, sys
from morphcloud.api import MorphCloudClient

request = json.load(sys.stdin)
env = request["environment"]
client = MorphCloudClient()
instance = client.instances.get(env["id"])

cmd = " ".join(request["command"])
if request.get("args"):
    cmd += " " + " ".join(request["args"])

result = instance.exec(cmd)

json.dump({
    "status": "ok", "exit_code": result.exit_code,
    "stdout": result.stdout, "stderr": result.stderr,
}, sys.stdout)
```

## `cleanup`

```python
import json, sys
from morphcloud.api import MorphCloudClient

request = json.load(sys.stdin)
client = MorphCloudClient()
instance = client.instances.get(request["environment"]["id"])
instance.stop()
json.dump({"status": "ok"}, sys.stdout)
```

## Configuration

```yaml
runner: morph
runners:
  morph:
    snapshot: aiki-agent-base
    vcpus: 2
    memory: 4096
    setup:
      - npm install -g @anthropic-ai/claude-code
    forward_env:
      - ANTHROPIC_API_KEY
    # Auth: reads MORPH_API_KEY from env
```

## Branching for Parallel Exploration (Advanced)

```python
# After provisioning, snapshot the setup state
snapshot = instance.snapshot()

# Branch into N parallel agent runs from the same starting point
for approach in approaches:
    branch = client.instances.start(snapshot_id=snapshot.id)
    branch.exec(f"claude --print '{approach}'")
```

## Limitations

- **Newer platform** — Smaller community than E2B, Modal, or Fly.io.
- **Pricing opacity** — MCU-based pricing not as transparent as per-vCPU-hour.
- **No self-hosting** — SaaS only.
