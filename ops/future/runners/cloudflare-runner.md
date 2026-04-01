# Cloudflare Sandbox Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [Cloudflare Sandbox SDK](https://developers.cloudflare.com/sandbox/) to run agents in isolated containers on Cloudflare's edge network — with active-usage CPU billing and scale-to-zero.

## What is Cloudflare Sandbox?

[Cloudflare Sandbox](https://developers.cloudflare.com/sandbox/) is a TypeScript SDK (`@cloudflare/sandbox`) for building secure code execution environments on Cloudflare's infrastructure. Each sandbox is an isolated container (Ubuntu 22.04) backed by Durable Objects for persistent identity and Cloudflare Containers for execution.

- **Active-usage CPU billing** — Pay only for CPU cycles actually consumed (20% utilization = 20% cost)
- **Scale to zero** — Containers auto-sleep after idle timeout
- **Streaming exec** — `exec()`, `execStream()`, and `startProcess()` with real-time stdout/stderr
- **Built-in git** — `sandbox.gitCheckout()` for native repo cloning
- **Instance types** — From `lite` (1/16 vCPU, 256 MiB) to `standard-4` (4 vCPU, 12 GiB)
- **Beta** — v0.7.0, TypeScript SDK only
- **Pricing** — $0.000020/vCPU-second (active), $0.0000025/GiB-second memory

## Different Architecture: Worker as Control Plane

The Cloudflare Sandbox SDK **only works from within a Cloudflare Worker**. This means the runner has a **two-layer architecture**:

```
┌────────────────┐     ┌──────────────────────┐     ┌─────────────────────┐
│                │     │                      │     │                     │
│  aiki-runner-  │────→│  CF Worker           │────→│  CF Sandbox         │
│  cloudflare    │HTTP │  (your control plane)│SDK  │  (isolated Linux    │
│  (local exec)  │     │  deployed on CF edge │     │   container)        │
│                │     │                      │     │                     │
└────────────────┘     └──────────────────────┘     └─────────────────────┘
```

Deploy the Worker once, then the runner plugin just makes HTTP calls.

## The Worker (Deploy Once)

```typescript
import { getSandbox, proxyToSandbox, type Sandbox } from '@cloudflare/sandbox';
export { Sandbox } from '@cloudflare/sandbox';

type Env = { Sandbox: DurableObjectNamespace<Sandbox> };

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const proxyResponse = await proxyToSandbox(request, env);
    if (proxyResponse) return proxyResponse;

    const url = new URL(request.url);
    const auth = request.headers.get('Authorization');
    if (auth !== `Bearer ${env.RUNNER_SECRET}`) {
      return new Response('Unauthorized', { status: 401 });
    }

    const body = await request.json() as any;

    if (url.pathname === '/provision') {
      const sandboxId = body.context?.task_id || crypto.randomUUID();
      const sandbox = getSandbox(env.Sandbox, sandboxId, {
        keepAlive: true,
        sleepAfter: body.config?.timeout_ms || 3600000,
      });
      if (body.context?.repo_url) {
        await sandbox.gitCheckout(body.context.repo_url, {
          branch: body.context.branch, targetDir: '/workspace',
        });
      }
      for (const cmd of (body.config?.setup || [])) {
        await sandbox.exec(cmd, { cwd: '/workspace' });
      }
      return Response.json({
        status: 'ok', environment: { id: sandboxId, cwd: '/workspace' },
      });
    }

    if (url.pathname === '/exec') {
      const sandbox = getSandbox(env.Sandbox, body.environment.id, { keepAlive: true });
      const cmd = [...(body.command || []), ...(body.args || [])].join(' ');
      const result = await sandbox.exec(cmd, {
        cwd: body.environment.cwd || '/workspace', env: body.env,
        timeout: body.timeout || 7200000,
      });
      return Response.json({
        status: 'ok', exit_code: result.exitCode,
        stdout: result.stdout, stderr: result.stderr,
      });
    }

    if (url.pathname === '/cleanup') {
      const sandbox = getSandbox(env.Sandbox, body.environment.id);
      await sandbox.destroy();
      return Response.json({ status: 'ok' });
    }

    if (url.pathname === '/status') {
      return Response.json({
        status: 'ok', name: 'cloudflare', version: '0.1.0',
        description: 'Cloudflare Sandbox runner',
      });
    }

    return new Response('Not found', { status: 404 });
  },
};
```

## `provision` / `exec` / `cleanup`

The runner plugin makes HTTP calls to the deployed Worker:

```bash
RESPONSE=$(curl -s -X POST "$WORKER_URL/provision" \
    -H "Authorization: Bearer $SECRET" \
    -H "Content-Type: application/json" \
    -d "{\"config\": $(echo "$CONFIG_JSON"), \"context\": {\"task_id\": \"$TASK_ID\", \"repo_url\": \"$REPO_URL\", \"branch\": \"$BRANCH\"}}")
echo "$RESPONSE"
```

Same pattern for `/exec` and `/cleanup`.

## Configuration

```yaml
runner: cloudflare
runners:
  cloudflare:
    worker_url: https://aiki-runner.your-account.workers.dev
    secret_env: CLOUDFLARE_RUNNER_SECRET
    instance_type: standard-1     # lite | basic | standard-1 | standard-2 | standard-4
    setup:
      - npm install -g @anthropic-ai/claude-code
    timeout_ms: 3600000
    enable_internet: true
```

## Limitations

- **Worker required** — Must deploy a CF Worker as the control plane.
- **TypeScript SDK only** — No Python SDK.
- **Container isolation** — Shared-kernel containers (not Firecracker microVMs).
- **Beta** — v0.7.0. SDK APIs may change.
- **Max 4 vCPU / 12 GiB** per sandbox.
- **State lost on sleep** — Must use R2 backup/restore for persistence.
