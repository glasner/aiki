---
status: draft
---

# Plugin Registry

**Status**: Draft
**Priority**: P2
**Depends On**: [Remote Plugins](remote-plugins.md), [Plugin Directory](plugin-directory.md)

**Related Documents**:
- [Remote Plugins](remote-plugins.md) — Plugin install from GitHub
- [Plugin Directory](plugin-directory.md) — Local plugin structure

---

## Problem

`aiki plugin install owner/repo` requires users to already know the exact plugin reference. There's no way to discover plugins — users must learn about them through word of mouth, READMEs, or external links.

This blocks adoption: plugin authors can't reach users, and users can't find plugins that solve their problems.

---

## Summary

A hosted registry service that indexes aiki plugins discovered on GitHub. The registry is a discovery layer — plugins remain hosted on GitHub. Users search the registry via CLI to find plugins, then install them with the existing `aiki plugin install` flow.

**Key principles:**
- The registry is an index, not a host. It stores metadata about plugins, not the plugins themselves.
- Content is populated by a GitHub scraper task template, not by user submissions.
- The CLI is the only interface (no web UI in v1).

---

## User Experience

### Searching

```bash
# Search by keyword
aiki plugin search security
#  aiki/way          The opinionated aiki workflow (review loops, lint gates)
#  acme/security     Security-focused code review templates
#  myorg/pci         PCI-DSS compliance checks

# Search by category
aiki plugin search --category review
#  aiki/way          The opinionated aiki workflow
#  fastco/deep-review  Multi-pass deep code review

# Show plugin details
aiki plugin show acme/security
#  acme/security
#  Security-focused code review templates
#
#  Status:     Not installed
#  Author:     acme
#  Repository: github.com/acme/security
#  Categories: security, review
#  Templates:  audit, vulnerability-scan, dependency-check
#  Hooks:      turn.completed (security scan)
#
#  Install: aiki plugin install acme/security

# Show installed plugin details
aiki plugin show aiki/way
#  aiki/way
#  The opinionated aiki workflow (review loops, lint gates)
#
#  Status:     Installed at ~/.aiki/plugins/aiki/way
#  Author:     aiki
#  Repository: github.com/aiki/way
#  Categories: workflow, review
#  Templates:  review, lint, gate
#  Hooks:      turn.completed (review loop)
#
#  Update: aiki plugin update aiki/way
```


### Installing

Like Terraform's registry pattern, the install command queries the registry for metadata (including the clone URL), then clones directly from that URL:

```bash
aiki plugin install acme/security
# 1. → Registry API: GET /plugins/acme/security
#    Response: { "github_repo": "github.com/acme/security", ... }
# 2. → git clone --depth 1 https://github.com/acme/security ~/.aiki/plugins/acme/security
```

**Why query the registry during install?**

1. **Indirection** - Plugin location can change without breaking references
2. **Validation** - Registry confirms the plugin exists and is valid before cloning
3. **Flexibility** - Future support for non-GitHub sources (GitLab, Bitbucket, self-hosted)
4. **Metadata** - Registry provides additional info (categories, description) for install confirmation

### Hosting

**Platform**: Cloudflare Workers (TypeScript)
**Domain**: `registry.aiki.sh`
**API Base**: `registry.aiki.sh/api/v1/`

TypeScript is native to Workers' V8 runtime — no WASM compilation, no container, no runtime to manage. We get global edge deployment (sub-10ms worldwide), zero cold starts, and the full Cloudflare ecosystem (KV, R2, D1) without impedance mismatch.

| Layer | Technology |
|-------|-----------|
| Runtime | Cloudflare Workers |
| Language | TypeScript |
| Data store | Cloudflare KV (plugin index) |
| Admin auth | Bearer token (environment secret) |
| Domain | `registry.aiki.sh` (Cloudflare DNS) |
| Deploy | `wrangler deploy` via GitHub Actions |
| Local dev | `wrangler dev` |

**What we give up (and why it's fine):**
- No long-running processes — registry is request/response
- 128MB memory limit — plugin index is kilobytes
- No filesystem — KV replaces in-memory JSON
- 10ms CPU limit (free) / 30ms (paid) — linear scan over hundreds of entries is sub-millisecond

<details>
<summary>Why not the other options?</summary>

| Passed on | Reason |
|-----------|--------|
| **Fly.io** | Good, but cold starts on scale-to-zero; container overhead for a stateless service |
| **Cloud Run** | Great free tier, but cold starts (~500-1000ms) and GCP complexity |
| **Lambda** | Requires handler model reshaping + API Gateway adds latency/cost |
| **Hetzner** | Always-on cost, ops burden for a service this simple |
| **Railway** | $5/mo minimum, no scale-to-zero |
| **Shuttle** | Code-level vendor lock-in, young platform |
| **Rust on Workers (WASM)** | WASM compilation friction killed the DX. TypeScript is native |

</details>

### Registry Service

A hosted service with:

1. **Read-only REST API** — Search and plugin detail endpoints
2. **Admin API** — Ingest endpoint used by the scraper (token-authenticated)
3. **GitHub integration** — Scraper reads plugin metadata from repos

**No database.** The plugin index lives in Cloudflare KV — a global, read-optimized key-value store. The scraper writes to KV; the Worker reads from it. At the expected scale (hundreds to low thousands of plugins), search is sub-millisecond and there's no database to manage.

### KV Schema

Single KV namespace: `PLUGIN_REGISTRY`

| Key | Value | Purpose |
|-----|-------|---------|
| `plugins:index` | JSON array of all plugin metadata | The full index, read on every search |
| `plugins:{ns}/{name}` | JSON object for one plugin | Fast single-plugin lookup |

The index key is written atomically by the scraper/admin API. KV's eventual consistency (60s propagation) is fine — plugin metadata changes are rare and not latency-sensitive.

### Data Model

The registry stores metadata, not plugin contents:

```
Plugin:
  namespace: string          # "acme"
  name: string               # "security"
  reference: string          # "acme/security" (unique key)
  github_repo: string        # "github.com/acme/security"
  description: string        # from repo README or plugin.yaml
  categories: string[]       # ["security", "review"]
  templates: string[]        # ["audit", "vulnerability-scan"]
  hooks: string[]            # ["turn.completed"]
  author: string             # GitHub username/org
  discovered_at: timestamp   # when scraper first found it
  refreshed_at: timestamp    # last metadata sync from GitHub
```

### Metadata Extraction

When the scraper discovers a plugin repo, it extracts:

1. **Templates** — Lists `.md` files in `templates/`
2. **Hooks** — Parses `hooks.yaml` for event names
3. **Description** — First paragraph of `README.md`, or `description` field from `plugin.yaml` if present
4. **Categories** — From `plugin.yaml` if present, or empty

Optional `plugin.yaml` in repo root:

```yaml
description: Security-focused code review templates
categories:
  - security
  - review
```

If no `plugin.yaml` exists, the scraper falls back to README extraction and empty categories. This keeps the zero-config experience from `remote-plugins.md` intact — plugin authors don't need to do anything special to be discoverable.

### Search Implementation

No search engine, no inverted index, no database. The plugin list fits in a single KV key — search is a linear scan with weighted scoring.

#### How It Works

1. Worker reads the `plugins:index` key from KV (cached per request)
2. On search request, iterate all plugins, score each against the query terms
3. Filter by category if `--category` is specified (pre-filter, before scoring)
4. Sort by score descending, return top results

#### Scoring

Query is lowercased and split into whitespace-delimited terms. Each plugin is scored by checking where terms appear:

| Match location | Points per term | Rationale |
|---------------|----------------|-----------|
| `reference` (exact match) | 10 | User searched for exactly this plugin |
| `reference` (contains) | 5 | Partial name match is strong signal |
| `categories` (exact match) | 4 | Category match means topical relevance |
| `description` (contains) | 1 | Weakest signal — broad text match |
| `templates` (exact match) | 2 | User may search for a capability |

A plugin with score 0 is excluded from results. Scores are summed across all query terms.

#### Example

Query: `security review`

| Plugin | "security" | "review" | Total |
|--------|-----------|----------|-------|
| `acme/security` | 10 (exact ref) | 0 | 10 |
| `acme/sec-review` | 1 (desc) | 5 (ref contains) | 6 |
| `myorg/pci` | 1 (desc) | 0 | 1 |

#### Empty Query

`aiki plugin search` with no query and no category returns all plugins sorted by `discovered_at` (most recent first). With `--category` only, returns all plugins in that category.

#### Future: Typo Tolerance

If typo tolerance becomes needed, add a lightweight fuzzy matching library (e.g., `fuse.js`). No architectural changes required.

### CLI Integration

The CLI talks to the registry API for all plugin operations (search, show, install):

```
aiki plugin search "security"
       │
       ▼
  registry.aiki.sh: GET /api/v1/plugins?q=security
       │
       ▼
  Returns: [{ref: "acme/security", description: "...", ...}]

aiki plugin show acme/security
       │
       ├─▶ Local filesystem: check ~/.aiki/plugins/acme/security
       │   (determines installation status)
       │
       └─▶ registry.aiki.sh: GET /api/v1/plugins/acme/security
           (fetches metadata)

aiki plugin install acme/security
       │
       ├─▶ Registry API: GET /plugins/acme/security
       │   Returns: { "github_repo": "github.com/acme/security", ... }
       │
       └─▶ GitHub: git clone --depth 1 https://github.com/acme/security
           (registry provides the URL, but doesn't proxy the clone)
```

**Registry URL**: `https://registry.aiki.sh/api/v1/`. The CLI hardcodes this as the default. No configuration needed.

### Project Structure

```
registry/
├── src/
│   ├── index.ts          # Worker entrypoint, router
│   ├── routes/
│   │   ├── search.ts     # GET /plugins search + list
│   │   ├── detail.ts     # GET /plugins/:ns/:name
│   │   └── admin.ts      # POST/DELETE admin endpoints
│   ├── scoring.ts        # Search scoring logic
│   └── types.ts          # Plugin type definitions
├── wrangler.toml         # Worker config, KV bindings
├── tsconfig.json
├── package.json
└── test/
    └── *.test.ts         # Vitest tests (wrangler integrates with vitest)
```

---
## GitHub Scraper

The scraper is an aiki task template that discovers plugins on GitHub and submits them to the registry.

### How It Works

1. Searches GitHub for repos matching aiki plugin conventions
2. For each candidate, clones and validates the repo structure
3. Extracts metadata (templates, hooks, description, categories)
4. Writes entries to `plugins.json` (or submits to the registry admin API)

### Search Strategy

```
GitHub Search queries:
  - topic:aiki-plugin
  - filename:hooks.yaml path:/ (repos with hooks.yaml at root)
  - filename:plugin.yaml "aiki" in:readme
```

The scraper is conservative — it validates every candidate before adding it. A repo must contain `hooks.yaml` and/or a `templates/` directory with `.md` files.

### Running the Scraper

```bash
# Run the scraper task template
aiki task start "Scrape GitHub for aiki plugins"
# The task template handles the search, validation, and ingestion
```

The scraper runs on-demand. It can also be scheduled (cron, CI) to keep the registry fresh.

### Handling Staleness

The scraper re-validates previously discovered plugins on each run. If a repo no longer exists or no longer contains valid plugin structure, it's marked as stale (not immediately removed). Stale plugins are excluded from search results after a configurable grace period.

---

## API

### Search (Public)

```
GET /api/v1/plugins?q={query}&category={category}&sort={sort}&limit={limit}&offset={offset}

Sort options: relevance (default), recent
Response: [{ reference, description, categories, templates, hooks, author, github_repo }]
```

### Plugin Detail (Public)

```
GET /api/v1/plugins/{namespace}/{name}

Response: { reference, description, categories, templates, hooks, author, github_repo, discovered_at, refreshed_at }
```

### Ingest (Admin)

```
POST /api/v1/admin/plugins
Authorization: Bearer {admin-token}
Body: { "repo": "github.com/owner/repo", "metadata": { ... } }

Used by the scraper to add/update plugin entries.
```

### Refresh (Admin)

```
POST /api/v1/admin/plugins/{namespace}/{name}/refresh
Authorization: Bearer {admin-token}

Re-validates and refreshes metadata for a specific plugin.
```

### Remove (Admin)

```
DELETE /api/v1/admin/plugins/{namespace}/{name}
Authorization: Bearer {admin-token}

Removes a plugin from the registry.
```

---

## Categories

Start with a small fixed set. Expand based on usage.

| Category | Description |
|----------|-------------|
| `review` | Code review templates and hooks |
| `security` | Security scanning and auditing |
| `testing` | Test generation and validation |
| `docs` | Documentation generation |
| `style` | Code style and formatting |
| `workflow` | General workflow automation |
| `ci` | CI/CD integration |

Categories are extracted from `plugin.yaml`. Repos without `plugin.yaml` have no categories but are still searchable by keyword.

---

## Decisions

1. **Registry hosting**: Cloudflare Workers + TypeScript at `registry.aiki.sh/api/v1/`. TypeScript is native to the V8 runtime — no WASM, no containers.
2. **Namespace ownership**: GitHub-verified in future phases. For v1 (scraper-only), namespaces match GitHub owners automatically since the scraper derives them from repo URLs.
3. **Plugin validation**: Yes. The scraper validates that repos contain `hooks.yaml` and/or `templates/`. Invalid repos are skipped.
4. **No user-facing publish in v1**: All content comes from the scraper. Self-publishing is a future phase.
5. **No database**: Plugin index lives in a single Cloudflare KV key. Search is a linear scan with weighted scoring — no search engine, no SQLite, no inverted index.

## Open Questions

1. **Staleness grace period**: How long before a stale plugin is removed from results? 30 days? 90 days?

---

## Future Ideas

- **Self-publishing** — `aiki plugin publish` for authors to register repos directly
- **Authentication** — GitHub device flow for self-publishing, namespace verification
- **Web UI** — Browse/search interface, plugin detail pages
- **Download metrics** — Install counting, sort by popularity
- **Featured plugins** — Editorially curated highlights
- **Private registries** — Org-scoped registries with access control
- **Webhook sync** — Auto-refresh when repos are pushed to (requires GitHub App)
- **Dependency declaration** — Plugins depending on other plugins
- **Ratings/reviews** — User feedback on plugins

