# TasteMatter Research

## Core Product Claim

TasteMatter is a local CLI visibility tool that indexes Claude Code session files to surface context trails — showing which files were touched, how recently, and how often — so developers can see what actually happened across sessions rather than relying on memory or plans.

## Primary User Persona & Workflow

**Persona:** Developers using Claude Code for agentic coding workflows who work across multiple sessions on long-running projects and need to understand what was actually built versus what was planned.

**Workflow entrypoint:**
1. One-time install via shell script (`curl -fsSL https://install.tastematter.dev/install.sh | bash`)
2. Initialize with `tastematter init`
3. A background daemon indexes Claude Code session files locally
4. Query context trails via CLI (e.g., `tastematter query flex --time 7d --limit 10`)
5. Results show file "hotness" (HOT/WARM/COLD), session counts, time-since-last-edit, and attention drift

Source: [tastematter.dev](https://tastematter.dev/)

## Pricing & Packaging

- **Price:** No pricing listed; the tool appears to be free for personal and commercial use
- **License:** Provided "as-is" for personal and commercial use; redistribution without permission prohibited; reverse engineering prohibited
- **Telemetry:** Anonymous usage metrics collected by default (command names, timing, platform info) via PostHog; opt-out available via `TASTEMATTER_NO_TELEMETRY=1` or config file
- **Data retention:** Telemetry kept 12 months before auto-deletion
- **Data locality:** All session data stays on the user's machine; no file paths, session content, or personal identifiers are transmitted

Sources: [Terms of Service](https://tastematter.dev/terms.html), [Privacy Policy](https://tastematter.dev/privacy.html)

## Evidence

### Product Website

- **Homepage tagline:** "Context Trails for Claude Code" — positions the tool as visibility (not memory) for agentic coding sessions
- **Key problem statement:** "Every Claude Code session starts fresh. Your work doesn't." — addresses context loss across sessions
- **Core features advertised:** automatic background indexing, file hotness tracking, session tracking, attention drift detection (plan vs. reality), file relationship mapping, abandoned work timestamps
- **Requires:** Claude Code
- **Parent company:** Taste Systems (taste.systems), operated by Digital Leverage LLC d/b/a Taste Systems
- **Founder:** Jacob Dietle (New York, NY)

Source: [tastematter.dev](https://tastematter.dev/)

### GitHub Repository

- **URL:** [github.com/jacob-dietle/tastematter](https://github.com/jacob-dietle/tastematter)
- **Stars:** 10 | **Forks:** 0 | **Watchers:** 1
- **Commits:** 6 (low activity, early-stage)
- **Latest release:** v0.1.0-alpha.29 (March 4, 2026) — pre-release/alpha stage
- **Tech stack:** CLI tool with JSONL data parsing, session indexing, chain building, file access metrics
- **License:** Skill and documentation free to use; CLI follows separate terms at tastematter.dev/terms

Source: [GitHub - jacob-dietle/tastematter](https://github.com/jacob-dietle/tastematter)

### Parent Company — Taste Systems

- **Website:** [taste.systems](https://www.taste.systems/)
- **Description:** GTM (Go-To-Market) operating system built around a "Context Operating System" (Context_OS)
- **Positioning:** Transforms expertise into a persistent context layer that AI can reference; "AI always has context — no more copy-pasting"
- **Pricing tiers (for Taste Systems, not TasteMatter):** Series A+ Context OS (4-week POC), Enterprise Platform (context-as-a-service), Self-Serve Creator (in development)
- **Relationship to TasteMatter:** TasteMatter appears to be a developer-focused tool within the broader Taste Systems ecosystem, applying the "context OS" philosophy specifically to Claude Code workflows

Sources: [taste.systems](https://www.taste.systems/), [RocketReach](https://rocketreach.co/jacob-dietle-email_374913361)

### Founder's Blog (Substack)

- **Blog:** "Speed to Insight" by Jacob Dietle
- **Key article:** ["What Even Is Context?"](https://jacobdietle.substack.com/p/what-even-is-context) — defines context engineering as "information that changes the meaning of other information" and argues taste is the "last moat" in an AI world
- **Other articles:** ["Applied Context Engineering"](https://jacobdietle.substack.com/p/applied-context-engineering), ["How I use AI to Amplify my Thought Process"](https://jacobdietle.substack.com/p/how-i-use-ai-to-amplify-my-thought)
- **Philosophy:** Taste is subjective, unquantifiable, and irreducible; context engineering lets individuals encode and scale their unique perspective at near-zero marginal cost

Source: [jacobdietle.substack.com](https://jacobdietle.substack.com/)

### Social / LinkedIn

- **Jacob Dietle on LinkedIn:** Founder at Taste Systems; posts about Context OS and GTM context engineering
- **Notable LinkedIn post:** Shared a "GTM Context OS quickstart guide" linking to [github.com/jacob-dietle/gtm-context-os-quickstart](https://github.com/jacob-dietle/gtm-context-os-quickstart)

Source: [LinkedIn - Jacob Dietle](https://www.linkedin.com/in/jacob-dietle/)

### Public Demos / Videos

- No public demos, walkthrough videos, or social threads showing product behavior were found as of March 2026. The product is in alpha (v0.1.0-alpha.29) with minimal public-facing content beyond the homepage and GitHub repo.

### Competitive Context

- **Claude-Mem** ([github.com/thedotmack/claude-mem](https://github.com/thedotmack/claude-mem)): Claude Code plugin for session capture and context injection — similar problem space but uses AI compression rather than file-level visibility
- **Claude Context** ([github.com/zilliztech/claude-context](https://github.com/zilliztech/claude-context)): MCP-based code search for Claude Code using vector search — focuses on codebase search rather than session history
- TasteMatter differentiates by focusing on *visibility* of what happened (file access patterns, attention drift) rather than *memory* (storing/retrieving conversation content)
