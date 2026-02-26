# Aiki Terminal-Bench Integration

Test whether Aiki improves Claude Code's performance on [Terminal-Bench](https://www.tbench.ai/).

## Background

Terminal-Bench is the standard benchmark for evaluating AI agents in terminal environments. It contains 100+ real-world tasks (code compilation, server setup, system admin, etc.) scored by verification scripts.

**Baseline**: Claude Code scores ~43.2% on Terminal-Bench (per published leaderboard).

**Hypothesis**: Aiki's structured task management methodology (plan-before-act, verify-after-each-step, avoid blind retries) should improve Claude Code's pass rate, especially on harder multi-step tasks.

## What This Tests

The integration provides a single agent (`ClaudeCodeAikiAgent`) that installs Claude Code + Aiki in the Terminal-Bench container and provides a CLAUDE.md with structured task management instructions.

Compared to the vanilla Claude Code agent, this adds:
- A CLAUDE.md with structured methodology (plan → execute → verify)
- Terminal best practices guidance
- Anti-pattern avoidance rules
- The `aiki` binary for task tracking (when build succeeds)

## Prerequisites

- Python 3.12+
- Docker
- `uv` package manager
- `ANTHROPIC_API_KEY` environment variable set

## Install

```bash
uv tool install terminal-bench
```

## Quick Start

### Single task test

```bash
cd tbench/
tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --dataset terminal-bench-core==2.0 \
    --task-id hello-world
```

### Full benchmark run

```bash
cd tbench/
tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --dataset terminal-bench-core==2.0 \
    --model anthropic/claude-opus-4-20250514 \
    --n-concurrent 4
```

### Compare against baseline

The baseline is the built-in Claude Code agent:

```bash
# Baseline (vanilla Claude Code)
tb run \
    --agent claude-code \
    --dataset terminal-bench-core==2.0 \
    --model anthropic/claude-opus-4-20250514 \
    --n-concurrent 4

# Treatment (Claude Code + Aiki)
cd tbench/
tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --dataset terminal-bench-core==2.0 \
    --model anthropic/claude-opus-4-20250514 \
    --n-concurrent 4
```

## Agent Options

Pass options via `--agent-kwarg`:

```bash
# Specify Claude Code version
tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --agent-kwarg version=1.0.0 \
    --dataset terminal-bench-core==2.0

# Specify model
tb run \
    --agent-import-path aiki_agent:ClaudeCodeAikiAgent \
    --model anthropic/claude-sonnet-4-20250514 \
    --dataset terminal-bench-core==2.0
```

## Files

| File | Purpose |
|------|---------|
| `aiki_agent.py` | `AbstractInstalledAgent` subclass — the agent Terminal-Bench runs |
| `aiki-setup.sh.j2` | Jinja2 template for container setup (installs Claude Code + Aiki) |
| `scripts/prebuild.sh` | Pre-build aiki binary for container injection |
| `scripts/run.sh` | Convenience script for running the benchmark |
| `scripts/compare.sh` | Runs both baseline and treatment, prints results |

## How It Works

1. Terminal-Bench creates a Docker container for each task
2. `aiki-setup.sh.j2` runs inside the container:
   - Installs Node.js + Claude Code (same as baseline)
   - Copies the pre-built aiki binary if available (see `scripts/prebuild.sh`)
   - Writes a CLAUDE.md with structured task methodology
   - Runs `aiki init` if the binary is present
3. Claude Code starts with the task instruction
4. The CLAUDE.md guides its approach (plan → execute → verify)
5. Terminal-Bench's verification script scores the result

## Interpreting Results

- **Higher pass rate than baseline (43.2%)** → Aiki's methodology helps
- **Same pass rate** → Methodology is neutral (overhead ≈ benefit)
- **Lower pass rate** → Setup overhead or instruction constraints hurt

Look especially at the **hard task subset** — that's where structured planning should provide the biggest uplift.
