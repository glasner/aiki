# Getting Started with Aiki

## 1) Install prerequisites

> Git is assumed to be installed already.

### macOS

```bash
brew install jj rust
rustup-init -y
```

### Linux (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install -y jj curl build-essential
curl https://sh.rustup.rs -sSf | sh -s -- -y
```

Make sure Cargo binaries are on your PATH:

```bash
# zsh
grep -qxF 'export PATH="$HOME/.cargo/bin:$PATH"' ~/.zshrc || \
  echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc

# bash
grep -qxF 'export PATH="$HOME/.cargo/bin:$PATH"' ~/.bashrc || \
  echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc

# Reload shell
source ~/.zshrc   # or source ~/.bashrc
```

## 2) Install Aiki

```bash
git clone https://github.com/glasner/aiki.git
cd aiki
cargo install --path cli
```

Then confirm:

```bash
aiki --version
```

## 3) Initialize Aiki in a repo

From any Git repository:

```bash
cd your-project
aiki init
```

This sets up Aiki in your repository and configures editor integrations:

- **Claude Code**: `~/.claude/settings.json`
- **Cursor**: `~/.cursor/hooks.json`
- **Codex**: `~/.codex/config.toml` (OTel exporter configured)
- **Zed**: `aiki acp claude-code`

Hooks are global, so one restart after first init is usually enough.

> **Note:** If you run into setup issues, you can run `aiki doctor` to diagnose common problems with your environment, dependencies, and configuration.

## 4) First workflow: Chat mode with task tracking

This workflow shows you how Aiki tracks work across different AI agents in real-time.

### 4.1 Ask Claude to make a change

Open Claude Code and ask it to make a simple change to your project. For example:

```
% claude

 ▐▛███▜▌   Claude Code v2.1.59
▝▜█████▛▘  Opus 4.6 · Claude Max
  ▘▘ ▝▝    ~/code/aiki
 ⎿  SessionStart:startup says: 合 aiki initialized

──────────────────────────────────────────────────────────────
❯ Add a comment to the main function explaining what it does
──────────────────────────────────────────────────────────────
```

You'll see Claude automatically start a task before making changes. The output will show something like:

```
⏺ Bash(cd /tmp/aiki/7f50e063/46c8f034 && aiki task start "Add comment to main function" --source prompt)
  ⎿  Started umsrkmq
     ---
     Run `aiki task comment add umsrkmq` to leave updates as you go
```

Notice the agent is working from `/tmp/aiki/.../`. Each agent session gets its own [Jujutsu workspace](https://jj-vcs.github.io/jj/latest/working-copy/) — an isolated copy of the repo. This means multiple agents can work concurrently without stepping on each other. When a task closes, its changes are tracked as a JJ change and automatically merged back. Any conflicts are resolved intelligently by agents through their understanding of the assocated tasks. 

### 4.2 See the task in progress

While Claude is working, check the task status in another terminal:

```bash
aiki task show <task-id> 
```

### 4.3 See Claude's task summary

When Claude finishes, it will close the task with a summary of what was done.

```
Tasks Completed

  - umsrkmq — Added comment to describe what the main function does
```

In a second terminal, you can view the details with the following command:

```bash
aiki task show <task-id> --output summary
```

To see the actual code changes, run:

```bash
aiki task diff <task-id> 
```

### 4.4 Ask Codex to review by referencing the task ID

Now switch to Codex (or another AI agent) and ask it to review Claude's work by providing the task ID:

```
$ codex

>_ OpenAI Codex (v0.112.0)                                │
│                                                         │
│ model:     gpt-5.3-codex-spark xhigh   /model to change │
│ directory: ~/code/aiki                                  │
╰─────────────────────────────────────────────────────────╯
› Review umsrkmq
```

Codex will automatically create a review task, examine the changes, and provide a summary of it's findings.

**What you've learned:**
- Tasks persist across AI agents (Claude → Codex)
- Each agent can see and reference work done by others
- Task summaries provide a clear audit trail of what was done
- You can track progress in real-time with `aiki task show`
- Each agent session is isolated in its own JJ workspace — no conflicts between concurrent agents
- Changes are tracked as JJ changes, which is what powers `aiki task diff`

## 5) First headless workflow

After you're comfortable with chat mode, try the fully automated headless workflow. Aiki's [SDLC](sdlc.md) is four commands that form a closed loop:

- **`aiki plan`** — collaborate with an agent on a spec
- **`aiki build`** — decompose the plan into subtasks and execute them in parallel
- **`aiki review`** — evaluate the output against structured criteria
- **`aiki fix`** — auto-fix review issues, re-reviewing until clean

### 5.1 Start planning with `aiki plan`

Run `aiki plan` with a path to start or continue planning:

```bash
aiki plan path/to/my-plan.md
```

**The command behavior adapts automatically:**

- **If the file doesn't exist**: You'll be prompted to describe what you want to accomplish. This helps the agent understand your goals before starting.
- **If the file exists**: The agent immediately starts reviewing your plan, asks clarifying questions, and helps refine it.

**You can also provide your description directly on the command line:**

```bash
aiki plan path/to/my-plan.md "Add documentation-only smoke test change"
```

This skips the interactive prompt and jumps straight to planning with your provided context.

At the end of planning, `aiki plan` reports a summary of the conversation and closes the planning session when you give the thumbs up.

### 5.2 Build from the plan

After plan is ready and saved as `path/to/my-feature.md`:

```bash
aiki build path/to/my-feature.md --fix
```

The `--fix` flag automatically runs a code review after the build completes, then creates and executes followup tasks for any issues found. This gives you a fully automated pipeline: that runs `aiki build | aiki review | aiki fix` automatically.

### 5.3 Track headless workflow

The build automatically displays a live status screen as it progresses through stages:

```
 path/to/my-feature.md

 [luppzupt] Add webhook support

 ▸ build  1/3  57s
    ✓ decompose  12s
    ▸ loop  1/3  45s                             ●━━◉
     ⎿ ✓ Implement webhook endpoint          claude  45s
     ⎿ ▸ Add payload signing               claude  12s
     ⎿ ○ Wire up retry logic
 ○ review
 ○ fix
```

The screen updates in real-time showing:
- **Build progress**: Which subtasks are running, completed, or pending
- **Agent activity**: Which AI agent is working on each subtask (claude, cursor, codex)
- **Timing**: Elapsed time for each stage and subtask
- **Lane DAG**: Visual representation of concurrent subtask execution (when using `--lanes`)

The screen continues through review and fix stages automatically:

```
 path/to/my-feature.md

 [luppzupt] Add webhook support

 ✓ build  3/3  1m57
 ✓ review  2 issues  42s
 ▸ fix  1/2  18s
    ✓ plan  2s
    ✓ decompose  4s
    ▸ loop  1/2  12s
     ⎿ ✓ Fix: Missing null check           claude  12s
     ⎿ ✓ Fix: Error message format         claude   6s
```
---

**You've now learned the core Aiki workflows:**
- ✓ Chat mode with persistent task tracking across agents
- ✓ Headless planning with `aiki plan`
- ✓ Automated build pipelines from plan files
- ✓ How tasks persist across Claude, Cursor, and Codex

Aiki keeps work organized and visible no matter which AI agent you're using. Tasks, plans, and changes are tracked in your repository, so you never lose context when switching tools or coming back later.

## 6) Next docs

- [SDLC: Plan, Build, Review, Fix](sdlc.md)
- [Customizing Defaults](customizing-defaults.md)
- [Creating Plugins](creating-plugins.md)
- [Contributing](contributing.md)
