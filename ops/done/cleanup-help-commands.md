# Cleanup: Remove nested `help` subcommands, keep root `help`

## Problem

Clap's `#[derive(Subcommand)]` automatically adds a `help` subcommand at **every level** of the command hierarchy. This means things like `aiki review issue help` and `aiki task help` work as subcommands.

The standard CLI convention is:
- **Root level:** `app help <command>` — keep this (e.g., `aiki help task`)
- **Nested levels:** use `--help` / `-h` flags only (e.g., `aiki task --help`, not `aiki task help`)

Examples: `git help commit` (root), `cargo help build` (root) — but `git commit help` doesn't exist.

## Audit Results

### Commands with `help` subcommand today

| Command path | Enum / struct | File | Line | Action |
|---|---|---|---|---|
| `aiki` (root) | `Cli` | `main.rs` | 33 | **Keep** |
| `aiki task` | `TaskCommands` | `commands/task.rs` | 240 | Disable |
| `aiki task template` | `TemplateCommands` | `commands/task.rs` | 227 | Disable |
| `aiki review` | `ReviewSubcommands` | `commands/review.rs` | 293 | Disable |
| `aiki review issue` | `ReviewIssueSubcommands` | `commands/review.rs` | 316 | Disable |
| `aiki session` | `SessionCommands` | `commands/session.rs` | 10 | Disable |
| `aiki plugin` | `PluginCommands` | `commands/plugin.rs` | 15 | Disable |
| `aiki epic` | `EpicCommands` | `commands/epic.rs` | 29 | Disable |
| `aiki build` | `BuildSubcommands` | `commands/build.rs` | 34 | Disable |
| `aiki event` (hidden) | `EventCommands` | `main.rs` | 192 | Disable |
| `aiki hooks` (hidden) | `HooksCommands` | `main.rs` | 199 | Disable |

### `--help` / `-h` support

All commands already support `--help` and `-h` — clap provides this by default and it's not been disabled anywhere. No changes needed here.

## Fix

Add `#[command(disable_help_subcommand = true)]` to all **nested** `#[derive(Subcommand)]` enums. Leave the root `Cli` struct unchanged so `aiki help <command>` continues to work.

### In `cli/src/main.rs`

1. **`EventCommands`** (line 192): Add `#[command(disable_help_subcommand = true)]`
2. **`HooksCommands`** (line 199): Add `#[command(disable_help_subcommand = true)]`

### In `cli/src/commands/task.rs`

3. **`TaskCommands`** (line 240): Add `#[command(disable_help_subcommand = true)]`
4. **`TemplateCommands`** (line 227): Add `#[command(disable_help_subcommand = true)]`

### In `cli/src/commands/review.rs`

5. **`ReviewSubcommands`** (line 293): Add `#[command(disable_help_subcommand = true)]`
6. **`ReviewIssueSubcommands`** (line 316): Add `#[command(disable_help_subcommand = true)]`

### In `cli/src/commands/session.rs`

7. **`SessionCommands`** (line 10): Add `#[command(disable_help_subcommand = true)]`

### In `cli/src/commands/plugin.rs`

8. **`PluginCommands`** (line 15): Add `#[command(disable_help_subcommand = true)]`

### In `cli/src/commands/epic.rs`

9. **`EpicCommands`** (line 29): Add `#[command(disable_help_subcommand = true)]`

### In `cli/src/commands/build.rs`

10. **`BuildSubcommands`** (line 34): Add `#[command(disable_help_subcommand = true)]`

## Implementation Pattern

For each nested enum, change:

```rust
#[derive(Subcommand)]
pub enum FooCommands {
```

to:

```rust
#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum FooCommands {
```

Do **not** change the root `Cli` struct — `aiki help task` should keep working.

## Verification

After changes, confirm:
- `aiki help` → works (shows root help)
- `aiki help task` → works (shows task help)
- `aiki --help` / `aiki -h` → works
- `aiki task help` → error (not a valid subcommand)
- `aiki task --help` / `aiki task -h` → works
- `aiki review issue help` → error
- `aiki review issue --help` → works
- Same pattern for all nested commands
