# Homebrew Installation for Aiki

## Goal

Make `aiki` installable via:

```bash
brew tap glasner/tap
brew install aiki
```

## Why this matters

Right now `aiki` is source-only (`cargo install --path cli`). This plan moves us to automated releases + Homebrew publishing from CI.

## One-page implementation

### 1) Prep

- Confirm repo is on public `github.com/glasner/aiki` (not the private remote).
- Create a Homebrew tap repo: `github.com/glasner/homebrew-tap`.

### 2) Enable cargo-dist (in `/Users/glasner/code/aiki/cli`)

```bash
cd /Users/glasner/code/aiki/cli
cargo install cargo-dist
cargo dist init
```

When asked:
- enable **homebrew** installer
- set tap to `glasner/homebrew-tap`

Expected output: generated `cli/Cargo.toml` metadata + `.github/workflows/release.yml`.

### 3) Set up publish token

1. Create a PAT with `repo` scope.
2. In the public repo (`glasner/aiki`) secrets, add:

```text
HOMEBREW_TAP_TOKEN=<pat>
```

This lets CI write updates to `glasner/homebrew-tap`.

### 4) Validate before any release

```bash
cd /Users/glasner/code/aiki/cli
dist plan
```

Check output for:
- installer includes `homebrew`
- expected targets for each platform

### 5) Update release docs

In `cli/RELEASING.md`:

1. bump version in `cli/Cargo.toml`
2. `cargo build`
3. `aiki doctor --fix`
4. commit + push to `glasner/aiki`
5. `git tag vX.Y.Z && git push origin vX.Y.Z`
6. CI publishes release and Homebrew formula automatically

## Quick decisions needed

- Confirm package license in Cargo metadata.
- Decide whether to include shell installer too (`installers = ["shell", "homebrew"]`).

## Success criteria

- `brew tap glasner/tap` succeeds for a clean machine.
- `brew install aiki` succeeds.
- A tag in `glasner/aiki` triggers GitHub release + formula update in `glasner/homebrew-tap`.
