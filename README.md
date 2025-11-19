changed
11

1# Aiki - AI Code Provenance Tracking

Aiki automatically tracks which AI agents contributed to your codebase, providing transparent attribution for AI-generated code changes.

## Features

- **Automatic Provenance Tracking**: Records AI agent changes in Jujutsu (jj) change descriptions
- **Line-Level Attribution**: See which AI agent wrote each line of code with `aiki blame`
- **Git Co-Author Attribution**: Automatically adds `Co-authored-by:` lines to Git commits for AI contributors
- **Multi-Editor Support**: Seamless integration with Claude Code and Cursor via hooks

## Quick Start

### Prerequisites

- Git (for co-author attribution feature)
- Rust toolchain (for building from source)

**Note:** Jujutsu (jj) is bundled directly into Aiki via `jj-lib`, so you don't need to install it separately.

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
- Configure commit signing (automatically detects GPG/SSH keys)
- Install Git hooks for automatic co-author attribution
- Configure Claude Code hooks (global user hooks in `~/.claude/settings.json`)
- Configure Cursor hooks (global user hooks in `~/.cursor/hooks.json`)
- Offer to automatically restart editors if they're running

### Commit Signing

Aiki automatically configures cryptographic signing for all AI-attributed changes to provide tamper-proof provenance.

**Automatic Setup During Init:**

During `aiki init`, Aiki detects your existing signing keys in priority order:
1. Git signing configuration (if already set up)
2. GPG keys (industry standard)
3. SSH keys (simpler alternative)

If keys are found, signing is configured automatically. If not, you'll be prompted:

```bash
⚠ No signing keys detected

Commit signing provides cryptographic proof of AI authorship.

What would you like to do?
  1. Generate new signing key (recommended)
  2. I have a key, let me specify it manually
  3. Skip signing for now
Choice [1]:
```

**Setting Up Signing Later:**

If you skipped signing during init, you can set it up anytime:

```bash
aiki doctor --fix
```

The doctor command will detect missing signing configuration and offer to set it up interactively.

**Why Signing Matters:**

- **Tamper-proof**: Cryptographically proves AI-attributed changes haven't been altered
- **Enterprise compliance**: Meets SOX, PCI-DSS, ISO 27001 audit requirements
- **Supply chain security**: Provides verifiable authorship for AI-generated code
- **Automatic**: Once configured, works transparently on every change

**Supported Backends:**

- **GPG**: Maximum compatibility, works with existing GPG infrastructure (auto-generates RSA 4096-bit keys)
- **SSH**: Simpler setup, auto-generates ed25519 keys (requires JJ 0.12+)

**Key Generation:**

The wizard can automatically generate keys for you:
- **GPG**: Creates a 4096-bit RSA key with 2-year expiration
- **SSH**: Creates an ed25519 key at `~/.ssh/id_ed25519_aiki`

Or you can specify an existing key manually during setup.

**Check Signing Status:**

```bash
aiki doctor
```

The doctor command checks:
- Whether signing is configured
- Whether your signing key is accessible
- Which backend you're using (GPG/SSH)
- Offers to set up signing with `--fix` if not configured

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

Verify cryptographic signatures on changes:

```bash
# Show signature status for each line
aiki blame src/main.rs --verify
```

Output with signature verification:

```
✓ abc12345 (Claude Code   session-123  High  )    1| fn main() {
✓ abc12345 (Claude Code   session-123  High  )    2|     println!("Hello");
⚠ def67890 (Cursor        session-456  High  )    3|     // unsigned
```

**Signature indicators:**
- **✓** - Valid cryptographic signature
- **✗** - Invalid or tampered signature
- **⚠** - No signature (unsigned change)
- **?** - Unknown signature status

**Note:** Verification is slower than regular blame as it checks each change's signature. Use `--verify` when you need to ensure changes haven't been tampered with.

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

### Verify Cryptographic Signatures

Verify the cryptographic signature and provenance metadata on a change:

```bash
# Verify the working copy change (default)
aiki verify

# Verify a specific change by ID
aiki verify abc123

# Verify a revision expression
aiki verify @-
```

Example output for a verified change:

```
Verifying change abc123...

Signature:
  ✓ Valid GPG signature
  Signer: John Doe <user@example.com>
  Key ID: 4ED556E9729E000F

Provenance:
  ✓ Metadata present and valid
  Agent: Claude Code
  Session: claude-session-abc123
  Tool: Edit
  Confidence: High

Result: VERIFIED ✓
```

Example output for an unsigned change:

```
Verifying change abc123...

Signature:
  ⚠ Not signed

Provenance:
  ✓ Metadata present and valid
  Agent: Claude Code
  Session: claude-session-abc123

Result: UNVERIFIED (no signature)
```

**Signature Status:**
- **VERIFIED ✓**: Valid signature + AI provenance metadata
- **SIGNED**: Valid signature but no AI metadata (not an AI change)
- **FAILED ✗**: Invalid or tampered signature
- **UNVERIFIED**: Change has AI metadata but no signature
- **NOT AN AI CHANGE**: No signature and no AI metadata

**Note:** Verification uses JJ's native signature verification, which supports GPG, SSH, and GPG-SM backends.

### Advanced Commands

#### Install Global Hooks

Install Aiki hooks globally for all editors and repositories:

```bash
aiki hooks install
```

This is an alternative to `aiki init` that:
- Installs global Git hooks for all repositories
- Configures Claude Code hooks globally
- Configures Cursor hooks globally
- Automatically detects and offers to restart running editors

**When to use:**
- Setting up Aiki for the first time across all projects
- After reinstalling or updating Aiki
- When hooks become misconfigured

**Note:** After running `aiki hooks install`, individual repositories will be automatically initialized when you first use them with Claude Code or Cursor.

#### ACP Proxy Server

Run Aiki as an ACP (Agent Client Protocol) bidirectional proxy:

```bash
# Proxy between an IDE and Claude Code
aiki acp claude-code

# Proxy with a custom agent binary
aiki acp claude-code --bin /path/to/custom-agent

# Proxy with agent arguments
aiki acp cursor -- --verbose --debug
```

The ACP proxy allows Aiki to:
- Intercept communication between IDEs (Zed, Neovim) and AI agents
- Track tool calls and file changes in real-time
- Record provenance with both client (IDE) and agent information
- Support future IDE integrations without custom hooks

**Supported agents:** `claude-code`, `cursor`, `codex`


### Benchmark Performance

Test and compare the performance of different Aiki workflows:

```bash
# Run the core workflow benchmark
aiki benchmark aiki/core
```

This benchmarks the complete Aiki workflow:
- Repository initialization with Git and Jujutsu
- SessionStart event (fires during `aiki init`)
- PostChange events (fires on each file edit)
- Total execution time

Example output:

```
Running benchmark: aiki/core

Phase 1: Repository Setup
  ✓ Git init: 35.1ms
  ✓ JJ init: 48.2ms

Phase 2: Aiki Setup
  ✓ SessionStart: 104.5ms

Phase 3: File Operations (10 iterations)
  ✓ File edit 1
    PostChange: 2.3ms
  ✓ File edit 2
    PostChange: 2.2ms
  ...

Event Timing:
  SessionStart (1 occurrences):
    Median: 104.5ms
    Range: 104.5ms - 104.5ms
  PostChange (10 occurrences):
    Median: 2.3ms
    Range: 2.2ms - 3.0ms

Total benchmark time: 402ms

Comparison to Previous Run:
  Previous total: 367ms
  Current total:  402ms
  Change: +35ms (+9.5%) 🔴 (slower)

  Event-level comparison:
    SessionStart:
      Previous: 99.7ms (median)
      Current:  104.5ms (median)
      Change:   +4.8ms (+4.8%) 🔴 (slower)
    PostChange:
      Previous: 2.3ms (median)
      Current:  2.3ms (median)
      Change:   +0.0ms (+0.0%) 🟢 (no change)
```

**Features:**

- **Event-level timing**: Shows performance for each hook event (SessionStart, PostChange)
- **Millisecond precision**: All timings displayed in milliseconds for accuracy
- **Statistical metrics**: Shows median, min, max, and count for each event type
- **Performance comparison**: Compares current run to previous with % change indicators
- **Result persistence**: Stores results in `.aiki/benchmarks/{flow}/` with:
  - `results.txt` - Human-readable benchmark report
  - `metrics.json` - Machine-readable event timing data

**Running multiple iterations:**

```bash
# Run 10 benchmark iterations for statistical reliability
for i in {1..10}; do aiki benchmark aiki/core; done
```

This helps identify performance variations and ensures consistent timing measurements across runs.

## How It Works

### Provenance Tracking

Aiki uses hooks to automatically record metadata when AI agents edit files. This metadata is stored in Jujutsu change descriptions using the `[aiki]...[/aiki]` format:

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

### Zed (via Agent Client Protocol)
- **Integration type**: ACP bidirectional proxy
- **Configuration**: Agent servers in `~/.config/zed/settings.json` (macOS) or `~/.config/zed/settings.json` (Linux)
- **Installation**: Automatic during `aiki init`
- **Agent binaries**: Automatically uses Zed's installed agents (no separate installation needed)
- **Supported agents**: Claude Code, Codex, Gemini
- **Requirements**: 
  - Zed editor installed
  - Agent enabled in Zed (create a thread with the agent once to trigger installation)
  - Node.js 18+ (for Node.js-based agents like Claude Code)
- **Scope**: Configured once globally, works for all projects

**How it works**: When you use an AI agent in Zed, Aiki acts as a transparent proxy, observing tool calls and automatically recording provenance metadata in your Jujutsu changes.

**Setup verification**: Run `aiki doctor` to check if Zed and agent binaries are properly detected.

### Claude Code (Standalone)
- **Hook type**: SessionStart and PostToolUse hooks
- **Configuration**: Global user-level in `~/.claude/settings.json`
- **Installation**: Automatic during `aiki init`
- **Scope**: Configured once globally, works for all projects

### Cursor
- **Hook type**: `afterFileEdit` hooks
- **Configuration**: Global user-level in `~/.cursor/hooks.json`
- **Installation**: Automatic during `aiki init`
- **Scope**: Configured once globally, works for all projects

### Hook Preservation

Aiki preserves existing hooks:
- **Claude Code**: Preserves existing marketplace configs and plugin settings
- **Cursor**: Appends to hook arrays, so existing `afterFileEdit` hooks continue to work

Since both editors use global hooks, you only need to restart your editors once after your first `aiki init` - subsequent projects will automatically have full Aiki support.

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
│   │   ├── commands/        # CLI command implementations
│   │   │   ├── init.rs      # Repository initialization
│   │   │   ├── doctor.rs    # Health checks and diagnostics
│   │   │   ├── blame.rs     # Line-level attribution
│   │   │   ├── authors.rs   # AI author extraction
│   │   │   ├── verify.rs    # Signature verification
│   │   │   ├── benchmark.rs # Performance testing
│   │   │   ├── hooks.rs     # Hook management
│   │   │   └── acp.rs       # ACP proxy server (Phase 6)
│   │   ├── flows/           # Flow execution system
│   │   ├── blame.rs         # Blame logic module
│   │   ├── authors.rs       # Authors logic module
│   │   ├── verify.rs        # Verification logic module
│   │   ├── provenance.rs    # Metadata parsing
│   │   ├── error.rs         # Error types
│   │   └── main.rs          # CLI entry point
│   └── templates/
│       └── prepare-commit-msg.sh  # Git hook template
└── ops/                      # Planning and architecture docs
    ├── done/                # Completed phases
    │   ├── phase-1.md       # Claude Code provenance
    │   ├── phase-2.md       # Cursor support
    │   ├── phase-4.md       # Cryptographic signing
    │   └── phase-6.md       # ACP support
    └── ROADMAP.md           # Long-term vision
```

## Development Status

See [ops/ROADMAP.md](ops/ROADMAP.md) and [ops/phase-4.md](ops/phase-4.md) for the complete roadmap.

## Contributing

This is currently an early-stage project. Contributions are welcome! Please see the [ops/](ops/) directory for architecture documentation.

## License

[License information to be added]

## Acknowledgments

Built with:
- [Jujutsu](https://martinvonz.github.io/jj/) - Next-generation version control
- [Claude Code](https://claude.ai/code) - AI-powered code editor
