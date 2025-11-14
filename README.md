# Aiki - AI Code Provenance Tracking

Aiki automatically tracks which AI agents contributed to your codebase, providing transparent attribution for AI-generated code changes.

## Features

- **Automatic Provenance Tracking**: Records AI agent changes in Jujutsu (jj) change descriptions
- **Line-Level Attribution**: See which AI agent wrote each line of code with `aiki blame`
- **Git Co-Author Attribution**: Automatically adds `Co-authored-by:` lines to Git commits for AI contributors
- **Multi-Editor Support**: Seamless integration with Claude Code and Cursor via hooks

## Quick Start

### Prerequisites

- Jujutsu (jj) - [Installation guide](https://martinvonz.github.io/jj/latest/install-and-setup/)
- Git (for co-author attribution feature)
- Rust toolchain (for building from source)

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/aiki.git
cd aiki

# Build and install
cd cli
cargo install --path .
```

### Initialize Aiki in Your Project

```bash
cd your-project
aiki init
```

This will:
- Initialize Jujutsu if not already present
- Create `.aiki/` directory structure
- Install Git hooks for automatic co-author attribution
- Configure Claude Code hooks (per-repository in `.claude/settings.json`)
- Configure Cursor hooks (global user hooks in `~/.cursor/hooks.json`)
- Offer to automatically restart editors if they're running

### Check Configuration Health

```bash
aiki doctor
```

This checks:
- Repository setup (JJ, Git, Aiki directory)
- Global hooks installation (Git, Claude Code, Cursor)
- Local configuration (Git core.hooksPath)

Add `--fix` to automatically repair issues:
```bash
aiki doctor --fix
```

## Usage

### View AI Attribution for a File

```bash
aiki blame src/main.rs
```

Output shows which AI agent contributed each line:

```
abc12345 (Claude Code   session-123  High  )    1| fn main() {
abc12345 (Claude Code   session-123  High  )    2|     println!("Hello, world!");
def67890 (Cursor        session-456  High  )    3|     // Added by Cursor
abc12345 (Claude Code   session-123  High  )    4| }
```

Filter by specific editor:

```bash
# Show only Claude Code contributions
aiki blame src/main.rs --agent claude-code

# Show only Cursor contributions
aiki blame src/main.rs --agent cursor
```

### Automatic Git Co-Author Attribution

When you commit changes that include AI-contributed code, Aiki automatically adds co-author lines:

```bash
git add src/main.rs
git commit -m "Add main function"

# Aiki automatically adds:
# Co-authored-by: Claude Code <claude-code@anthropic.ai>
```

These co-author lines appear in:
- `git log` output
- GitHub commit history
- Git blame annotations

### View AI Authors

You can view which AI agents contributed to your changes:

```bash
# Show all AI authors for working copy changes (default)
aiki authors
Claude Code <claude-code@anthropic.ai>
Cursor <cursor@cursor.sh>

# Show authors for Git staged changes
aiki authors --changes=staged
Claude Code <claude-code@anthropic.ai>
Cursor <cursor@cursor.sh>

# Git trailer format (for commit messages)
aiki authors --format=git --changes=staged
Co-authored-by: Claude Code <claude-code@anthropic.ai>
Co-authored-by: Cursor <cursor@cursor.sh>

# JSON format (for tooling)
aiki authors --format=json
[
  {"name":"Claude Code","email":"claude-code@anthropic.ai","agent_type":"ClaudeCode"},
  {"name":"Cursor","email":"cursor@cursor.sh","agent_type":"Cursor"}
]
```

**Note:** The `authors` command always shows all AI contributors. Use `blame --agent <type>` to filter by specific editor.

## How It Works

### Provenance Tracking

Aiki uses Claude Code's PostToolUse hooks to automatically record metadata when AI agents edit files. This metadata is stored in Jujutsu change descriptions using the `[aiki]...[/aiki]` format:

```
[aiki]
agent=claude-code
session=claude-session-abc123
tool=Edit
confidence=High
method=Hook
[/aiki]
```

### Line-Level Attribution

The `aiki blame` command uses Jujutsu's built-in annotation capabilities to trace each line back to its originating change, then extracts the AI agent information from the change description.

### Git Integration

The `prepare-commit-msg` Git hook:
1. Analyzes staged changes to identify modified line ranges
2. Uses blame logic to attribute those lines to AI agents
3. Formats attributions as `Co-authored-by:` lines
4. Appends them to the commit message

The hook chains to any previously configured hooks, so it won't interfere with existing Git workflows.

## Editor Support

Aiki currently supports:

### Claude Code
- **Hook type**: PostToolUse hooks
- **Configuration**: Per-repository in `.claude/settings.json`
- **Installation**: Automatic during `aiki init`
- **Scope**: Each repository has its own configuration

### Cursor
- **Hook type**: `afterFileEdit` hooks
- **Configuration**: Global user-level in `~/.cursor/hooks.json`
- **Installation**: Automatic during `aiki init`
- **Scope**: Configured once globally, works for all projects

### Hook Preservation

Aiki preserves existing hooks:
- **Claude Code**: Preserves existing marketplace configs and plugin settings
- **Cursor**: Appends to hook arrays, so existing `afterFileEdit` hooks continue to work

Since Cursor hooks are global, you only need to restart Cursor once after your first `aiki init` - subsequent projects will automatically have Cursor support.

## Architecture

Aiki is built on **Jujutsu (jj)**, not Git. Key concepts:

- **Change**: The atomic unit in jj (mutable, with stable change IDs)
- **Change ID**: Stable identifier that persists across rewrites
- **Change Description**: Where Aiki stores provenance metadata

While Aiki tracks changes in Jujutsu, it provides Git integration for broader compatibility.

For more details, see [CLAUDE.md](CLAUDE.md) for terminology guidelines and [ops/phase-1.md](ops/phase-1.md) for implementation details.

## Project Structure

```
aiki/
├── cli/                      # Main Rust CLI application
│   ├── src/
│   │   ├── blame.rs         # Line-level attribution logic
│   │   ├── git_coauthors.rs # Git co-author extraction
│   │   ├── config.rs        # Initialization and configuration
│   │   ├── provenance.rs    # Metadata parsing
│   │   └── record_change.rs # Hook integration
│   └── templates/
│       └── prepare-commit-msg.sh  # Git hook template
├── claude-code-plugin/       # Claude Code marketplace plugin
└── ops/                      # Planning and architecture docs
    ├── phase-1.md           # Current phase implementation
    └── ROADMAP.md           # Long-term vision
```

## Development Status

**Phase 1: Claude Code Provenance** - Complete ✅
- ✅ Milestone 1.1: Hook-based tracking
- ✅ Milestone 1.2: Line-level attribution with `aiki blame`
- ✅ Milestone 1.3: Git co-author attribution

**Phase 2: Cursor Support** - In Progress 🚧
- ✅ Milestone 2.1: Cursor hook installation and provenance tracking
- ✅ Milestone 2.2: Multi-editor query support with filtering

**Phase 3+: Additional Features** - Planned
- Windsurf integration (Phase 10)
- Hook management CLI enhancements (Phase 3)
- Cryptographic commit signing (Phase 4)
- Autonomous review & self-correction (Phase 5)

See [ops/ROADMAP.md](ops/ROADMAP.md) for the complete roadmap.

## Contributing

This is currently an early-stage project. Contributions are welcome! Please see the [ops/](ops/) directory for architecture documentation.

## License

[License information to be added]

## Acknowledgments

Built with:
- [Jujutsu](https://martinvonz.github.io/jj/) - Next-generation version control
- [Claude Code](https://claude.ai/code) - AI-powered code editor
