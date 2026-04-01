# exe.dev Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable (could be a shell script, Go binary, etc.) that uses SSH to manage exe.dev VMs.

## `provision`

```bash
# Create VM
VM_JSON=$(ssh exe.dev new --json)
VM_NAME=$(echo "$VM_JSON" | jq -r '.vm_name')
SSH_DEST=$(echo "$VM_JSON" | jq -r '.ssh_dest')

# Clone repo
ssh "$SSH_DEST" "git clone $REPO_URL /workspace && cd /workspace && git checkout $BRANCH"

# Run setup commands
for cmd in "${SETUP_COMMANDS[@]}"; do
    ssh "$SSH_DEST" "$cmd"
done

# Return environment
echo '{"status":"ok","environment":{"id":"'$VM_NAME'","host":"'$SSH_DEST'","cwd":"/workspace"}}'
```

## `exec`

```bash
# Build the command string with env vars
ENV_EXPORTS=""
for key in $(echo "$ENV_JSON" | jq -r 'keys[]'); do
    val=$(echo "$ENV_JSON" | jq -r ".[\"$key\"]")
    ENV_EXPORTS="$ENV_EXPORTS export $key=\"$val\";"
done

# Execute via SSH
ssh "$SSH_DEST" "cd $CWD && $ENV_EXPORTS ${COMMAND[*]}" > /tmp/stdout 2> /tmp/stderr
EXIT_CODE=$?

# Return result
jq -n --arg stdout "$(cat /tmp/stdout)" \
      --arg stderr "$(cat /tmp/stderr)" \
      --argjson exit_code "$EXIT_CODE" \
      '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":$stderr}'
```

## `cleanup`

```bash
if [ "$EPHEMERAL" = "true" ]; then
    ssh exe.dev rm "$VM_NAME"
fi
echo '{"status":"ok"}'
```

## Worker Loop Mode (Advanced)

The exe.dev runner can optionally use exe.dev's built-in worker loop system instead of raw SSH command execution. This provides automatic session recovery, health gates, and persistent memory (Deja).

```yaml
runners:
  exe:
    vm: my-dev-vm
    mode: worker
    worker:
      max_sessions: 10
      model: claude-opus-4-20250514
```

When `mode: worker`, the `exec` operation uses:
```bash
ssh "$SSH_DEST" "worker start aiki-$TASK_ID --task '$PROMPT' --dir /workspace --max $MAX_SESSIONS"
```

And monitors via the HTTP API:
```bash
# Poll worker status
ssh "$SSH_DEST" "curl -s -H 'X-Exedev-Userid: aiki' http://localhost:9999/api/conversations"
```
