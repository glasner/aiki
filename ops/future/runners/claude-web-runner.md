# Claude Code Web Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that submits tasks to [Claude Code on the web](https://code.claude.com/docs/en/claude-code-on-the-web) — Anthropic's hosted cloud environment for running Claude Code sessions.

## What is Claude Code on the web?

Claude Code on the web runs Claude Code sessions on Anthropic-managed VMs. Each session gets an isolated environment with the repo cloned from GitHub, pre-installed toolchains (Python, Node.js, Ruby, Go, Rust, Java, C++), and network access.

- Runs on Anthropic-managed VMs with full isolation
- Pre-installed: all major language runtimes, package managers, build tools, PostgreSQL, Redis
- Network: limited by default (allowlisted domains), configurable to full or none
- Auth via GitHub (repo access) + Claude account (Pro/Max/Team/Enterprise)
- `CLAUDE.md` / `AGENTS.md` respected automatically
- Sessions can be teleported back to terminal (`claude --teleport`)

## How the Runner Works

This runner bypasses the normal agent→runner flow. It submits tasks directly to Claude Code's cloud.

**`provision`:**
```bash
claude --version > /dev/null 2>&1 || {
    echo '{"status":"error","error":"claude CLI not found"}'
    exit 1
}
echo '{"status":"ok","environment":{"type":"claude-web"}}'
```

**`exec`:**
```bash
SESSION_OUTPUT=$(claude --remote "$PROMPT" --output-format json 2>/tmp/claude-stderr)
SESSION_ID=$(echo "$SESSION_OUTPUT" | jq -r '.session_id')

>&2 echo "[runner] Web session created: $SESSION_ID"

while true; do
    STATUS=$(claude tasks status "$SESSION_ID" --output-format json 2>/dev/null | jq -r '.status')
    if [ "$STATUS" = "completed" ] || [ "$STATUS" = "failed" ] || [ "$STATUS" = "stopped" ]; then break; fi
    sleep 10
done

RESULT=$(claude tasks show "$SESSION_ID" --output-format json 2>/dev/null)
EXIT_CODE=$(echo "$RESULT" | jq -r 'if .status == "completed" then 0 else 1 end')
STDOUT=$(echo "$RESULT" | jq -r '.summary // .last_message // ""')

jq -n --arg stdout "$STDOUT" --argjson exit_code "$EXIT_CODE" \
    '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":""}'
```

**`cleanup`:**
```bash
echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: claude-web
runners:
  claude-web:
    environment: default
    network: limited  # limited | full | none
```

## Limitations

- **Claude Code only** — Can't run Codex, Gemini, or other agents
- **GitHub only** — Only works with GitHub-hosted repositories
- **Rate limited** — Shares rate limits with all Claude usage on your account
- **No REST API** — Relies on the `claude` CLI
