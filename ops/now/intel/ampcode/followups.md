# Ampcode Intel: Follow-up Execution Tasks

**Date:** 2026-03-05
**Source:** `ops/now/intel/ampcode/opportunities.md`

---

## feature/iterative-review-loops

**Derived from:** Opportunity #1 — Iterative review loops (score: 4.80)

**Hypothesis:** Formalizing the build→review→fix→re-review cycle as a first-class workflow primitive will be Aiki's strongest differentiator vs Ampcode and all competitors. Developers currently suffer through manual review→fix→hope cycles; an automated iterative loop that spawns fix tasks from findings and re-reviews until convergence will dramatically reduce review friction and increase code quality.

**Success metric:** A user can run `aiki review` on a task, have issues auto-spawn fix subtasks, and have each fix automatically re-reviewed — completing a full review→fix→re-review cycle without manual intervention. Target: at least 2 iterations (initial review + re-review after fixes) working end-to-end on a real codebase change.

**Scope guardrails:**
- **In scope:** Loop orchestration (review spawns fix tasks, fixes trigger re-review), convergence detection (stop when no new issues found or max iterations reached), review-diff awareness (re-review only examines the fix delta, not the entire change)
- **Out of scope:** Composable review criteria (separate task), cross-session memory, custom convergence policies, UI/dashboard for loop visualization

**Estimated effort:** M (1-3 days)

---

## feature/composable-review-criteria

**Derived from:** Opportunity #2 — Composable review criteria (score: 4.25)

**Hypothesis:** Generic AI reviews miss project-specific standards (security policies, naming conventions, architectural rules). Allowing users to define composable, persistent review criteria will make Aiki reviews significantly more valuable than one-size-fits-all alternatives — and directly counter Ampcode's Checks system with a more flexible, user-owned approach.

**Success metric:** Users can define review criteria in a config file (e.g., `.aiki/review-criteria/security.md`, `.aiki/review-criteria/style.md`), compose multiple criteria sets for a single review (`aiki review --criteria security,style`), and `aiki review` produces findings that reference which criteria triggered them. Validated by running a review with custom criteria on a test repo and confirming criteria-specific findings appear.

**Scope guardrails:**
- **In scope:** Criteria definition format (markdown-based, human-readable), criteria discovery and loading from project config, composability (multiple criteria sets applied in one review), criteria attribution in review findings
- **Out of scope:** Criteria sharing/marketplace, team-wide criteria enforcement, criteria versioning, integration with CI/CD pipelines

**Estimated effort:** M (1-3 days)

---

## experiment/cross-session-review-memory

**Derived from:** Opportunity #4 — Cross-session review memory (score: 3.90)

**Hypothesis:** Reviews that remember prior findings across sessions will reduce repetitive feedback and increase developer trust in automated reviews. If a reviewer flagged a pattern issue in session N, session N+1 should build on that knowledge rather than rediscovering it. We believe a lightweight memory layer (storing past review findings indexed by file/pattern) will measurably reduce duplicate findings.

**Success metric:** Run `aiki review` on the same codebase across 3 sessions with overlapping changes. Measure: (a) the second and third reviews reference or skip findings already addressed in prior sessions, (b) net new findings per review decrease when prior issues were fixed, and (c) reviewers don't re-flag resolved issues. Target: >50% reduction in duplicate findings between session 1 and session 3.

**Scope guardrails:**
- **In scope:** Prototype a review memory store (JJ-backed, local-first), index past findings by file path and issue pattern, inject prior findings as context during review, measure duplicate finding rates across sessions
- **Out of scope:** Full production implementation, team-shared memory, semantic similarity matching (use exact pattern matching for prototype), memory pruning/expiry policies

**Estimated effort:** S (< 1 day) — Prototype only: store findings as structured markdown, inject as context on next review

---

## positioning/local-first-privacy

**Derived from:** Opportunity #7 — Local-first privacy positioning (score: 3.35)

**Hypothesis:** Ampcode's dependency on Sourcegraph's server infrastructure creates a privacy concern for enterprise teams and regulated industries. Aiki's JJ/git-based local storage is an inherent architectural advantage — all task data, review history, and agent context stays in the user's repo, never leaving their machine. Explicitly positioning this advantage will resonate with security-conscious teams evaluating agent tooling.

**Success metric:** Produce a positioning document and messaging framework that: (a) clearly articulates the local-first privacy advantage in 2-3 key messages, (b) provides direct comparison points vs Ampcode's server model, and (c) can be used in landing page copy, README, and sales conversations. Validated by internal review confirming messages are accurate, compelling, and non-FUD.

**Scope guardrails:**
- **In scope:** Messaging framework (key messages, proof points, objection handling), comparison table (Aiki local-first vs Ampcode server-stored), updates to README/landing page copy, enterprise-focused talking points
- **Out of scope:** New engineering work (the architecture already supports this), compliance certifications (SOC2, HIPAA), formal security audits, pricing/packaging changes

**Estimated effort:** S (< 1 day) — Messaging and documentation only, no engineering required
