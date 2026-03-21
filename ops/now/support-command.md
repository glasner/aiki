# Plan: `aiki support` Command

## Overview

Add an `aiki support` command group for user-facing support actions. First subcommand:

- **`aiki support issue`** — create a GitHub issue on `glasner/aiki` (bug or feature request)

Future subcommands (not in scope now): `chat`, `docs`.

## Design Decisions

### Command structure: `support issue` with `--bug`/`--feature` flags

```
aiki support issue "crashes on close"                # bug by default
aiki support issue "crashes on close" --bug          # explicit bug
aiki support issue "custom priorities" --feature     # feature request
aiki support issue                                   # interactive: prompts for description
```

**Why `issue` as a subcommand, not `bug`/`feature`:**
- Developers know what a GitHub issue is — lean into that
- `support` namespace accommodates non-issue subcommands (`chat`, `docs`) later
- One command, not two parallel code paths

**Why `--bug`/`--feature` flags, not `--tag`:**
- These are the two things we need; no reason to expose arbitrary labels
- `--bug` is the default (most common support case) — you usually don't need to type it
- Mutually exclusive flags, enforced by clap

### GitHub issue creation via `gh` CLI

- Shell out to `gh issue create` (same pattern as jj/git subprocess calls)
- Require `gh` to be installed and authenticated
- Detect with `which` crate (already a dependency)
- No new Rust dependencies needed

### Target repo

- Hardcoded to `glasner/aiki` via `--repo glasner/aiki` flag on `gh issue create`
- Works regardless of which repo the user is currently in

## Command Details

### `aiki support issue [description] [--bug|--feature]`

**Bug (default)** — collects diagnostic snapshot (subsumes the plan.md `aiki bug` design):

1. **Environment** — aiki version, OS/arch, shell, git version, jj version
2. **Repository state** — project root, jj/git/aiki init status, working copy change ID, jj status
3. **Task system** — task count by status, in-progress tasks, recent events
4. **Configuration** — hooks installed, plugins, instructions file
5. **Sessions** — active session count and details

**Feature (`--feature`)** — minimal context only:
- aiki version
- OS/arch

**Creates GitHub issue:**
- **Title:** user-provided description (or prompted interactively)
- **Labels:** `bug` (default) or `enhancement` (with `--feature`)
- **Body:** markdown report with description + diagnostics (bug) or minimal context (feature)

**Flow:**
1. Check `gh` is available
2. If no description arg, prompt: `Describe the issue: `
3. Collect diagnostics (full for bug, minimal for feature)
4. Show preview of what will be submitted
5. Confirm: `Create issue on glasner/aiki? [Y/n]`
6. Run `gh issue create --repo glasner/aiki --title "..." --label {label} --body "..."`
7. Print issue URL on success

**Flags:**
- `--bug` — file as bug report with full diagnostics (default)
- `--feature` — file as feature request with minimal context
- `--dry-run` — print the report to stdout without creating an issue

## Implementation

### Files to create/modify

1. **`src/commands/support.rs`** (new) — command implementation
   - `SupportCommands` enum with `Issue` variant
   - `pub fn run(command: SupportCommands) -> Result<()>`
   - `fn run_issue(description: Option<String>, is_feature: bool, dry_run: bool) -> Result<()>`
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

### CLI definition (clap)

```rust
#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum SupportCommands {
    /// Create a GitHub issue on the aiki repo
    Issue {
        /// Description of the issue (used as title)
        description: Option<String>,

        /// File as a feature request instead of a bug report
        #[arg(long, conflicts_with = "bug")]
        feature: bool,

        /// File as a bug report (default)
        #[arg(long)]
        bug: bool,

        /// Print report to stdout without creating an issue
        #[arg(long)]
        dry_run: bool,
    },
}
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
- Integration: `aiki support issue --dry-run "test"` prints report without calling `gh`

## Not in scope (for now)

- `aiki support chat` — future
- `aiki support docs` — future
- Clipboard copy of report
- Log file attachment
- Anonymization/redaction of paths
- Issue templates on the GitHub repo side
- Separate `--title` flag (title = description, truncated if long)
