# Releasing

## Versioning

Aiki uses [semver](https://semver.org/): `major.minor.patch` (e.g., `v0.1.0`).

- **Patch** (`0.1.x`): Bug fixes, doc tweaks
- **Minor** (`0.x.0`): New features, non-breaking changes
- **Major** (`x.0.0`): Breaking changes

## Version Source of Truth

The version in `cli/Cargo.toml` is the single source of truth. It's used at compile time for:
- The `aiki --version` output
- The `<aiki version="...">` tag in AGENTS.md/CLAUDE.md

## How to Release

```bash
# 1. Bump the version in cli/Cargo.toml
#    e.g., version = "0.2.0"

# 2. Rebuild so the new version is compiled in
cargo build

# 3. Update AGENTS.md block to match (uses new compiled version)
aiki doctor --fix

# 4. Commit the version bump

# 5. Create the release (auto-generates notes from commits since last tag)
gh release create v0.2.0 --title "v0.2.0" --generate-notes
```

## Conventions

- Tag format: `v0.1.0` (prefixed with `v`)
- Release titles match the tag: `v0.1.0`
- Release notes: auto-generated from commits, edited if needed
- No build artifacts — aiki is source-distributed
