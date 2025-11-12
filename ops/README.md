# Aiki: AI Collaboration Built on Complete Provenance

## TL;DR

### The Burning Problem

**Developers waste 65+ minutes per feature fixing AI-generated code through manual iteration loops.**

You're using Claude Code to add authentication. It generates 200 lines. You test locally → type error. You ask it to fix → works locally. You commit → CI fails with security issues. You fix → commit again → CI fails with complexity warnings. You fix again → tests break. After 3-5 cycles and 65+ minutes, the feature finally works.

**This happens 3-5 times per day.** Developers spend more time fixing AI mistakes than they save using AI.

**The root cause:** No feedback mechanism between AI generation and quality validation. AI commits blindly, humans discover issues through slow manual testing or CI failures (5-10 minutes per cycle).

### Our Wedge (Phase 1)

**Autonomous review feedback loop** - AI commits code → Aiki reviews instantly (2-3 seconds) → AI reads feedback → AI fixes issues → repeats until passing → clean commit.

**Time savings: 65 minutes → 30 seconds.**

The AI self-corrects using structured feedback before code reaches Git. No manual iteration, no CI failures, no broken commits.

**This is our entry point.** It solves the immediate burning problem and validates our technical foundation.

### The Larger Vision

Once the wedge proves AI self-correction works, we solve progressively larger orchestration problems:

**Four Value Pillars (all built on complete provenance):**

1. **Quality (Phase 1 - THE WEDGE)** - Autonomous review + AI self-correction
   - Solves: AI generates broken code repeatedly
   - Value: 65 minutes → 30 seconds per feature

2. **Coordination (Phases 2-4)** - Multi-agent orchestration
   - Phase 2: Local agents overwrite each other's work
   - Phase 3: Cloud agent PRs bypass quality gates
   - Phase 4: Team-wide agents conflict without visibility
   - Value: Eliminate wasted work from agent conflicts

3. **Compliance (Phase 5)** - Enterprise governance
   - Solves: Regulatory requirements for AI-assisted code
   - Value: Enterprises adopt AI with confidence

4. **Provenance (Foundation)** - Edit-level history
   - Git tracks commits. Aiki tracks the entire AI creative process.
   - Which agent, what changed, iteration history, review results
   - Enables all other pillars

### The Plan

**Six phases, each validating the next:**

- **Phase 1** (0-6mo): Autonomous Review & Provenance - **THE WEDGE**
- **Phase 2** (6-12mo): Local Multi-Agent Coordination
- **Phase 3** (12-16mo): PR Review for Cloud Agents
- **Phase 4** (16-28mo): Shared JJ Brain & Team Coordination *(requires JJ OSS contributions)*
- **Phase 5** (28-40mo): Enterprise Compliance
- **Phase 6** (40mo+): Native Agent Integration *(requires vendor partnerships)*

**Technical Foundation:** Jujutsu (JJ) provides edit-level provenance that Git cannot - tracking every AI iteration, not just final commits. Built on JJ, outputs standard Git commits. Zero workflow disruption.

**Market:** 1M+ Cursor users, 20M+ Copilot users, 92% of developers using or planning to use AI assistants. Only 3% highly trust AI accuracy - we solve the trust problem.

**Key Validation Question:** Is autonomous review valuable enough to validate building the full orchestration platform?

---

# Part 1: The Burning Problem

## What Developers Experience Daily

**The AI Code Generation Loop Today:**

You're using Cursor to add a new feature:

```
9:00 AM - You: "Add authentication to the API endpoints"
         Cursor generates 200 lines of code

9:02 AM - You test locally: curl http://localhost:8000/api/login
         Error: "verify_token() missing return type annotation"

9:05 AM - You: "Fix the type error"
         Cursor adds type hints

9:07 AM - You test again: Works! Authentication seems to work locally

9:10 AM - You commit the code
         git commit -m "Add authentication"

9:15 AM - CI fails: Missing imports, security issues
         "API key hardcoded in auth.py line 45"

9:20 AM - You: "Fix the security issues"
         Cursor moves API key to environment variable

9:22 AM - You test locally again: Still works

9:25 AM - You commit again
         git commit -m "Fix security issues"

9:30 AM - CI fails: High complexity warning
         "authenticate() has cyclomatic complexity of 18"

9:35 AM - You: "Simplify the authenticate function"
         Cursor refactors, but now tests fail locally

9:40 AM - You: "The tests are broken, fix them"
         Cursor fixes tests

9:42 AM - You test locally: Works again

9:45 AM - You commit again
         git commit -m "Refactor authentication"

9:50 AM - CI finally passes

10:05 AM - Code review: "This authentication is incomplete and has unguarded edge cases"
          You spend another hour fixing issues the AI introduced
```

**Total time wasted: 65+ minutes** (and this is just one feature)
- Local testing and iteration: 12 minutes
- 3 commit/CI cycles: 35 minutes
- Final code review and fixes: 60 minutes

**This loop repeats 3-5 times per day.** Developers spend more time fixing AI mistakes than they save using AI.

**The root problem:** No feedback mechanism between AI generation and quality validation. Developer discovers issues through:
- Manual local testing (slow, incomplete)
- CI failures (5-10 minutes per cycle)
- Code review (hours later)

---

## Why This Problem Exists

**AI tools today generate code in isolation:**
- No feedback loop between generation and quality checks
- Commit code immediately without validation
- Humans discover issues only after manual test or commit (CI, code review, production)
- Each fix requires new human prompt and iteration

**Developers become AI babysitters:**
- Prompt AI to generate code
- Review AI output manually
- Find issues AI should have caught
- Prompt AI to fix issues
- Repeat until code is acceptable
- This takes 3-5x longer than it should

**The missing piece: Autonomous feedback and correction**
- AI should validate its own output
- AI should fix its own mistakes
- AI should iterate until review passes
- Humans should only intervene when necessary

---

## Market Evidence

### AI Coding Tool Adoption is Exploding

- **Cursor:** 1M+ users, 360K paying customers (fastest to $100M ARR)
- **GitHub Copilot:** 20M+ users (68% of developers using AI tools)
- **92% of developers** use or plan to use AI coding assistants
- **87% of developers** already actively using AI coding tools

**The shift is happening now.** AI-assisted development is becoming the default, not the exception.

### But AI Quality is a Massive Problem

- Critical analyses note AI tools generate code with **"broken or nonsensical control flows"** and **"subtle logical errors"**
- Only **3% of developers "highly trust"** AI accuracy despite 70% usage
- **76% of developers** spend more than half their time on maintenance tasks (fixing issues) rather than innovation
- Enterprise developers report AI-generated PRs are **"hard to review"** with **"frequent errors that consume developer time"**

**The trust gap:** 70% usage but only 3% high trust. This limits adoption and slows velocity.

### The Cost is Real and Measurable

- Developers iterate **3-5 times per feature** with AI assistance
- Each iteration: **15-30 minutes** of context switching and manual fixes
- CI failures from AI-generated code are common
- Human code review catches issues AI should have prevented

**What we need to validate:**
- How often do AI tools generate broken code on first attempt?
- How much time do developers spend fixing AI mistakes?
- Will AI tools effectively self-correct with review feedback?

**Phase 1 will measure:**
- Issue detection rate (how many problems caught pre-commit)
- Self-correction success rate (AI fixing its own mistakes)
- Time saved per developer per day
- Reduction in failed commits/CI runs

---

## Core Problems Preventing AI Velocity

### Problem 1: AI Generates Broken Code Repeatedly

**Common AI mistakes:**
- Type errors and missing imports
- Security vulnerabilities (hardcoded secrets, SQL injection)
- Logic errors and edge cases not handled
- Poor code structure (high complexity, unclear intent)
- Missing tests or insufficient coverage
- Non-compliant with team standards

**Current approach (manual iteration):**
```
Human prompt → AI generates → Human reviews → Human prompts fixes → AI generates → repeat
```

**Each cycle takes 15-30 minutes** because:
- Human must context-switch to review
- Human must understand AI's code
- Human must identify issues
- Human must formulate fix instructions
- AI may not understand feedback correctly

### Problem 2: No Automated Quality Gate at the Right Time

**Today's automated gates happen too late:**
```
AI generates → Human commits → CI fails (5-10 min) → Human fixes
```

Problems:
- Issues discovered after commit (too late, history polluted)
- CI takes 5-10 minutes to run (slow feedback loop)
- Human is always in the loop (bottleneck)
- No learning across iterations (AI repeats same mistakes)

### Problem 3: AI Can't Self-Improve Without Feedback

**AI tools lack self-correction capability:**
- Generate code based on prompt
- Have no way to validate output quality
- Can't see their own mistakes
- Don't learn from failures within session

**Result:** Developers spend more time fixing AI mistakes than AI saves them in writing code.

### Problem 4: Trust Deficit Limits AI Adoption

While 70% of developers use AI coding tools, only 3% "highly trust" their accuracy.

**Developers can't fully trust AI because:**
- No quality guarantee before commit
- Mistakes discovered late (review or production)
- No automated correction mechanism
- Each AI prompt is a gamble (will it work this time?)

**This limits AI adoption:** Teams that could benefit from AI assistance hesitate because they can't trust the output quality.

---

# Part 2: Our Wedge Solution

## The Solution: Autonomous Review Feedback Loop

**Aiki creates a closed loop where AI validates and corrects its own work before reaching Git:**

1. **AI generates code** (Cursor, Copilot, etc.)
2. **AI attempts commit** (normal `git commit` workflow)
3. **Aiki intercepts** (pre-commit hook, before commit reaches Git)
4. **Autonomous review runs** (static analysis + AI review in 2-5 seconds)
5. **Results fed back to AI agent** (not just shown to human)
6. **AI reads review results** (understands what's wrong)
7. **AI fixes issues** (generates corrections)
8. **AI attempts commit again** (repeats until passing or escalates)
9. **Human sees final result** (only reviewed, passing code)

**This is the wedge.** It solves the immediate burning problem and requires no vendor cooperation.

---

## With Aiki: The 30-Second Experience

```bash
# Developer asks Cursor to implement feature and commit when done
# Cursor writes code

# Cursor attempts to commit (Aiki intercepts)
git commit -m "Add authentication"

# Aiki runs autonomous review (2-3 seconds)
Aiki Review Results:
  ❌ Type error: Missing return type annotation on verify_token()
  ❌ Security: API key hardcoded in auth.py:45
  ⚠  Complexity: authenticate() has cyclomatic complexity of 18
  ❌ Missing: No tests for auth failure cases

Review Status: FAILED (3 critical issues, 1 warning)

# Aiki presents options to Cursor (the AI agent)
Options for Cursor:
  [r] Read review results and attempt to fix
  [e] Escalate to developer (request human review)
  [i] Ignore warnings and commit anyway
  [c] Cancel commit

# Cursor chooses to attempt fixes
Cursor: [r]

# Cursor reads the issues and generates fixes
Cursor: I see the issues. Let me fix them...
  ✓ Added return type annotations
  ✓ Moved API key to environment variable
  ✓ Refactored authenticate() into smaller functions
  ✓ Added test cases for auth failures

# Cursor attempts commit again (automatically)
# Aiki runs review again
Aiki Review Results:
  ✓ No type errors
  ✓ No security issues
  ✓ Complexity within limits (max: 8)
  ✓ Test coverage at 87%

Review Status: PASSED
Auto-approved. Committing...

[main abc1234] Add authentication
 4 files changed, 187 insertions(+)

# What just happened:
# - AI caught its own mistakes via autonomous review
# - AI fixed issues without human intervention
# - Developer only sees the final, reviewed version
# - Total time: 30 seconds, not 65 minutes
```

**Before Aiki:** 65+ minutes of manual iteration (local testing → commit → CI fails → fix → repeat)

**With Aiki:** 30 seconds of automated iteration (AI self-corrects before commit)

---

## How It Works: The Autonomous Review Cycle

### Installation (One-Time Setup)

```bash
brew install aiki
cd my-project
aiki init
```

**What happens:**
- Creates `.aiki` directory
- Initializes Jujutsu (JJ) using `.aiki` directory
- Links JJ to current git repository (read only)
- Registers pre-commit hooks (`.git/hooks/pre-commit`)
- Otherwise no change to `.git` repo
- Developer and AI tools continue using `git` commands normally

*Note: Will not affect others that do not have the Aiki CLI installed.*

### Step 1: AI Generates Code and Attempts Commit

```bash
# Developer prompts AI tool
"Add authentication to the API"

# AI (Cursor/Copilot) generates code
# AI attempts commit

git commit -m "Add authentication"

# AIKI INTERCEPTS HERE (before commit reaches Git)
```

### Step 2: Aiki Runs Autonomous Review (2-5 seconds)

```bash
# Comprehensive review:
# • Static analysis (linting, type checking, security)
# • AI review (GPT-4/Claude analyzes quality)
# • Complexity analysis
# • Test coverage check

Aiki Review Results:
  ❌ Type error: Missing return type on verify_token()
  ❌ Security: API key hardcoded in auth.py:45
  ⚠  Complexity: authenticate() cyclomatic 18 (limit: 10)
  ❌ Tests: No test coverage for auth failures

Review Status: FAILED (3 critical, 1 warning)

# Aiki presents options to Cursor (the AI agent)
Options for Cursor:
  [r] Read review results and attempt to fix
  [e] Escalate to developer (request human review)
  [i] Ignore warnings and commit anyway
  [c] Cancel commit
```

### Step 3: AI Reads Feedback and Self-Corrects

```bash
# Cursor (AI agent) chooses to attempt fixes
Cursor: [r]

# Cursor reads the issues and generates fixes
Cursor: I see the issues. Let me fix them...
  ✓ Adding return type annotations
  ✓ Moving API key to environment variable
  ✓ Refactoring authenticate() into smaller functions
  ✓ Adding test cases for failure scenarios

# Cursor attempts commit again (automatically)
```

### Step 4a: Review Passes, Code Commits (Happy Path)

```bash
# Aiki reviews again
Aiki Review Results:
  ✓ No type errors
  ✓ No security issues
  ✓ Complexity within limits (max: 8)
  ✓ Test coverage: 87%

Review Status: PASSED
Auto-committing...

[main abc1234] Add authentication
 5 files changed, 203 insertions(+)

# Provenance tracked:
# - Cursor made initial attempt (failed)
# - Aiki review caught 3 critical + 1 warning
# - Cursor self-corrected in 1 iteration
# - Final commit passed all checks
```

**Result (Happy Path):**
- AI fixed its own mistakes (no human intervention)
- Review happened before Git (clean history)
- Human sees only final, reviewed version
- **Agent-level provenance:** Aiki tracks which agent (Cursor/Copilot/human) made each edit
- Total time: 30 seconds (vs 65 minutes manual iteration)

### Step 4b: AI Escalates When Unable to Fix (Alternative Path)

```bash
# AI attempts to fix issues but struggles
Cursor: Reading review feedback...
  ⚠ Attempting to fix complexity issues...
  ⚠ Refactored authenticate() but still complexity 14 (limit: 10)
  ⚠ Attempted 3 times, cannot reduce further

# Cursor attempts commit again
# Aiki reviews again
Aiki Review Results:
  ✓ No type errors
  ✓ No security issues
  ❌ Complexity: authenticate() still 14 (limit: 10)
  ✓ Test coverage: 87%

Review Status: FAILED (1 critical issue remains)

# Aiki presents options to Cursor (the AI agent)
Options for Cursor:
  [r] Read results and try again (attempt 4)
  [e] Escalate to developer (request human review)
  [i] Ignore warning and commit anyway
  [c] Cancel commit

# Cursor (AI agent) decides to escalate
Cursor: [e] - This requires architectural decisions beyond my capability.

# NOW Aiki escalates to developer
Aiki: Cursor has requested human review.

Review Summary:
  Resolved by Cursor: 3 issues (type errors, security, tests)
  Remaining: 1 issue (complexity)

  Cursor's note: "Requires architectural decisions about function splitting"

Options for Developer:
  [r] Review and fix manually
  [i] Override and commit anyway
  [c] Cancel commit, continue working

# Developer chooses to review
r

# Aiki shows detailed complexity analysis
Aiki: Complexity Analysis for authenticate()

Current: 14 cyclomatic complexity (limit: 10)
Suggestion: Consider splitting into:
  - validate_credentials()
  - check_rate_limit()
  - generate_session_token()

# Developer makes architectural decision and fixes
# Then commits successfully
```

**Result (Alternative Path):**
- AI resolved 3/4 issues automatically (type errors, security, tests)
- Remaining complexity requires human architectural decision
- Developer informed with specific suggestions
- **Provenance tracks AI's 3 attempts** and escalation reason
- Human intervention only on issues AI cannot handle
- Total time: 10 minutes (AI tried first, human finished)

---

## Why This Solves The Burning Problem

### Before Aiki: Manual Iteration Hell

```
AI generates → Human tests locally → Issues found
→ Human prompts fixes → AI generates → Human tests
→ Human commits → CI fails (5-10 min wait)
→ Human prompts fixes → AI generates → Human commits
→ CI fails again (5-10 min wait)
→ Human prompts fixes → AI generates → Human commits
→ CI passes → Code review finds more issues
→ Human spends hour fixing

Total: 65+ minutes per feature
```

### With Aiki: Autonomous Correction

```
AI generates → AI attempts commit
→ Aiki reviews (3 sec) → AI reads feedback
→ AI fixes → AI attempts commit
→ Aiki reviews (3 sec) → Passes
→ Clean commit to Git

Total: 30 seconds per feature
```

### Key Improvements

**130x faster feedback:**
- CI takes 5-10 minutes per cycle
- Aiki takes 2-5 seconds per cycle
- AI can iterate rapidly without waiting

**AI self-correction:**
- AI reads structured feedback (not vague human comments)
- AI generates targeted fixes (not new implementation)
- AI learns what passes review (within session)

**Clean Git history:**
- Only reviewed code reaches Git
- No "fix CI", "fix linting", "fix tests" commits
- Provenance tracks iteration internally (via JJ)

**Human intervention only when needed:**
- Happy path: AI fixes everything (70%+ of time)
- Alternative path: AI escalates when stuck (30% of time)
- Humans see clear summary of what AI tried

---

## Phase 1 MVP Scope (Ruthlessly Minimal)

**Goal:** Validate that AI self-correction via autonomous review works and developers value it enough to pay.

### Three Core Features

**1. `aiki init` — One-time setup**
- Developer installs: `brew install aiki`
- Setup per repo: `aiki init`
- Git commands work normally (transparent)

**2. `git commit` intercept — Autonomous review**
- Triggers 2-5 second review before commit
- Static analysis + AI review + complexity + test coverage
- Three outcomes: auto-approve, flag for review, block

**3. AI self-correction loop**
- Feed structured review results to AI agent
- AI reads issues and generates fixes
- AI attempts commit again automatically
- Loop until passing or human intervenes

### What This Proves

**Technical Validation:**
- Can we build JJ daemon that works reliably?
- Can we keep autonomous review fast (<5 sec)?
- Can AI agents self-correct from structured feedback?
- Does JJ provide the right foundation?

**Product Validation:**
- Do developers value autonomous review?
- Does AI self-correction actually work in practice?
- Is the UX seamless enough (zero friction)?
- Do developers trust reviewed AI output more?

**Market Validation:**
- Will developers use it daily?
- Will they pay $29/month?
- Do they tell others about it (viral)?
- Does it unlock demand for coordination features (Phase 2)?

---

## What We Defer (Not in Phase 1)

❌ **Multi-agent coordination** - Defer to Phase 2
❌ **Repository-wide provenance UI** - Defer to Phase 2
❌ **Remote team coordination** - Defer to Phase 4
❌ **Enterprise compliance features** - Defer to Phase 5
❌ **Native agent SDK** - Defer to Phase 6

**Phase 1 focus:** Prove autonomous review + AI self-correction works and solves the burning problem.

---

## Success Criteria & Validation Timeline

### Month 3 (Internal Validation)
- [ ] JJ daemon working on team's machines
- [ ] We're using it daily for our own development
- [ ] Catching bugs we would have missed
- [ ] AI self-correction working >70% of time
- [ ] Git workflow unaffected

**Decision:** Continue to external beta or pivot?

### Month 6 (External Validation)
- [ ] 100 active developers using Aiki
- [ ] >80% report catching bugs with autonomous review
- [ ] AI self-correction success rate >60%
- [ ] <5% false positive rate
- [ ] 20 willing to pay $29/month
- [ ] <10% monthly churn

**Decision:** Proceed to Phase 2 (coordination) or iterate Phase 1?

### Month 9 (Growth Signal)
- [ ] 500 active developers
- [ ] 100 paying ($2.9K MRR)
- [ ] Viral sharing happening organically
- [ ] Users requesting coordination features
- [ ] Clear path to multi-agent orchestration

**Decision:** Full investment in Phase 2-6 roadmap or reassess?

---

## Just Enough About Jujutsu (The Technical Foundation)

### Why We Need More Than Git

**Git was designed for careful, manual commits:**
- Staging area assumes human curation
- Commits are expensive (history is sacred)
- One commit = one logical change
- Designed for humans who commit every few hours

**AI autonomous iteration breaks this model:**
- AI needs to try multiple versions rapidly
- Each attempt should be tracked (for provenance)
- Failed attempts shouldn't pollute Git history
- Need lightweight iteration without staging friction

### Jujutsu Enables Rapid AI Iteration

**Working Copy as Commit:**
- Every AI attempt is a first-class change in the DAG
- Easy to track iteration history internally
- Clean evolution visible to Aiki, invisible to Git
- No staging area friction

**Git Compatibility:**
- Final reviewed commit outputs to standard Git
- GitHub/GitLab work normally
- CI/CD unchanged
- Zero workflow disruption for developers

**Operation Log:**
- Complete provenance of AI iteration process
- "How many attempts did AI take?"
- "What issues were caught and fixed?"
- Audit trail for learning and debugging

**In short:** JJ enables rapid AI iteration behind the scenes while maintaining clean Git history for humans. This foundation enables not just Phase 1 (review), but all future phases (coordination, compliance, integration).

*Full technical details in Appendix.*

---

# Part 3: The Larger Set of Problems

Once Phase 1 proves autonomous review works, there's a much larger set of orchestration problems to solve.

## Beyond Individual Developers

Phase 1 solves the **individual developer + single AI agent** problem (broken code, manual iteration).

But modern development involves **multiple AI agents working concurrently** across teams. This creates entirely new coordination problems that Git wasn't designed to handle.

---

## Problem Set 2: Local Multi-Agent Conflicts (Phase 2)

### The Problem

**Today's reality:**
- Developers use multiple AI tools locally (Cursor + Copilot + custom agents)
- All agents work on the same local filesystem
- Each AI works independently, unaware of others
- AIs overwrite each other's changes sequentially
- Conflicts discovered late (at commit or code review)

**Example:**
```
9:00 AM - Cursor adds caching to auth.py (lines 45-50)
          Developer continues working...

9:02 AM - Copilot autocomplete "optimizes" auth.py (lines 45-48)
         → Copilot unknowingly overwrites Cursor's work

9:05 AM - Developer attempts commit
         → Git has no idea two AIs conflicted
         → Which changes should be kept?
```

**Cost:** Wasted AI work, unclear provenance, manual conflict resolution.

---

## Problem Set 3: Cloud Agent PR Quality Gaps (Phase 3)

### The Problem

**Today's reality:**
- Cloud-based AI agents (Copilot Workspace, Devin, Sweep) generate PRs
- These agents work in isolated cloud environments
- Can't install Aiki daemon in their environment
- Their PRs bypass all Aiki quality gates
- Teams get inconsistent quality

**Example:**
```
Team workflow:
- Dev A (local with Aiki): Cursor + Aiki → reviewed commits
- Dev B (local with Aiki): Copilot + Aiki → reviewed commits
- Dev C (cloud agent): Copilot Workspace → unreviewed PR ❌
- Dev D (cloud agent): Devin → unreviewed PR ❌

Result: 50% of code changes bypass Aiki review
```

**Cost:** Inconsistent quality, cloud agent PRs still have the same problems Phase 1 solves for local agents.

---

## Problem Set 4: Team-Wide Coordination Chaos (Phase 4)

### The Problem

**Today's reality (even with Phases 1-3):**
- Multiple developers with Aiki installed, each running local agents
- Each local Aiki works independently
- No visibility into what other developers' agents are working on
- Conflicts discovered only when pushing/merging
- Wasted work when agents unknowingly work on same code

**Example:**
```
10:00 AM - Dev A (local): Cursor starts refactoring auth.py
10:15 AM - Dev B (local): Copilot starts adding features to auth.py
11:00 AM - Dev A pushes changes
11:05 AM - Dev B tries to push → conflict!
          Dev B's 45 minutes of work needs to be rebased/reworked
```

**Cost:** Wasted developer and AI time, frustration, reduced velocity.

---

## Problem Set 5: Enterprise Compliance Needs (Phase 5)

### The Problem

**Enterprise requirements:**
- Must maintain audit trails for all code changes
- Require mandatory human review for sensitive code paths
- Need custom policies per codebase
- Demonstrate compliance to auditors (SOX, PCI-DSS, etc.)

**Current limitation:**
- Git provides commit-level history
- No provenance of AI iteration process
- No path-based policy enforcement
- No mandatory review gates
- Incomplete audit trails for regulators

**Cost:** Enterprises hesitate to adopt AI tools due to compliance risks.

---

## Problem Set 6: AI Vendor Integration Gaps (Phase 6)

### The Problem

**Current limitation (Phases 1-5):**
- Aiki observes AI tools passively
- Reviews after AI completes work
- Feeds results back post-attempt
- AI agents can't check for conflicts before starting
- AI agents can't get incremental feedback during execution

**What's missing:**
- Real-time feedback loop during AI execution
- Intent capture and verification upfront
- Pre-work conflict awareness
- Active participation in coordination
- Trust scoring and continuous learning

**Cost:** AI agents work less efficiently than they could with deeper integration.

---

## Why These Can't Be Solved Today

**Git doesn't provide:**
- Agent-level attribution (which AI made which edit?)
- Edit-level provenance (iteration history before commit)
- Real-time coordination across multiple agents
- Policy enforcement at pre-commit time

**Existing tools don't address:**
- Multi-agent coordination specifically
- Agent-aware provenance tracking
- Pre-commit autonomous review for agents
- Team-wide AI orchestration

**This is a greenfield opportunity.** No tool currently solves multi-agent code orchestration.

---

# Part 4: Our Plan to Solve It

## The Provenance Foundation

**Git records commits. Aiki records the entire creative process.**

Everything we build relies on **complete provenance** - tracking not just final states, but every edit, iteration, and decision along the way.

### What Git Captures

- Final commit (after all iterations)
- Single author (even if AI + human collaborated)
- Commit message (written after the fact)
- Diff (final state, not how we got there)

### What Aiki Captures (via Jujutsu)

- **Every edit** before it becomes a commit
- **Which agent** made each edit (Cursor, Copilot, human)
- **Iteration history**: attempt 1 failed review, attempt 2 passed
- **Review results**: what issues were caught, how AI fixed them
- **Intent**: what the agent/developer was trying to do
- **Timeline**: how long each iteration took

### Why Complete Provenance Matters

**For developers (Phase 1):**
- Understand AI's decision-making process
- Debug: "Why did the AI make this choice?"
- Learn: "What did the AI try before this solution?"
- Trust: See that AI caught and fixed its own mistakes

**For teams (Phases 2-4):**
- Coordination: "Which agents are working on what right now?"
- Debugging: "When did this bug get introduced and by whom?"
- Performance: "Which AI produces the best quality code?"
- Conflict resolution: "Did Agent B intentionally overwrite Agent A's work?"

**For enterprises (Phase 5):**
- Audit trail: "Show every change to sensitive code paths"
- Compliance: "Prove all auth/ changes had required security reviews"
- Incident response: "Trace this security issue to its origin"
- Regulatory: Complete provenance from intent → edit → review → approval

**For agent improvement (Phase 6):**
- Learning: Agents query their own history to improve
- Trust scoring: Track quality over time based on provenance
- Self-reflection: "I typically make type errors, let me be more careful"

**This complete provenance is the foundation that makes all four value pillars possible.**

### Example: Git vs Aiki Provenance

**Scenario:** Developer commits authentication feature added by Cursor

**What Git shows:**
```bash
git log
commit abc1234
Author: developer@company.com
Date: Mon Jan 15 09:30:00 2024
    Add authentication to API endpoints
```

**What Aiki shows:**
```bash
aiki provenance abc1234

Provenance for commit abc1234 "Add authentication to API endpoints"

Timeline:
  09:00:15 - Cursor (via developer prompt "add auth to API")
             Initial implementation: auth.py, middleware.py
             Status: Generated 187 lines

  09:00:18 - Aiki autonomous review (attempt 1)
             ❌ FAILED - 3 critical issues found
             - Type error: missing return type on verify_token()
             - Security: API key hardcoded in auth.py:45
             - Complexity: authenticate() cyclomatic complexity 18 (limit 10)

  09:00:20 - Cursor self-correction (reading review feedback)
             Fixing: Added return types, moved key to env, refactored
             Status: Modified 4 functions

  09:00:28 - Aiki autonomous review (attempt 2)
             ✅ PASSED - All checks passed
             - No type errors
             - No security issues
             - Complexity within limits (max: 8)
             - Test coverage: 87%

  09:00:30 - Git commit
             Final commit to main branch

Metrics:
  - Agent: Cursor (trust score: 0.89)
  - Iterations: 2
  - Self-correction time: 15 seconds
  - Issues caught pre-commit: 3 critical
  - Human intervention: None required

Intent: "Add authentication to API endpoints"
Verification: ✅ Code matches stated intent
```

**Value:** Developer sees not just the final commit, but the entire quality improvement process.

*Note: This is an example of what full provenance could look like, not something we deliver on day one.*

---

## Four Value Pillars Built on Provenance

### 1. Provenance (The Foundation)
- Edit-level history, not just commit-level
- Which agent made which change, when, and why
- Iteration history showing what was tried and failed
- Review results and corrections captured
- Intent verification: does code match stated goal?

**Value:** Complete understanding of how code evolved through AI collaboration

### 2. Quality (Phase 1 - THE WEDGE)
- Autonomous review feedback loop
- AI self-correction before reaching Git
- Pre-commit quality gates
- Trust scoring over time based on provenance
- Reduce manual iteration from 65 min → 30 sec

**Value:** AI generates higher quality code with less human intervention

### 3. Coordination (Phases 2-4)
- Multi-agent orchestration via shared provenance
- Conflict prevention between concurrent AI agents
- Sequential overwrite detection (local)
- PR review for cloud agents (webhooks)
- Team-wide visibility and pre-merge conflict detection
- Smart conflict resolution when agents overlap

**Value:** Multiple AI agents work together without stepping on each other

### 4. Compliance (Phase 5)
- Enterprise governance and policy enforcement
- Mandatory review gates for sensitive code paths
- Complete audit trails for regulators
- Custom review policies for company standards
- Immutable provenance for security incidents

**Value:** Enterprise confidence in AI-assisted development with regulatory compliance

---

## The Six-Phase Roadmap

Each phase builds on the previous, validating assumptions before investing in the next.

### Phase 1 (Months 0-6): Autonomous Review & Agent Provenance

**THE WEDGE - Solves the burning problem**

**Problem:** AI generates broken code, developers waste 65+ minutes per feature fixing it

**Solution:** Autonomous review feedback loop + agent-level provenance

**What we build:**
- `aiki init` - JJ daemon setup
- `git commit` intercept - autonomous review (2-5 sec)
- AI self-correction loop - feed results back to agent
- Basic provenance tracking - which agent, when, what

**Value delivered:**
- Immediate time savings (65 min → 30 sec per feature)
- Quality improvement (catch bugs pre-commit)
- Agent attribution (know which AI did what)
- Clean Git history (no failed CI commits)

**Success criteria:**
- 100 active developers by month 6
- >80% report catching bugs they'd have missed
- AI self-correction success rate >60%
- 20 willing to pay $29/month

**Validation:** Do developers value autonomous review enough to pay? Does AI self-correction work reliably?

---

### Phase 2 (Months 6-12): Local Multi-Agent Coordination

**Problem:** Multiple local AIs (Cursor + Copilot) overwrite each other's changes

**Solution:** Sequential overwrite detection, auto-merge, quarantine

**What we build:**
- Detect when multiple agents edit same files/lines locally
- Show complete timeline of agent activity
- Auto-merge compatible changes on remote rebase
- Quarantine conflicting changes, push clean code

**Example flow:**
```bash
git push origin main

Aiki: Remote has new commits, rebasing...
  ⚠ Conflict in auth.py:
      Remote (teammate): Added rate limiting (lines 52-60)
      Local (Cursor): Added caching (lines 65-75)

  [a] Accept both (auto-merge) ✓ recommended
  [q] Quarantine conflicts, push clean changes
  [m] Manual resolution

# Developer chooses auto-merge
a

Aiki:
  ✓ Auto-merged successfully
  ✓ Running autonomous review on merged result
  ✓ All checks passed
  ✓ Pushing to remote

Total time: 5 seconds (vs 2-3 minutes manual rebase)
```

**Value delivered:**
- Eliminate local agent conflicts
- Smart rebase on remote changes
- Quarantine functionality (push clean code, resolve conflicts later)
- Local multi-agent provenance

**Success criteria:**
- 1,000 active developers
- 50%+ reduction in merge conflicts
- Users request team coordination features

**Validation:** Does local coordination reduce wasted work? Is there demand for team-wide features?

---

### Phase 3 (Months 12-16): PR Review for Non-Aiki Agents

**Problem:** Cloud agents (Copilot Workspace, Devin, Sweep) generate PRs that bypass Aiki quality gates

**Solution:** GitHub/GitLab webhook integration for PR autonomous review

**What we build:**
- GitHub/GitLab webhook integration (monitor all PRs)
- Run autonomous review on every PR (same review aiki)
- GitHub bot comments with review results
- PR labels and status checks (aiki-review-passed/failed)

**Example flow:**
```
Developer opens PR from Copilot Workspace (no Aiki installed)

# Aiki webhook receives PR notification
Aiki: New PR #236 from copilot-workspace-bot
  Files changed: auth.py, middleware.py

# Aiki runs autonomous review on PR
Aiki Autonomous Review:
  ❌ Type error: Missing return type on verify_token()
  ❌ Security: API key hardcoded in auth.py:45
  ⚠ Complexity: authenticate() cyclomatic 18 (limit: 10)

# Aiki adds comment to PR
🤖 Aiki Review Bot commented:
  Found 3 issues that need attention before merge.
  This code would have been caught by Aiki's pre-commit review.

  Consider installing Aiki: https://aiki.dev/install

# Aiki adds labels
Labels: ❌ aiki-review-failed, ⚠ needs-human-review
```

**Value delivered:**
- Consistent quality across all agents (local and cloud)
- Cloud agent PRs reviewed automatically
- No agent cooperation required (works via webhooks)
- Teams get uniform quality standards

**Success criteria:**
- Review 10,000+ PRs from cloud agents
- Catch issues in >70% of cloud agent PRs
- Teams report improved PR quality

**Why this is achievable:** Uses existing autonomous review with standard GitHub/GitLab APIs. No JJ OSS contributions needed. Estimated 3-4 months to build and test.

**Validation:** Does PR review catch meaningful issues from cloud agents?

---

### Phase 4 (Months 16-28): Shared JJ Brain & Team Coordination

**Problem:** Agents across team conflict, no shared coordination layer

**Solution:** Distributed JJ mirroring for team-wide pre-merge conflict detection

**What we build:**
- Shared JJ Brain (centralized coordination repository)
- Major contributions to Jujutsu OSS project:
  - Distributed JJ repository mirroring
  - Performance optimizations for team-scale
  - Enhanced conflict detection algorithms
  - API improvements for remote coordination
- Pre-merge conflict detection across team
- Repository-wide provenance and activity tracking

**Example flow:**
```bash
# Dev A with Cursor working locally
git commit -m "Refactor authentication"

Aiki: Checking shared JJ brain for conflicts...
  ⚠ Warning: 2 other agents working on auth.py

  Dev B (2 hours ago): Copilot adding MFA support
    Lines 45-60, PR #234 (open)

  Cloud agent (30 mins ago): Copilot Workspace refactoring
    Lines 30-70, PR #235 (open)

  Your changes overlap with both PRs at lines 45-55

Recommendations:
  [c] Continue anyway (will conflict at merge)
  [w] Wait for PR #234 to merge first
  [r] Rebase on top of PR #234 now
  [v] View detailed conflict analysis
```

**Value delivered:**
- Team-wide real-time coordination
- Pre-merge conflict awareness
- Repository-wide provenance
- Prevent wasted work from conflicts

**Success criteria:**
- 50+ teams using shared coordination
- 80%+ reduction in merge conflicts
- Successful JJ OSS contributions accepted

**Why this is ambitious:** Requires 6-9 months of JJ OSS contributions (distributed mirroring, performance, conflict resolution) + 3-6 months building Aiki's coordination layer. We'll work closely with JJ maintainers (Martin von Zweigbergk and team at Google).

**Risk:** JJ team may not accept our contributions. Mitigation: Build relationship early, contribute smaller features in Phases 1-2, align with JJ's original vision.

**Validation:** Does remote coordination scale across teams? Can we successfully contribute to JJ OSS?

---

### Phase 5 (Months 28-40): Enterprise Compliance

**Problem:** Regulatory requirements for AI-assisted code

**Solution:** Policy aiki, mandatory gates, audit trails

**What we build:**
- Path-based policy aiki (different rules per code path)
- Mandatory review gates (enforce human approvals)
- Custom review models (company-specific standards)
- Compliance reporting (SOX, PCI-DSS, ISO 27001)
- Immutable audit trails with full provenance

**Example configuration:**
```yaml
compliance:
  high_risk_paths:
    - "auth/*"
    - "payment/*"
    - "security/*"

  policies:
    auth:
      autonomous_review: required
      human_review: required (2 approvers)
      security_scan: required

    payment:
      autonomous_review: required
      human_review: required (security team)
      penetration_test: quarterly
```

**Value delivered:**
- Enterprise governance for AI development
- Demonstrable compliance for auditors
- Custom policies per team/project
- Complete audit trails

**Success criteria:**
- 10+ enterprise contracts ($10K-100K/year)
- Successful compliance audits
- Enterprises report increased AI adoption confidence

**Validation:** Will enterprises pay for compliance features?

---

### Phase 6 (Months 40+): Native Agent Integration

**ASPIRATIONAL - Requires vendor partnerships**

**Problem:** AI agents want deeper collaboration than passive observation

**Solution:** Agent SDK for real-time feedback and active coordination

**What we build:**
- Aiki SDK for agent frameworks
- Real-time feedback during execution
- Intent capture and verification
- Pre-work conflict awareness
- Agent trust scoring and learning

**Example integration:**
```python
from aiki import Workspace

aiki = Workspace(repo="org/backend")
change = aiki.start_change(intent="add Redis caching to auth")

# Agent edits code
edit_files()

# Agent checkpoints for review
review = aiki.checkpoint()

if review.has_critical_issues():
    rollback_and_retry()

# Agent submits final work
aiki.submit()
```

**Value delivered:**
- Agents get feedback during execution (not after)
- Intent verification upfront
- Conflict awareness before starting work
- Higher quality through real-time guidance

**Partnership strategy:**
- Only pursue if Phases 1-5 gain traction (10K+ users)
- Pitch to vendors: "Your users love Aiki, deepen integration"
- Co-market as premium tier: "Cursor + Aiki"

**Note:** Phases 1-5 deliver value WITHOUT vendor cooperation.

**Validation:** Will agent vendors integrate? Do users demand it?

---

## Why This Sequence Works

**Each phase validates the next:**

1. **Phase 1 validates:** Technical foundation (JJ + autonomous review), market demand (developers will pay), AI self-correction works
   - If this fails: Core assumptions wrong, don't proceed
   - If this succeeds: Build Phase 2

2. **Phase 2 validates:** Coordination value, local multi-agent demand, path to team features
   - If this fails: Coordination less valuable than quality, focus on deepening Phase 1
   - If this succeeds: Build Phase 3

3. **Phase 3 validates:** PR review catches issues, teams want consistent quality, webhook approach works
   - If this fails: Cloud agent problem not as painful, skip to Phase 4
   - If this succeeds: Build Phase 4

4. **Phase 4 validates:** Team coordination value, JJ OSS contributions accepted, scales across teams
   - If this fails: Shared coordination too complex, focus on local + PR review
   - If this succeeds: Build Phase 5

5. **Phase 5 validates:** Enterprise demand, compliance ROI, willingness to pay premium
   - If this fails: Enterprises satisfied with Phases 1-4, don't need compliance
   - If this succeeds: Consider Phase 6

6. **Phase 6 validates:** Vendor partnerships viable, SDK adoption, agent improvement
   - If vendors don't cooperate: Phases 1-5 still deliver full value

**Risk reduction:**
- Test core technology early (Phase 1: 4-6 months)
- Validate market demand incrementally
- Generate revenue to fund later phases
- Learn from users what matters most
- Can pivot or stop after any phase

**Traditional approach risk:** Build entire platform (18-24 months) → Launch → Pray for PMF

**Our approach risk:** Validate foundation (6 months) → If successful, expand (each phase 6-12 months)

---

## What We Defer

**Not building in any phase:**

❌ **Alternative VCS support** - JJ-on-Git only, no native SVN/Mercurial
❌ **Custom AI models** - Use GPT-4/Claude via API, don't train our own
❌ **IDE plugins** - CLI/daemon only, no native IDE extensions
❌ **Mobile apps** - Desktop development only
❌ **Real-time collaboration** - Async coordination, not live pair programming

**Focus:** Solve multi-agent orchestration exceptionally well. Don't build a full development platform.

---

# Part 5: Can We Execute?

## Technical Requirements

### Phase 1 MVP Core Capabilities

**1. Systems Programming** - JJ daemon, Git hooks, process monitoring
**2. LLM Orchestration** - Autonomous review, prompt optimization
**3. Developer Tools** - CLI, workflow integration
**4. Web Development** - Provenance UI, backend services

### Specific Components

| Component | Complexity | Estimated Effort |
|-----------|------------|------------------|
| JJ daemon + Git interception | High | 2-3 months |
| Agent detection (process monitoring) | Medium | 2-4 weeks |
| Autonomous review integration | Medium | 1-2 months |
| Basic provenance tracking | Low | 2-4 weeks |
| Simple web UI | Low | 3-4 weeks |

**Total for Phase 1 MVP: 4-6 months with 3-4 person team**

---

## Team Capability Assessment

### Skills Needed for Phase 1

**Immediate requirements:**
- [ ] **Deep JJ knowledge** (4-6 week learning curve expected)
- [ ] **Git internals expertise** (hooks, protocols, object model)
- [ ] **Systems programming** (daemon, process monitoring)
- [ ] **LLM integration** (we have this from Forge)
- [ ] **macOS/Linux process monitoring** (for agent detection)

### Critical Questions

**1. Do we have Git internals expertise?**
- Can we build Git hooks that intercept commands reliably?
- Understand Git object model, refs, remotes?
- If no: Need to hire or ramp up (8-12 weeks)

**2. Can we build JJ daemon?**
- Systems programming in Rust/Go?
- Process management, IPC?
- If no: Significantly extends timeline

**3. Can we make autonomous review fast enough?**
- <5 second latency requirement
- LLM API orchestration, caching strategy
- We have some experience from Forge

**4. Can we detect agents reliably?**
- Process monitoring varies by OS
- IDE-specific heuristics needed
- Requires experimentation (2-4 week spike)

### Skills Needed for Later Phases

**Phase 2-3:**
- [ ] **GitHub/GitLab APIs** (webhook integration, PR manipulation)
- [ ] **Distributed systems** (JJ mirror coordination)

**Phase 4 (Critical):**
- [ ] **Deep Jujutsu internals expertise** (contribute to JJ OSS)
- [ ] **Rust programming** (JJ is written in Rust)
- [ ] **Version control system design** (distributed DAG, conflict resolution)
- [ ] **Open source collaboration** (work with JJ maintainers at Google)
- [ ] **Performance aikiering** (scale JJ to team coordination)

---

## Risk Mitigation Strategy

### 4-Week Spike to De-Risk Technical Assumptions

**Week 1-2: JJ Exploration**
- [ ] Build minimal JJ wrapper around Git
- [ ] Prove we can intercept git commands
- [ ] Validate JJ learning curve is manageable
- [ ] Test JJ performance with real repositories

**Week 3: Agent Detection Prototype**
- [ ] Test process monitoring on macOS/Linux
- [ ] Can we reliably identify Cursor vs Copilot?
- [ ] Fallback strategies if process monitoring fails?
- [ ] Accuracy testing with multiple AI tools

**Week 4: Review Latency Test**
- [ ] Integrate LLM for code review
- [ ] Measure end-to-end latency
- [ ] Achievable to stay under 5 seconds?
- [ ] Caching strategy for common patterns

**Go/No-Go Decision After Spike:**
- If all three validate: Proceed to full Phase 1 (4-6 months)
- If any fails: Re-evaluate approach or pivot
- Investment: 4 weeks to validate vs 6 months blind

---

## What Could Go Wrong

### Threat 1: GitHub Ships Agent Coordination

**Probability:** High (12-18 months)
**Impact:** High (commoditizes basic features)

**Mitigation:**
- Ship Phase 1 fast (4-6 months to external beta)
- Go deeper than they will initially (provenance, autonomous review)
- Build brand as "the agent coordination layer"
- By the time they ship, we have 10K+ users and network effects
- Our JJ foundation enables deeper features than Git-based solution

### Threat 2: JJ Doesn't Gain Adoption or We Can't Contribute

**Probability:** Medium
**Impact:** High for Phase 4+ (remote coordination depends on JJ improvements)

**Mitigation:**
- Phases 1-3 work fine with current JJ capabilities (no contributions needed)
- JJ is implementation detail (users don't see it)
- Can rebuild on pure Git if needed (lose some benefits)
- Google using it internally is positive signal
- Build relationship with JJ maintainers early (during Phase 1)
- Contribute smaller features in Phases 1-2 to prove collaboration
- Alternative: Fork JJ if necessary (not ideal, but possible)

**Real risk:** Phase 4 blocked if JJ team doesn't accept contributions. This is why we validate Phases 1-3 first.

### Threat 3: Agent Detection Proves Unreliable

**Probability:** Medium
**Impact:** High (core feature breaks)

**Mitigation:**
- Multiple detection strategies (process monitoring + IDE extensions + heuristics)
- Graceful degradation (fall back to user tagging: "which agent?")
- Over time, agents may self-identify (as ecosystem matures)
- Phase 6 SDK solves this completely (agents integrate directly)

### Threat 4: Autonomous Review Too Slow or Expensive

**Probability:** Low-Medium
**Impact:** High (UX breaks, margins compress)

**Mitigation:**
- Aggressive caching (same code reviewed once, reuse results)
- Tiered review (fast static checks always, deep AI review on-demand)
- Cost controls (review budgets per user, optimization over time)
- Smaller/faster models for common cases (GPT-4 for complex only)

### Threat 5: Market Not Ready (AI Coding Still Too Niche)

**Probability:** Low (we're seeing explosive growth now)
**Impact:** High (no PMF, limited TAM)

**Mitigation:**
- Start with single-agent review (value even with one AI)
- Cursor alone has 1M+ users (sufficient TAM for Phase 1)
- 92% of developers using or planning to use AI (market is here)
- Focus on early adopters (Cursor power users, bleeding edge teams)
- Can pivot to related problems if orchestration too early

### Threat 6: AI Self-Correction Doesn't Work Reliably

**Probability:** Medium
**Impact:** Critical (kills the wedge)

**Mitigation:**
- This is what the 4-week spike validates
- Test with real AI tools (Cursor, Copilot) on real codebases
- Measure self-correction success rate before committing
- If <60% success: Pivot to human-in-loop review (less ambitious)
- Structured feedback format is key (specific, actionable)

---

## Fallback Plans

**If JJ proves too complex:**
- Build on pure Git with smarter conflict detection
- Lose some coordination benefits but autonomous review still works
- Provenance tracking still possible (just less elegant)

**If agent detection unreliable:**
- Require IDE extensions (Cursor/Copilot plugins)
- Smaller initial TAM but more accurate
- Still delivers autonomous review value

**If autonomous review too slow:**
- Make it async (review after commit, before push)
- Or simpler/faster review models (trade depth for speed)
- Or tiered review (fast always, deep optional)

---

# Part 6: Why We'll Win

## Competitive Landscape

### Current Alternatives (None Solve Multi-Agent Orchestration)

**Status Quo: Git + Manual Resolution**
- What developers do today
- Painful but familiar
- Our competition is inertia

**Merge Queues (Aviator, Mergify, GitHub Merge Queue)**
- Focus: CI optimization, not agent coordination
- Limitation: No agent awareness, no provenance
- React to conflicts, don't prevent them
- **We complement, not compete**

**Git Workflow Tools (git-branchless, Graphite)**
- Focus: Human workflow optimization
- Limitation: Assume single author, no multi-agent coordination
- **We complement, not compete**

**Jujutsu (Direct Usage)**
- Focus: Better VCS for humans
- Limitation: No agent awareness, no autonomous review, requires learning new tool
- **We build on top of JJ, make it invisible**

**Code Review Tools (CodeRabbit, Codeium)**
- Focus: Post-commit review on PRs
- Limitation: Too late (after conflicts already exist), no coordination
- **We complement, not compete** (we review pre-commit)

### The Gap

**No tool optimizes for multiple AI agents working concurrently on the same codebase.**

Tools optimize for humans OR for CI/CD, but not for the new reality of multi-agent development.

---

## Our Competitive Advantages

### 1. First Mover on AI Agent Orchestration

- No direct competitors addressing this space
- 12-18 month window before GitHub/GitLab catch up
- Technical complexity is a moat (JJ foundation hard to replicate)
- We can go deep while they go broad

### 2. Agent-Native from Day One

- Built for multi-agent coordination, not retrofitted
- Provenance as core feature, not bolt-on
- Every design decision optimized for AI workflows
- Deep understanding of agent behavior patterns

### 3. Git Compatibility = Zero Switching Cost

- Developers keep using Git commands
- Works with existing GitHub/GitLab workflows
- No team training required
- Outputs standard Git commits
- Reduces adoption friction to near-zero

### 4. Positioned Between Agents and Git (Not Competing)

- We don't compete with Cursor, Copilot (we make them better)
- We don't compete with GitHub (we make it work for agents)
- We're infrastructure layer, not tool replacement
- Everyone wins: agents get quality feedback, Git gets clean commits, GitHub gets better PRs

### 5. Technical Moat via JJ Foundation

- Jujutsu enables rapid iteration and coordination
- Working-copy-as-commit perfect for AI workflows
- Competitors building on Git face fundamental limitations
- Hard to replicate without rewriting on JJ

---

## Our Moat Over Time

**Year 1: Speed & Technical Depth**
- First to market with agent coordination
- JJ technical advantage (complex to replicate)
- Deep autonomous review capabilities
- Brand: "the agent orchestration layer"

**Year 2: Data & Learning**
- Provenance graph becomes valuable
- Agent trust scores improve with data
- Autonomous review gets better with feedback
- Pattern recognition across millions of commits
- Network effects: more users = better review models

**Year 3: Network Effects & Lock-In**
- More agents integrate → more value for all
- Cross-team coordination becomes valuable
- Enterprise compliance features lock in customers
- Switching cost increases (historical provenance)
- Ecosystem of integrations and tools

**Defensibility increases over time** as data compounds and network effects kick in.

---

# Part 7: Go-to-Market

## Phase 1: Bottom-Up PLG (Months 0-12)

**Target:** Individual developers using Cursor + Copilot

**Channels:**
- Product Hunt launch ("Autonomous review for AI code")
- Hacker News post ("I built this to solve my own pain with Cursor")
- Twitter/X from personal account (show before/after demos)
- Cursor community (Discord, forums, unofficial channels)
- Developer influencers (50K+ followers who use AI tools)
- Content: Blog posts on AI quality problems, demo videos

**Pricing:**
- Free tier: Autonomous review (100 commits/month), basic provenance
- Pro tier: $29/dev/month - unlimited reviews, full provenance, priority support
- 30-day free trial for Pro

**Success Metrics:**
- Month 3: 50 active users (internal + early beta)
- Month 6: 100 active users, 20 paying
- Month 9: 500 active users, 100 paying ($2.9K MRR)
- Month 12: 1,000 active users, 200 paying ($5.8K MRR)

**Conversion tactics:**
- Show time saved (dashboard: "You saved 45 hours this month")
- Show bugs caught (notifications: "Prevented 12 security issues")
- Viral sharing (social proof: "Cursor + Aiki = 🚀")

---

## Phase 2: Team Sales (Months 12-24)

**Target:** 10-50 person aikiering teams with high AI adoption

**Channels:**
- Inbound from Phase 1 users (bottom-up virality)
- Targeted outreach to VP Eng / CTO
- DevTools conferences (craft conf, QCon, DeveloperWeek)
- Content marketing (blog: "How we eliminated merge conflicts with AI agents")
- Case studies from Phase 1 teams

**Pricing:**
- Team tier: $25/dev/month (minimum 5 devs, annual contract)
- Includes: Shared coordination, team analytics, priority support
- Volume discounts: 50+ devs = $20/dev/month

**Success Metrics:**
- Month 18: 20 teams, $15K MRR
- Month 24: 50 teams, $50K MRR

**Sales process:**
- Self-service signup with sales assist
- 30-day team trial
- Success criteria: >50% team adoption, measurable conflict reduction
- Expansion: Start with 5-10 devs, expand to full team

---

## Phase 3: Enterprise (Months 24-40)

**Target:** 500+ person companies with compliance requirements

**Channels:**
- Dedicated enterprise sales team (hire AEs)
- Security/compliance conferences (RSA, Black Hat)
- Compliance-focused content (SOX, PCI-DSS whitepapers)
- Partnerships with audit firms (Big 4)
- Executive briefings and demos

**Pricing:**
- Enterprise tier: $10K-100K/year (custom contracts)
- Includes: Compliance features, custom policies, audit trails, dedicated support, SLAs
- Pricing based on: Developer count, compliance requirements, support level

**Success Metrics:**
- Month 30: 3 enterprise contracts, $100K ARR
- Month 36: 10 enterprise contracts, $500K ARR
- Month 40: 20 enterprise contracts, $1M+ ARR

**Sales process:**
- Full sales cycle (3-6 months)
- POC with pilot team (30-60 days)
- Security review, compliance validation
- Executive sign-off, procurement process
- Multi-year contracts, annual payment

---

# Part 8: Appendix

## A. Jobs To Be Done

### Developer Using Multiple AI Tools

**When** I'm shipping features using AI coding agents (Cursor + Copilot + custom),
**I want to** trust that agent changes are safe, conflict-free, and fully traceable,
**So that** I can move at AI speed without reviewing bad code or resolving conflicts.

**Goals:**
- Work with multiple AI agents without coordination overhead
- Trust agent-generated code through automatic quality gates
- Understand full context behind every agent change
- Review only high-risk changes, auto-approve low-risk
- Focus on building features, not firefighting conflicts

**Pain Points:**
- Functional: Time spent resolving AI-created merge conflicts
- Social: Can't maintain or build on opaque agent-generated code
- Emotional: Anxiety about agent-generated code quality

---

### DevEx Team Lead

**When** I'm responsible for development velocity across a team using multiple AI tools,
**I want to** enforce consistent quality gates and debug coordination issues,
**So that** we maintain code quality while maximizing AI velocity gains.

**Goals:**
- Define quality policies once, enforce across all agents
- See metrics on agent performance and conflict rates
- Debug coordination issues with detailed provenance
- Adjust policies based on empirical data
- Block high-risk changes, auto-approve low-risk

**Pain Points:**
- Functional: No centralized control over heterogeneous agent ecosystem
- Social: Can't optimize process without performance data
- Emotional: Anxiety about losing control with AI tools

---

### Security / Compliance Officer

**When** I'm responsible for security in an environment with AI-assisted development,
**I want to** enforce mandatory review gates and maintain immutable audit trails,
**So that** we meet regulatory requirements while enabling developer velocity.

**Goals:**
- Enforce different policies for different code paths (strict for auth, permissive for tests)
- Ensure mandatory human review for sensitive code changes
- Maintain complete audit trail with full provenance
- Trace any change back to origin for incident investigation
- Demonstrate to auditors that AI changes undergo appropriate review

**Pain Points:**
- Functional: Existing compliance assumes human-only development
- Social: Can't do root cause analysis without understanding how code evolved
- Emotional: Anxiety about AI-introduced vulnerabilities

---

## B. Jujutsu Technical Deep Dive

### Project Background

**Jujutsu (jj)** is an experimental version control system started by Martin von Zweigbergk as a hobby project in late 2019. It evolved into his full-time work at Google, with assistance from other Googlers. However, the project explicitly states: "this is not a Google product."

**Project:** [martinvonz/jj on GitHub](https://github.com/martinvonz/jj)

### Key Innovations for Multi-Agent Coordination

**1. Working Copy as Commit**
- Unlike Git's staging area, JJ treats working directory as actual commit
- Changes recorded automatically as commits, amended on subsequent changes
- For Aiki: Every agent's workspace is first-class commit in DAG
- Enables tracking AI iteration without polluting Git history

**2. Automatic Rebase**
- When commits are modified, descendants automatically rebased
- Makes patch-based workflows trivial
- Enables Aiki to continuously integrate concurrent agent changes
- No manual rebase commands needed

**3. Conflicts as First-Class Objects**
- Inspired by Darcs: conflicts are data, not errors
- Conflicts recorded within commits, propagated through descendants
- Resolution propagates automatically across all agent workspaces
- Combines Git's `rebase --update-refs` with `rerere` by design

**4. Operation Log and Undo**
- Every repository operation tracked with snapshots
- Complete provenance of all agent actions
- Easy rollback of problematic changes
- Audit trail for debugging and learning

**5. DAG-Based Model**
- Commits maintained in directed acyclic graph
- Centralized snapshot management
- All agent changes exist as nodes in same DAG
- Makes coordination, conflict detection, resolution straightforward

### Git Compatibility

JJ uses Git as production-ready storage backend via gitoxide Rust library. Creates standard Git commits compatible with any Git remote. Users can maintain colocated repositories using both `jj` and `git` commands interchangeably.

**For Aiki:** Coordinate agents through JJ's DAG internally, output standard Git commits to GitHub/GitLab externally. Zero workflow disruption for teams.

### Why Jujutsu for Multi-Agent Development?

**Git was designed for:**
- Humans who branch, work in isolation, merge periodically
- Careful manual commits with staging area
- Linear workflows with occasional conflicts

**Jujutsu was designed for:**
- Continuous integration of concurrent work
- Automatic conflict resolution and propagation
- Rapid iteration without staging friction
- Working-copy-as-commit model

**For multi-agent development:** AI agents commit every few seconds, work concurrently, need rapid iteration with automatic conflict handling. JJ's design maps perfectly to this workflow, while Git's design creates friction.

---

## C. Configuration Examples

### Developer Configuration

```yaml
# .aiki/config.yml
review:
  mode: advisory  # strict | advisory | off

  auto_approve:
    max_files: 3
    max_complexity_delta: 10
    trusted_agents: [cursor, copilot]

  always_review:
    paths: ["auth/*", "security/*", "payment/*"]
    complexity_increase: >15

  reviewers:
    primary: gpt-4-turbo
    secondary: claude-sonnet-4  # for consensus on flagged changes
```

### Team Configuration (Phase 2)

```yaml
# Repository-level .aiki/config.yml
review:
  remote_prs:
    mode: strict  # All PRs reviewed before merge

    auto_merge_if:
      score: >8.0
      agent_trust: >0.85
      max_files: 5
      no_critical_issues: true

    block_merge_if:
      critical_issues: true
      security_issues: true

    require_human_review:
      paths: ["auth/*", "payment/*", "security/*"]
      complexity_delta: >20
      agent_trust: <0.7
```

### Enterprise Compliance (Phase 5)

```yaml
# Enterprise-level policy
compliance:
  sox_controls:
    paths: ["finance/*", "accounting/*", "reporting/*"]
    require:
      - human_review: 2  # Two human approvals
      - autonomous_review: true
      - security_scan: true
      - change_justification: required

  pci_dss:
    paths: ["payment/*", "card/*"]
    require:
      - autonomous_review: true
      - security_specialist_review: true
      - penetration_test: quarterly
```

---

## D. Alternative Approaches FAQ

### Why Not Just Improve Git?

**Git's fundamental model is incompatible with multi-agent coordination:**
- Linear history with manual branching
- Staging area assumes human curation
- Conflicts are exceptions requiring manual resolution
- No built-in provenance beyond commit author
- Designed for humans committing every few hours, not AI agents committing every few seconds

**JJ was designed for continuous concurrent work from the ground up.** Its working-copy-as-commit model, automatic rebase, and first-class conflicts make multi-agent coordination natural rather than bolted-on.

### Why Not Build on Existing Merge Queues?

**Merge queues (Aviator, Mergify) solve a different problem:**
- Optimize CI throughput, not agent coordination
- React to conflicts after they exist, don't prevent them
- No agent awareness or provenance
- Too late in workflow (conflicts already happened)

**Aiki prevents conflicts before they reach merge queue** through pre-merge coordination and autonomous review.

### Why Not IDE Extensions Only?

**IDE extensions would require:**
- Partnership with every IDE/agent vendor upfront
- Each vendor implements coordination separately
- Fragmented experience, no cross-tool coordination
- High adoption barrier (need all vendors to cooperate)

**Aiki works with any agent via CLI/daemon, no vendor cooperation needed for Phases 1-5.** Phase 6 SDK is optional enhancement, not requirement.

### Why Not Wait for GitHub to Build This?

**GitHub will eventually ship basic features, but:**
- 12-18 month window before they ship anything
- Will start simple (basic conflict detection, agent attribution)
- Won't go deep on provenance or autonomous review initially
- Enterprise complexity not their initial focus
- Git-based limitations (can't do what JJ enables)

**We can build a deeper moat during this window** through JJ foundation, autonomous review depth, and early user adoption. By the time GitHub ships, we have 10K+ users and network effects.

---

## E. Questions for Discussion

### Technical Feasibility

1. **Do we have the skills to build JJ daemon?** Or do we need to hire systems programmers?
2. **Can we learn JJ internals in 4-6 weeks?** Or is learning curve prohibitive?
3. **Is agent detection reliable enough?** Can we achieve >90% accuracy across tools?
4. **Can we keep autonomous review under 5 seconds?** LLM latency + caching strategy viable?
5. **What's our fallback if JJ proves too complex?** Pure Git approach still valuable?

### Market Validation

6. **Do we personally experience this pain?** Are we using Cursor/Copilot daily and feeling the friction?
7. **Is the problem real enough?** Will developers pay $29/month for autonomous review?
8. **Is the timing right?** Or is multi-agent coding still too niche/early?
9. **What's our competitive window?** When does GitHub ship basic agent coordination?
10. **Can we reach developers effectively?** Do we know how to execute PLG motion?

### Team Fit

11. **Does this problem excite the team?** Or is it just "another idea" we're evaluating?
12. **Can we commit 12+ months?** Phase 1 alone is 4-6 months, full validation needs 12+
13. **Are we willing to pivot fully?** Or trying to hedge with parallel tracks?
14. **What's our Plan B if this doesn't work?** Can we return to Forge or other opportunities?
15. **Why us specifically?** What's our unfair advantage for solving this problem?

### Execution Risk

16. **What's our MVP timeline?** Is 4-6 months realistic? Or more like 9-12 months?
17. **How do we de-risk early?** Is 4-week spike sufficient to validate technical assumptions?
18. **What would make us abort?** Clear failure criteria after spike? After Phase 1?
19. **How do we measure success?** Beyond vanity metrics, what indicates real PMF?
20. **What's our funding runway?** Can we survive 12+ months to meaningful validation?

---

## Summary: Is This Worth Pursuing?

### The Case For

✅ **Real, burning problem** - Developers waste 65+ minutes per feature fixing AI code
✅ **Large, growing market** - 1M+ Cursor users, 20M+ Copilot users, 92% AI adoption
✅ **Technical moat** - JJ provides defensible advantage
✅ **Clear wedge** - Autonomous review solves immediate pain
✅ **Phased validation** - Each phase validates next, reduces risk
✅ **Timing is right** - 12-18 month window before GitHub
✅ **No direct competitors** - Greenfield opportunity
✅ **Path to revenue** - Developers willing to pay for quality/time savings

### The Case Against

❌ **Technical complexity** - JJ learning curve, Git internals expertise needed
❌ **Competitive threat** - GitHub will eventually ship basic features
❌ **Market timing risk** - Multi-agent coding might still be too early
❌ **Execution risk** - Complex distributed system to build and scale
❌ **Team fit question** - Do we have the right skills and passion?
❌ **JJ adoption risk** - Phase 4 blocked if JJ doesn't gain traction or accept contributions

### The Decision Framework

**This comes down to:**

1. **Do we personally feel this pain?** If yes, we'll sustain passion. If no, hard to maintain conviction.

2. **Can we build the MVP?** Honestly assess: JJ, Git, systems programming, LLM orchestration skills.

3. **Can we ship in 4-6 months?** Longer timeline significantly increases risk.

4. **Do we have 12+ month commitment?** Can't validate market in less time.

5. **Is this better than alternatives?** Including continuing Forge or other opportunities.

### Proposed Next Step

**4-week technical spike** to validate core assumptions:
- Week 1-2: JJ exploration and Git interception
- Week 3: Agent detection prototype
- Week 4: Autonomous review latency testing

**Then decide:** Proceed to Phase 1, pivot approach, or pursue different opportunity.

**Investment:** 4 weeks to de-risk vs 6 months building blindly.

---

**What's your gut reaction? Does this narrative flow better than v2?**
