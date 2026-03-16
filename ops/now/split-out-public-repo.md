# Split Out Public Repos via `git subtree`

**Date**: 2026-03-16
**Status**: Draft
**Purpose**: Enable publishing specific monorepo subfolders (starting with `cli/`) as standalone public repos while keeping the private monorepo as the source of truth for development.

**Conversation**: https://claude.ai/code/session_01Agve5fRpWqXD9oDNrdueid

---

## Context

We develop in a private monorepo (`glasner/aiki`). Certain subfolders — starting with `cli/` — need to be available as public repos so users can discover, clone, and contribute to them. After evaluating options (subtree split, splitsh-lite, josh-proxy, submodules, inverted subtree), we chose **`git subtree` with CI-automated splitting** as the best fit.

### Why this approach

- **Private monorepo stays the source of truth** — day-to-day dev workflow doesn't change
- **No extra tooling** — `git subtree` is built into git
- **CI handles the sync** — no manual steps after initial setup
- **Clean public repo** — consumers see a normal standalone repo with its own history
- **Bidirectional if needed** — `git subtree pull` can bring external contributions back

### Alternatives considered

| Approach | Why not |
|----------|---------|
| Public repo as source of truth (subtree'd in) | Better for community-first projects, but adds sync friction for our dev workflow where monorepo is primary |
| `splitsh-lite` | Faster splitting, but overkill for our repo size. Can swap in later if `git subtree split` gets slow |
| josh-proxy | Elegant bidirectional sync, but requires running a proxy service |
| git submodules | Poor DX (recursive clones, pinned commits), limited jj support |

---

## Plan

### Phase 1: Initial setup

1. **Create the public repo** on GitHub (e.g., `glasner/aiki-cli`)
   - Empty repo, no README or license (we'll push the initial content)
   - Add a description, topics, etc.

2. **Verify `cli/` is self-contained**
   - No path dependencies in `Cargo.toml` pointing outside `cli/`
   - No imports or build scripts referencing `../` paths
   - All tests pass when run from `cli/` alone

3. **Do the initial split and push**
   ```bash
   # From the monorepo root
   git subtree split --prefix=cli -b cli-public

   # Add the public repo as a remote
   git remote add cli-public git@github.com:glasner/aiki-cli.git

   # Push the split branch
   git push cli-public cli-public:main
   ```

4. **Verify the public repo**
   - Clone it fresh, run `cargo build && cargo test`
   - Confirm commit history looks clean (only cli/ commits, paths rewritten to root)

### Phase 2: Automate with CI

Add a GitHub Actions workflow to the **private monorepo** that syncs on every push to `main` that touches `cli/`:

```yaml
# .github/workflows/sync-cli-public.yml
name: Sync cli/ to public repo

on:
  push:
    branches: [main]
    paths: [cli/**]

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # full history needed for subtree split

      - name: Split and push cli/
        env:
          DEPLOY_KEY: ${{ secrets.CLI_PUBLIC_DEPLOY_KEY }}
        run: |
          # Configure deploy key for push access to public repo
          mkdir -p ~/.ssh
          echo "$DEPLOY_KEY" > ~/.ssh/deploy_key
          chmod 600 ~/.ssh/deploy_key
          export GIT_SSH_COMMAND="ssh -i ~/.ssh/deploy_key -o StrictHostKeyChecking=no"

          # Split cli/ subtree
          git subtree split --prefix=cli -b cli-split

          # Push to public repo
          git remote add cli-public git@github.com:glasner/aiki-cli.git
          git push cli-public cli-split:main --force
```

**Setup required:**
- Generate an SSH deploy key pair
- Add the public key as a deploy key on `glasner/aiki-cli` (with write access)
- Add the private key as `CLI_PUBLIC_DEPLOY_KEY` secret on `glasner/aiki`

### Phase 3: Public repo CI

Set up CI on the **public repo** for consumers:

- `cargo build` / `cargo test` on PRs
- `cargo publish` workflow for releases (manual or tag-triggered)
- Standard open-source niceties: README, LICENSE, CONTRIBUTING.md

### Phase 4: Handle external contributions (when needed)

If someone opens a PR on the public repo:

```bash
# Pull their changes back into the monorepo
git subtree pull --prefix=cli https://github.com/glasner/aiki-cli.git main --squash
```

This creates a merge commit in the monorepo incorporating the public repo changes.

---

## Extending to more subfolders

To add another public subfolder later (e.g., `sdk/`):

1. Create the public repo
2. Add another job to the CI workflow (or parameterize it)
3. Run the initial `git subtree split --prefix=sdk`

The pattern is the same for each subfolder.

---

## jj considerations

- `git subtree` operates on the git backend, not jj directly
- In a colocated jj+git repo, run subtree commands via the underlying git
- The CI workflow runs pure git, so no jj dependency there
- If jj adds native subtree support in the future, we can migrate

---

## Open questions

- [ ] Repo name: `glasner/aiki-cli` or just `glasner/aiki`? (since `cli` is the main user-facing artifact)
- [ ] License file: needs to exist in `cli/` so it's included in the public repo
- [ ] Should the public repo have its own CHANGELOG?
- [ ] Tag strategy: mirror monorepo tags, or independent versioning on the public repo?
