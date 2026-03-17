# Vision: Aiki

**Status:** internal working draft  
**Audience:** Jordan / Aiki team

---

## The thesis

Aiki is an **opinionated SDLC operating system**.

The core idea is simple:
- software delivery has recurring steps,
- those steps should have strong defaults,
- and teams should customize them deeply through **hooks, task templates, policies, and contracts**,
- without having to redesign the whole software-delivery process from scratch.

Aiki should not feel like:
- a generic workflow runtime,
- a graph authoring environment,
- or a blank orchestration substrate.

It should feel like:
- the right software-delivery model, already shaped,
- with powerful customization at the seams that actually matter.

---

## The core belief

Most teams do not want to invent a software factory.

They want:
- the right phases,
- the right handoffs,
- the right review points,
- the right escalation boundaries,
- and the right definition of done.

The product should encode those directly.

The value is not maximum process freedom.
The value is:
- **minimum process design burden**,
- with **maximum leverage**.

---

## Product definition

Aiki should be understood as:

> **the software-delivery system that gives you the right SDLC skeleton by default, then lets you deeply customize each core step.**

That is the center.

Not:
- “AI orchestration platform”
- “multi-agent workflow engine”
- “graph-based software factory”
- “generic autonomous development runtime”

Those may describe some implementation truth, but they are the wrong product identity.

---

## The default SDLC model

Aiki should expose a canonical answer to the core delivery flow.

At minimum, some version of:
1. **Define / Plan**
2. **Implement**
3. **Review**
4. **Fix**
5. **Approve / Escalate**
6. **Complete / Release**

This does not need to be rigid in presentation, but it does need to be explicit.

If Aiki cannot clearly name the default steps, then the opinionated system is not yet real.

---

## Where customization belongs

The customization model should be equally explicit.

Teams should be able to customize:
- **hooks** at each phase boundary,
- **task templates** for recurring work,
- **review criteria**,
- **checks / verification logic**,
- **handoff rules**,
- **escalation policies**,
- **prompting behavior**,
- **repo / team-specific conventions**.

The key principle:

> **Customize the behavior of the SDLC, not the existence of the SDLC.**

That is the difference between a delivery system and a blank orchestration runtime.

---

## What Aiki should feel like

Aiki should feel like:
- a software-delivery operating model,
- not a workflow programming language,
- not a REPL wrapper,
- not a collection of prompts,
- and not an abstract “agent platform.”

Users should feel:
- there is a clear path for work to move through,
- there is a clear place to customize behavior,
- there is a clear place where review happens,
- there is a clear place where escalation happens,
- and there is a clear explanation for why something is considered done.

---

## Product principles

### 1) Strong defaults beat blank-slate flexibility
Aiki should be useful before the user designs anything elaborate.

### 2) Customization should happen at the seams
Hooks, templates, contracts, and policies are the power layer.

### 3) Verification must be native
Review, checks, and correction are not optional extras. They are part of the SDLC.

### 4) Handoffs must be explicit
Aiki should make it clear:
- what is being handed off,
- why,
- under what rules,
- and what counts as acceptance.

### 5) Recoverability matters
Partially completed work should not collapse into confusion. Aiki needs visible progress, inspectability, and continuation semantics.

### 6) The system should lower thinking overhead
Users should think about delivery decisions, not workflow topology.

---

## What this means competitively

Aiki should not compete as:
- the most general runtime,
- the most flexible graph engine,
- or the most programmable workflow substrate.

That is a trap.

The better competitive position is:
- there are core SDLC steps,
- Aiki gives you the right default model,
- and Aiki gives you deep control over each critical step without asking you to rebuild the whole system.

The internal shorthand:

> **Aiki is opinionated but deeply customizable.**

That phrase is important.

Because the failure mode is obvious:
- if we only sound opinionated, we sound rigid;
- if we only sound customizable, we sound generic.

The product has to be both.

---

## What must become clearer in the product

### 1) The canonical steps
We need a stable, team-understandable model of the SDLC phases.

### 2) The seam map
We need a clear map of what users can customize and where.

### 3) The state model
Users need to see:
- what step is active,
- what already happened,
- what failed,
- what requires review,
- what requires escalation,
- and what is next.

### 4) The definition of done
Aiki should make “done” explicit and evidence-backed.

### 5) The packaged use cases
Aiki should be easy to package into:
- safe coding workflows,
- review/fix loops,
- bug-fix flows,
- release gates,
- and repo/team-specific delivery policies.

---

## Anti-goals

Aiki should not drift into:
- generic orchestration language as the headline,
- graph-definition as the primary user interface,
- maximum flexibility as the product promise,
- or broad “agent platform” positioning.

Those directions weaken the product.

They make Aiki easier to compare to tools whose core identity is runtime orchestration, instead of reinforcing the actual thesis.

---

## Bottom line

Aiki is not trying to help teams invent a software factory from scratch.

Aiki is trying to give teams the **right software-delivery system by default** — then let them **deeply customize each core step** through hooks, templates, contracts, and policies.

That is the vision.
