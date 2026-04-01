# Vercel Sandbox Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the Vercel Sandbox CLI (`sandbox`) or SDK (`@vercel/sandbox`) to run agents in ephemeral Firecracker microVMs on Vercel's infrastructure.

## What is Vercel Sandbox?

[Vercel Sandbox](https://vercel.com/sandbox) is an ephemeral compute primitive for safely running untrusted code. Each sandbox is a Firecracker microVM (Amazon Linux 2023) with its own filesystem, process tree, and networking. Sandboxes spin up in milliseconds and run for up to 5 hours (Pro/Enterprise) or 45 minutes (Hobby).

- Up to 8 vCPUs, 2 GB RAM per vCPU
- Runtimes: node22, node24, python3.13
- Pre-installed: git, curl, tar, dnf (for installing system packages)
- sudo access available
- Usage-based pricing: $0.128/CPU-hr, $0.0106/GB-hr memory
- SDK (`@vercel/sandbox`) and CLI (`sandbox`) available
- Auth: Vercel OIDC tokens or access tokens (VERCEL_TOKEN + team/project IDs)

## Why Vercel Sandbox for aiki?

- **Zero infrastructure** — No self-hosted servers, no SSH keys, no Terraform templates. Just a Vercel account.
- **Millisecond provisioning** — Sandboxes start faster than exe.dev VMs (~2s) or Coder workspaces (30s+).
- **Native agent support** — Vercel ships a [coding agent template](https://github.com/vercel-labs/coding-agent-template) supporting Claude Code, Codex, and others.
- **Ephemeral by default** — Sandboxes auto-destroy on timeout. No cleanup needed.

## `provision`

```bash
# Create sandbox with the Sandbox CLI
SANDBOX_ID=$(sandbox create \
    --runtime node22 \
    --vcpus 4 \
    --timeout 300 \
    --json | jq -r '.id')

# Clone repo into sandbox
sandbox exec "$SANDBOX_ID" -- \
    git clone "$REPO_URL" /vercel/sandbox/workspace

sandbox exec "$SANDBOX_ID" -- \
    bash -c "cd /vercel/sandbox/workspace && git checkout $BRANCH"

# Run setup commands (install agent CLI)
for cmd in "${SETUP_COMMANDS[@]}"; do
    sandbox exec "$SANDBOX_ID" -- bash -c "$cmd"
done

echo '{"status":"ok","environment":{"id":"'$SANDBOX_ID'","cwd":"/vercel/sandbox/workspace"}}'
```

Or via the SDK (for a Node.js-based runner):
```typescript
import { Sandbox } from "@vercel/sandbox";

const sandbox = await Sandbox.create({
    source: { url: repoUrl, type: "git" },
    resources: { vcpus: 4 },
    timeout: 300_000, // 5 minutes
    runtime: "node22",
});

// Install agent CLI
await sandbox.runCommand({
    cmd: "npm", args: ["install", "-g", "@anthropic-ai/claude-code"],
    sudo: true,
});
```

## `exec`

```bash
# Execute agent command inside sandbox
sandbox exec "$SANDBOX_ID" -- \
    bash -c "cd $CWD && $ENV_EXPORTS ${COMMAND[*]}" \
    > /tmp/stdout 2> /tmp/stderr
EXIT_CODE=$?

jq -n --arg stdout "$(cat /tmp/stdout)" \
      --arg stderr "$(cat /tmp/stderr)" \
      --argjson exit_code "$EXIT_CODE" \
      '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":$stderr}'
```

## `cleanup`

```bash
# Sandboxes auto-destroy on timeout, but explicit stop is cleaner
sandbox stop "$SANDBOX_ID" 2>/dev/null || true
echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: vercel
runners:
  vercel:
    runtime: node22      # or python3.13, node24
    vcpus: 4             # 1-8
    timeout: 300          # seconds (max 18000 for Pro)
    setup:
      - npm install -g @anthropic-ai/claude-code
    # Auth: reads VERCEL_TOKEN, VERCEL_TEAM_ID, VERCEL_PROJECT_ID from env
```

## Limitations

- **Max 5 hours** — Long-running tasks may hit the timeout ceiling. exe.dev and Coder have no inherent time limit.
- **No persistent storage** — Sandboxes are truly ephemeral. All state must be pushed via git before timeout.
- **Vercel account required** — Tied to Vercel's platform and billing.
- **Limited system packages** — Amazon Linux 2023 base. Some tools may need `dnf install`.
