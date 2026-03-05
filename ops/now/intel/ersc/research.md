# ERSC — Research Notes

## Intake
- **Core claim:** East River Source Control is building a collaboration platform on top of the Jujutsu (jj) version control system, focused on high-quality code review, understanding codebase evolution, and treating source control as infrastructure — targeting modern code collaboration for both humans and machines at any scale. ([source](https://ersc.io/))
- **Primary persona:** Software engineers and development teams who use or want to adopt Jujutsu (jj) as their VCS, looking for a forge/collaboration layer (code review, stacked PRs, developer tooling) comparable to what GitHub provides for Git.
- **Pricing:** Not publicly available. The company is in stealth mode with no launched product yet.
- **GitHub:** No dedicated GitHub org found for ERSC. The team contributes to [jj-vcs/jj](https://github.com/jj-vcs/jj) (the Jujutsu VCS itself). An `ersc-docs` GitHub org exists but is unrelated (Elden Ring mod docs).
- **First impression:** ERSC is a very early-stage, stealth-mode startup (founded 2025, raised ~$5M from Vermilion Cliffs Ventures) building what amounts to "a forge for Jujutsu" — the missing collaboration/review layer that jj lacks today. The team has deep expertise: Benjamin Brittain (founder, ex-Google, build systems), David Barsky (co-founder, Rust/distributed systems, ex-Facebook/Amazon), and Steve Klabnik (well-known Rust community figure, joined to work on jj full-time). No product is publicly available yet — the website is essentially a placeholder with a newsletter signup.

## Product & Features
- **Jujutsu-native forge:** ERSC is building the collaboration/hosting layer ("forge") purpose-built for jj, analogous to what GitHub is for Git. ([source](https://lobste.rs/s/kflxi5/east_river_source_control))
- **High-quality code review:** A core stated pillar — likely building review UX that leverages jj's change-id tracking and patch-based workflows for better interdiff and stacked-PR review. ([source](https://ersc.io/))
- **Codebase evolution understanding:** Stated goal to help users understand how codebases change over time, suggesting history visualization or analytics features. ([source](https://ersc.io/))
- **Source control as infrastructure:** Positioning SCM as a foundational platform layer, not just a tool — implies API-first design, integrations, and treating SCM as a service for both human developers and CI/machine consumers. ([source](https://ersc.io/))
- **Stacked PRs (inferred):** Community members infer "stacked PRs, deeply integrated developer tooling" as likely priorities based on team background and jj's natural support for stacked changes. ([source](https://news.ycombinator.com/item?id=44195211))
- **"JJHub" concept (speculative):** Podcast discussion referenced plans for a "JJHub" platform, though details are behind a paywall. ([source](https://fallthrough.transistor.fm/43))
- **No public product yet:** The website contains only a tagline, newsletter signup, and contact form. No feature pages, docs, or demos exist. ([source](https://ersc.io/))

## Technical Architecture
- **Built on Jujutsu (jj):** jj is a Git-compatible VCS with 26.3k GitHub stars, 10.9k commits, swappable backends, and native change-id tracking. jj allows gradual adoption — Git users can work alongside jj users transparently. ([source](https://github.com/jj-vcs/jj))
- **jj-yak (related project by founder):** Benjamin Brittain's `jj-yak` is a virtualized remote backend for jj, consisting of: (1) a CLI communicating via gRPC, (2) a daemon implementing an NFS server + caching layer, and (3) a centralized backend storing all commits. Written in Rust. 73 commits, experimental. ([source](https://github.com/benbrittain/jj-yak))
- **Likely Rust-based stack:** All three team members have deep Rust expertise. jj itself is written in Rust. jj-yak uses Rust + gRPC + NFS. The website uses Qwik (JS framework) for the frontend. ([source](https://github.com/benbrittain), [source](https://davidbarsky.com/))
- **jj change-id header:** jj puts a `change-id` header in Git commit headers, enabling different (better) code review workflows when the forge supports it. Current forges (GitHub, GitLab) don't fully leverage this. ([source](https://codeberg.org/forgejo/discussions/issues/325))
- **Patch-based code review gap:** Current GitHub-style forges struggle with patch-based and stacked workflows because it's hard to track what's been reviewed between force-pushes. jj's model naturally supports this, but needs a forge built for it. ([source](https://codeberg.org/forgejo/discussions/issues/325))

## Recent Activity
- **2025 (exact date unknown):** East River Source Control founded by Benjamin Brittain and David Barsky in New York, NY. ([source](https://pitchbook.com/profiles/company/894743-11))
- **2025-06-30:** Raised $4.86M (reported as ~$5M) from Vermilion Cliffs Ventures. SEC Form D filed. ([source](https://pitchbook.com/profiles/company/894743-11), [source](https://www.streetinsider.com/SEC+Filings/Form+D+East+River+Source+Contro/25033178.html))
- **2025-07-09:** Featured in AlleyWatch Startup Daily Funding Report. ([source](https://www.alleywatch.com/2025/07/the-alleywatch-startup-daily-funding-report-7-9-2025/))
- **2025 (mid-year):** Steve Klabnik announced joining ERSC to work on jj full-time, departing Oxide. Discussed on Fallthrough podcast. ([source](https://fallthrough.transistor.fm/43))
- **2025-08-28:** ERSC Bluesky account created (@ersc.io). ([source](https://bsky.app/profile/ersc.io))
- **2025 (late):** Website posted on Lobsters and HN, generating discussion. Team member noted it was made "for friends and immediate colleagues" and they're "nowhere near ready to launch a product." ([source](https://lobste.rs/s/kflxi5/east_river_source_control), [source](https://news.ycombinator.com/item?id=44195211))
- **2026 (current):** Still in stealth mode. No product launch announced. Website copyright updated to 2026. ([source](https://ersc.io/))

## Team
- **Benjamin Brittain** (Founder): Systems programmer, Rust expert, jj-vcs org member on GitHub, creator of jj-yak (virtualized remote backend), buckle (buck2 launcher). Previously worked on build systems at Google. ([source](https://github.com/benbrittain), [source](https://www.linkedin.com/in/benjamin-brittain/))
- **David Barsky** (Co-founder, Secretary): 10 years building distributed systems. Deep Rust expertise. Worked on rust-analyzer at Meta, expanded Rust usage at Amazon. Contributor to tracing, tokio, tower. ([source](https://davidbarsky.com/), [source](https://rocketreach.co/east-river-source-control-management_b69754fec97a7073))
- **Steve Klabnik**: Well-known Rust community figure, author of "The Rust Programming Language" book. Created jujutsu-tutorial (406 GitHub stars). Joined from Oxide to work on jj full-time. ([source](https://github.com/steveklabnik), [source](https://fallthrough.transistor.fm/43))
- **Brian Schroeder** (Software Engineer): 16 years experience in distributed systems, language tooling, and search infrastructure. Previously at Dropbox, Monic, and Ockam. ([source](https://rocketreach.co/brian-schroeder-email_48300875))

## Community & Social
- **Bluesky:** 1,101 followers on @ersc.io. Bio: "doin' source control stuff." Created August 2025. ([source](https://bsky.app/profile/ersc.io))
- **LinkedIn:** Company page exists at linkedin.com/company/east-river-source-control. ([source](https://www.linkedin.com/company/east-river-source-control))
- **Lobsters reception:** Mixed — recognized as strong team but noted premature public attention. Described as "GitHub but better" by one commenter. Team acknowledged being "nowhere near ready to launch." ([source](https://lobste.rs/s/kflxi5/east_river_source_control))
- **HN reception:** Brief discussion. Community sees "JJ forge" as the natural next step. Requested design docs or blog posts for substance. ([source](https://news.ycombinator.com/item?id=44195211))
- **Jujutsu ecosystem momentum:** jj itself has 26.3k stars and active development. Growing ecosystem of tools (jj.nvim, jj-yak, tutorials). Multiple blog posts and talks. Community is waiting for a native forge. ([source](https://github.com/jj-vcs/jj))
- **Podcast coverage:** Fallthrough podcast ep. 43 featured Steve Klabnik discussing joining ERSC, jj backends, ecosystem evolution, and "JJHub" concept. ([source](https://fallthrough.transistor.fm/43))

## Key Observations
- **Massive market gap:** jj has 26k+ stars and growing adoption, but no purpose-built forge exists. Users must use GitHub/GitLab with Git compatibility, losing jj's best features (change-ids, stacked changes, patch-based review). ERSC is positioning to fill this gap.
- **Exceptionally strong team for the problem:** The combination of build systems expertise (Brittain), Rust infrastructure experience (Barsky), Rust community credibility (Klabnik), and distributed systems depth (Schroeder) is unusually well-suited for building a next-gen VCS forge.
- **Very early stage, high opacity:** No product, no docs, no demos, no public roadmap. The website is a placeholder. All product details are inferred from team background, jj ecosystem needs, and community speculation. This makes competitive assessment difficult.
- **"Humans and machines" framing is notable:** The tagline "source control for humans and machines" suggests AI/automation-first design, not just developer tooling. This could mean API-first architecture, agent-friendly workflows, or CI-native collaboration patterns.
- **jj-yak as a technical signal:** Brittain's jj-yak project (gRPC + NFS virtualized backend) hints at ERSC's likely architecture — a server-side platform that virtualizes repository access, rather than just a web UI on top of Git.
- **Risk factors:** (1) jj itself is still "experimental" despite daily use; (2) GitHub's network effects are massive; (3) no public product after ~1 year of operation; (4) small team (4 people known) tackling a very large problem surface.

## Sources
- https://ersc.io/ — Landing page
- https://lobste.rs/s/kflxi5/east_river_source_control — Lobsters discussion with team commentary
- https://news.ycombinator.com/item?id=44195211 — HN discussion
- https://pitchbook.com/profiles/company/894743-11 — PitchBook profile ($4.86M raised, stealth mode)
- https://bsky.app/profile/ersc.io — Bluesky profile (1.1k followers)
- https://github.com/jj-vcs/jj — Jujutsu VCS repo (26.3k stars)
- https://github.com/benbrittain — Benjamin Brittain GitHub (jj-yak, buckle)
- https://github.com/benbrittain/jj-yak — Virtualized remote backend for jj
- https://github.com/steveklabnik — Steve Klabnik GitHub (jujutsu-tutorial)
- https://davidbarsky.com/ — David Barsky personal site
- https://fallthrough.transistor.fm/43 — Podcast: "JJ and How to Evolve an Open Source Ecosystem"
- https://rocketreach.co/east-river-source-control-management_b69754fec97a7073 — Team org chart
- https://www.streetinsider.com/SEC+Filings/Form+D+East+River+Source+Contro/25033178.html — SEC Form D filing
- https://www.alleywatch.com/2025/07/the-alleywatch-startup-daily-funding-report-7-9-2025/ — Funding report
- https://codeberg.org/forgejo/discussions/issues/325 — Patch-based code review discussion (jj context)
- https://tracxn.com/d/companies/east-river-source-control/__D9WLdnnjFtuC23n9TlhJpYYyL8SdfYxFxMS9oY3hHxM — Tracxn profile
