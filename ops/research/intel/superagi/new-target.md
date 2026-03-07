# intel/new-target — SuperAGI

## Goal
Produce a complete competitive-intel brief on SuperAGI and convert findings into Aiki-relevant opportunities.

## Target
- Name: SuperAGI
- URL: https://github.com/TransformerOptimus/SuperAGI
- GitHub: https://github.com/TransformerOptimus/SuperAGI
- Date opened: 2026-03-05

## Tasks

### 1) Intake + normalization ✅

**Core product claim:**
SuperAGI is a dev-first open-source autonomous AI agent framework that enables developers to build, manage, and run concurrent autonomous agents with extensible toolkits — though the company has pivoted its commercial product (superagi.com) to an AI-native CRM platform for sales, marketing, and support teams.

**Primary user persona & workflow entrypoint:**
- **OSS repo persona:** Developer building autonomous AI agent workflows. Entrypoint: clone repo → Docker Compose → web UI at localhost:3000 → provision agents with toolkits from the marketplace.
- **Commercial product persona (pivot):** Sales/revenue teams (SDRs, AEs, sales leaders). Entrypoint: sign up at app.superagi.com → configure CRM → deploy "Digital Employees" (autonomous AI agents for outbound, pipeline management, meeting intelligence).

**Pricing/packaging:**
- OSS framework: MIT-licensed, free and self-hosted.
- SuperAGI Cloud (legacy agent platform): free tier available at app.superagi.com; no public paid tier pricing found.
- Commercial CRM product (superagi.com): All listed applications marked "Free" on the website; no explicit paid tiers or enterprise pricing publicly visible. Likely freemium or sales-led enterprise model (not publicly documented).

**Project health signals:**
- 17.2k GitHub stars, 2.2k forks — strong initial traction from the 2023 AI agent hype wave.
- Last release: v0.0.14 on 2024-01-16 — over a year without a release.
- Last commit: 2025-01-22 (security fix from external contributor) — the team has stopped contributing to the OSS repo.
- 215 open issues, minimal recent maintainer activity — effectively unmaintained OSS project.
- Company focus has clearly shifted to the closed-source AI-native CRM product.

**Sources:**
- GitHub repo: https://github.com/TransformerOptimus/SuperAGI
- Website: https://superagi.com/
- Cloud app: https://app.superagi.com
- Marketplace: https://marketplace.superagi.com/

### 2) Deep research pass
Collect evidence (URLs + short notes) from:
- Product website and docs
- Blog/changelog/release notes
- Public repo(s) and org page
- Public demos/videos/social threads where behavior is shown

Output artifact: `ops/now/intel/superagi/research.md`

### 3) Aiki relevance map
For each major capability, classify:
- Overlap with Aiki wedge (autonomous review)
- Potential threat level (low/med/high)
- Opportunity type: copy / counter / ignore
- Why now (timing signal)

Output artifact: `ops/now/intel/superagi/aiki-map.md`

### 4) Opportunity scoring
Create a ranked list of opportunities (top 10 max) scored on:
- User pain severity (1-5)
- Strategic fit to Aiki (1-5)
- Build complexity (1-5, inverse)
- GTM leverage (1-5)

Scoring formula:
`score = 0.35*pain + 0.35*fit + 0.20*gtm + 0.10*(6-complexity)`

Output artifact: `ops/now/intel/superagi/opportunities.md`

### 5) Convert top opportunities to execution tasks
Create exactly:
- 2 `feature/*` tasks
- 1 `experiment/*` task
- 1 `positioning/*` task

Each task must include:
- Hypothesis
- Success metric
- Scope guardrails
- Estimated effort (S/M/L)

Output artifact: `ops/now/intel/superagi/followups.md`

## Deliverable
`ops/now/intel/superagi/brief.md` containing:
1. What this project is
2. What matters for Aiki
3. Top 3 recommendations this week
4. Risks if we do nothing

## Definition of done
- All artifact files above created.
- Every claim backed by a source URL.
- Final brief includes explicit copy/counter/ignore decisions.
