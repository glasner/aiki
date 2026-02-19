# Updated In-Repo Documentation

## Summary

Create user-facing documentation for Aiki. Today the README is comprehensive but dense, and there are no standalone guides. We need:

1. A streamlined README focused on the value proposition and quick orientation
2. A getting-started guide that walks a new user through first use
3. A guide for customizing default behavior (hooks, templates, context injection)
4. A guide for creating and sharing custom plugins
5. A contributors guide for people who want to work on Aiki itself

All docs live in a new `docs/` directory at repo root. The README links into them.

## Deliverables

### 1. Rewrite `README.md`

The current README tries to be both a marketing page and a reference manual. Trim it to:

- **What is Aiki** — 2-3 sentences on the value proposition (AI code provenance + task orchestration + flow automation)
- **Key features** — bullet list, each 1 line (provenance tracking, multi-editor support, task system, flow engine, plugins, code review pipeline, cryptographic signing)
- **Quick start** — 5 lines max: install, init, use. Link to getting-started guide for details.
- **Documentation** — links to the four docs below
- **How it works** — keep the event table and architecture overview, but move detailed flow/template/plugin docs to their own guides
- **Editor support** — keep the summary table, remove per-editor paragraphs
- **Project structure** — keep, it's useful for contributors

**Remove from README (moved to guides):**
- Detailed task management examples → getting-started guide
- Detailed review pipeline examples → getting-started guide
- Template customization details → customizing-defaults guide
- Flow engine deep-dive → customizing-defaults guide

### 2. `docs/getting-started.md`

Walk a new user from zero to productive. Sections:

1. **Prerequisites** — Git, Rust toolchain (for building from source)
2. **Installation** — clone + cargo install
3. **Initialize a project** — `aiki init`, what it does, what gets created
4. **Health check** — `aiki doctor`, `aiki doctor --fix`
5. **Editor setup** — brief per-editor (Claude Code, Cursor, Codex, Zed/ACP), noting that `aiki init` handles most of it
6. **Your first AI session** — what happens when you start coding with an AI editor:
   - Session starts → Aiki creates a fresh JJ change
   - AI edits files → provenance metadata recorded automatically
   - You `git commit` → co-author lines added automatically
   - `aiki blame` → see who wrote what
7. **Task management basics** — create, start, comment, close. The essential workflow. Link to README task section for full reference.
8. **Code review pipeline** — `aiki review` → `aiki fix`. Show the pipe pattern.
9. **Next steps** — links to customizing-defaults and creating-plugins guides

Tone: tutorial, not reference. Show one way to do things, don't enumerate all flags.

### 3. `docs/customizing-defaults.md`

For users who want to change Aiki's behavior without writing a full plugin.

1. **Overview** — Aiki's behavior is driven by flows (declarative YAML). The bundled `aiki/core` flow handles provenance. You can extend or override it.
2. **The hookfile** — `.aiki/hooks.yml` is the entry point. Explain `include:` and how it stacks.
3. **Events** — table of all 17+ events with 1-line descriptions. Group by category (session, turn, file, shell, web, mcp, commit, task).
4. **Actions** — table of available actions (`shell`, `jj`, `context`, `autoreply`, `commit_message`, `log`, `let`, `self`, `hook`, `task.run`, `review`) with 1-line descriptions and a short example each.
5. **Variables** — `$event.*`, `$ENVVAR`, let-bindings, `self.*` built-in functions. Show how to access them.
6. **Control flow** — `if`/`then`/`else`, `switch`/`case`. One example each.
7. **Failure handling** — `on_failure` with `continue`, `stop`, `block`. Explain the difference.
8. **Context injection** — how to add instructions to agent prompts at session start and every turn. Prepend vs append. Show a real example (e.g., "always run tests before committing").
9. **Autoreplies** — how to send follow-up messages to agents after they respond. Show a real example (e.g., task reminder).
10. **Overriding templates** — create `.aiki/templates/aiki/plan.md` to override the built-in plan template. Resolution order: project → user → built-in.
11. **User-level customization** — `~/.aiki/hooks/` and `~/.aiki/templates/` for cross-project defaults.
12. **Common recipes** — 3-4 practical examples:
    - Inject project-specific instructions at session start
    - Block `git push` without running tests first
    - Add custom commit message trailers
    - Run a linter after every file edit

### 4. `docs/creating-plugins.md`

For users who want to create reusable, shareable plugins.

1. **What is a plugin?** — a GitHub repo with `hooks.yaml` and/or `templates/`. One repo = one plugin.
2. **Plugin structure** — show the minimal directory layout. `hooks.yaml` is optional. `templates/` is optional. At least one must exist.
3. **Naming and references** — `namespace/plugin` format. The namespace is your GitHub username/org. Three-part refs for templates: `namespace/plugin/template`.
4. **The `aiki` namespace** — reserved, maps to `glasner` GitHub org. Don't use it for your own plugins.
5. **Creating a hooks plugin** — walk through creating a plugin that adds a `shell.permission_asked` handler (e.g., block dangerous commands). Show the YAML.
6. **Creating a templates plugin** — walk through creating a plugin with custom review/plan templates. Show the markdown with `{{> }}` partials.
7. **Dependencies** — how they're auto-derived from content (no manifest needed). Explain what counts as a reference vs. what doesn't (code blocks excluded).
8. **Testing locally** — how to test a plugin before publishing: symlink into `~/.aiki/plugins/namespace/plugin/` or use project-level overrides.
9. **Publishing** — push to GitHub. Users install with `aiki plugin install owner/repo`.
10. **Plugin management** — `install`, `update`, `list`, `remove` commands. Brief since users can discover via `aiki plugin --help`.
11. **Composition** — `include:` in hooks.yaml, `before:`/`after:` blocks, `hook:` action. How plugins can build on each other.
12. **Best practices** — keep plugins small and focused, document with comments in hooks.yaml, avoid side effects in `change.permission_asked` handlers (they gate operations).

### 5. `docs/contributing.md`

For people who want to contribute to Aiki itself (the CLI, core flows, built-in templates).

1. **Development setup** — clone, `cargo build`, `cargo test`. Mention the Rust toolchain version if pinned.
2. **Project structure** — brief tour of `cli/src/` layout: `commands/`, `flows/`, `tasks/`, `plugins/`, `events/`, `editors/`, etc. Link to the tree in README for full view.
3. **Architecture overview** — the key mental model: events → flow engine → actions. How editor hooks translate to unified events. How the bundled `aiki/core` flow works.
4. **Key abstractions** — `AikiEvent`, `FlowEngine`, `TaskManager`, `ProvenanceRecord`. What each does, where to find it.
5. **JJ vs Git terminology** — the critical distinction. Summarize the rules from `cli/src/CLAUDE.md`: use "change" not "commit" for JJ concepts, change ID is stable, commit ID is transient. Link to the full CLAUDE.md for the deep dive.
6. **Error handling** — use `AikiError` variants via `thiserror`, not `anyhow::bail!`. Show the pattern for adding a new error variant. Link to CLAUDE.md for full guidelines.
7. **Adding a new CLI command** — step-by-step: create `commands/my_command.rs`, add to `commands/mod.rs`, dispatch in `main.rs`. Follow the `pub fn run() -> Result<()>` pattern.
8. **Adding a new event type** — where to define it in `events/`, how to wire it into the flow engine, how to emit it from editor integrations.
9. **Adding a new flow action** — where actions are defined, how to add a new one, how to test it.
10. **Testing** — `cargo test` for unit tests, integration tests in `cli/tests/`. Mention any test conventions (e.g., temp dirs, no network deps in tests).
11. **The `ops/` directory** — how planning works: `ops/now/` for active specs, `ops/done/` for completed, `ops/next/` for future. Specs are written before implementation.
12. **Code style** — Rust idioms followed in this project (`#[must_use]`, `impl AsRef<Path>`, etc.). Link to CLAUDE.md for full list.
13. **Submitting changes** — PR workflow, what reviewers look for, how to run `aiki doctor` to validate.

## Approach

- Write all four docs as standalone markdown files in `docs/`
- Rewrite README.md to be concise, linking to docs/ for details
- Use real `aiki` commands and real YAML/markdown syntax throughout (no pseudocode)
- Keep tone practical and tutorial-oriented, not academic
- Each guide should be usable independently — include enough context to stand alone

## Open Questions

- Should we also create a `docs/reference.md` with the full CLI reference (`aiki --help` output for every subcommand)? Or defer that to `aiki --help` itself?
- Should the getting-started guide cover JJ concepts (changes vs commits)? The user doesn't need to know JJ to use Aiki, but it helps explain `aiki blame` output.
