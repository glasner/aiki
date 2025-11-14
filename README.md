# Aiki - AI Code Provenance Tracking

Aiki automatically tracks which AI agents contributed to your codebase, providing transparent attribution for AI-generated code changes.

## Features

- **Automatic Provenance Tracking**: Records AI agent changes in Jujutsu (jj) change descriptions
- **Line-Level Attribution**: See which AI agent wrote each line of code with `aiki blame`
- **Git Co-Author Attribution**: Automatically adds `Co-authored-by:` lines to Git commits for AI contributors
- **Claude Code Integration**: Seamless integration via PostToolUse hooks

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
- Configure Claude Code integration
- Offer to automatically restart Claude Code if it's running

## Usage

### View AI Attribution for a File

```bash
aiki blame src/main.rs
```

Output shows which AI agent contributed each line:

```
abc12345 (ClaudeCode   session-123  High  )    1| fn main() {
abc12345 (ClaudeCode   session-123  High  )    2|     println!("Hello, world!");
def67890 (Unknown      -            -     )    3|     // Human-written comment
abc12345 (ClaudeCode   session-123  High  )    4| }
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
# Show authors for working copy changes (default)
aiki authors
Claude Code <claude-code@anthropic.ai>

# Show authors for Git staged changes
aiki authors --changes=staged
Claude Code <claude-code@anthropic.ai>

# Git trailer format (for commit messages)
aiki authors --format=git --changes=staged
Co-authored-by: Claude Code <claude-code@anthropic.ai>

# JSON format (for tooling)
aiki authors --format=json
[{"name":"Claude Code","email":"claude-code@anthropic.ai","agent_type":"ClaudeCode"}]
```

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

**Phase 2: Multi-Agent Support** - Planned
- Cursor integration
- Windsurf integration
- Unified multi-agent tracking

See [ops/ROADMAP.md](ops/ROADMAP.md) for the complete roadmap.

## Contributing

This is currently an early-stage project. Contributions are welcome! Please see the [ops/](ops/) directory for architecture documentation.

## License

[License information to be added]

## Acknowledgments

Built with:
- [Jujutsu](https://martinvonz.github.io/jj/) - Next-generation version control
- [Claude Code](https://claude.ai/code) - AI-powered code editor
