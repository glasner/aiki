# Remove `aiki verify` and Signing Code

## Motivation

The `aiki verify` command and the signing subsystem (GPG/SSH key detection, setup wizard, JJ signing configuration) add significant complexity but provide little practical value today. Signing was built speculatively for a "cryptographic proof of AI authorship" story that never materialized into real user demand. Removing it simplifies onboarding (`aiki init` no longer prompts about keys), reduces maintenance burden, and shrinks the binary.

## Scope

Remove all code related to:
1. The `aiki verify` command
2. The `signing` module (key detection, setup wizard, config)
3. The `--verify` flag on `aiki blame`
4. Signing-related error variants
5. Signing configuration in `aiki init` and `aiki doctor`
6. The `update_jj_signing_config` helper in `config.rs`
7. Documentation references

## Files to Change

### Delete entirely
| File | What it is |
|------|-----------|
| `cli/src/verify.rs` | `verify_change()`, `SignatureStatus`, `VerificationResult`, `format_verification_result()` |
| `cli/src/commands/verify.rs` | `aiki verify` command handler |
| `cli/src/signing/mod.rs` | Re-exports for signing module |
| `cli/src/signing/config.rs` | `SigningConfig`, `SigningBackend`, key detection functions |
| `cli/src/signing/setup_wizard.rs` | `SignSetupWizard`, interactive key generation |
| `cli/tests/verify_tests.rs` | Integration tests for `aiki verify` command |
| `cli/tests/signing_tests.rs` | Integration tests for signing configuration |

### Modify

#### `cli/src/main.rs`
- Remove `mod signing;` (line 22)
- Remove `mod verify;` (line 28)
- Remove the `Verify { revision }` variant from `Commands` enum (lines 88–93)
- Remove `Commands::Verify { revision } => commands::verify::run(revision)` match arm (line 261)
- Remove `--verify` flag from `Blame` struct (lines 74–76)
- Update `Commands::Blame` match arm to remove `verify` field (lines 255–259)

#### `cli/src/lib.rs`
- Remove `pub mod signing;` (line 23)
- Remove `pub mod verify;` (line 29)

#### `cli/src/commands/mod.rs`
- Remove `pub mod verify;` (line 40)

#### `cli/src/commands/init.rs`
- Remove `use crate::signing;` (line 9)
- Remove the entire signing configuration block (lines 221–316) — the section starting with `// Configure commit signing` through the closing brace of the `None` match arm
- This eliminates the interactive prompt during `aiki init` asking about key generation

#### `cli/src/commands/doctor.rs`
- Remove `use crate::signing;` (line 7)
- Remove the "Check commit signing" section (lines 288–334) — the entire `if project_root.join(".jj").exists()` block that checks `signing::read_signing_config`

#### `cli/src/commands/blame.rs`
- Remove `verify` parameter from `run()` signature (line 8)
- Remove `verify` argument from `blame_cmd.format_blame()` call (line 47)

#### `cli/src/provenance/blame.rs`
- Remove `use crate::verify;` (line 18)
- In `format_blame()`: remove `verify: bool` parameter (line 236), remove the signature cache logic (lines 243–257), remove the `sig_indicator` logic (lines 268–279), update line formatting to not include signature indicators
- Update call site in `cli/src/commands/blame.rs` to drop the `verify` argument

#### `cli/src/error.rs`
- Remove signing/GPG error variants (lines 111–131):
  - `GpgSmNotSupported`
  - `SshKeyNotFound`
  - `NoUserEmailConfigured`
  - `GitUserNotConfigured`
  - `GpgKeyIdExtractionFailed`
  - `GpgKeyGenerationFailed`
  - `SshKeyLocationFailed`

#### `cli/src/config.rs`
- Remove `update_jj_signing_config()` function (lines 798–845)
- Keep `read_jj_repo_config()` and `write_jj_repo_config()` — they're general-purpose

#### `cli/tests/blame_tests.rs`
- Remove `test_blame_verify_shows_signature_status()` test (lines 268–370)

### Documentation

| File | Action |
|------|--------|
| `README.md` | Remove `aiki verify` examples (lines 153–159) and `aiki blame --verify` example (line 100) |
| `cli/docs/getting-started.md` | Remove `aiki verify` examples (lines 213–214) and `aiki blame --verify` example (line 101) |
| `cli/docs/contributing.md` | Remove `signing/` from module tree listing (line 57) |
| `summary.md` | Remove verify row from command table (line 33) |
| `ops/ROADMAP.md` | Leave as-is (historical record) |
| `ops/done/phase-4.md` | Leave as-is (historical record) |

## Dependency Order

The changes are straightforward — no circular dependencies. Suggested order:

1. Delete the 7 files (`verify.rs`, `commands/verify.rs`, `signing/*`, `tests/verify_tests.rs`, `tests/signing_tests.rs`)
2. Remove module declarations from `main.rs`, `lib.rs`, `commands/mod.rs`
3. Remove `Verify` command variant and match arm from `main.rs`
4. Remove `--verify` flag from `Blame` in `main.rs`
5. Clean up `commands/blame.rs` → `provenance/blame.rs` (remove verify param)
6. Clean up `commands/init.rs` (remove signing block)
7. Clean up `commands/doctor.rs` (remove signing check)
8. Clean up `error.rs` (remove signing error variants)
9. Remove `update_jj_signing_config` from `config.rs`
10. Remove verify test from `cli/tests/blame_tests.rs`
11. Update docs (`README.md`, `cli/docs/getting-started.md`, `cli/docs/contributing.md`, `summary.md`)
12. `cargo build` — fix any remaining compilation errors
13. `cargo test` — verify tests pass (signing tests will be gone with the deleted files)

## Cargo.toml

No signing-specific dependencies to remove. The crates used by signing (`dirs`, `toml`, `tempfile`) are also used by other modules.

## What to Keep

- `config::read_jj_repo_config()` / `config::write_jj_repo_config()` — general-purpose, used elsewhere
- The `provenance` module (blame, record, etc.) — provenance tracking is orthogonal to signing
- The `--verify` flag concept could be resurrected later if needed, but removing it now simplifies blame

## Risk

Low. No other features depend on signing. The `verify` module is a leaf. The signing setup wizard is only called from `init` and `doctor`. The `--verify` flag on blame is cosmetic.
