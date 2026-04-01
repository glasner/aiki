# Coder Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that uses the Coder REST API and CLI.

## Two Modes

### Mode A: Tasks API (preferred)

For Coder deployments with Tasks API (v2.27+). The runner uses Coder's native task orchestration — Coder handles workspace provisioning, agent startup, and lifecycle.

**`provision`:**
```bash
TASK_JSON=$(curl -s -X POST "$CODER_URL/api/experimental/tasks/" \
  -H "Coder-Session-Token: $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"prompt\": \"$PROMPT\", \"template_id\": \"$TEMPLATE_ID\"}")

TASK_ID=$(echo "$TASK_JSON" | jq -r '.id')
echo '{"status":"ok","environment":{"id":"'$TASK_ID'","type":"coder-task"}}'
```

**`exec`:**
```bash
while true; do
    STATUS_JSON=$(curl -s "$CODER_URL/api/experimental/tasks/$TASK_ID" \
        -H "Coder-Session-Token: $TOKEN")
    STATUS=$(echo "$STATUS_JSON" | jq -r '.status')
    if [ "$STATUS" = "completed" ] || [ "$STATUS" = "failed" ]; then break; fi
    sleep 5
done

echo "$STATUS_JSON" | jq '{status:"ok", exit_code: (if .status == "completed" then 0 else 1 end), stdout: .output, stderr: ""}'
```

**`cleanup`:**
```bash
if [ "$EPHEMERAL" = "true" ]; then
    WORKSPACE_ID=$(curl -s "$CODER_URL/api/experimental/tasks/$TASK_ID" \
        -H "Coder-Session-Token: $TOKEN" | jq -r '.workspace_id')
    curl -s -X DELETE "$CODER_URL/api/v2/workspaces/$WORKSPACE_ID" \
        -H "Coder-Session-Token: $TOKEN"
fi
echo '{"status":"ok"}'
```

### Mode B: Workspace + SSH (simpler)

For deployments without Tasks API. Runner creates a workspace, SSHs in, runs the command directly.

**`provision`:**
```bash
WORKSPACE_JSON=$(curl -s -X POST "$CODER_URL/api/v2/organizations/$ORG/members/me/workspaces" \
  -H "Coder-Session-Token: $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"name\": \"aiki-${TASK_ID:0:8}\", \"template_id\": \"$TEMPLATE_ID\", \"ttl_ms\": 3600000}")

WORKSPACE_ID=$(echo "$WORKSPACE_JSON" | jq -r '.id')
WORKSPACE_NAME=$(echo "$WORKSPACE_JSON" | jq -r '.name')

while true; do
    STATUS=$(curl -s "$CODER_URL/api/v2/workspaces/$WORKSPACE_ID" \
        -H "Coder-Session-Token: $TOKEN" | jq -r '.latest_build.status')
    if [ "$STATUS" = "running" ]; then break; fi
    sleep 2
done

echo '{"status":"ok","environment":{"id":"'$WORKSPACE_ID'","name":"'$WORKSPACE_NAME'","cwd":"/workspace"}}'
```

**`exec`:**
```bash
coder ssh "$WORKSPACE_NAME" -- bash -c "cd $CWD && $ENV_EXPORTS ${COMMAND[*]}" \
    > /tmp/stdout 2> /tmp/stderr
EXIT_CODE=$?

jq -n --arg stdout "$(cat /tmp/stdout)" \
      --arg stderr "$(cat /tmp/stderr)" \
      --argjson exit_code "$EXIT_CODE" \
      '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":$stderr}'
```

## Configuration

```yaml
runner: coder
runners:
  coder:
    url: https://coder.example.com
    token_env: CODER_SESSION_TOKEN
    template: ai-agent
    organization: default
    ephemeral: true
    mode: tasks  # or "workspace"
```
