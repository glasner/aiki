# Plan: `aiki support` Command

## Overview

Add an `aiki support` command group for user-facing support actions. Initial subcommands:

- **`aiki support bug`** — file a bug report as a GitHub issue on `glasner/aiki`
- **`aiki support feature`** — file a feature request as a GitHub issue on `glasner/aiki`

Future subcommands (not in scope now): `chat`, `docs`.

## Design Decisions

### Command structure: flat subcommands under `support`

```
aiki support bug "task close crashes with circular link error"
aiki support feature "support custom task priorities"
aiki support bug                  # interactive: prompts for description
aiki support feature              # interactive: prompts for description
```

**Why flat, not `aiki support issue --bug`:**
- Consistent with existing patterns (`aiki task add`, `aiki plugin install`)
- More discoverable than flags
- `support` namespace accommodates non-issue subcommands (`chat`, `docs`) naturally as peers

**Why not `aiki bug` (as in plan.md):**
- `support` groups all user-support actions under one namespace
- Leaves `bug` available as a future alias if desired

### GitHub issue creation via `gh` CLI

- Shell out to `gh issue create` (same pattern as jj/git subprocess calls)
- Require `gh` to be installed and authenticated
- Detect with `which` crate (already a dependency)
- No new Rust dependencies needed

### Target repo

- Hardcoded to `glasner/aiki` via `--repo glasner/aiki` flag on `gh issue create`
- Works regardless of which repo the user is currently in

## Subcommand Details

### `aiki support bug [description]`

**Collects diagnostic snapshot** (subsumes the plan.md `aiki bug` design):

1. **Environment** — aiki version, OS/arch, shell, git version, jj version
2. **Repository state** — project root, jj/git/aiki init status, working copy change ID, jj status
3. **Task system** — task count by status, in-progress tasks, recent events
4. **Configuration** — hooks installed, plugins, instructions file
5. **Sessions** — active session count and details

**Creates GitHub issue:**
- **Title:** user-provided description (or prompted)
- **Labels:** `bug`
- **Body:** markdown report with description + diagnostic snapshot

**Flow:**
1. Collect diagnostic snapshot (resilient — each section catches its own errors)
2. If no description arg, prompt: `Describe the bug: `
3. Show preview of what will be submitted
4. Confirm: `Create issue on glasner/aiki? [Y/n]`
5. Run `gh issue create --repo glasner/aiki --title "..." --label bug --body "..."`
6. Print issue URL on success

**Flags:**
- `--dry-run` — print the report to stdout without creating an issue (replaces original `aiki bug` stdout behavior)
- `--output <file>` — write report to file instead of creating issue (backward compat with plan.md)

### `aiki support feature [description]`

**Simpler — no full diagnostic snapshot.**

Collects minimal context:
- aiki version
- OS/arch

**Creates GitHub issue:**
- **Title:** user-provided description (or prompted)
- **Labels:** `enhancement`
- **Body:** markdown template with description + minimal context

**Flow:**
1. If no description arg, prompt: `Describe the feature: `
2. Show preview
3. Confirm: `Create issue on glasner/aiki? [Y/n]`
4. Run `gh issue create --repo glasner/aiki --title "..." --label enhancement --body "..."`
5. Print issue URL on success

## Implementation

### Files to create/modify

1. **`src/commands/support.rs`** (new) — command implementation
   - `SupportCommands` enum with `Bug` and `Feature` variants
   - `pub fn run(command: SupportCommands) -> Result<()>`
   - `fn run_bug(description: Option<String>, dry_run: bool, output: Option<PathBuf>) -> Result<()>`
   - `fn run_feature(description: Option<String>) -> Result<()>`
   - Diagnostic snapshot collectors (reuse infra from doctor.rs, prerequisites.rs)
   - `fn create_github_issue(repo: &str, title: &str, labels: &[&str], body: &str) -> Result<String>`

2. **`src/commands/mod.rs`** — add `pub mod support;`

3. **`src/main.rs`** — add `Support` variant:
   ```rust
   /// Get support — file bugs, request features
   Support {
       #[command(subcommand)]
       command: commands::support::SupportCommands,
   },
   ```
   And dispatch:
   ```rust
   Commands::Support { command } => commands::support::run(command),
   ```

4. **`src/error.rs`** — add error variant:
   ```rust
   #[error("`gh` CLI not found. Install from https://cli.github.com/ and run `gh auth login`")]
   GhNotFound,

   #[error("Failed to create GitHub issue: {0}")]
   GhIssueFailed(String),
   ```

### Diagnostic snapshot implementation

Reuse existing infrastructure:
- `env!("CARGO_PKG_VERSION")` for aiki version
- `std::env::consts::{OS, ARCH}` for platform
- `check_command_version` from `prerequisites.rs` for git/jj versions
- `RepoDetector` for repo state
- `TaskGraph` via `tasks::storage` + `tasks::graph` for task stats
- `global::global_sessions_dir()` for session listing
- Doctor.rs patterns for config state checks

Each collector returns `String` and is resilient — failures produce `<not available>` rather than aborting.

### `gh` CLI integration

```rust
fn check_gh_available() -> Result<()> {
    if which::which("gh").is_err() {
        return Err(AikiError::GhNotFound);
    }
    Ok(())
}

fn create_github_issue(repo: &str, title: &str, labels: &[&str], body: &str) -> Result<String> {
    check_gh_available()?;
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["issue", "create", "--repo", repo, "--title", title, "--body", body]);
    for label in labels {
        cmd.args(["--label", label]);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        return Err(AikiError::GhIssueFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
```

### Issue body templates

**Bug report:**
```markdown
## Bug Report

### Description
{user description}

### Diagnostic Snapshot

#### Environment
- aiki: {version}
- OS: {os} ({arch})
- Shell: {shell}
- git: {git_version}
- jj: {jj_version}

#### Repository
- Project root: {root}
- JJ: {jj_status}
- Git: {git_status}
- .aiki/: {aiki_status}
- Working copy change: {change_id}

#### Tasks
- Total: {total} ({open} open, {in_progress} in-progress, {closed} closed)
- In-progress: {list}

#### Sessions
- Active: {count}

#### Configuration
- Claude Code hooks: {status}
- Git hooks: {status}
- Plugins: {list}
- Instructions: {file}
```

**Feature request:**
```markdown
## Feature Request

### Description
{user description}

### Context
- aiki: {version}
- OS: {os} ({arch})
```

### Error handling

- `gh` not installed → `AikiError::GhNotFound` with install instructions
- `gh` not authenticated → captured from stderr, suggest `gh auth login`
- Not in a repo (for bug diagnostics) → sections show `<not available>`, command still works
- Each diagnostic section is independently resilient

### Testing

- Unit test: snapshot formatting produces valid markdown
- Unit test: `SupportCommands` enum parses correctly via clap
- Integration: `aiki support bug --dry-run "test"` prints report without calling `gh`

## Not in scope (for now)

- `aiki support chat` — future
- `aiki support docs` — future
- Clipboard copy of report
- Log file attachment
- Anonymization/redaction of paths
- Issue templates on the GitHub repo side
- `--title` separate from description (title = first line or truncated description)
