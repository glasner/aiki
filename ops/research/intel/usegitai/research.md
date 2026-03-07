# UseGitAI — Deep Research

## 1. Product Website and Docs

### Core Product
Git AI is an open-source Git extension that tracks AI-generated code through the entire SDLC — from development through pull requests to production. It attributes every AI-written line to the specific agent, model, and transcript that generated it.
— [Homepage](https://usegitai.com/)

### How It Works
- Supported AI agents actively report which lines they generated (not detection-based)
- Line-level attributions are stored in **Git Notes**, which survive rebase, cherry-pick, and squash operations
- Transcripts are stored locally, in Git AI Cloud, or in a self-hosted prompt store — keeping repos lean and free of sensitive data
- Works 100% offline, no login required
- Uses post-commit hooks with zero workflow disruption
— [Docs](https://usegitai.com/docs)

### Key Features
- **AI Blame**: Extended `git blame` showing agent, model, and session info per line
- **`/ask` skill**: Query AI agents about code they generated — retrieves original session context so the agent answers "as the original author" rather than guessing
- **Prompt Storage**: Preserves every prompt and associates it with corresponding code lines
- **Personal Dashboards**: Measure % AI per commit, compare accepted rates by agent/model
- **IDE Plugins**: VS Code, Cursor, and Windsurf with color-coded gutter decorations
- **Statistics**: `git-ai stats` command for local analytics
— [Docs](https://usegitai.com/docs), [/ask Blog Post](https://usegitai.com/blog/ask-the-agent)

### Supported Agents (12+)
Cursor, Claude Code, GitHub Copilot, Codex, Gemini CLI, OpenCode, Continue, Droid, Junie, Rovo Dev, Amp, Windsurf
— [Docs](https://usegitai.com/docs)

### Claude Code Integration
- Native support since September 1, 2025
- Hooks configured automatically via `PreToolUse` and `PostToolUse` on `Write|Edit|MultiEdit` operations
- Settings stored in `~/.claude/settings.json`
- Command: `git-ai checkpoint claude --hook-input stdin`
— [Claude Code Docs](https://usegitai.com/docs/cli/claude-code)

### Pricing
| Tier | Cost | Key Differentiators |
|------|------|---------------------|
| **Open Source** | Free | Local CLI, AI attribution, AI blame, local prompt storage, personal dashboards, % AI metrics. Offline, no login. |
| **Teams** | Free trial → paid (undisclosed) | Repo-level AI code %, AI code durability, AI readiness scoring, ROI tracking, team prompt/context store, link prod incidents to AI code, tips for engineers |
| **Enterprise** | Custom | Self-hosted, multi-SCM, data lake export |
— [Pricing](https://usegitai.com/pricing)

### Trust & Security
Dedicated trust page at [trust.usegitai.com](https://trust.usegitai.com)

---

## 2. Blog / Changelog / Release Notes

### Blog Posts

**"Ask the Agent"** — February 18, 2026
- Introduces `/ask` skill: query AI agents about code they wrote, even weeks later
- Agents without session history give "plausible-sounding but often inaccurate" answers; `/ask` retrieves original context
- Key quote: agents become "noticeably smarter" when they use `/ask` during planning
- Recommends adding to AGENTS.md: "In plan mode, always use the /ask skill to read the code and the original transcript that generated it"
— [Blog](https://usegitai.com/blog/ask-the-agent)

**"Keep the Prompts"** — January 6, 2026
- Documents prompt preservation feature — every prompt associated with corresponding code lines for full traceability
— [Blog](https://usegitai.com/blog)

**"Git AI is now 1.0"** — November 8, 2025
- Major release establishing standards for tracking AI code from dev to production
— [Blog](https://usegitai.com/blog)

### Recent Releases (from GitHub)

| Version | Date | Highlights |
|---------|------|------------|
| v1.1.8 | Mar 3, 2025 | Amp CLI support, checkpoint plugin, single-pass blame analysis API |
| v1.1.7 | Feb 28, 2025 | Worktree support, Windows path fixes, UTF-8 BOM handling |
| v1.1.6 | Feb 27, 2025 | Idempotent SQLite migrations, cross-repo checkpoint fixes |
| v1.1.5 | Feb 24, 2025 | Test coverage 54%→90%+, dual-mode (global hooks + wrapper), `.git-ai-ignore` |
| v1.1.4 | Feb 17, 2025 | VS Code GitHub Copilot hooks preview, OpenCode SQLite parsing |
| v1.1.3 | Feb 12, 2025 | Stability/reliability release |

**Development velocity**: Weekly releases, active focus on Windows compat, agent integrations, and reliability hardening.
— [GitHub Releases](https://github.com/acunniffe/git-ai/releases)

---

## 3. Public Repos and Org Page

### Repository: git-ai-project/git-ai
- **URL**: [github.com/acunniffe/git-ai](https://github.com/acunniffe/git-ai) (redirects to git-ai-project org)
- **Stars**: 1,200+
- **Forks**: 88
- **License**: Apache 2.0
- **Language**: Rust
- **Total commits**: 2,155
- **Status**: Actively maintained, weekly releases

### Founder
Aidan Cunniffe (GitHub: [acunniffe](https://github.com/acunniffe))
- HN username: `addcn`

### Installation
```bash
curl -sSL https://usegitai.com/install.sh | bash
```
— [GitHub README](https://github.com/acunniffe/git-ai)

---

## 4. Public Demos / Videos / Social Threads

### Hacker News — "Show HN: Tracking AI Code with Git AI"
- **URL**: [news.ycombinator.com/item?id=45878276](https://news.ycombinator.com/item?id=45878276)
- **Score**: 6 points, 4 comments (~3 months ago)
- **Key quote from creator**: Ratios of generated-to-accepted lines above 4-5 indicate the user may be outside the AI's optimal usage pattern
- **Community reactions**:
  - rattray (Stainless): "feels indispensable once adopted"
  - XiZhao: "Git AI is awesome -- very needed"
  - brene: suggested feeding consistently-edited code back into agent context to reduce token usage
- **Creator response**: Exploring a Git AI MCP (Model Context Protocol) to enhance agent capabilities

### Social Media Presence
- **Discord**: Active community at [discord.gg/XJStYvkb5U](https://discord.gg/XJStYvkb5U) (linked from homepage)
- **Twitter/X**: No dedicated product account found. Note: @GITAI_HQ on X is an unrelated space robotics company.
- **LinkedIn**: No dedicated product page found. GITAI on LinkedIn is the same unrelated space robotics company.
- **YouTube**: No demo videos found via search.
- **Homepage**: Includes embedded product screenshots showing AI blame view, commit breakdown (AI/human/mixed percentages), and dashboard analytics — but no video demos.

### Product Behavior (from docs/blog)
- The homepage shows a commit breakdown visualization: percentage of lines that are AI-generated, human-written, and mixed per commit
- `git ai blame` extends standard blame with agent/model/session metadata per line
- IDE gutter decorations color-code AI vs human lines
- `/ask` retrieves the original session transcript so agents can answer as the original author
