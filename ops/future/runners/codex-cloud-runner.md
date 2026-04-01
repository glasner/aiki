# Codex Cloud Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that submits tasks to [OpenAI Codex Cloud](https://developers.openai.com/codex/cloud/) — OpenAI's hosted environment for running Codex agent sessions.

## What is Codex Cloud?

Codex Cloud runs the Codex agent in isolated cloud containers. Each task gets a container with the repo cloned from GitHub, common toolchains pre-installed, and configurable internet access (disabled by default during agent execution).

- Runs in isolated cloud containers managed by OpenAI
- Universal image: Node.js, Python, Go, Rust, Java, Ruby + common tools
- Internet disabled during agent phase (enabled during setup scripts)
- Auth: OpenAI API key or ChatGPT subscription
- `AGENTS.md` respected for project-specific commands
- Container caching (up to 12 hours) for faster follow-ups

## How the Runner Works

Like the Claude Code web runner, this bypasses the normal agent→runner flow and submits tasks directly to the provider's cloud.

**`provision`:**
```bash
codex --version > /dev/null 2>&1 || {
    echo '{"status":"error","error":"codex CLI not found"}'
    exit 1
}
echo '{"status":"ok","environment":{"type":"codex-cloud"}}'
```

**`exec`:**
```bash
RESULT=$(codex cloud exec --env "$ENVIRONMENT" --json "$PROMPT" 2>/tmp/codex-stderr)
EXIT_CODE=$?
STDOUT=$(echo "$RESULT" | jq -r '.output // .summary // ""')

jq -n --arg stdout "$STDOUT" --argjson exit_code "$EXIT_CODE" \
    '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":""}'
```

**`cleanup`:**
```bash
echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: codex-cloud
runners:
  codex-cloud:
    environment: default
    # Auth: reads OPENAI_API_KEY from env
```

## Limitations

- **Codex only** — Can't run Claude Code, Gemini, or other agents
- **GitHub only** — Only works with GitHub-hosted repositories
- **No public REST API yet** — Relies on `codex` CLI or SDK
- **Internet disabled during agent** — Must install packages in setup script
