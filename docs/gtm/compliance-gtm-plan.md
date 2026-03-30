# GTM Plan: Aiki as the Compliance Layer for AI-Assisted Development

## The Opportunity

The EU AI Act is entering enforcement. Two articles create immediate, concrete
obligations for any team shipping AI-assisted code:

- **Article 12** — requires logs of AI system operations
- **Article 14** — requires human oversight of AI-generated outputs

Most engineering teams today have git blame and hope. That is not a compliance
posture. It is a liability.

Aiki already solves this. Every AI task is tracked, attributed, reviewable, and
exportable — from the first session. The product does not need to change. The
positioning does.

---

## Target Buyer

### Primary: Engineering Leadership at EU-regulated companies

- **VP/Director of Engineering** at companies with EU customers or EU-based
  developers. They are the ones who will get the audit letter. They need to
  answer: "How do you govern your AI-assisted code?"
- **Head of Platform / Developer Experience** at companies adopting AI coding
  tools at scale. They are tasked with enabling AI without creating risk.

### Secondary: Compliance / Legal / InfoSec

- **CISO / Head of Compliance** at companies already managing SOC2, ISO 27001,
  or similar frameworks. AI governance is landing on their desk next. They need
  evidence of controls, not promises.
- **GRC teams** evaluating AI risk. They want exportable audit artifacts.

### Company Profile

- 50-500 developers (large enough to have compliance obligations, small enough
  that a CLI tool can land without enterprise sales)
- Already using or evaluating Claude Code, Cursor, or Codex
- Shipping to EU customers or operating under EU jurisdiction
- Industries with existing regulatory muscle: fintech, healthtech, govtech,
  automotive software, defense-adjacent

---

## Core Message

**One line:** Aiki gives your team an EU AI Act-ready audit trail for every
AI-assisted code change — automatically, from the first session.

**Expanded:** Your developers are already using AI to write code. The EU AI Act
requires you to log those operations and prove human oversight. Aiki captures
what changed, why, who approved it, and what was reviewed — without changing how
your team works. One CLI. No server. No cloud. Everything local and exportable.

---

## What We Already Have (Product-Market Fit Evidence)

Aiki's existing features map directly to compliance requirements:

| EU AI Act Requirement | Aiki Capability |
|---|---|
| Article 12: Logs of AI operations | Every AI task is recorded with timestamps, descriptions, and change IDs (`aiki task show`) |
| Article 12: Traceability | `aiki blame` provides line-level attribution of AI vs. human changes |
| Article 14: Human oversight | Review gates (`aiki review`) enforce structured human-in-the-loop checkpoints |
| Article 14: Ability to intervene | Stop conditions, control points (`aiki doctor`), and review loops with up to 10 fix iterations |
| Audit trail completeness | Task DAG captures the full decision chain: plan, decompose, build, review, fix |
| Exportability | All data is local SQLite/JJ. No vendor lock-in. Full data ownership. |

---

## GTM Motions

### Motion 1: Content-Led Awareness (Weeks 1-4)

**Goal:** Own the "AI compliance for engineering teams" narrative before anyone
else does.

**Actions:**

1. **Publish "The Engineer's Guide to EU AI Act Compliance"**
   - Practical, not legal. What Articles 12 and 14 actually require from your
     codebase. What evidence an auditor would ask for. What most teams are
     missing.
   - Ends with: "Here is what a compliant workflow looks like" (Aiki demo).
   - Distribute on: blog, LinkedIn, Hacker News, dev-focused compliance
     newsletters.

2. **LinkedIn series: "5 questions your auditor will ask about AI-generated code"**
   - One post per question. Each post shows what the answer looks like with Aiki.
   - Engage with the compliance/GRC community, not just devtools.

3. **Technical blog: "How we built an audit trail that satisfies Article 12"**
   - Architecture walkthrough. JJ change IDs, task DAG, blame attribution.
   - Targets the platform/DX engineering audience who will evaluate and adopt.

### Motion 2: Direct Outreach to Regulated Teams (Weeks 2-6)

**Goal:** Get Aiki into 10 teams at companies with EU compliance obligations.

**Actions:**

1. **Identify 50 target companies** using these signals:
   - EU-headquartered or EU-revenue-dependent
   - Public job postings mentioning AI coding tools (Claude, Cursor, Copilot)
   - Existing compliance frameworks (SOC2, ISO 27001, GDPR)
   - Engineering blogs discussing AI adoption

2. **Cold outreach to VP Eng / Head of Platform** with this hook:
   > "Your team is probably using Claude Code or Cursor already. When your
   > compliance team asks how you log and review AI-generated code changes,
   > what do you show them? We built the answer."

3. **Offer a 30-minute compliance audit walkthrough:**
   - Install Aiki on one repo
   - Run a task, show the audit trail
   - Export the evidence an auditor would want
   - Leave them with a working setup, not a slide deck

### Motion 3: Integration with Compliance Toolchains (Weeks 4-8)

**Goal:** Make Aiki's audit data flow into existing compliance workflows.

**Actions:**

1. **Build an `aiki export` command** that outputs audit records in standard
   formats:
   - JSON (for custom integrations)
   - CSV (for compliance teams who live in spreadsheets)
   - SARIF (for security toolchain integration)

2. **Write integration guides** for common compliance tools:
   - Vanta, Drata, Secureframe (SOC2 automation)
   - Jira/Linear (linking AI tasks to tickets for traceability)
   - SIEM/log aggregation (Splunk, Datadog) for centralized audit logs

3. **Partner with one compliance automation vendor** (Vanta or Drata) for a
   joint case study: "How [Company] achieved EU AI Act compliance for their
   AI-assisted codebase."

### Motion 4: Community and Credibility (Ongoing)

**Goal:** Build trust with the compliance-aware engineering audience.

**Actions:**

1. **Speak at compliance-adjacent dev events:**
   - DevSecOps conferences (OWASP, BSides)
   - AI governance events (EU AI Act implementation forums)
   - Platform engineering meetups (where DX leads gather)

2. **Open-source compliance templates:**
   - Pre-built `.aiki/hooks.yml` configurations for common compliance postures
   - "EU AI Act starter kit" — hooks that enforce review gates on all AI tasks
   - Share these as a GitHub repo/template

3. **Build a "Compliance Score" command** (`aiki compliance-check`):
   - Scans repo for compliance gaps: unreviewed AI tasks, missing attribution,
     tasks without human oversight checkpoints
   - Outputs a report card teams can show their compliance officer
   - This becomes a viral adoption driver: "Run this on your repo and see your
     score"

---

## Pricing Angle

Aiki is currently open source (MPL-2.0). The compliance GTM opens a clear
monetization path:

- **Free / OSS:** Core task tracking, audit trail, blame attribution, review
  loops. Everything a team needs to be compliant.
- **Paid / Team tier:** Centralized audit log aggregation across repos,
  compliance reporting dashboards, export integrations, team-wide policy
  enforcement, SSO.

The compliance buyer has budget. They are already paying for Vanta ($15-50k/yr),
Drata, or manual audit prep. Positioning Aiki as "the AI governance layer that
plugs into your existing compliance stack" justifies a seat-based or repo-based
fee.

---

## Success Metrics (90 days)

| Metric | Target |
|---|---|
| Companies running Aiki in a compliance context | 10 |
| "EU AI Act + AI coding" content pieces published | 5 |
| Inbound inquiries mentioning compliance | 20 |
| `aiki export` command shipped | Yes |
| Compliance starter kit (hooks template) published | Yes |
| Partnership conversation with 1 compliance vendor | Started |

---

## What Needs to Ship

Aiki's core is already compliance-ready. The gaps are packaging, not product:

1. **`aiki export`** — structured export of audit records (JSON/CSV). This is
   the #1 blocker for compliance teams who need to hand evidence to auditors.

2. **`aiki compliance-check`** — repo-level compliance score. Checks for
   unreviewed AI tasks, missing attribution, incomplete audit trails. Great for
   adoption and for giving compliance officers something they can run themselves.

3. **Compliance-focused documentation page** — maps Aiki features to EU AI Act
   articles. This becomes a sales asset and an SEO magnet.

4. **Hooks starter kit for compliance** — pre-configured `.aiki/hooks.yml` that
   enforces: all AI tasks require review, all reviews require human approval,
   all tasks must have descriptions.

---

## Timeline

| Week | Focus |
|---|---|
| 1-2 | Ship compliance docs page + first LinkedIn content piece |
| 2-3 | Ship `aiki export` (JSON/CSV) |
| 3-4 | Publish "Engineer's Guide to EU AI Act Compliance" |
| 4-5 | Ship `aiki compliance-check` + hooks starter kit |
| 5-6 | Begin outreach to 50 target companies |
| 6-8 | Integration guide for 1 compliance vendor |
| 8-12 | Iterate based on feedback from first 10 compliance users |

---

## The Bet

Every engineering team using AI coding tools will need to answer "how do you
govern this?" within the next 12 months. The EU AI Act makes it a legal
requirement. SOC2 auditors are already asking. Enterprise procurement is already
blocking.

The teams that adopt governance tooling early will move faster, not slower —
because they will be the ones whose compliance teams say "yes, keep using AI"
instead of "stop until we figure this out."

Aiki is that tooling. The product exists. The compliance angle is the fastest
path to adoption because the buyer has urgency, budget, and a deadline.
