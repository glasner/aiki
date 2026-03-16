# Homebrew Installation for Aiki

Make `aiki` installable via `brew install glasner/aiki/aiki`.

## Status Quo

- Rust binary built with Cargo (`cli/Cargo.toml`, version 0.1.0)
- Source-only distribution: clone + `cargo install --path cli`
- No CI/CD (no `.github/workflows/`)
- No pre-built binaries on GitHub Releases
- Manual release process documented in `cli/RELEASING.md`

## Target

```bash
brew tap glasner/aiki
brew install aiki
```

## Plan

### Phase 1: Cross-compile CI (`.github/workflows/release.yml`)

Create a GitHub Actions workflow triggered on tag push (`v*`) that:

1. **Builds for 3 targets:**
   - `aarch64-apple-darwin` (macOS Apple Silicon)
   - `x86_64-apple-darwin` (macOS Intel)
   - `x86_64-unknown-linux-gnu` (Linux)

2. **Matrix strategy:**
   ```yaml
   strategy:
     matrix:
       include:
         - target: aarch64-apple-darwin
           os: macos-latest
         - target: x86_64-apple-darwin
           os: macos-13
         - target: x86_64-unknown-linux-gnu
           os: ubuntu-latest
   ```

3. **Steps per target:**
   - Checkout repo
   - Install Rust toolchain + target
   - `cargo build --release --manifest-path cli/Cargo.toml --target ${{ matrix.target }}`
   - Package: `tar czf aiki-$TAG-$TARGET.tar.gz -C target/$TARGET/release aiki`
   - Compute SHA256: `shasum -a 256 aiki-*.tar.gz > checksums.txt`
   - Upload artifact

4. **Release job** (after all builds):
   - Download all artifacts
   - Create GitHub Release with `gh release create`
   - Attach all tarballs + checksums

**Depends on:** Nothing (greenfield)
**Output:** Tagged releases produce downloadable `aiki-vX.Y.Z-{target}.tar.gz` artifacts

### Phase 2: Homebrew Tap Repository

Create a new GitHub repo: `glasner/homebrew-aiki`

Contents:
```
homebrew-aiki/
├── Formula/
│   └── aiki.rb
└── README.md
```

**Formula (`Formula/aiki.rb`):**
```ruby
class Aiki < Formula
  desc "AI-native task tracking and agent orchestration CLI"
  homepage "https://github.com/glasner/aiki"
  version "0.1.0"
  license "MIT"  # confirm actual license

  on_macos do
    on_arm do
      url "https://github.com/glasner/aiki/releases/download/v#{version}/aiki-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/glasner/aiki/releases/download/v#{version}/aiki-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/glasner/aiki/releases/download/v#{version}/aiki-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/glasner/aiki/releases/download/v#{version}/aiki-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "aiki"
  end

  test do
    assert_match "aiki #{version}", shell_output("#{bin}/aiki --version")
  end
end
```

**Depends on:** Phase 1 (needs release artifact URLs and SHA256 hashes)

### Phase 3: Formula Auto-Update

Add a second workflow to the **aiki repo** (`.github/workflows/update-homebrew.yml`) that runs after the release workflow completes:

1. Download checksums from the new release
2. Clone `glasner/homebrew-aiki`
3. Update `Formula/aiki.rb` with new version, URLs, and SHA256 hashes
4. Commit and push (using a GitHub token with cross-repo access)

This keeps the formula in sync without manual SHA copy-paste on every release.

**Depends on:** Phase 1 + Phase 2

### Phase 4: Update Release Process

Update `cli/RELEASING.md` to reflect the new automated flow:

```markdown
## How to Release

1. Bump version in `cli/Cargo.toml`
2. `cargo build` to recompile
3. `aiki doctor --fix` to update AGENTS.md
4. Commit and push
5. `git tag v0.2.0 && git push origin v0.2.0`
6. CI builds binaries, creates GitHub Release, updates Homebrew formula
```

**Depends on:** Phase 3

## Decisions Needed

1. **License** — What license is aiki distributed under? The formula needs this.
2. **Linux ARM** — Do we want `aarch64-unknown-linux-gnu` support too? Adds a 4th build target.
3. **Tap repo ownership** — `glasner/homebrew-aiki` or a different org?
4. **Source-build fallback** — Should the formula also support `brew install --build-from-source` via `depends_on "rust" => :build`? Adds complexity but covers edge cases.
5. **homebrew-core** — Do we want to pursue inclusion in homebrew-core eventually? Requires 50+ stars, notable project criteria, and formula must build from source.

## Work Estimate

| Phase | Scope |
|-------|-------|
| Phase 1 | ~150 lines of GitHub Actions YAML |
| Phase 2 | New repo + ~40 lines of Ruby formula |
| Phase 3 | ~60 lines of GitHub Actions YAML |
| Phase 4 | Small edit to RELEASING.md |

Phases 1-2 are the critical path. Phase 3 is quality-of-life. Phase 4 is docs cleanup.
