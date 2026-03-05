# Ampcode Competitive Intel Brief

**Date:** 2026-03-05 | **Target:** Amp (formerly Ampcode) by Sourcegraph → spinning out as Amp, Inc.

---

## 1. What This Project Is

Amp is a proprietary, closed-source agentic coding CLI built by Sourcegraph (now spinning out as an independent company). It differentiates through **multi-model orchestration** — assigning specialized LLMs to distinct roles (Claude Opus for coding, Gemini 3 Pro for review, GPT-5.4 for architectural advice, Gemini Flash for search) rather than running a single model. It offers server-stored thread collaboration, usage-based pricing with no claimed markup on model costs, and a novel ad-supported free tier (now closed to new signups). Amp sunset its VS Code/Cursor extensions in Feb 2026 to go all-in on the terminal agent paradigm. Community sentiment is broadly positive on output quality but divided on cost predictability and server-side data storage. ([ampcode.com](https://ampcode.com), [HN: Amp spin-out](https://news.ycombinator.com/item?id=46124649))

---

## 2. What Matters for Aiki

### Capability Decisions

| Capability | Decision | Rationale |
|-----------|----------|-----------|
| **Checks system** (composable review criteria) | **Counter** | Squarely in Aiki's review wedge. Amp shipped this Feb 2026 — user-defined review criteria in `.agents/checks/` that compose together. Aiki must match or exceed this composability. ([ampcode.com/chronicle](https://ampcode.com/chronicle)) |
| **Subagent spawning** (parallel isolated execution) | **Counter** | Direct overlap with `aiki task run`. Amp's subagents are ephemeral and model-optimized; Aiki's are persistent and workflow-tracked. Lean into durability and observability as the differentiator. ([ampcode.com/manual](https://ampcode.com/manual)) |
| **Oracle system** (inline second opinion) | **Copy (deprioritized)** | Architectural guidance mid-conversation is valuable but adjacent to Aiki's post-hoc review wedge. Worth studying for later; not urgent. ([ampcode.com/models](https://ampcode.com/models)) |
| **Handoff system** (context transfer) | **Copy (deprioritized)** | Automatic context analysis for task delegation could improve `aiki task run` quality. Monitor and adapt when task delegation friction becomes a user complaint. ([ampcode.com/manual](https://ampcode.com/manual)) |
| **Multi-model orchestration** | **Ignore** | Amp assigns models per role; Aiki is agent-runtime agnostic. Different layer. Model selection is a thin moat — any tool can adopt this. ([ampcode.com/models](https://ampcode.com/models)) |
| **Skills framework** | **Ignore** | Markdown-based plugin packages. Aiki extends via workflow (CLAUDE.md + tasks), not plugins. Different philosophy, no threat. ([ampcode.com/manual](https://ampcode.com/manual)) |
| **Thread collaboration** | **Ignore** | Enterprise-attractive but outside Aiki's autonomous review wedge. Aiki tasks already persist across agents/sessions. ([ampcode.com/manual](https://ampcode.com/manual)) |
| **MCP support, cross-repo search, custom toolboxes, AGENTS.md, ad-supported tier** | **Ignore** | Table stakes or orthogonal to Aiki's wedge. ([ampcode.com/manual](https://ampcode.com/manual)) |

### Aiki's Defensible Advantages

- **Iterative loops:** Aiki's build→review→fix→re-review cycle is a structured workflow. Amp's review is one-shot, not iterative.
- **Persistence:** Tasks, reviews, and provenance survive across sessions and agents. Amp threads are server-stored but not workflow-aware.
- **Runtime agnosticism:** Aiki works across Claude Code, Codex, Cursor. Amp is a single proprietary runtime.
- **Local-first:** Everything in JJ/git, never leaves the machine. Amp stores threads on Sourcegraph servers — a privacy concern noted by multiple HN commenters. ([HN: Amp Free](https://news.ycombinator.com/item?id=45616908))

---

## 3. Top 3 Recommendations This Week

### 1. Ship iterative review loops (Score: 4.80)

Formalize the review→fix→re-review cycle as a first-class primitive. When `aiki review` finds issues, auto-spawn fix subtasks; when fixes land, auto-trigger re-review on the delta. Stop when clean or max iterations reached. This is the **single strongest differentiator** vs Amp and every other competitor — Amp's Checks are one-shot, not iterative. Target: end-to-end review→fix→re-review cycle working without manual intervention. Est. effort: M (1-3 days).

### 2. Add composable review criteria (Score: 4.25)

Direct counter to Amp's Checks system (shipped Feb 2026). Let users define review criteria in `.aiki/review-criteria/*.md` files and compose them: `aiki review --criteria security,style`. Findings should attribute which criteria triggered them. This neutralizes Amp's most threatening capability and turns Aiki's review from generic to project-specific. Est. effort: M (1-3 days). ([ampcode.com/chronicle](https://ampcode.com/chronicle))

### 3. Publish local-first privacy positioning (Score: 3.35)

Zero engineering required — this is messaging. Amp's server-side thread storage is a documented concern for privacy-conscious users and enterprise teams. Produce a positioning doc: key messages, comparison table (local vs server-stored), and README/landing page copy updates. Strike while the iron is hot — Amp Free's training-mode requirement generated negative sentiment. Est. effort: S (< 1 day). ([tessl.io review](https://tessl.io/blog/amp-s-new-business-model-ad-supported-ai-coding/), [HN: Amp Free](https://news.ycombinator.com/item?id=45616908))

---

## 4. Risks If We Do Nothing

1. **Amp deepens Checks into iterative loops.** If Amp extends their composable review system to support review→fix→re-review cycles, they directly encroach on Aiki's primary wedge. Their Feb 2026 Checks launch and rapid changelog cadence ([ampcode.com/chronicle](https://ampcode.com/chronicle)) suggest this is plausible within weeks, not months. First-mover advantage on iterative review is Aiki's to lose.

2. **Review becomes table stakes.** Amp has a dedicated review model (Gemini 3 Pro) and composable criteria. Claude Code and Cursor will likely follow. If Aiki doesn't ship composable review criteria before the market converges, "autonomous review" stops being a differentiator and becomes a checkbox feature.

3. **Enterprise teams choose Amp for collaboration.** Amp's thread sharing, team visibility, and enterprise tier ($59/user/month with SSO, audit controls, IP allowlisting) make it enterprise-ready today. If Aiki doesn't articulate its local-first privacy advantage now, privacy-sensitive enterprise teams may default to Amp's polished collaboration story without realizing the data residency tradeoff. ([ampcode.com](https://ampcode.com), [HN: Amp spin-out](https://news.ycombinator.com/item?id=46124649))

4. **Moat erosion is mutual.** HN commenters note Amp's moat is "context assembly and prompting" — thin and replicable ([HN: Amp spin-out](https://news.ycombinator.com/item?id=46124649)). The same applies to Aiki if its workflow advantages aren't deepened. The window to build durable differentiation through iterative review loops is narrow.
