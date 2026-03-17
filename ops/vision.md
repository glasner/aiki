# Vision: Aiki

**Status:** internal founder draft

---

## The point

Aiki is an **opinionated SDLC operating system**.

Not:
- a generic orchestration platform,
- a graph runtime,
- a workflow programming language,
- or a blank substrate for agent experimentation.

The thesis is simpler:
- software delivery already has recurring steps,
- those steps should come with strong defaults,
- and teams should customize them deeply through **hooks, task templates, policies, and contracts**.

That is the product.

---

## The product belief

Most teams do not want to design a software factory.

They want:
- the right phases,
- the right handoffs,
- the right review points,
- the right escalation boundaries,
- and a clear definition of done.

Aiki should encode that directly.

The value is **not** maximum process freedom.
The value is:
- **minimum process design burden**,
- with **maximum leverage**.

---

## The core line

> **Aiki gives teams the right SDLC skeleton by default, then lets them deeply customize each core step.**

That is the line.

If we say “orchestration platform,” we get dragged into the wrong comparison set.
If we say “workflow engine,” we make ourselves sound generic.
If we say “agent platform,” we lose the product.

---

## The default model

Aiki should have a canonical answer for the core delivery flow.

At minimum:
1. **Define / Plan**
2. **Implement**
3. **Review**
4. **Fix**
5. **Approve / Escalate**
6. **Complete / Release**

If we cannot name the default steps clearly, then the opinionated system is not real yet.

---

## The customization model

Customization belongs at the seams.

Teams should be able to customize:
- hooks at each phase boundary,
- task templates,
- review criteria,
- verification logic,
- handoff rules,
- escalation policies,
- prompting behavior,
- repo and team conventions.

The principle is:

> **Customize the behavior of the SDLC, not the existence of the SDLC.**

That is the difference between Aiki and a blank orchestration runtime.

---

## What Aiki should feel like

Aiki should feel like:
- a software-delivery system,
- with clear movement from step to step,
- clear places to customize behavior,
- clear places where review happens,
- clear places where escalation happens,
- and clear evidence for why something is considered done.

Users should think about:
- delivery decisions,
- quality rules,
- and team policy.

They should **not** have to think about:
- workflow topology,
- graph semantics,
- or factory design.

---

## Product principles

### 1) Strong defaults beat blank-slate flexibility
Aiki should be useful before the user designs anything elaborate.

### 2) Customization should happen at the seams
Hooks, templates, contracts, and policies are the power layer.

### 3) Verification must be native
Review, checks, and correction are part of the SDLC, not optional extras.

### 4) Handoffs must be explicit
Aiki should make it obvious what is being handed off, why, and under what acceptance rules.

### 5) Recoverability matters
Partially completed work should not collapse into confusion. Aiki needs visible progress, inspectability, and continuation semantics.

### 6) Lower the thinking overhead
The system should reduce process design work, not create more of it.

---

## Competitive implication

The best competitive framing is:

- some systems help users **design arbitrary workflows**,
- Aiki should help users **run the right software-delivery model by default**.

Internal shorthand:

> **Aiki is opinionated but deeply customizable.**

That phrase matters.

If we only sound opinionated, we sound rigid.
If we only sound customizable, we sound generic.
We need both.

---

## What must become explicit in the product

### 1) The canonical steps
We need a stable model of the SDLC phases.

### 2) The seam map
We need a clear map of what is customizable and where.

### 3) The state model
Users need to see:
- what step is active,
- what already happened,
- what failed,
- what requires review,
- what requires escalation,
- what is next.

### 4) The definition of done
“Done” must be explicit and evidence-backed.

### 5) The packaged use cases
Aiki should be easy to package into:
- safe coding flows,
- review/fix loops,
- bug-fix flows,
- release gates,
- and team policy overlays.

---

## Anti-goals

Aiki should not drift into:
- generic orchestration language as the headline,
- graph-definition as the main interface,
- maximum flexibility as the core promise,
- or broad “agent platform” positioning.

Those directions weaken the product and make comparison easier for the wrong competitors.

---

## Bottom line

Aiki is not here to help teams invent a software factory from scratch.

Aiki is here to give teams the **right software-delivery system by default** — then let them **deeply customize each core step** through hooks, templates, contracts, and policies.
