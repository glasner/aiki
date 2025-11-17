# Aiki Plugin for Claude Code

Automatic provenance tracking for AI-generated code changes.

## What it does

The Aiki plugin automatically tracks every code change made by Claude Code, providing:

- **100% accurate attribution** - Hook-based detection captures every Edit/Write operation
- **Complete history** - Records all changes in a queryable database
- **JJ integration** - Links changes to JJ version control operations
- **Session tracking** - Groups related changes by Claude Code session
- **Zero overhead** - Fast, lightweight tracking (~15-25ms per edit)

## How it works

1. **Automatic tracking**: When Claude Code edits or writes a file, the PostToolUse hook triggers
2. **Event handling**: `aiki hooks handle` processes the event through the flow engine
3. **Provenance recording**: Metadata is embedded in JJ change descriptions
4. **JJ snapshot**: The working copy state is automatically captured and a new change is created
5. **Flow execution**: The core flow runs custom provenance functions

## Requirements

- **Aiki CLI** installed: `cargo install --path . --bin aiki`
- **JJ repository** initialized: `aiki init` in your project
- **Git repository** (JJ runs in colocated mode with Git)

## Installation

### Automatic (Recommended)

This plugin is automatically configured when you run `aiki init` in your project:

```bash
cd your-project
aiki init
```

Then open the project in Claude Code and trust the repository when prompted. The plugin will install automatically.

### Manual Installation

If you want to install the plugin manually:

```bash
/plugin marketplace add /path/to/aiki/claude-code-plugin
/plugin install aiki@aiki
```

Then restart Claude Code.

## Usage

Once installed, the plugin works automatically. No commands needed!

Every time Claude Code:
- Edits a file (`Edit` tool)
- Writes a file (`Write` tool)

The change is automatically recorded with:
- File path
- Old and new content
- Session ID
- Timestamp
- JJ commit ID

## Querying Attribution

Use the Aiki CLI to query provenance data:

```bash
# View recent activity
aiki status

# View attribution history
aiki history --limit 10

# See line-by-line attribution
aiki blame <file>

# View statistics
aiki stats
```

## Configuration

The plugin configuration is stored in `.claude/settings.json`:

```json
{
  "extraKnownMarketplaces": {
    "aiki": {
      "source": {
        "source": "directory",
        "path": "./claude-code-plugin"
      }
    }
  },
  "enabledPlugins": {
    "aiki@aiki": true
  }
}
```

## Disabling

To temporarily disable tracking:

```bash
/plugin disable aiki
```

To re-enable:

```bash
/plugin enable aiki
```

## Uninstalling

```bash
/plugin remove aiki
```

## Development

The plugin is part of the Aiki repository:

- Plugin files: `claude-code-plugin/`
- CLI implementation: `cli/`
- Documentation: `ops/phase-1.md`

## Architecture

The plugin uses Claude Code's PostToolUse hook system to capture changes at the source:

```
Claude Code (Edit/Write) 
    ↓
PostToolUse Hook (automatic)
    ↓
aiki hooks handle --agent claude-code --event PostToolUse
    ↓
Event Bus → Flow Engine
    ↓
Core Flow: build_description() → jj describe → jj new
    ↓
Provenance in change description ([aiki]...[/aiki])
```

## Support

- **Issues**: https://github.com/glasner/aiki/issues
- **Documentation**: `ops/phase-1.md` in the repository
- **License**: MIT

## Phase Roadmap

- **Phase 1 (Current)**: Claude Code hook integration with 100% accuracy
- **Phase 2**: Multi-editor support (Cursor, Windsurf, etc.)
- **Phase 3**: Autonomous review and self-correction loop
