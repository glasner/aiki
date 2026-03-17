# Vision v2: Aiki

**Status:** internal working draft  
**Purpose:** alternate vision framing for comparison with `ops/vision.md`

---

## The missing truth

Aiki has to do two things at once:

1. **make your existing chat agent work better immediately**
2. **give you a path from chat to factory**

If we only tell the long-term story, Aiki sounds heavy.
If we only tell the short-term story, Aiki sounds incremental.

The product has to be both.

---

## The core thesis

Aiki is a system for improving AI software delivery across an adoption ladder:

1. **Chat**
2. **One-shot**
3. **Multiple one-shot**
4. **Factory**

The product should help users at every step of that evolution.

That means Aiki is **not** just for teams already running a software factory.
It is also for the team using Claude Code, Codex, Cursor, or another chat-style agent today who wants better outcomes without changing their workflow first.

---

## Day 1 promise

Aiki should make your current agent better **out of the box**.

No workflow migration required.
No need to adopt a whole new process first.

The initial value should feel like:
- better review,
- better checks,
- better recovery from bad output,
- better handoffs,
- better task shaping,
- and more reliable completion.

The Day 1 message is:

> **Aiki makes the chat agent you already use work better immediately.**

That is the wedge.

---

## Long-term promise

Aiki is also the path from ad hoc prompting to a real delivery system.

The long-term message is:

> **Aiki helps you evolve from chat, to one-shots, to coordinated runs, to a full software factory.**

That is the expansion story.

---

## The adoption ladder

### 1) Chat
This is where most users start.

They already have:
- a chat agent,
- a REPL agent,
- or an editor-integrated coding agent.

At this stage, Aiki should behave like an augmentation layer:
- improve the quality of tasks,
- improve review,
- improve fix loops,
- improve reliability,
- and improve “done” detection.

The key rule:

> **Do not require workflow change to deliver initial value.**

### 2) One-shot
Once the user trusts the system more, they start pushing the agent into bounded one-shot tasks.

At this stage, Aiki should provide:
- better task templates,
- clearer constraints,
- better verification,
- and better handoff semantics.

This is where the SDLC structure starts becoming visible.

### 3) Multiple one-shot
Now the user is not just running one bounded task.
They are coordinating several bounded tasks.

At this stage, Aiki should help with:
- decomposition,
- phase-specific task templates,
- parallel execution patterns,
- review and merge points,
- escalation rules,
- and continuity between runs.

This is where Aiki starts to feel like a system rather than a plugin.

### 4) Factory
At the far end, Aiki becomes a full software-delivery operating system.

At this stage, the user expects:
- clear SDLC phases,
- explicit review and fix loops,
- visible state across work,
- policy-driven escalation,
- recoverability,
- and repeatable delivery patterns.

This is the end state, not the required starting point.

---

## Why this matters

Without this adoption ladder, Aiki is easy to misdescribe.

If we lead only with the system view, we sound like:
- a workflow platform,
- a software factory,
- or a heavy operating model.

If we lead only with the chat-improvement view, we sound like:
- a thin add-on,
- a prompt pack,
- or a local optimization.

The truth is:

> **Aiki starts as an augmentation layer and grows into an operating system.**

That is the right shape.

---

## The product identity

Aiki should be understood as:

> **the system that improves the agent you already use today, and gives you a path to progressively more structured software delivery over time.**

That means the product identity is neither:
- pure chat enhancement,
- nor pure factory runtime.

It is the bridge between them.

---

## The role of the opinionated SDLC model

The SDLC model still matters.
But it should be understood as the **expansion spine**, not just the Day 1 pitch.

Aiki should still have a canonical default flow, something like:
1. **Define / Plan**
2. **Implement**
3. **Review**
4. **Fix**
5. **Approve / Escalate**
6. **Complete / Release**

But the user does not need to adopt that whole system on day one.

The better framing is:
- Aiki has the right SDLC skeleton underneath,
- and users grow into more of it as their usage matures.

---

## Where customization fits

Customization remains critical.

Users should be able to customize through:
- hooks,
- task templates,
- review criteria,
- checks,
- handoff rules,
- escalation policies,
- prompts,
- team and repo conventions.

But the role of customization is now clearer:
- at early stages, it tunes augmentation,
- at later stages, it tunes the operating system.

The principle remains:

> **Customize the behavior of the SDLC, not the existence of the SDLC.**

---

## Product principles

### 1) Immediate value without migration
Aiki must help the current agent workflow before it asks the user to change anything.

### 2) Structure should emerge progressively
The product should reveal more of the SDLC system as the user moves from chat toward factory.

### 3) Strong defaults matter more over time
The further up the ladder the user goes, the more valuable opinionated defaults become.

### 4) Verification must be native at every stage
From chat improvement to factory operation, review / checks / correction are always central.

### 5) Customization should deepen with maturity
More mature users should gain more power without needing a different product.

### 6) The path should feel continuous
It should not feel like the user has to “switch products” as they mature.

---

## Competitive implication

This framing is stronger because it avoids a bad choice between:
- “Aiki is just an enhancement layer”
- and “Aiki is just a factory platform”

The better answer is:
- Aiki starts where the user already is,
- then helps them move upward.

That is a better story than pure orchestration, because it gives us:
- a wedge,
- an adoption path,
- and a clear long-term destination.

---

## What this implies for the product

### 1) We need a great Day 1 experience
Aiki should visibly improve chat-style agent use without workflow migration.

### 2) We need a visible maturity ladder
The product should make it obvious how users grow from:
- chat,
- to one-shot,
- to multiple one-shot,
- to factory.

### 3) We need a canonical SDLC model underneath
The ladder only works if the product has a clear spine.

### 4) We need explicit boundaries for what gets better at each stage
Users should understand what Aiki unlocks next.

### 5) We need packaging for each rung
Examples:
- chat enhancement,
- reliable one-shot execution,
- coordinated multi-task runs,
- full SDLC / factory workflows.

---

## The line

If we want the shortest useful articulation, it is:

> **Aiki makes the agent you already use work better immediately — no workflow change required — and gives you a path from chat, to one-shots, to coordinated runs, to a full software factory.**

That is likely closer to the real vision than `ops/vision.md` alone.
