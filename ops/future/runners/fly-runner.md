# Fly.io Machines Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the [Fly Machines](https://fly.io/docs/machines/) REST API to run agents on Firecracker microVMs across 35+ global regions.

## What is Fly.io Machines?

[Fly Machines](https://fly.io/docs/machines/) are fast-launching Firecracker microVMs with a REST API for full lifecycle management. Each Machine runs a Docker image with subsecond start times, billed per-second with no monthly minimums.

- **Subsecond start times** — Firecracker-based
- **35+ regions worldwide** — Run agents close to the repo's CI or team location
- **REST API** — Full CRUD: create, start, stop, destroy, exec via HTTP
- **Per-second billing** — No monthly fee. Stopped machines = no charge (except storage)
- **Firecracker isolation** — Hardware-level microVM isolation

## `provision`

```bash
MACHINE_JSON=$(curl -s -X POST \
    "https://api.machines.dev/v1/apps/$FLY_APP/machines" \
    -H "Authorization: Bearer $FLY_API_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"config\": {
            \"image\": \"$IMAGE\",
            \"guest\": {\"cpus\": $CPUS, \"memory_mb\": $MEMORY_MB},
            \"env\": {$(echo "$FORWARD_ENV_JSON")},
            \"auto_destroy\": true
        },
        \"region\": \"$REGION\"
    }")

MACHINE_ID=$(echo "$MACHINE_JSON" | jq -r '.id')

while true; do
    STATE=$(curl -s "https://api.machines.dev/v1/apps/$FLY_APP/machines/$MACHINE_ID" \
        -H "Authorization: Bearer $FLY_API_TOKEN" | jq -r '.state')
    if [ "$STATE" = "started" ]; then break; fi
    sleep 1
done

fly machine exec "$MACHINE_ID" -- git clone "$REPO_URL" /workspace
fly machine exec "$MACHINE_ID" -- bash -c "cd /workspace && git checkout $BRANCH"

for cmd in "${SETUP_COMMANDS[@]}"; do
    fly machine exec "$MACHINE_ID" -- bash -c "$cmd"
done

echo '{"status":"ok","environment":{"id":"'$MACHINE_ID'","cwd":"/workspace"}}'
```

## `exec`

```bash
fly machine exec "$MACHINE_ID" -- \
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
curl -s -X DELETE \
    "https://api.machines.dev/v1/apps/$FLY_APP/machines/$MACHINE_ID" \
    -H "Authorization: Bearer $FLY_API_TOKEN"
echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: fly
runners:
  fly:
    app: aiki-runners
    region: iad
    cpus: 2
    memory_mb: 2048
    image: ghcr.io/org/aiki-agent:latest
    auto_destroy: true
    forward_env:
      - ANTHROPIC_API_KEY
    # Auth: reads FLY_API_TOKEN from env
```

## Limitations

- **Requires pre-built Docker image** — Or install tools during provisioning (slower).
- **App must exist** — Must pre-create the Fly app (`fly apps create aiki-runners`).
- **No SDK** — REST API only. Shell scripts or HTTP libraries required.
