# Getting Started with Aiki

Aiki is practical if you treat it like an onboarding checklist: install it, init once, run the health check, then run one real task.

This page is the minimal path from zero to first workflow.

## 1) Install prerequisites

You need:

- **Git**
- **Rust toolchain** (for building from source)
- **Jujutsu (`jj`)**

### macOS

```bash
brew install git rustup-init jj
rustup-init -y
```

### Linux (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install -y git curl build-essential
curl https://sh.rustup.rs -sSf | sh -s -- -y
cargo install --locked jj-cli  # or use your distro package if available
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

Verify:

```bash
git --version
rustc --version
cargo --version
jj --version
```

## 2) Install Aiki

```bash
git clone https://github.com/glasner/aiki.git
cd aiki/cli
cargo install --path .
```

Then confirm:

```bash
aiki --version
```

## 3) Initialize a repo for Aiki

From any Git repository:

```bash
cd your-project
aiki init
```

That does all of this:

- Creates `.aiki/` with defaults
- Adds `.aiki/hooks.yml`
- Generates `.aiki/repo-id`
- Configures Git hooks (`~/.aiki/githooks`)
- Boots local JJ workspace metadata
- Installs editor hooks (Claude Code, Cursor, Codex)

Quick checks:

```bash
ls .aiki
ls .aiki/hooks.yml
git config core.hooksPath
jj root
```

## 4) Run health check

Use this to catch setup issues fast:

```bash
aiki doctor
```

If anything is fixable:

```bash
aiki doctor --fix
```

If you hit template warnings on first run:

```bash
cp -R /path/to/aiki/.aiki/templates .aiki/
ls .aiki/templates/aiki
```

Expected symptom: `No templates directory found at: .aiki/templates`.

Current setup is healthy even with non-blocking telemetry warning:
`OTel receiver not listening`.

## 5) First workflow: plan → build → fix

Run this when you want one end-to-end proof that the orchestration works.

### 5.1 Write a plan

Create `ops/now/my-feature.md` with clear goal, scope, and acceptance criteria.

### 5.2 Execute

```bash
aiki build ops/now/my-feature.md --fix
```

Need only review first?

```bash
aiki build ops/now/my-feature.md --review
```

### 5.3 Watch progress

```bash
aiki task                # list active/completed tasks
aiki task show <task-id> # inspect a task
```

The build path is: `plan → decompose → loop → review → fix`.

## 6) Editor integrations

`aiki init` wires hooks automatically:

- **Claude Code**: `~/.claude/settings.json`
- **Cursor**: `~/.cursor/hooks.json`
- **Codex**: `~/.codex/config.toml` (OTel exporter configured)
- **Zed**: `aiki acp claude-code`

Hooks are global, so one restart after first init is usually enough.

## 7) Next docs

- [SDLC: Plan, Build, Review, Fix](sdlc.md)
- [Customizing Defaults](customizing-defaults.md)
- [Creating Plugins](creating-plugins.md)
- [Contributing](contributing.md)
