# Plugin Registry: Buckets and Proposed Integrations

This document identifies the key Jobs-to-be-Done (JTBD) for both **users** (developers using Aiki) and **agents** (AI coding assistants operating within Aiki sessions), and maps them to plugin buckets with concrete integration targets.

Each bucket describes:
- **User JTBD** — what the developer needs accomplished
- **Agent JTBD** — what the AI agent needs to do its job effectively
- **Integration pattern** — which Aiki events and actions the plugins hook into
- **Proposed integrations** — specific vendors/tools with functionality summaries

---

## 1. Code Quality & Linting

**User JTBD:** "Make sure AI-generated code meets our team's style and quality standards automatically."

**Agent JTBD:** "Know which linters to run and auto-fix issues before the user sees them."

**Integration pattern:** Hook into `change.completed` (post-write linting/formatting), `shell.permission_asked` (gate commits on lint pass), and `session.started` (inject project lint config as context).

| Integration | Functionality | Link |
|---|---|---|
| **ESLint** | JavaScript/TypeScript linting with auto-fix. Plugin runs `eslint --fix` on changed files after writes. | [eslint.org](https://eslint.org/) |
| **Prettier** | Opinionated code formatter for JS/TS/CSS/JSON/MD. Auto-formats on `change.completed`. | [prettier.io](https://prettier.io/) |
| **Biome** | Fast unified linter + formatter for JS/TS (ESLint + Prettier replacement). Single tool, single pass. | [biomejs.dev](https://biomejs.dev/) |
| **Ruff** | Extremely fast Python linter and formatter. Replaces flake8, isort, black. Auto-fix on write. | [docs.astral.sh/ruff](https://docs.astral.sh/ruff/) |
| **Clippy** | Rust linter (via `cargo clippy`). Catches common mistakes and suggests idiomatic improvements. | [doc.rust-lang.org/clippy](https://doc.rust-lang.org/clippy/) |
| **RuboCop** | Ruby static code analyzer and formatter. Enforces community style guide. | [rubocop.org](https://rubocop.org/) |
| **golangci-lint** | Go meta-linter that runs 50+ linters in parallel. Fast, configurable. | [golangci-lint.run](https://golangci-lint.run/) |

---

## 2. Testing & Test Runners

**User JTBD:** "Ensure AI changes don't break existing functionality and that new code has tests."

**Agent JTBD:** "Run the right test suite for this project, interpret results, and fix failures before presenting work."

**Integration pattern:** Hook into `turn.completed` (run tests after agent finishes work), `shell.permission_asked` (gate `git push` on test pass), and `session.started` (inject test commands as context). Use `autoreply` to trigger fix loops on failure.

| Integration | Functionality | Link |
|---|---|---|
| **Jest** | JavaScript testing framework. Run affected tests, parse failures, feed back to agent for fixes. | [jestjs.io](https://jestjs.io/) |
| **Vitest** | Vite-native test runner, Jest-compatible API. Faster for Vite-based projects. | [vitest.dev](https://vitest.dev/) |
| **pytest** | Python testing framework. Run with `--tb=short` for agent-parseable output. Coverage via pytest-cov. | [pytest.org](https://pytest.org/) |
| **cargo test** | Rust's built-in test runner. Compile-and-test with structured output. | [doc.rust-lang.org/cargo/commands/cargo-test.html](https://doc.rust-lang.org/cargo/commands/cargo-test.html) |
| **Go test** | Go's built-in test runner. Run `go test ./...` with race detection. | [pkg.go.dev/testing](https://pkg.go.dev/testing) |
| **Playwright** | End-to-end browser testing. Run after UI changes, capture screenshots on failure. | [playwright.dev](https://playwright.dev/) |
| **RSpec** | Ruby testing framework. BDD-style specs with rich failure output. | [rspec.info](https://rspec.info/) |

---

## 3. Security Scanning

**User JTBD:** "Catch security vulnerabilities before they reach production — in code, dependencies, and secrets."

**Agent JTBD:** "Check that my changes don't introduce known vulnerabilities, leaked secrets, or insecure patterns."

**Integration pattern:** Hook into `change.completed` (scan changed files), `shell.permission_asked` (block push if vulns found), `read.permission_asked` (block reads of sensitive files like `.env`). Use `block` action for critical findings.

| Integration | Functionality | Link |
|---|---|---|
| **Semgrep** | Static analysis with security-focused rulesets (OWASP, injection, XSS). Lightweight, supports 30+ languages. | [semgrep.dev](https://semgrep.dev/) |
| **Snyk** | Dependency vulnerability scanning + license compliance. Scans lockfiles and suggests upgrades. | [snyk.io](https://snyk.io/) |
| **Trivy** | All-in-one scanner for vulnerabilities in code, containers, IaC, and secrets. | [trivy.dev](https://trivy.dev/) |
| **Gitleaks** | Detect hardcoded secrets (API keys, tokens, passwords) in code and git history. | [github.com/gitleaks/gitleaks](https://github.com/gitleaks/gitleaks) |
| **OSV-Scanner** | Google's open-source vulnerability scanner. Checks dependencies against the OSV database. | [google.github.io/osv-scanner](https://google.github.io/osv-scanner/) |
| **Socket** | Supply chain security — detects typosquatting, install scripts, and telemetry in npm/PyPI packages. | [socket.dev](https://socket.dev/) |

---

## 4. CI/CD Integration

**User JTBD:** "See CI results in my Aiki workflow without switching to a browser. Let the agent react to failures."

**Agent JTBD:** "Know whether the CI pipeline passed or failed, and get enough detail to fix failures."

**Integration pattern:** Hook into `shell.completed` (after `git push`, poll CI status), `task.closed` (verify CI green before marking done). Use `context` to inject CI status, `autoreply` to trigger fix loops on CI failure.

| Integration | Functionality | Link |
|---|---|---|
| **GitHub Actions** | Monitor workflow runs, fetch logs on failure, re-trigger jobs. Most common CI for GitHub repos. | [github.com/features/actions](https://github.com/features/actions) |
| **GitLab CI** | Pipeline status, job logs, and artifact retrieval for GitLab-hosted repos. | [docs.gitlab.com/ci](https://docs.gitlab.com/ee/ci/) |
| **CircleCI** | Fetch pipeline status and job output. Supports SSH debug for flaky tests. | [circleci.com](https://circleci.com/) |
| **Buildkite** | Agent-friendly CI with structured logs and API access. Popular in larger orgs. | [buildkite.com](https://buildkite.com/) |

---

## 5. Issue Tracking & Project Management

**User JTBD:** "Link AI work to the issues I'm tracking. Auto-update ticket status when work is done."

**Agent JTBD:** "Understand the full context of the issue I'm working on — description, acceptance criteria, related discussions."

**Integration pattern:** Hook into `task.started` (fetch issue details, inject as context), `task.closed` (update ticket status, post summary comment), `session.started` (list assigned/in-progress issues). Use `context` to give agents issue details.

| Integration | Functionality | Link |
|---|---|---|
| **Linear** | Fetch issue details, update status, post comments. GraphQL API, fast, developer-centric. | [linear.app](https://linear.app/) |
| **Jira** | Read issue details, transition status, add worklogs. Dominant in enterprise. | [atlassian.com/software/jira](https://www.atlassian.com/software/jira) |
| **GitHub Issues** | Read/write issues, labels, and comments. Native to GitHub-hosted projects. | [github.com/features/issues](https://github.com/features/issues) |
| **Shortcut** | Issue tracking with Slack-native workflow. Popular with small-to-mid teams. | [shortcut.com](https://shortcut.com/) |
| **Notion** | Databases as issue trackers. Rich context (docs, specs) linked to tasks. | [notion.so](https://www.notion.so/) |
| **Asana** | Task and project management with custom fields. Common in cross-functional teams. | [asana.com](https://asana.com/) |

---

## 6. Communication & Notifications

**User JTBD:** "Get notified when AI work finishes, fails, or needs my attention — in the tools I already use."

**Agent JTBD:** "Alert the right people when something noteworthy happens during a session."

**Integration pattern:** Hook into `session.ended` (send summary), `task.closed` (notify completion), `turn.completed` with failure conditions (alert on errors). Fire-and-forget `shell` actions to send messages.

| Integration | Functionality | Link |
|---|---|---|
| **Slack** | Post messages to channels or DMs via webhooks or API. Session summaries, review requests, failure alerts. | [slack.com](https://slack.com/) |
| **Discord** | Webhook-based notifications to channels. Similar to Slack, popular in OSS communities. | [discord.com](https://discord.com/) |
| **Microsoft Teams** | Post adaptive cards or messages via incoming webhooks. Enterprise standard. | [microsoft.com/en-us/microsoft-teams](https://www.microsoft.com/en-us/microsoft-teams) |
| **Email (SMTP/SES)** | Send email notifications for session completion or review requests. Universal fallback. | [aws.amazon.com/ses](https://aws.amazon.com/ses/) |
| **PagerDuty** | Trigger incidents for critical failures (e.g., security scan finds high-severity vuln). | [pagerduty.com](https://www.pagerduty.com/) |

---

## 7. Documentation & API Specs

**User JTBD:** "Keep documentation in sync with code changes. Auto-generate API docs when endpoints change."

**Agent JTBD:** "Know the project's documentation standards and update docs alongside code changes."

**Integration pattern:** Hook into `change.completed` (detect if changed files affect documented APIs), `turn.completed` (remind agent to update docs), `session.started` (inject doc standards as context).

| Integration | Functionality | Link |
|---|---|---|
| **OpenAPI / Swagger** | Validate OpenAPI specs, detect drift between code and spec, auto-generate stubs. | [swagger.io](https://swagger.io/) |
| **TypeDoc** | Generate TypeScript API documentation. Verify docs build after type changes. | [typedoc.org](https://typedoc.org/) |
| **rustdoc** | Rust documentation generator. Ensure `cargo doc` builds cleanly after changes. | [doc.rust-lang.org/rustdoc](https://doc.rust-lang.org/rustdoc/) |
| **Storybook** | UI component documentation and visual testing. Verify stories after component changes. | [storybook.js.org](https://storybook.js.org/) |
| **Mintlify** | Developer docs platform. Sync docs from code, detect stale content. | [mintlify.com](https://mintlify.com/) |

---

## 8. Observability & Metrics

**User JTBD:** "Track how AI coding sessions perform — success rates, time-to-completion, quality metrics."

**Agent JTBD:** "Report structured telemetry about what happened during the session for team dashboards."

**Integration pattern:** Hook into `session.ended` (emit session metrics), `task.closed` (emit task metrics), `turn.completed` (track turn counts). Use `shell` to push metrics to collectors.

| Integration | Functionality | Link |
|---|---|---|
| **Datadog** | Push custom metrics and events via DogStatsD or API. Dashboards for AI coding KPIs. | [datadoghq.com](https://www.datadoghq.com/) |
| **Grafana / Prometheus** | Push metrics to Prometheus pushgateway, visualize in Grafana. Self-hosted option. | [grafana.com](https://grafana.com/) |
| **Honeycomb** | Trace-based observability. Model each session as a trace, turns as spans. | [honeycomb.io](https://www.honeycomb.io/) |
| **PostHog** | Product analytics + session replay. Track AI coding feature adoption and outcomes. | [posthog.com](https://posthog.com/) |

---

## 9. Cloud & Infrastructure

**User JTBD:** "Let the agent validate infrastructure changes (IaC) and check deployment status."

**Agent JTBD:** "Validate Terraform/CloudFormation changes before they're applied. Check service health."

**Integration pattern:** Hook into `change.completed` (validate IaC files on write), `shell.permission_asked` (gate destructive infra commands), `session.started` (inject cloud context — region, account, environment).

| Integration | Functionality | Link |
|---|---|---|
| **Terraform** | Run `terraform validate` and `terraform plan` on changed `.tf` files. Block applies without review. | [terraform.io](https://www.terraform.io/) |
| **Pulumi** | TypeScript/Python/Go IaC. Run `pulumi preview` after changes. | [pulumi.com](https://www.pulumi.com/) |
| **AWS CDK** | Synth and diff CDK stacks after changes. Prevent accidental resource deletion. | [aws.amazon.com/cdk](https://aws.amazon.com/cdk/) |
| **Kubernetes (kubectl)** | Validate manifests with `kubectl --dry-run`, check rollout status. | [kubernetes.io](https://kubernetes.io/) |
| **Docker** | Validate Dockerfiles, lint with hadolint, build images after changes. | [docker.com](https://www.docker.com/) |

---

## 10. Database & Schema Management

**User JTBD:** "Ensure database migrations are safe and reversible. Don't let AI drop columns in production."

**Agent JTBD:** "Understand the current schema, generate safe migrations, and validate them before applying."

**Integration pattern:** Hook into `change.completed` (validate migration files), `shell.permission_asked` (block destructive SQL — `DROP TABLE`, `TRUNCATE`), `session.started` (inject schema as context).

| Integration | Functionality | Link |
|---|---|---|
| **Prisma** | TypeScript ORM. Validate schema changes, generate migrations, check for drift. | [prisma.io](https://www.prisma.io/) |
| **Drizzle** | TypeScript ORM with SQL-like syntax. Generate and validate migrations. | [orm.drizzle.team](https://orm.drizzle.team/) |
| **Alembic** | Python (SQLAlchemy) migration tool. Autogenerate and review migration scripts. | [alembic.sqlalchemy.org](https://alembic.sqlalchemy.org/) |
| **Atlas** | Database schema management tool. Declarative migrations with safety checks and linting. | [atlasgo.io](https://atlasgo.io/) |
| **Django Migrations** | Auto-detect model changes, generate migration files, check for backwards-incompatible changes. | [djangoproject.com](https://www.djangoproject.com/) |

---

## 11. Package & Dependency Management

**User JTBD:** "Keep dependencies up to date and don't let AI add packages with known issues or license problems."

**Agent JTBD:** "Check that newly added dependencies are safe, maintained, and license-compatible."

**Integration pattern:** Hook into `change.completed` (scan lockfile changes for new deps), `shell.permission_asked` (intercept `npm install`, `pip install` to validate before adding).

| Integration | Functionality | Link |
|---|---|---|
| **Renovate** | Automated dependency updates with merge confidence scores. PRs with changelogs. | [docs.renovatebot.com](https://docs.renovatebot.com/) |
| **Dependabot** | GitHub-native dependency updates. Security alerts and auto-PRs. | [github.com/dependabot](https://github.com/dependabot) |
| **npm audit** | Check npm packages for known vulnerabilities. Built into npm CLI. | [docs.npmjs.com/cli/commands/npm-audit](https://docs.npmjs.com/cli/v10/commands/npm-audit) |
| **pip-audit** | Audit Python dependencies against the OSV and PyPI advisory databases. | [github.com/pypa/pip-audit](https://github.com/pypa/pip-audit) |
| **cargo-deny** | Lint Rust dependencies for vulnerabilities, licenses, and banned crates. | [embarkstudios.github.io/cargo-deny](https://embarkstudios.github.io/cargo-deny/) |

---

## 12. Code Review & PR Automation

**User JTBD:** "Automate the tedious parts of code review — style nits, coverage checks, conventional commits."

**Agent JTBD:** "Self-review my work before presenting it, catching issues the user would flag."

**Integration pattern:** Hook into `turn.completed` (trigger self-review via `autoreply`), `commit.message_started` (enforce conventional commit format), `task.closed` (auto-create PR with summary). Leverages Aiki's built-in review/fix loop.

| Integration | Functionality | Link |
|---|---|---|
| **Conventional Commits** | Enforce commit message format (`feat:`, `fix:`, `chore:`). Parse for changelogs. | [conventionalcommits.org](https://www.conventionalcommits.org/) |
| **Danger** | Automated code review rules (PR too large, missing tests, changelog not updated). | [danger.systems](https://danger.systems/) |
| **Codecov** | Coverage reporting. Block PRs that decrease coverage below threshold. | [codecov.io](https://about.codecov.io/) |
| **SonarQube** | Comprehensive code quality — bugs, vulnerabilities, code smells, duplication. | [sonarqube.org](https://www.sonarsource.com/products/sonarqube/) |

---

## Priority and Sequencing

Recommended build order based on breadth of impact and user demand:

| Phase | Buckets | Rationale |
|---|---|---|
| **Phase 1: Foundation** | Code Quality, Testing, Security Scanning | Every project needs these. Highest daily-use frequency. |
| **Phase 2: Workflow** | Issue Tracking, CI/CD, Communication | Connect Aiki to the tools teams already live in. |
| **Phase 3: Depth** | Code Review, Package Management, Documentation | Deeper automation for mature teams. |
| **Phase 4: Platform** | Observability, Cloud/Infra, Database | Specialized but high-value for platform teams. |

---

## Plugin Design Principles

1. **Small and composable** — one plugin per tool, not per bucket. Users pick what they use.
2. **Fail-open by default** — use `on_failure: continue` for non-critical actions. Don't block the developer.
3. **Context-first** — prefer `context` injection (tell the agent what to do) over `autoreply` (force the agent to act). Let agents be smart.
4. **Zero-config start** — plugins should work with sensible defaults. Config is optional.
5. **Namespace convention** — first-party plugins use `aiki/` namespace (e.g., `aiki/eslint`, `aiki/pytest`). Community plugins use `org/name`.
