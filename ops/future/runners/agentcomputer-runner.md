# AgentComputer Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [AgentComputer](https://www.agentcomputer.ai/) CLI to run agents in full Ubuntu VMs with sub-second provisioning, persistent disk, and native multi-agent support.

## What is AgentComputer?

[AgentComputer](https://www.agentcomputer.ai/) provides cloud computers purpose-built for AI agents. Each sandbox is a full Ubuntu VM (not a container) with hardware-level isolation via KVM, persistent disk, and native support for Claude, Codex, and CUA agents.

- **~0.4-0.5s provisioning** — Full Ubuntu VMs spin up in under a second
- **Persistent disk** — Install anything, keep everything across restarts
- **Native agent support** — Claude, Codex, and CUA agents pre-installed in every sandbox
- **Multi-agent collaboration** — Agents delegate work to each other via SSH with zero friction
- **Full SSH access** — `computer ssh <name>` for full shell access
- **CLI** — `npm i -g aicomputer` installs the `computer` CLI
- **KVM isolation** — Real VMs with own kernel, memory, and disk

## Why AgentComputer for aiki?

- **Sub-second full VMs** — Fastest provisioning for full VMs (not containers or microVMs). Only Unikraft is faster, and Unikraft can't exec into instances.
- **Agent-native** — Claude, Codex, CUA pre-installed. No setup step needed for common agents.
- **Multi-agent workflows** — Unique SSH delegation model where agents can spawn and coordinate with other agents in the same persistent ecosystem.
- **Persistent disk** — Unlike ephemeral sandboxes (Vercel, E2B), state persists across restarts. Good for long-running, iterative agent tasks.

## `provision`

```bash
# Create a new computer
COMPUTER_NAME="aiki-${TASK_ID:0:8}"

computer create "$COMPUTER_NAME" 2>/dev/null
# Computer creates in ~0.4s

# Wait for SSH to be ready
for i in $(seq 1 30); do
    computer ssh "$COMPUTER_NAME" -- true 2>/dev/null && break
    sleep 1
done

# Clone repo
computer ssh "$COMPUTER_NAME" -- "git clone $REPO_URL /workspace && cd /workspace && git checkout $BRANCH"

# Run setup commands
for cmd in "${SETUP_COMMANDS[@]}"; do
    computer ssh "$COMPUTER_NAME" -- "$cmd"
done

echo '{"status":"ok","environment":{"id":"'$COMPUTER_NAME'","host":"'$COMPUTER_NAME'.computer.agentcomputer.ai","cwd":"/workspace"}}'
```

## `exec`

```bash
COMPUTER_NAME=$(echo "$REQUEST" | jq -r '.environment.id')
CWD=$(echo "$REQUEST" | jq -r '.environment.cwd')

# Build env exports
ENV_EXPORTS=""
for key in $(echo "$ENV_JSON" | jq -r 'keys[]'); do
    val=$(echo "$ENV_JSON" | jq -r ".[\"$key\"]")
    ENV_EXPORTS="$ENV_EXPORTS export $key=\"$val\";"
done

# Execute via SSH
computer ssh "$COMPUTER_NAME" -- bash -c "cd $CWD && $ENV_EXPORTS ${COMMAND[*]}" \
    > /tmp/stdout 2> /tmp/stderr
EXIT_CODE=$?

jq -n --arg stdout "$(cat /tmp/stdout)" \
      --arg stderr "$(cat /tmp/stderr)" \
      --argjson exit_code "$EXIT_CODE" \
      '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":$stderr}'
```

## `cleanup`

```bash
COMPUTER_NAME=$(echo "$REQUEST" | jq -r '.environment.id')

if [ "$EPHEMERAL" = "true" ]; then
    computer rm "$COMPUTER_NAME" 2>/dev/null || true
fi

echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: agentcomputer
runners:
  agentcomputer:
    ephemeral: true
    setup:
      - npm install -g @anthropic-ai/claude-code  # May already be pre-installed
    forward_env:
      - ANTHROPIC_API_KEY
      - OPENAI_API_KEY
    # Auth: `computer login` (one-time setup)
```

## Limitations

- **Limited public API docs** — CLI-driven; no documented REST API or SDK beyond the `computer` CLI.
- **Pricing unclear** — Storage at $0.08/GB/month, data transfer at $0.07/GB/month; compute pricing not publicly documented.
- **SaaS only** — No self-hosting option.
- **Newer platform** — Smaller community than established sandbox providers.
- **No streaming exec** — SSH stdout captured on completion, not streamed.
