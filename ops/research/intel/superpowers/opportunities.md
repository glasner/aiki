# Superpowers — Opportunity Scoring

Ranked opportunities derived from the [relevance map](aiki-map.md) and [research](research.md).

## Scoring Dimensions

| Dimension | Weight | Scale | Description |
|-----------|--------|-------|-------------|
| User pain severity | 0.35 | 1-5 | How acutely users feel this problem |
| Strategic fit to Aiki | 0.35 | 1-5 | Alignment with Aiki's core wedge (persistent infrastructure, autonomous review, task tracking) |
| GTM leverage | 0.20 | 1-5 | Ability to drive adoption, differentiation messaging, or ecosystem leverage |
| Build complexity (inverse) | 0.10 | 6 - complexity (1-5) | Lower complexity = higher score |

**Formula:** `score = 0.35 × pain + 0.35 × fit + 0.20 × gtm + 0.10 × (6 - complexity)`

---

## Ranked Opportunities

| Rank | Opportunity | Pain | Fit | GTM | Cmplx | Score |
|------|------------|------|-----|-----|-------|-------|
| 1 | Two-stage review (spec + quality) | 5 | 5 | 4 | 3 | **4.60** |
| 2 | Cross-session execution tracking | 4 | 5 | 4 | 2 | **4.35** |
| 3 | Complementary positioning with Superpowers | 3 | 5 | 5 | 1 | **4.30** |
| 4 | Spec-compliance checking (task→diff) | 4 | 5 | 4 | 3 | **4.25** |
| 5 | Verification gates on task close | 4 | 5 | 3 | 2 | **4.15** |
| 6 | Review issue lineage and followup automation | 4 | 5 | 3 | 3 | **4.05** |
| 7 | Task system as plan runtime | 4 | 4 | 4 | 2 | **4.00** |
| 8 | Transparent workspace isolation messaging | 3 | 5 | 3 | 1 | **3.90** |
| 9 | Robust parallel execution tracking | 3 | 4 | 3 | 2 | **3.45** |
| 10 | Superpowers skills integration layer | 2 | 3 | 5 | 3 | **3.05** |

---

## Detailed Rationale

### 1. Two-Stage Review (Spec Compliance + Code Quality) — Score: 4.60

**Type:** Copy from Superpowers' subagent-driven-development skill.

**Description:** Split `aiki review` into two explicit phases: (1) spec-compliance — does the diff match the task description? and (2) code quality — is the code well-structured, tested, and maintainable? Superpowers does this at the subagent level; Aiki can do it as infrastructure with persistent issue tracking.

**Scores:**
- **Pain (5):** Review skipping is the #1 complaint in agentic workflows. Superpowers issues #613 (hard gates to prevent review skipping) and #614 (require tool verification before asserting facts) confirm this is acute. Agents declaring "done" without genuine review wastes developer time and erodes trust.
- **Fit (5):** Autonomous review is Aiki's core wedge. Two-stage review deepens it by making review more structured and thorough. Aiki's task→diff provenance enables automated spec-compliance checking that Superpowers' behavioral approach cannot match.
- **GTM (4):** Direct response to documented Superpowers pain points. Can message: "Superpowers users asked for hard review gates (#613). Aiki does it natively." Not a 5 because it requires explanation of the two-stage concept.
- **Complexity (3):** Requires review phase architecture, structured output for each phase, and potentially different review criteria per phase. Builds on existing `aiki review` infrastructure but is a meaningful extension.

### 2. Cross-Session Execution Tracking — Score: 4.35

**Type:** Counter Superpowers' executing-plans skill.

**Description:** Make Aiki's cross-session task persistence a headline feature. When a session crashes mid-plan, Aiki knows exactly what was done (completed subtasks) and what remains (open subtasks). Superpowers' batch execution is ephemeral — if the session ends, execution state is lost.

**Scores:**
- **Pain (4):** Session crashes and context window exhaustion during long tasks are common. Losing progress is frustrating, especially on multi-hour autonomous runs. Not a 5 because many tasks fit within a single session.
- **Fit (5):** JJ-backed persistent state is Aiki's architectural differentiator. Cross-session tracking leverages what Aiki already does uniquely — persist task state outside the agent's context window.
- **GTM (4):** Clear, simple message: "Superpowers executes plans within a session. Aiki tracks execution across sessions." The "couple hours at a time" autonomy Jesse Vincent describes makes this increasingly relevant.
- **Complexity (2):** Core infrastructure already exists (tasks persist in JJ). Needs better recovery UX: showing an incoming agent what was done, what's in-progress, and what's next. Mostly a presentation/onboarding problem.

### 3. Complementary Positioning with Superpowers — Score: 4.30

**Type:** Counter via strategic positioning.

**Description:** Position Aiki explicitly as complementary infrastructure to Superpowers rather than a competitor. Message: "Use Superpowers to teach agents how to work. Use Aiki to track, verify, and review the work they produce." This turns Superpowers' 71.7K-star adoption into a distribution channel.

**Scores:**
- **Pain (3):** Developers currently confused about tool overlap — some may avoid Aiki thinking it duplicates Superpowers. This is a real but not acute pain (more confusion than suffering).
- **Fit (5):** Perfectly aligned with Aiki's wedge. Superpowers is behavioral (ephemeral prompts); Aiki is infrastructure (persistent state). They are genuinely complementary, not competitive. This positioning reinforces Aiki's core identity.
- **GTM (5):** Highest GTM score. Turns a 71.7K-star competitor into a funnel. Every Superpowers user is a potential Aiki user if the complementary message lands. Integration guides, blog posts, and "Aiki + Superpowers" tutorials could reach a massive audience.
- **Complexity (1):** Primarily messaging, documentation, and integration guides. No significant code changes required. Could include a lightweight Superpowers skill that teaches agents about `aiki task` commands.

### 4. Spec-Compliance Checking (Task Description → Diff) — Score: 4.25

**Type:** Copy + differentiate.

**Description:** Automatically compare task descriptions against actual code diffs to verify the agent did what was asked. Aiki's task→diff provenance model uniquely enables this — the task description is structured metadata, the diff is the artifact. Superpowers' spec-compliance check is manual; Aiki can automate it.

**Scores:**
- **Pain (4):** Agents frequently drift from their assigned task, implementing tangential changes or missing requirements. This is a consistent complaint across all agentic coding tools.
- **Fit (5):** Uniquely leverages Aiki's task→diff provenance chain. No other tool has structured metadata linking task descriptions to code diffs at the infrastructure level. This is where Aiki's data model creates defensible differentiation.
- **GTM (4):** Concrete, measurable outcome: "Did this diff match the task?" Easy to demonstrate in demos and write-ups. Slightly below 5 because the concept requires some explanation.
- **Complexity (3):** Requires semantic comparison between task description and diff content. Likely needs LLM-assisted analysis to determine whether changes match intent. Not trivial, but well-scoped.

### 5. Verification Gates on Task Close — Score: 4.15

**Type:** Copy from Superpowers' verification-before-completion skill.

**Description:** Add a verification step to `aiki task close` that requires evidence the work actually works — tests pass, build succeeds, or explicit human confirmation. Superpowers addresses this as a behavioral skill; Aiki can enforce it at the infrastructure level.

**Scores:**
- **Pain (4):** Agents declaring tasks "done" when tests fail or the fix doesn't work is a persistent problem. Superpowers issue #614 ("require tool verification before asserting facts") confirms this is widely felt.
- **Fit (5):** Enhances the task lifecycle, which is core to Aiki. Verification gates make `aiki task close` more meaningful — a closed task means verified, not just "agent said so."
- **GTM (3):** Addresses a known pain point but is less flashy than review improvements. Harder to message as a headline feature; more of a "quality of life" improvement that builds trust over time.
- **Complexity (2):** Relatively straightforward to implement. Could start with a simple "did tests pass?" check and expand to more sophisticated verification. Biggest question is configurability (what counts as "verified"?).

### 6. Review Issue Lineage and Followup Automation — Score: 4.05

**Type:** Strengthen existing capability.

**Description:** Enhance the `aiki review` → `aiki fix` pipeline with full lineage tracking. Review issues should link back to the original task, the specific diff lines, and the review criteria that flagged them. Fix tasks should carry this provenance so the fixing agent understands the full context.

**Scores:**
- **Pain (4):** Review findings frequently get lost or aren't acted on. Agents doing fix work often lack context about why the review flagged an issue, leading to superficial fixes or regressions.
- **Fit (5):** Directly enhances Aiki's review wedge and leverages the provenance system. The review→fix pipeline is already Aiki's signature workflow; making it more robust deepens the moat.
- **GTM (3):** Sophisticated capability but harder to explain simply. More of a "power user" feature than a top-of-funnel draw. Valuable for retention and word-of-mouth among serious users.
- **Complexity (3):** Requires structured issue metadata, lineage tracking through task creation, and context propagation to fix agents. Meaningful engineering work but well-defined scope.

### 7. Task System as Plan Runtime — Score: 4.00

**Type:** Counter Superpowers' writing-plans skill.

**Description:** Position Aiki's task system as the runtime layer that executes plans. Superpowers produces static markdown plans with file paths and code snippets. Aiki can import these plans as subtask hierarchies, track execution, detect drift from the plan, and report completion status.

**Scores:**
- **Pain (4):** Static plans drift as execution progresses — steps become irrelevant, new steps emerge, and the plan document becomes stale. Developers working with Superpowers' plans have no way to know which steps are actually done vs. skipped.
- **Fit (4):** Aligns with Aiki's task tracking but requires extending the task system to understand plan semantics. Not a perfect 5 because plan import is somewhat tangential to the review wedge.
- **GTM (4):** Clear comparison: "Plans are static documents. Tasks are live state." Resonates with developers who've experienced plan drift. Could offer a "plan import" feature that converts Superpowers-format plans into Aiki subtasks.
- **Complexity (2):** Plan import is mostly parsing; the task infrastructure already handles subtask hierarchies. The main work is building a clean import flow and plan-drift detection.

### 8. Transparent Workspace Isolation Messaging — Score: 3.90

**Type:** Counter Superpowers' using-git-worktrees skill.

**Description:** Make Aiki's automatic, transparent workspace isolation a visible selling point. Aiki creates JJ-backed isolated workspaces per agent session automatically — agents don't need to know about worktrees. Superpowers teaches agents to create worktrees manually, which doesn't scale to many concurrent agents.

**Scores:**
- **Pain (3):** Multi-agent conflicts happen but aren't constant for most users. Pain is higher for teams running many concurrent agents; lower for individual developers.
- **Fit (5):** Already built into Aiki's architecture. This is about surfacing an existing capability as a differentiator, not building something new.
- **GTM (3):** Technical differentiator that's harder to message simply. "Automatic isolation" resonates with power users but may not be a top-of-funnel draw. More of a supporting point than a headline.
- **Complexity (1):** Already built. Needs documentation, comparison pages, and messaging materials. Zero code changes required.

### 9. Robust Parallel Execution Tracking — Score: 3.45

**Type:** Counter Superpowers' dispatching-parallel-agents skill.

**Description:** Improve the UX of Aiki's parallel task execution (async + wait + show). Add real-time progress visibility for parallel tasks, aggregate completion reporting, and better error handling when one parallel branch fails.

**Scores:**
- **Pain (3):** Parallel dispatch works today but tracking is imprecise. Users can't easily see which parallel tasks are progressing vs. stuck. Not a sharp pain — more of a "could be better" friction.
- **Fit (4):** Builds on existing async+wait infrastructure. Aligns with Aiki's orchestration capabilities but is incremental rather than a wedge-defining feature.
- **GTM (3):** Incremental improvement to an existing capability. Less dramatic than review or verification features. Useful for competitive comparison but unlikely to drive adoption on its own.
- **Complexity (2):** Infrastructure exists; needs UX improvements. Real-time progress requires task status polling or event mechanisms. Meaningful but not complex.

### 10. Superpowers Skills Integration Layer — Score: 3.05

**Type:** Ecosystem integration.

**Description:** Build a lightweight integration layer that allows Superpowers skills to interoperate with Aiki's task system. For example, a Superpowers skill could automatically create Aiki tasks when a plan is written, or trigger Aiki review when a subagent completes work.

**Scores:**
- **Pain (2):** Developers can already use both tools together manually. The integration friction exists but isn't blocking — most users either use one or the other, not both.
- **Fit (3):** Extends beyond Aiki's core infrastructure wedge into ecosystem integration. Valuable strategically but dilutes focus on the core product.
- **GTM (5):** Taps directly into Superpowers' 71.7K-star ecosystem. An official integration would get visibility through Superpowers' community channels and marketplace. Highest GTM potential.
- **Complexity (3):** Requires understanding Superpowers' skill lifecycle, building hook points, and maintaining compatibility as Superpowers evolves. Cross-project dependencies add ongoing maintenance burden.

---

## Summary

The top three opportunities cluster around Aiki's core review and task infrastructure wedge:

1. **Two-stage review (4.60)** is the highest-scoring opportunity because it addresses the most acute user pain (review skipping), perfectly fits Aiki's core wedge, and directly responds to documented Superpowers community complaints. It deepens Aiki's autonomous review moat.

2. **Cross-session execution tracking (4.35)** leverages Aiki's unique JJ-backed persistence to solve a real problem that Superpowers' ephemeral approach cannot address. As autonomous agent sessions grow longer, this becomes increasingly critical.

3. **Complementary positioning (4.30)** is the highest-leverage GTM opportunity — turning a 71.7K-star competitor into a distribution channel through clear "behavioral layer vs. infrastructure layer" messaging. Low complexity (mostly documentation) with outsized potential reach.

The scoring reveals a clear pattern: Aiki's strongest opportunities are those that deepen its infrastructure differentiation (persistent state, structured review, provenance) rather than copying Superpowers' behavioral approach. The top 5 opportunities all score 4.0+ and share a common theme: **making Aiki the verification and tracking layer that agents need but skills alone cannot provide.**
