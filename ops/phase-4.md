# Phase 4: Cryptographic Commit Signing - Implementation Plan

## Overview

Add cryptographic signing to AI-attributed changes using JJ's native commit signing capabilities. This provides tamper-proof provenance and enables enterprise compliance.

**Architecture**: Leverage JJ's built-in signing support (GPG, SSH, GPG-SM) to cryptographically sign all changes containing `[aiki]` metadata. No new dependencies required - uses standard signing tools already on users' systems.

**Key Insight**: JJ already supports commit signing. We just need to enable it automatically and integrate it with our provenance workflow.

## Goals

1. **Tamper-proof attribution** - Cryptographically verify AI provenance hasn't been altered
2. **Enterprise compliance** - Meet SOX, PCI-DSS, ISO 27001 audit requirements  
3. **Supply chain security** - Provide verifiable authorship for AI-generated code
4. **Automatic signing** - Configure once, works transparently thereafter

## Milestone 4.1: Automatic Signing Setup

**Goal**: Enable JJ commit signing automatically during `aiki init` with intelligent key detection.

### Tasks

1. Detect existing GPG/SSH keys on user's system
2. Configure JJ signing in `.jj/repo/config.toml`
3. Add `--signing` flag to `aiki init`
4. Support multiple backends (GPG, SSH, GPG-SM)
5. Add signing section to `aiki doctor` health checks
6. Document signing setup in README

### Key Detection Strategy

**GPG Detection:**
```bash
# Check for GPG keys
gpg --list-secret-keys --keyid-format LONG

# If found, use signing.backend = "gpg"
# Auto-detect key from git config user.email or user.signingkey
```

**SSH Detection:**
```bash
# Check for SSH keys
ls ~/.ssh/*.pub

# If found, use signing.backend = "ssh"
# Default to id_ed25519.pub or id_rsa.pub
```

**Priority:**
1. GPG keys (if available)
2. SSH keys (if available)
3. Prompt user to set up keys (wizard in Milestone 4.2)

### JJ Configuration

When signing is enabled, Aiki configures `.jj/repo/config.toml`:

```toml
[signing]
behavior = "own"        # Sign all commits you author when modified
backend = "gpg"         # or "ssh" or "gpgsm"

# GPG backend (auto-detected)
# Uses user.signingkey from git config or derives from user.email

# SSH backend (requires additional config)
# backend = "ssh"
# key = "~/.ssh/id_ed25519.pub"
# [signing.backends.ssh]
# allowed-signers = ".jj/allowed-signers"
```

### `aiki init --signing` Flow

```bash
$ aiki init --signing

Initializing Aiki...
✓ JJ repository initialized
✓ Git repository (colocated)

Configuring commit signing...
✓ Detected GPG key: 4ED556E9729E000F (user@example.com)
✓ Enabled commit signing (backend: gpg, behavior: own)

✓ Global hooks installed
✓ All AI changes will be cryptographically signed

Aiki initialized successfully!
```

### SSH Backend Setup

For SSH signing, Aiki needs to create `allowed-signers` file:

```bash
# .jj/allowed-signers
user@example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... user@example.com
```

This file maps email addresses to public keys for verification.

### Success Criteria

- ✅ `aiki init --signing` detects and configures available keys
- ✅ GPG backend works with existing GPG setup
- ✅ SSH backend works with existing SSH keys
- ✅ Signing configuration persists in `.jj/repo/config.toml`
- ✅ `aiki doctor` checks signing configuration
- ✅ Falls back gracefully if no keys available (prompts for setup)

## Milestone 4.2: Interactive Key Setup Wizard

**Goal**: Guide users through key setup if they don't have GPG/SSH keys configured.

### Tasks

1. Create `aiki sign setup` interactive wizard
2. Support GPG key generation workflow
3. Support SSH key workflow (link to existing keys)
4. Add key verification step
5. Handle git config integration (user.signingkey)
6. Test on fresh systems without keys

### Interactive Wizard Flow

```bash
$ aiki sign setup

Welcome to Aiki Signing Setup
==============================

Commit signing provides cryptographic proof that AI-attributed
changes haven't been tampered with.

Checking for existing keys...

No signing keys found. Let's set one up!

Which signing method would you like to use?

  1. GPG (recommended for maximum compatibility)
  2. SSH (simpler, requires JJ 0.12+)

Choice [1]: 1

GPG Key Setup
=============

Do you have an existing GPG key? [y/N]: n

Let's create a GPG key pair.

  Email: user@example.com
  Name: John Doe
  
Generating GPG key... (this may take a moment)
✓ GPG key created: 4ED556E9729E000F

Would you like to set this as your default git signing key? [Y/n]: y
✓ Updated git config user.signingkey

Configuring JJ signing...
✓ Enabled commit signing (backend: gpg)

Setup complete! All AI changes will now be signed.

Test your setup:
  1. Make an edit with Claude Code or Cursor
  2. Run: jj log -r @ --summary
  3. You should see: "Signed with GPG key 4ED556E9..."
```

### GPG Generation Command

```bash
gpg --batch --generate-key <<EOF
Key-Type: RSA
Key-Length: 4096
Subkey-Type: RSA
Subkey-Length: 4096
Name-Real: ${NAME}
Name-Email: ${EMAIL}
Expire-Date: 2y
%no-protection
%commit
EOF
```

### Success Criteria

- ✅ Wizard guides users through GPG setup
- ✅ Wizard supports existing SSH keys
- ✅ Generated keys work with JJ signing
- ✅ Git config updated appropriately
- ✅ Clear verification step at end
- ✅ User-friendly error messages

## Milestone 4.3: Signature Verification Commands

**Goal**: Add commands to verify signatures on AI-attributed changes.

### Tasks

1. Implement `aiki verify <change-id>` command
2. Check JJ signature validity
3. Verify `[aiki]` metadata present
4. Match signer to expected author
5. Add `--all` flag to verify all AI changes
6. Display detailed verification results

### Verification Logic

```rust
pub fn verify_change(change_id: &str) -> Result<VerificationResult> {
    // 1. Load the JJ commit
    let commit = load_commit(change_id)?;
    
    // 2. Check if commit is signed
    let signature = commit.signature()?;
    if signature.is_none() {
        return Ok(VerificationResult::NotSigned);
    }
    
    // 3. Verify signature validity
    let valid = verify_signature(&commit, &signature)?;
    if !valid {
        return Ok(VerificationResult::InvalidSignature);
    }
    
    // 4. Check for [aiki] metadata
    let provenance = ProvenanceRecord::from_description(commit.description())?;
    if provenance.is_none() {
        return Ok(VerificationResult::NoProvenance);
    }
    
    // 5. All checks passed
    Ok(VerificationResult::Valid {
        signer: signature.signer,
        provenance: provenance.unwrap(),
    })
}
```

### Command Output

```bash
$ aiki verify abc123

Verifying change abc123...

Signature:
  ✓ Valid GPG signature
  Key ID: 4ED556E9729E000F
  Signer: user@example.com
  Signed: 2025-01-15 14:32:10 UTC

Provenance:
  ✓ Metadata present and valid
  Agent: Claude Code
  Session: claude-session-abc123
  Tool: Edit
  Confidence: High
  Method: Hook

Result: VERIFIED ✓

This change has been cryptographically verified as:
- Created by Claude Code
- Signed by user@example.com  
- Not tampered with since signing

$ aiki verify --all

Verifying all AI-attributed changes...

✓ abc123 - Verified (Claude Code, 2025-01-15)
✓ def456 - Verified (Cursor, 2025-01-14)
✗ ghi789 - Invalid signature (tampered?)
✓ jkl012 - Verified (Claude Code, 2025-01-13)

Summary:
  Total: 247 changes
  Verified: 246 (99.6%)
  Failed: 1 (0.4%)
  
⚠ 1 change failed verification. Run 'aiki verify ghi789' for details.
```

### Success Criteria

- ✅ `aiki verify <change-id>` validates signatures
- ✅ Shows detailed signer and provenance info
- ✅ `--all` flag verifies entire repo
- ✅ Detects tampered signatures
- ✅ Clear pass/fail indication
- ✅ Performance: < 10ms per verification

## Milestone 4.4: Compliance Audit Reports

**Goal**: Generate signed provenance reports for enterprise compliance and audits.

### Tasks

1. Implement `aiki audit-report` command
2. Collect all AI-attributed changes
3. Verify all signatures
4. Generate report in multiple formats (text, JSON, HTML, PDF)
5. Include compliance metadata
6. Sign the report itself (meta-signing)

### Report Contents

**Summary Section:**
- Total changes analyzed
- AI vs human attribution breakdown
- Signature verification status
- Editor/agent breakdown
- Timeline of AI contributions

**Detailed Section:**
- Change-by-change provenance
- Signature details (key, timestamp)
- Agent session information
- Confidence levels

**Compliance Section:**
- Verification methodology
- Cryptographic standards used
- Tamper detection results
- Audit trail completeness

### Command Usage

```bash
$ aiki audit-report

Generating compliance report...

Analyzing repository...
  ✓ Found 1,247 AI-attributed changes
  ✓ Verified 1,247 signatures (100%)
  ✓ Analyzed commit timeline (2024-01-01 to 2025-01-15)

Generating report...
  ✓ Report created: aiki-audit-2025-01-15.html
  ✓ Report signed with GPG key 4ED556E9729E000F

Report summary:
  Total changes: 1,247
  AI changes: 1,247 (100%)
  Human changes: 0 (0%)
  Signature verification: PASSED
  
Open report: file:///.../aiki-audit-2025-01-15.html

$ aiki audit-report --format json --output audit.json

$ aiki audit-report --format pdf --output compliance-report.pdf
```

### Report Signing

The report itself should be signed:

```bash
# Generate report
$ aiki audit-report --format pdf

# Creates:
# - aiki-audit-2025-01-15.pdf
# - aiki-audit-2025-01-15.pdf.sig (GPG signature)

# Auditors can verify:
$ gpg --verify aiki-audit-2025-01-15.pdf.sig
```

### Success Criteria

- ✅ Reports include all AI changes
- ✅ Signature verification summary
- ✅ Multiple output formats (HTML, JSON, PDF)
- ✅ Reports are themselves signed
- ✅ Compliance-ready format
- ✅ Includes audit trail metadata

## Testing Strategy

### Unit Tests

- Signing configuration generation
- Key detection logic
- Verification algorithms
- Report generation

### Integration Tests

- `aiki init --signing` with existing GPG keys
- `aiki init --signing` with SSH keys
- `aiki sign setup` wizard flow
- `aiki verify` with signed changes
- `aiki verify` with unsigned changes
- `aiki verify` with tampered signatures
- Audit report generation

### End-to-End Tests

1. Fresh repo → `aiki init --signing` → verify signing works
2. Edit with Claude Code → verify change is signed
3. Manually edit `[aiki]` metadata → verify signature breaks
4. Generate audit report → verify report accuracy

## Success Metrics

### Completion Criteria

- ✅ Automatic signing configuration during `aiki init --signing`
- ✅ Key detection for GPG and SSH
- ✅ Interactive setup wizard for users without keys
- ✅ `aiki verify` validates signatures + provenance
- ✅ `aiki audit-report` generates compliance reports
- ✅ All AI changes signed automatically
- ✅ Zero performance impact on hooks

### User Experience Goals

- Setup takes < 2 minutes for users with existing keys
- Setup wizard completes in < 5 minutes for new users
- Verification is fast (< 100ms for single change)
- Doctor checks signing configuration
- Clear error messages for signing failures

### Technical Goals

- Support GPG, SSH, and GPG-SM backends
- Works on macOS, Linux, Windows
- Compatible with existing Git signing workflows
- No changes to provenance format
- Signing happens transparently after `aiki record-change`

## Dependencies

**No new Rust dependencies required!**

JJ already includes signing support via `jj-lib`:
- `jj_lib::signing` module
- `Signature` struct
- `verify_signature()` function

External dependencies (users must have installed):
- GPG: `gpg` command (for GPG backend)
- SSH: OpenSSH 8.0+ (for SSH backend, JJ 0.12+)
- Git: For reading `user.signingkey` config

## Architecture Notes

### How Signing Integrates

```
Editor Hook → aiki hooks handle → Provenance embedded
                                      ↓
                           jj describe -m "[aiki]..."
                                      ↓
                           JJ auto-signs (if enabled)
                                      ↓
                           Signed change with provenance
```

**Key Point**: Signing happens at the JJ level, not in Aiki code. We just:
1. Configure JJ signing (`.jj/repo/config.toml`)
2. Verify signatures using `jj-lib` APIs
3. Generate reports from signed data

### Why This Works

- **JJ handles signing** - We leverage existing, battle-tested code
- **Transparent to hooks** - No changes to hook handlers
- **Standard tools** - Uses GPG/SSH tools users already have
- **Provenance + signature** - Both in JJ, no separate database

### Security Considerations

- **Key management** - Users responsible for protecting private keys
- **Signature verification** - Uses standard GPG/SSH verification
- **Tamper detection** - Any edit to `[aiki]` block breaks signature
- **Report signing** - Audit reports are themselves signed

## Future Extensions

### Phase 5: Automatic Repair (doctor --fix)
- Auto-configure signing if keys detected
- Auto-generate keys if user approves
- Fix broken signing configuration

### Phase 11: Enterprise Compliance
- Team-wide signing policies
- Required signing enforcement
- Centralized key management
- Audit log export

## Next Steps

After Phase 4 completion:
- **Phase 5**: Autonomous Review & Self-Correction Loop
- **Phase 6**: Multi-Agent Provenance (Fallback Detection)
- **Phase 10**: Windsurf Support (3rd editor)
- **Phase 11**: Enterprise Compliance (builds on signing)
