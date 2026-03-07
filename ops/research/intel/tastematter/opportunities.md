# TasteMatter — Scored Opportunities

Scoring formula: `score = 0.35*pain + 0.35*fit + 0.20*gtm + 0.10*(6-complexity)`

| Rank | Opportunity | Pain | Fit | GTM | Complexity | Score |
|------|-------------|------|-----|-----|------------|-------|
| 1 | Provenance-First Counter-Positioning | 4 | 5 | 5 | 1 | **4.65** |
| 2 | Attention Drift Detection | 5 | 5 | 4 | 4 | **4.50** |
| 3 | Cross-Session Context Enrichment | 4 | 4 | 3 | 2 | **3.80** |
| 4 | Stale Task Detection | 3 | 5 | 2 | 1 | **3.70** |
| 5 | File Hotness for Review Prioritization | 3 | 4 | 3 | 2 | **3.45** |
| 6 | Complementary Integration Path | 2 | 3 | 4 | 3 | **2.85** |
| 7 | Pre-emptive Workflow Expansion Defense | 2 | 3 | 3 | 3 | **2.65** |

---

## 1. Provenance-First Counter-Positioning — 4.65

Position Aiki's explicit intent tracking ("why" + "whether done") against TasteMatter's passive file observation ("what was touched"). Craft messaging around "structured provenance > inferred trails."

- **Pain (4):** Developers need to understand *intent* behind changes, not just activity. As agent-driven sessions multiply, knowing which files were touched is insufficient — you need to know why.
- **Fit (5):** This IS Aiki's core wedge. No new features needed — just sharpened positioning and messaging that leverages existing strengths (task descriptions, sources, summaries).
- **GTM (5):** Direct competitive narrative. "TasteMatter shows you touched auth.rs 12 times; Aiki tells you *why* and whether the work is done." Immediately differentiating.
- **Complexity (1):** Marketing/messaging work, not engineering. Can be executed immediately.

## 2. Attention Drift Detection — 4.50

Build plan-vs-reality comparison into Aiki's review workflow. Compare task descriptions and sources against actual file changes (via `aiki task diff`) to surface when work diverges from plan.

- **Pain (5):** Autonomous agents frequently drift from instructions. The gap between "what was planned" and "what was built" is the top pain point in agentic workflows. TasteMatter validates this with their most compelling feature.
- **Fit (5):** Fits naturally into Aiki's review workflow. Task descriptions + source links define the plan; task diffs show reality. The comparison is a natural extension of the existing task→review chain.
- **GTM (4):** Directly counters TasteMatter's strongest differentiated feature. Having this in Aiki removes their key expansion vector.
- **Complexity (4):** Requires comparing task intent (descriptions, source docs) against actual file changes. Needs heuristics or lightweight NLP to detect meaningful divergence vs. incidental file touches.

## 3. Cross-Session Context Enrichment — 3.80

Strengthen task summaries and source tracking to provide richer cross-session history than TasteMatter's passive session parsing. Ensure "what happened across sessions?" is better answered by structured context than by file access logs.

- **Pain (4):** Claude Code sessions are ephemeral. Understanding what happened across multiple sessions is a real gap. TasteMatter validates this pain point with their session tracking feature.
- **Fit (4):** Extends the existing task model. Better summary prompts, richer source chains, and improved `aiki task list` output would close this gap without new architecture.
- **GTM (3):** Supports the "intent > observation" counter-position. Not a standalone selling point, but reinforces the overall narrative.
- **Complexity (2):** Improving existing features (summary quality, source chain display) rather than building new ones.

## 4. Stale Task Detection — 3.70

Auto-surface tasks that have been `in_progress` too long without comments or activity. Prevent accumulation of zombie tasks in multi-agent workflows.

- **Pain (3):** Zombie tasks are a slow drain. In multi-agent workflows, agents start tasks, get interrupted, and context is lost. Annoying but not a showstopper.
- **Fit (5):** Trivial extension of the existing task model. Time-since-last-activity is already implicit in JJ history. Just needs a query/alert surface.
- **GTM (2):** Not a major selling point on its own, but contributes to "Aiki keeps your work clean" messaging.
- **Complexity (1):** Very simple time-based check against task metadata. Could be shipped in a day.

## 5. File Hotness for Review Prioritization — 3.45

Integrate file change frequency as a signal in `aiki review` to prioritize which files get human attention. Rapidly-changing files get higher review urgency.

- **Pain (3):** Helpful for focusing limited human attention, but experienced developers can usually sense which files matter. More valuable as team/agent count grows.
- **Fit (4):** Directly enhances the review wedge. "These files changed 8 times across 3 tasks — review them first" is a natural addition to review output.
- **GTM (3):** Shows Aiki thinks about review quality, not just review existence. Nice differentiator in demos.
- **Complexity (2):** Lightweight signal computed from task diffs. Aggregate change counts per file across recent tasks.

## 6. Complementary Integration Path — 2.85

Explore integration where TasteMatter's passive signals (file hotness, session history) feed into Aiki's active workflow. Position as "TasteMatter for visibility, Aiki for action."

- **Pain (2):** Not a direct user pain. Strategic play for ecosystem positioning.
- **Fit (3):** Adjacent but not core. Aiki's value is in orchestrating work, not aggregating third-party signals.
- **GTM (4):** Coopetition narrative expands addressable use cases. "Works great with TasteMatter" lowers switching cost for their users.
- **Complexity (3):** Requires API/integration design and coordination with the TasteMatter project. Medium effort.

## 7. Pre-emptive Workflow Expansion Defense — 2.65

Build lightweight workflow features (review triggers from file patterns, task suggestions from change heuristics) that TasteMatter might expand into, blocking their most likely expansion vector.

- **Pain (2):** Speculative. TasteMatter hasn't moved into workflow territory yet.
- **Fit (3):** Defensive rather than additive. Building speculative features dilutes focus.
- **GTM (3):** Defensive positioning — "we already have that" — but hard to market proactively.
- **Complexity (3):** Medium effort to build features for threats that may not materialize.
