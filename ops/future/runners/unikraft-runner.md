# Unikraft Cloud Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the `kraft` CLI and Unikraft Cloud REST API to run agents as unikernel instances — ultra-lightweight, single-purpose VMs with hardware-level isolation.

## What is Unikraft Cloud?

[Unikraft Cloud](https://unikraft.com/) is a cloud platform built on [unikernels](https://unikraft.org/) — minimal VM images containing only the OS components needed for a single application. Each instance is a Firecracker-class microVM with hardware-level isolation (stronger than containers) but near-process-level lightness.

- 10-50ms cold starts (fastest of any runner)
- Scale-to-zero with millisecond wake-up
- 10,000-100,000+ instances per server
- Persistent volumes available
- CLI (`kraft`) and REST API
- Auth: API token (`UKC_TOKEN`)

## Different Execution Model

**Unikraft Cloud does not support exec/shell/SSH into running instances.** Unikernels are single-purpose — there's no shell, no SSH daemon, no way to run arbitrary commands inside a running instance.

This means the runner uses a **deploy model** instead of an **exec model**:

```
Other runners (exe, vercel, coder):        Unikraft runner:
  1. Provision environment                   1. Build image containing agent + repo
  2. Exec command inside it                  2. Deploy instance (runs agent as entrypoint)
  3. Collect output                          3. Monitor via instance logs
  4. Cleanup                                 4. Cleanup
```

The agent CLI, repo, and task prompt are baked into the image at build time. The instance runs the agent as its sole process.

## `provision`

Build and deploy an image that runs the agent:

```bash
# Create a temporary Dockerfile for the agent task
cat > /tmp/aiki-agent.Dockerfile << 'DOCKERFILE'
FROM node:22-slim
RUN npm install -g @anthropic-ai/claude-code
WORKDIR /workspace
DOCKERFILE

# Deploy (builds image + creates + starts instance)
DEPLOY_JSON=$(kraft cloud --metro fra0 deploy \
    --name "aiki-${TASK_ID:0:8}" \
    -e "AIKI_TASK=$TASK_ID" \
    -e "AIKI_SESSION_MODE=background" \
    -e "ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY" \
    -M 512 \
    --restart never \
    --output json \
    /tmp/)

INSTANCE_NAME=$(echo "$DEPLOY_JSON" | jq -r '.name')
echo '{"status":"ok","environment":{"id":"'$INSTANCE_NAME'","type":"unikraft"}}'
```

Or, using a pre-built agent image from a registry:

```bash
kraft cloud --metro fra0 instance create \
    --name "aiki-${TASK_ID:0:8}" \
    -e "AIKI_TASK=$TASK_ID" \
    -e "AGENT_PROMPT=$PROMPT" \
    -M 512 \
    --start \
    --restart never \
    myregistry/aiki-agent:latest
```

## `exec`

Since there's no exec-into-instance, the runner monitors instance logs:

```bash
while true; do
    STATUS=$(kraft cloud instance get "$INSTANCE_NAME" --output json | jq -r '.status')
    if [ "$STATUS" = "stopped" ] || [ "$STATUS" = "error" ]; then
        break
    fi
    sleep 5
done

LOGS=$(kraft cloud instance logs "$INSTANCE_NAME")
EXIT_CODE=$(kraft cloud instance get "$INSTANCE_NAME" --output json | jq -r '.exit_code // 0')

jq -n --arg stdout "$LOGS" --argjson exit_code "$EXIT_CODE" \
    '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":""}'
```

## `cleanup`

```bash
kraft cloud instance remove "$INSTANCE_NAME" 2>/dev/null || true
echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: unikraft
runners:
  unikraft:
    metro: fra0            # Datacenter region
    memory: 512            # MB
    image: myregistry/aiki-agent:latest  # Pre-built agent image (recommended)
    restart: never         # One-shot execution
    scale_to_zero: off
    forward_env:
      - ANTHROPIC_API_KEY
      - OPENAI_API_KEY
```

## Pre-built Agent Image (Recommended)

```dockerfile
FROM node:22-slim
RUN npm install -g @anthropic-ai/claude-code
RUN curl -fsSL https://get.aiki.dev | sh
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
```

```bash
#!/bin/bash
# entrypoint.sh — clone repo, run agent on task
git clone "$REPO_URL" /workspace
cd /workspace && git checkout "$BRANCH"
claude --print --dangerously-skip-permissions "$AGENT_PROMPT"
```

## Limitations

- **No exec/shell/SSH** — Cannot run arbitrary commands inside a running instance. Must bake everything into the image.
- **Single-purpose** — Each instance runs one process. No interactive debugging.
- **Image build overhead** — First deployment requires building a Docker image (mitigated by using pre-built images).
- **Output via logs only** — No stdout/stderr streaming; must poll `kraft cloud instance logs`.
- **Early platform** — Unikraft Cloud is newer than exe.dev/Coder/Vercel. API surface may evolve.
