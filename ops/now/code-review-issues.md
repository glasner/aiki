# Code Review Plan - Critical Issues to Address

Generated from comprehensive plan review on 2026-01-07

## Critical Issues (Must Fix)

### 1. Task Status Lifecycle Mismatch
**Problem**: Review plan adds `NeedsReview`, `NeedsFix`, `NeedsHuman` statuses but milestone-1.4 only defines `Open`, `InProgress`, `Closed`.

**Fix**: Update milestone-1.4-task-system.md to include the new statuses or redesign review to use existing statuses.

### 2. Event Emission Contradiction
**Problem**: Plan says "all reviews emit events" but also describes `--skip-flow` flag that doesn't emit events.

**Fix**: Choose one approach - recommend removing `--skip-flow`. Users can override flow handlers instead.

### 3. Agent Selection Fallback Chain Incomplete
**Problem**: Multiple places describe agent selection but the complete fallback chain is unclear.

**Fix**: Document explicit decision tree covering all cases (authoring agent present/missing, session history present/missing, etc.)

### 4. Task-Review ID Mapping Undefined
**Problem**: `fix:` action needs to look up task_id from review_id but the mapping location/format isn't specified.

**Fix**: Add `task_id` to `ReviewEvent` struct and store on `aiki/reviews` branch.

### 5. Blocking Action Semantics Unclear
**Problem**: `fix:` is described as "blocking" but flow engine is event-driven. Timeout behavior undefined.

**Fix**: Add "Flow Engine Extensions" section defining blocking semantics, timeouts, and failure handling.

## High Priority Issues

### 6. Phase 1 Too Ambitious
**Problem**: Phase 1 includes core review, auto-remediation, iterations, prompt history, self-review, multi-agent support.

**Fix**: Split into 4 smaller milestones:
- v1: Read-only review (one agent, CLI only)
- v2: Flow integration (events, basic remediation)
- v3: Auto-fix (single iteration, task integration)
- v4: Iterations & multi-agent

### 7. Self-Review Mode Questionable Value
**Problem**: Agents reviewing their own work seems like theater - deterministic models don't "catch mistakes" they just made.

**Fix**: Move to Phase 3 or remove entirely. Focus on independent review first.

### 8. Prompt History Integration Too Early
**Problem**: Phase 1 includes prompt history but it's a nice-to-have that bloats core work.

**Fix**: Extract to Phase 1.5 after core review validates.

### 9. Missing Error Handling
**Problem**: No specification for agent failures, timeouts, invalid JSON, orphaned tasks.

**Fix**: Add comprehensive "Error Handling" section with specific behaviors.

### 10. Unclear Dependencies
**Problem**: Review depends on task system Phase 1 and potentially prompt history but ordering isn't clear.

**Fix**: Add prerequisites table showing required milestones and their status.

## Medium Priority Issues

### 11. No Cost Management
**Problem**: O3 reviews are expensive. Auto-remediation could cause runaway costs.

**Fix**: Add `max_reviews_per_hour` and `max_fix_iterations_per_day` config options.

### 12. Over-Complicated Agent Config
**Problem**: Three agents × two modes × hardcoded vs configurable prompts = complexity explosion.

**Fix**: Phase 1 ships ONE agent (Claude Opus only). Add others in Phase 2.

### 13. Iteration Strategy Premature
**Problem**: Quick mode for iterations 1-2, deep for iteration 3 is optimization without data.

**Fix**: Start with 3 deep-thinking iterations. Measure if iteration 3 is even needed before adding quick mode.

### 14. Missing Observability
**Problem**: No metrics for review success rate, fix iteration distribution, agent accuracy.

**Fix**: Add metrics events for monitoring and analysis.

### 15. Task Auto-Assignment Complexity
**Problem**: Auto-assigning requires reading JJ metadata, handling missing data, fallback logic - all to avoid `aiki task assign`.

**Fix**: Consider making assignment manual in Phase 1, auto in Phase 2.

## Architectural Concerns

### 16. Circular Event Dependencies
**Problem**: `fix:` → `change.completed` → flow handler → potential infinite loops.

**Fix**: Define explicit loop prevention (max depth, cycle detection).

### 17. Task System Extensions Uncoordinated
**Problem**: Review adds features to task system (new statuses, auto-assignment, nested subtasks) without updating milestone-1.4 spec.

**Fix**: Either update milestone-1.4 or decouple review from task system in v1.

### 18. Review-Task Coupling Too Tight
**Problem**: "Reviews create tasks" is baked into core design. Makes reviews require task overhead.

**Fix**: Consider making tasks optional - reviews can work standalone, flows can opt into task creation.

## Recommendations

### Ship in Phases

**Milestone 1: Review Read-Only (2 weeks)**
- Single agent (Opus), no quick mode  
- Gerrit JSON output
- Store in JJ change descriptions
- CLI only (no flows)
- No auto-remediation

**Milestone 2: Flow Integration (1 week)**
- Add `review:` flow action
- Add `review.completed` event  
- Basic flow handlers (just display)
- Validate event loop

**Milestone 3: Auto-Fix Simple (2 weeks)**
- Add `fix:` action (single iteration)
- Requires task system Phase 1 complete
- Task auto-close on success
- Measure single-iteration success rate

**Milestone 4: Iterations & Multi-Agent (2 weeks)**
- Add multi-iteration (if metrics show need)
- Add quick mode (if cost is issue)
- Add codex/gemini
- Add self-review (if demand)

### Prototype First

Before implementing Phase 1, build standalone prototype:
```bash
aiki review-prototype --review-agent opus --target @
```

Ship as separate command, iterate, then integrate. Validate core value before building automation.

### Consider Decoupling

Reviews as standalone system that CAN create tasks but doesn't REQUIRE them:
- Simpler Phase 1 (no task status extensions)
- Each system independently useful
- Integration via flows (not core coupling)

Trade-off: Loses baked-in task integration, but more modular.

## Summary

**Strengths**: Solid architecture (event-sourced on JJ, Gerrit standard, modular agents)

**Weaknesses**: Phase 1 too ambitious, task integration too tight, missing error handling, unclear dependencies

**Path Forward**: Split into 4 small milestones, ship read-only review first, validate before automating, decouple where possible


From Codex:

• Findings

  - Critical: The spec contradicts itself on event emission—--skip-flow says it does not emit review.completed, but later states “All
    review commands emit the review.completed event.” This is a behavioral fork that will break automation and user expectations. ops/now/
    code-review.md:1147, ops/now/code-review.md:1173
  - High: “Cryptographically isolated read‑only enforcement” is asserted without a concrete isolation mechanism; headless CLIs can still
    write files or run commands, and the plan doesn’t define OS sandboxing, permissions, or verification. This is a security gap, not just
    an implementation detail. ops/now/code-review.md:70, ops/now/code-review.md:80
  - High: The “CLI‑first” architecture is contradicted by the Gemini SDK requirement for schema enforcement; this creates a split design
    path and adds an API dependency that violates the stated principle. ops/now/code-review.md:70, ops/now/code-review.md:719
  - Medium: Event payload and flow usage are inconsistent—flows reference $event.review.issues, but the review.completed schema only
    documents issues_found, not the issue list. This will cause runtime nulls or missing data. ops/now/code-review.md:195, ops/now/code-
    review.md:1079
  - Medium: Output contract is inconsistent—“no custom parsing; CLI schema enforcement” vs. later open question proposing a parser to
    convert LLM output to Gerrit JSON. The plan needs one authoritative path. ops/now/code-review.md:724, ops/now/code-review.md:1275

  Open questions/assumptions

  - Is the intent to require a sandbox (like macOS sandbox-exec or containerization) for review agents, or just best‑effort
    “advisory_only”? Right now the enforcement model is undefined.
