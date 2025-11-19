# Plan: Phase 8 - Zed Extension (One-Click Setup & Status UI)

## Problem
While Phase 6 provides ACP proxy support, setup requires manual CLI steps and users have no visual feedback about Aiki's status. This creates friction:
- Users must run `aiki init` and `aiki hooks install` manually
- No visual indication that Aiki is active and working
- Review results only visible via CLI commands
- Configuration requires editing JSON files
- No discoverability for Aiki features within Zed

**Current user flow (CLI-only):**
```bash
$ aiki init
$ aiki hooks install
# Edit ~/.config/zed/settings.json manually
# Restart Zed
# No visual feedback that it's working
```

### Solution
Build a thin Zed extension that provides one-click setup and visual status UI. The extension sits ABOVE the ACP proxy (Phase 6) and delegates all logic to the `aiki` CLI tool.

**Key Principle:** The extension is a UI/UX layer only. All real work happens in the `aiki` CLI.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  Zed Extension (UI Layer)                       │
│  - Command palette integration                  │
│  - Status bar indicator                         │
│  - Settings UI                                  │
│  - Notifications                                │
└──────────────────┬──────────────────────────────┘
                   │ Delegates to CLI
                   ↓
┌─────────────────────────────────────────────────┐
│  aiki CLI (All Logic)                           │
│  - aiki init                                    │
│  - aiki hooks install                           │
│  - aiki doctor                                  │
│  - aiki acp (proxy)                             │
└─────────────────────────────────────────────────┘
```

## What We Build

### Zed Extension Features
1. **One-click installation** - Run `aiki init` from command palette
2. **Status bar indicator** - Shows "Aiki ✓" or "Aiki ⚠" with current status
3. **Command palette commands** - Access Aiki features without CLI
4. **Settings UI** - Configure Aiki from Zed's settings panel
5. **Notifications** - Alert users to review failures or issues
6. **Health check** - Display `aiki doctor` results in UI

### Extension Implementation

```rust
// extension/src/lib.rs
use zed_extension_api as zed;

struct AikiExtension {
    status: AikiStatus,
}

enum AikiStatus {
    NotInstalled,
    Installed,
    Running,
    Error(String),
}

impl zed::Extension for AikiExtension {
    fn new() -> Self {
        AikiExtension {
            status: AikiStatus::NotInstalled,
        }
    }
    
    fn command_palette_entries(&mut self) -> Vec<CommandEntry> {
        vec![
            CommandEntry {
                name: "Aiki: Initialize Repository",
                action: || self.run_init(),
            },
            CommandEntry {
                name: "Aiki: Show Status",
                action: || self.show_status(),
            },
            CommandEntry {
                name: "Aiki: Run Health Check",
                action: || self.run_doctor(),
            },
            CommandEntry {
                name: "Aiki: Configure Policies",
                action: || self.open_config(),
            },
        ]
    }
    
    fn status_bar_items(&mut self) -> Vec<StatusBarItem> {
        vec![StatusBarItem {
            text: format!("Aiki {}", self.status_icon()),
            tooltip: self.status_tooltip(),
            on_click: || self.show_status(),
        }]
    }
    
    fn language_server_command(
        &mut self,
        language_server_id: &str,
    ) -> Result<Command> {
        // Wrap agent with aiki acp proxy
        if self.is_ai_agent(language_server_id) {
            return Ok(Command {
                command: "aiki",
                args: vec!["acp", self.agent_type(language_server_id)],
            });
        }
        Ok(Command::default())
    }
}

impl AikiExtension {
    fn run_init(&mut self) {
        // Execute: aiki init
        let output = std::process::Command::new("aiki")
            .arg("init")
            .output()
            .expect("Failed to run aiki init");
        
        if output.status.success() {
            self.status = AikiStatus::Installed;
            self.show_notification("Aiki initialized successfully");
        } else {
            self.status = AikiStatus::Error(
                String::from_utf8_lossy(&output.stderr).to_string()
            );
        }
    }
    
    fn run_doctor(&mut self) {
        // Execute: aiki doctor
        let output = std::process::Command::new("aiki")
            .arg("doctor")
            .output()
            .expect("Failed to run aiki doctor");
        
        let result = String::from_utf8_lossy(&output.stdout);
        self.show_panel("Aiki Health Check", &result);
    }
    
    fn status_icon(&self) -> &str {
        match self.status {
            AikiStatus::NotInstalled => "○",
            AikiStatus::Installed => "◐",
            AikiStatus::Running => "✓",
            AikiStatus::Error(_) => "⚠",
        }
    }
}
```

## User Experience

### Installation Flow
```
# User in Zed:
Cmd+Shift+P → "Install Aiki Extension"
  ↓
Extension downloaded from Zed marketplace
  ↓
Status bar shows: "Aiki ○" (not initialized)
  ↓
Click status bar → "Initialize Aiki in this repository?"
  ↓
Extension runs: aiki init && aiki hooks install
  ↓
Status bar updates: "Aiki ✓"
  ↓
Notification: "Aiki is ready! All AI changes will be tracked."
```

### Status Indicators
```
Aiki ○  - Not initialized (click to set up)
Aiki ◐  - Installed but not running
Aiki ✓  - Running, all checks passing
Aiki ⚠  - Error or health check failed
```

### Command Palette
```
Cmd+Shift+P:
  - Aiki: Initialize Repository
  - Aiki: Show Status
  - Aiki: Run Health Check
  - Aiki: Configure Policies
  - Aiki: Show Review Results
  - Aiki: Open Documentation
```

### Status Panel
```
┌─────────────────────────────────────┐
│ Aiki Status                     ✓   │
├─────────────────────────────────────┤
│ Repository: Initialized             │
│ ACP Proxy: Running                  │
│ Last Activity: 2m ago               │
│                                     │
│ Recent Reviews:                     │
│  ✓ auth.rs - 0 issues               │
│  ⚠ utils.rs - 2 issues caught       │
│                                     │
│ [Run Health Check] [View Settings]  │
└─────────────────────────────────────┘
```

## Value Delivered

### For End Users
- **Zero-friction setup** - Install extension, click "Initialize", done
- **Visual feedback** - Always know if Aiki is working
- **Discoverability** - Find Aiki features via command palette
- **Better UX** - No need to drop to terminal for basic tasks
- **Confidence** - Status indicator provides peace of mind

### For Aiki Adoption
- **Lower barrier to entry** - Non-technical users can set up Aiki
- **Professional appearance** - Feels like a native Zed feature
- **Increased visibility** - Extension marketplace exposure
- **Better onboarding** - Guided setup flow
- **Viral growth** - Easy to recommend ("Just install the extension")

## Technical Components

| Component | Complexity | Priority |
|-----------|------------|----------|
| Zed extension scaffold | Low | High |
| Command palette integration | Low | High |
| Status bar indicator | Low | High |
| CLI delegation (init, doctor) | Low | High |
| Settings UI schema | Medium | Medium |
| Notification system | Low | Medium |
| Status panel | Medium | Low |
| Extension marketplace submission | Low | Medium |

## Implementation Notes

### Extension Structure
```
extension/
├── extension.toml           # Extension metadata
├── Cargo.toml              # Rust dependencies
└── src/
    ├── lib.rs              # Main extension code
    └── commands.rs         # Command implementations
```

### Extension Manifest (extension.toml)
```toml
id = "aiki"
name = "Aiki"
description = "AI code provenance tracking and autonomous review"
version = "0.1.0"
schema_version = 1
authors = ["Aiki Team"]
repository = "https://github.com/your-org/aiki"
```

### Delegation Pattern
The extension never implements business logic. It always delegates to `aiki` CLI:

```rust
// ✅ GOOD: Delegate to CLI
fn run_init(&mut self) {
    Command::new("aiki").arg("init").spawn()?;
}

// ❌ BAD: Duplicate logic in extension
fn run_init(&mut self) {
    // Don't duplicate aiki init logic here!
}
```

### Error Handling
```rust
fn ensure_aiki_installed(&self) -> Result<()> {
    if !Command::new("aiki").arg("--version").status()?.success() {
        return Err(anyhow!(
            "Aiki CLI not found. Install via: cargo install aiki"
        ));
    }
    Ok(())
}
```

## Success Criteria

- ✅ Extension installable from Zed marketplace
- ✅ One-click "Initialize Repository" from command palette
- ✅ Status bar shows Aiki status (○/◐/✓/⚠)
- ✅ `aiki doctor` results displayed in panel
- ✅ Settings UI for common configurations
- ✅ Notifications for errors and important events
- ✅ All logic delegated to `aiki` CLI (no duplication)
- ✅ Works on macOS, Linux, Windows
- ✅ Documentation for extension users

## Timeline

- Extension scaffold + command palette: 1-2 days
- Status bar indicator: 1 day
- CLI delegation (init, doctor, hooks): 1 day
- Settings UI: 1-2 days
- Notifications: 1 day
- Testing + marketplace submission: 2 days

**Total: ~1.5 weeks**

## Why This Matters

**Before Phase 8 (CLI-only):**
```
User: "How do I set up Aiki?"
Docs: "Run these 5 terminal commands..."
User: *gets confused, gives up*
```

**After Phase 8 (Extension):**
```
User: "How do I set up Aiki?"
Docs: "Install the Zed extension, click Initialize"
User: *done in 30 seconds*
```

**Impact:**
- 10x easier onboarding
- Professional, polished UX
- Marketplace visibility
- Viral growth potential
- Makes Aiki feel "native" to Zed

## Phase Dependencies

**Depends on:**
- Phase 6 (ACP Support) - Extension wraps the `aiki acp` proxy
- Phase 3 (CLI Streamlining) - Extension delegates to `aiki doctor`

**Enables:**
- Phase 9 (Autonomous Review Flow) - Extension shows review results in UI
- Future: Extensions for other IDEs (Neovim, VSCode, JetBrains)
