# Generic SSH Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

A single external executable that works with **any IaaS provider** offering SSH-accessible VMs — Hetzner Cloud, Vultr, DigitalOcean, Linode/Akamai, Scaleway, Oracle Cloud, Latitude.sh, and others.

## Why a Generic SSH Runner?

Traditional IaaS providers all follow the same pattern: provision a VM via API, SSH in to run commands, destroy via API. Rather than building a separate runner for each provider, a single SSH runner with pluggable provisioning backends covers them all.

```
┌─────────────────────────────────────────────┐
│           aiki-runner-ssh                    │
│                                              │
│  ┌──────────────┐    ┌───────────────────┐  │
│  │  SSH exec    │    │  Provision backend │  │
│  │  (universal) │    │  (per-provider)    │  │
│  │              │    │                    │  │
│  │  ssh $HOST   │    │  hetzner | vultr   │  │
│  │  "command"   │    │  do | linode | ... │  │
│  └──────────────┘    └───────────────────┘  │
└─────────────────────────────────────────────┘
```

## Supported Providers

| Provider | Provisioning Speed | Pricing | Regions | Unique Value |
|----------|-------------------|---------|---------|-------------|
| **Hetzner Cloud** | 15-30s | From €3.49/mo | EU, US | Best price-to-performance |
| **Vultr** | 1-15 min | From $2.50/mo | 32 regions | GPU instances, global |
| **DigitalOcean** | Minutes | From $4/mo | 15 regions | Developer-friendly |
| **Linode/Akamai** | Minutes | From $5/mo | 25 regions | Established, good docs |
| **Scaleway** | Seconds | Hourly | EU | EU data residency, ARM |
| **Oracle Cloud** | Minutes | Free tier available | 51 regions | Free 4-core ARM + 24 GB RAM |
| **Latitude.sh** | ~5s | On-demand | 20+ cities | Bare metal, physical isolation |

## `provision`

```bash
PROVIDER=$(echo "$REQUEST" | jq -r '.config.provider')

case "$PROVIDER" in
    hetzner)
        SERVER_JSON=$(hcloud server create \
            --name "aiki-${TASK_ID:0:8}" --type "$VM_TYPE" --image "$IMAGE" \
            --ssh-key "$SSH_KEY_NAME" --output json)
        HOST=$(echo "$SERVER_JSON" | jq -r '.server.public_net.ipv4.ip')
        SERVER_ID=$(echo "$SERVER_JSON" | jq -r '.server.id')
        ;;
    vultr)
        INSTANCE_JSON=$(vultr-cli instance create \
            --region "$REGION" --plan "$PLAN" --os "$OS_ID" \
            --ssh-keys "$SSH_KEY_ID" --label "aiki-${TASK_ID:0:8}" --output json)
        HOST=$(echo "$INSTANCE_JSON" | jq -r '.main_ip')
        SERVER_ID=$(echo "$INSTANCE_JSON" | jq -r '.id')
        ;;
    digitalocean)
        DROPLET_JSON=$(doctl compute droplet create "aiki-${TASK_ID:0:8}" \
            --region "$REGION" --size "$SIZE" --image "$IMAGE" \
            --ssh-keys "$SSH_KEY_FP" --output json)
        HOST=$(echo "$DROPLET_JSON" | jq -r '.[0].networks.v4[0].ip_address')
        SERVER_ID=$(echo "$DROPLET_JSON" | jq -r '.[0].id')
        ;;
esac

# Wait for SSH (universal)
for i in $(seq 1 60); do
    ssh -o ConnectTimeout=2 -o StrictHostKeyChecking=no "root@$HOST" true 2>/dev/null && break
    sleep 2
done

ssh "root@$HOST" "git clone $REPO_URL /workspace && cd /workspace && git checkout $BRANCH"

for cmd in "${SETUP_COMMANDS[@]}"; do ssh "root@$HOST" "$cmd"; done

echo '{"status":"ok","environment":{"id":"'$SERVER_ID'","host":"'$HOST'","cwd":"/workspace","provider":"'$PROVIDER'"}}'
```

## `exec`

Universal SSH execution — identical for all providers:

```bash
HOST=$(echo "$REQUEST" | jq -r '.environment.host')
CWD=$(echo "$REQUEST" | jq -r '.environment.cwd')

ssh "root@$HOST" "cd $CWD && $ENV_EXPORTS ${COMMAND[*]}" > /tmp/stdout 2> /tmp/stderr
EXIT_CODE=$?

jq -n --arg stdout "$(cat /tmp/stdout)" \
      --arg stderr "$(cat /tmp/stderr)" \
      --argjson exit_code "$EXIT_CODE" \
      '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":$stderr}'
```

## `cleanup`

```bash
PROVIDER=$(echo "$REQUEST" | jq -r '.environment.provider')
SERVER_ID=$(echo "$REQUEST" | jq -r '.environment.id')

case "$PROVIDER" in
    hetzner)      hcloud server delete "$SERVER_ID" ;;
    vultr)        vultr-cli instance delete "$SERVER_ID" ;;
    digitalocean) doctl compute droplet delete "$SERVER_ID" --force ;;
esac

echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: ssh
runners:
  ssh:
    provider: hetzner          # hetzner | vultr | digitalocean | linode | scaleway | oracle | latitude
    region: fsn1
    vm_type: cx22
    image: ubuntu-24.04
    ssh_key: my-key
    ephemeral: true
    setup:
      - apt-get update && apt-get install -y nodejs npm
      - npm install -g @anthropic-ai/claude-code
    forward_env:
      - ANTHROPIC_API_KEY
    # Auth: reads provider-specific env vars (HCLOUD_TOKEN, VULTR_API_KEY, etc.)
```

## Limitations

- **Slowest provisioning** — 15 seconds to several minutes depending on provider.
- **SSH setup required** — Must register SSH keys with the provider.
- **Provider CLI dependencies** — Needs `hcloud`, `vultr-cli`, `doctl`, etc. installed.
- **No streaming exec** — SSH stdout is captured on completion, not streamed.
