# OpenComputer Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [OpenComputer](https://docs.opencomputer.dev/) SDK to run agents in full KVM-isolated VMs with snapshots, forking, and elastic resource scaling.

## What is OpenComputer?

[OpenComputer](https://docs.opencomputer.dev/) provides full Linux VMs in the cloud for AI agents. Each sandbox is a real virtual machine (not a container) with its own kernel, memory, and disk — hardware-level isolation via KVM. VMs are persistent, long-running, and can hibernate when idle.

- **KVM VM isolation** — Real VMs with own kernel, memory, and disk (not containers)
- **Snapshots & forking** — Named snapshots you can fork from. Try N approaches in parallel from the same starting point, like git branches for VMs.
- **Elasticity** — Dynamic resource scaling (resize CPU/memory on running VMs)
- **Persistent & long-running** — Sessions can run for hours or days, not minutes. No cold starts between steps.
- **Hibernation** — VMs can hibernate when idle to reduce costs
- **TypeScript SDK** — `@opencomputer/sdk` with Sandbox, Exec, Agent, Filesystem, PTY, Image, Snapshots, Secret Stores
- **Python SDK** — Same modules as TypeScript
- **CLI** — `oc sandbox`, `oc exec`, `oc shell`, `oc checkpoint`, `oc patch`, `oc preview`, `oc config`
- **Built-in agent support** — `sandbox.agent.start()` runs Claude inside the VM with full filesystem and shell access

## Why OpenComputer for aiki?

- **Snapshots + forking** — The killer feature. Snapshot after repo clone + dependency install, then fork N parallel agent runs from the same point. Similar to Morph Cloud's branching, but with full KVM VMs.
- **Elasticity** — Dynamically resize CPU/memory on running VMs. Scale up when an agent needs more resources for compilation, scale down during idle waits.
- **KVM isolation** — Strongest isolation tier (same as Morph Cloud, stronger than Docker-based runners like Daytona).
- **Persistent + hibernation** — VMs persist across sessions and can hibernate to save costs. Good for long-lived agent environments.
- **Rich SDK** — TypeScript and Python SDKs with exec, filesystem, PTY, snapshots, and agent-specific APIs.

## `provision`

Using the TypeScript SDK:

```typescript
import { Sandbox } from "@opencomputer/sdk";

const request = JSON.parse(await readStdin());
const config = request.config || {};
const context = request.context || {};

const sandbox = await Sandbox.create({
    // Resource configuration
    cpu: config.cpu || 2,
    memory: config.memory || 4096,
    disk: config.disk || 10240,
    // Or start from a snapshot
    snapshot: config.snapshot || undefined,
});

// Clone repo
if (context.repo_url) {
    await sandbox.exec(`git clone ${context.repo_url} /workspace`);
    if (context.branch) {
        await sandbox.exec(`cd /workspace && git checkout ${context.branch}`);
    }
}

// Run setup commands
for (const cmd of (config.setup || [])) {
    await sandbox.exec(cmd);
}

console.log(JSON.stringify({
    status: "ok",
    environment: {
        id: sandbox.id,
        cwd: "/workspace",
    }
}));
```

Or using the CLI:

```bash
SANDBOX_ID=$(oc sandbox create --json | jq -r '.id')

oc exec "$SANDBOX_ID" -- git clone "$REPO_URL" /workspace
oc exec "$SANDBOX_ID" -- bash -c "cd /workspace && git checkout $BRANCH"

for cmd in "${SETUP_COMMANDS[@]}"; do
    oc exec "$SANDBOX_ID" -- bash -c "$cmd"
done

echo '{"status":"ok","environment":{"id":"'$SANDBOX_ID'","cwd":"/workspace"}}'
```

## `exec`

```typescript
import { Sandbox } from "@opencomputer/sdk";

const request = JSON.parse(await readStdin());
const env = request.environment;

const sandbox = await Sandbox.connect(env.id);

const cmd = [...(request.command || []), ...(request.args || [])].join(" ");
const result = await sandbox.exec(cmd, {
    cwd: env.cwd || "/workspace",
    env: request.env,
    timeout: request.timeout || 7200000,
});

console.log(JSON.stringify({
    status: "ok",
    exit_code: result.exitCode,
    stdout: result.stdout,
    stderr: result.stderr,
}));
```

## `cleanup`

```typescript
import { Sandbox } from "@opencomputer/sdk";

const request = JSON.parse(await readStdin());
const env = request.environment;
const config = request.config || {};

const sandbox = await Sandbox.connect(env.id);

if (config.ephemeral) {
    await sandbox.kill();
} else {
    // Hibernate — preserves state, stops billing for CPU
    await sandbox.hibernate();
}

console.log(JSON.stringify({ status: "ok" }));
```

## Configuration

```yaml
runner: opencomputer
runners:
  opencomputer:
    cpu: 2
    memory: 4096             # MB
    disk: 10240              # MB
    ephemeral: true          # Kill after task, or hibernate to preserve state
    snapshot: null            # Start from a named snapshot (pre-built with agent tools)
    setup:
      - npm install -g @anthropic-ai/claude-code
      - pip install codex-cli
    forward_env:
      - ANTHROPIC_API_KEY
      - OPENAI_API_KEY
    # Auth: configured via `oc config` or OC_API_KEY env var
```

## Snapshots & Forking for Parallel Exploration

```typescript
// After provisioning with all dependencies installed, take a snapshot
const snapshot = await sandbox.snapshot("aiki-agent-base");

// Later: fork N parallel agent runs from the same starting point
for (const approach of approaches) {
    const fork = await Sandbox.create({ snapshot: "aiki-agent-base" });
    await fork.exec(`claude --print '${approach}'`);
    // Collect results, pick the best one
}
```

## Elasticity (Dynamic Resource Scaling)

```typescript
// Scale up before a heavy build step
await sandbox.resize({ cpu: 8, memory: 16384 });

// Run the build
await sandbox.exec("npm run build");

// Scale back down for lighter agent work
await sandbox.resize({ cpu: 2, memory: 4096 });
```

This is unique among runners — no other sandbox platform supports dynamic resource scaling on a running VM.

## OpenComputer vs Morph Cloud

Both support snapshot/fork patterns, but with different trade-offs:

| | OpenComputer | Morph Cloud |
|---|---|---|
| **Isolation** | KVM VM (own kernel) | VM (own kernel) |
| **Snapshots** | Yes (named) | Yes (instant) |
| **Forking** | Yes | Yes (sub-250ms) |
| **Elasticity** | Yes (dynamic resize) | No |
| **SDKs** | TypeScript, Python | Python, TypeScript |
| **CLI** | Yes (`oc`) | No |
| **Hibernation** | Yes (cost savings) | No |
| **Agent-specific API** | Yes (`sandbox.agent.start()`) | No |
| **Best for** | Elastic scaling, long-lived agents | Rapid branching, parallel exploration |

## Limitations

- **Newer platform** — Less community adoption than E2B, Modal, or Fly.io.
- **Pricing not public** — Not documented in available sources.
- **Docs access limited** — Some docs pages return 403. API surface may not be fully documented.
- **No self-hosting** — SaaS only.
