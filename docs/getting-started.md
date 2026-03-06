# Getting Started with Aiki

This guide walks you from zero to productive with Aiki — AI code provenance tracking and workflow orchestration.

## Prerequisites

- **Git** — for version control
- **Rust toolchain** — for building from source (rustup recommended)
- **Jujutsu (jj)** — required for repository initialization

Jujutsu (jj) must be installed in your environment before running `aiki init`.

Install prerequisites (examples):

### macOS (Homebrew)

```bash
brew install git rustup-init jj
rustup-init -y
```

### Linux

```bash
# Debian/Ubuntu example
sudo apt update
sudo apt install -y git curl build-essential
curl https://sh.rustup.rs -sSf | sh -s -- -y

# Install jj (pick one)
cargo install --locked jj-cli
# or use your distro package if available
```

Add Cargo's bin directory to your shell profile permanently:

```bash
# zsh
grep -qxF 'export PATH="$HOME/.cargo/bin:$PATH"' ~/.zshrc || \
  echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc

# bash
grep -qxF 'export PATH="$HOME/.cargo/bin:$PATH"' ~/.bashrc || \
  echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
```

Reload your shell config (or open a new shell):

```bash
# zsh
source ~/.zshrc
# bash
source ~/.bashrc
```

Verify prerequisites:

```bash
git --version
rustc --version
cargo --version
jj --version
```

## Installation

```bash
git clone https://github.com/glasner/aiki.git
cd aiki/cli
cargo install --path .
```

Verify the installation:

```bash
aiki --version
```

Expected: prints an Aiki version and confirms it's on your PATH.

## Initialize a Project

Navigate to any Git repository and run:

```bash
cd your-project
aiki init
```

This will:
- Initialize Jujutsu (non-colocated, independent from your `.git`)
- Create the `.aiki/` directory with default configuration
- Install Git hooks for automatic co-author attribution
- Configure editor hooks globally (Claude Code, Cursor)

Init success checks:
- `.aiki/` exists
- `.aiki/repo-id` exists
- `.aiki/hooks.yml` exists
- `git config core.hooksPath` points to `~/.aiki/githooks`
- `jj root` returns the project root

## Health Check

Verify everything is set up correctly:

```bash
aiki doctor
```

This checks repository setup, global hooks, and local configuration. If it finds issues:

```bash
aiki doctor --fix
```

If your first `aiki plan`/`aiki build` reports missing templates (for example `No templates directory found at: .aiki/templates` or `Template not found: aiki/review`), bootstrap templates once:

```bash
cp -R /path/to/aiki/.aiki/templates .aiki/
```

Then verify:

```bash
ls .aiki/templates/aiki
```

## Editor Setup

`aiki init` configures all supported editors automatically. Here's what gets set up:

| Editor | What happens |
|--------|-------------|
| **Claude Code** | Lifecycle hooks added to `~/.claude/settings.json` |
| **Cursor** | File edit hooks added to `~/.cursor/hooks.json` |
| **Codex** | OTel receiver configured |
| **Zed** | ACP proxy available via `aiki acp claude-code` |

Since hooks are global, you only need to restart your editor once after `aiki init`. Aiki preserves any existing hooks you had.

## First Workflow: Plan → Build → Fix

Aiki is designed for a simple first run: write a plan, execute it, then let the review/fix loop run automatically.

### 1) Write a Plan

Use a markdown plan file (for example, `ops/now/my-feature.md`) with implementation goals.

### 2) Run Build with Review+Fix

```bash
aiki build ops/now/my-feature.md --fix
```

Use `--review` first if you want review without auto-fix, then rerun with `--fix` when ready.

### 3) Check Task Progress

```bash
# Watch the tasks that were created for the plan and review/fix steps
aiki task

# Open details for a task
aiki task show <task-id>

# Watch the build-review-fix task chain
aiki task show <build-task-id>
```

The build command orchestrates: **plan → decompose → loop → review → fix** (with review/fix iteration enabled by `--fix`).

Use this flow in a real repo before introducing additional commands.

## Next Steps

- [SDLC: Plan, Build, Review, Fix](sdlc.md) — the full AI development lifecycle
- [Customizing Defaults](customizing-defaults.md) — modify Aiki's behavior with flows, events, context injection, and template overrides
- [Creating Plugins](creating-plugins.md) — build reusable, shareable hooks and templates
- [Contributing](contributing.md) — develop Aiki itself
