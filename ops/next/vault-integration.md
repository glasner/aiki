# Vault Integration

Make credentials from external secret stores available to agents and plugins.

**Status:** Planning
**Phase:** TBD (new capability, complements plugin ecosystem)

---

## Problem

Agents and plugins frequently need credentials to do useful work — API keys, database passwords, cloud tokens, service accounts. Today there is no standard way to get secrets into agent sessions or hook flows:

- Developers manually export env vars before launching agents
- Secrets end up hardcoded in hook YAML or shell scripts
- Each team builds ad-hoc wrappers around `vault` / `aws secretsmanager` CLI tools
- No audit trail of which agent accessed which secret
- No way for a plugin to declare "I need credential X" and have it resolved automatically

This blocks several important use cases:
- **Plugins that call external APIs** (Jira, Linear, Slack, GitHub Enterprise, Datadog)
- **Review plugins that need service credentials** (SonarQube, Snyk, custom linters behind auth)
- **Deployment/CI plugins** that need cloud provider creds
- **Multi-agent teams** where each agent needs scoped credentials

---

## Goals

1. **Provider-agnostic abstraction** — support HashiCorp Vault, AWS Secrets Manager, GCP Secret Manager, Azure Key Vault, 1Password CLI, and plain environment variables through a single interface
2. **Plugin-declared dependencies** — plugins declare what secrets they need; aiki resolves them at runtime
3. **Agent-scoped access** — secrets are injected into agent sessions with least-privilege scoping
4. **Audit trail** — log which secrets were accessed, by which agent, in which session
5. **Zero plaintext in config** — secret values never appear in hook YAML, task descriptions, or JJ history

---

## Design

### Concept: Secret References

A **secret reference** is a URI-style identifier that names a secret without revealing its value:

```
vault://secret/data/myapp/api-key#field=token
aws://us-east-1/myapp/database#key=password
env://ANTHROPIC_API_KEY
op://Development/API Keys/Slack Bot Token
file:///home/user/.secrets/github-token
```

Secret references appear in:
- Plugin `hooks.yaml` (`secrets:` block)
- Project `.aiki/secrets.yaml` (binding declarations)
- `aiki vault` CLI commands (manual resolution)

Secret **values** never appear in any tracked file.

### Architecture

```
┌──────────────────────────────────────────────┐
│                Agent Session                  │
│                                               │
│  hook YAML uses {{secrets.my_api_key}}        │
│  shell actions get $AIKI_SECRET_MY_API_KEY    │
│  context actions can reference resolved vals  │
└──────────────┬───────────────────────────────┘
               │ resolve at session.started
               ▼
┌──────────────────────────────────────────────┐
│            Secret Resolver                    │
│                                               │
│  1. Read .aiki/secrets.yaml (binding map)     │
│  2. Match secret refs to providers            │
│  3. Call provider to fetch value              │
│  4. Cache for session lifetime                │
│  5. Inject into VariableResolver as           │
│     secrets.* namespace                       │
│  6. Set AIKI_SECRET_* env vars for shells     │
│  7. Log access event (no values)              │
└──────────────┬───────────────────────────────┘
               │
    ┌──────────┼──────────┬──────────┐
    ▼          ▼          ▼          ▼
┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
│  Env   │ │ Vault  │ │  AWS   │ │  1PW   │
│Provider│ │Provider│ │Provider│ │Provider│
└────────┘ └────────┘ └────────┘ └────────┘
```

### Provider Trait

```rust
/// A backend that can resolve secret references to values.
pub trait SecretProvider: Send + Sync {
    /// Provider scheme (e.g., "vault", "aws", "env", "op", "file")
    fn scheme(&self) -> &str;

    /// Resolve a secret reference to its plaintext value.
    /// Called once per session per secret (results are cached).
    fn resolve(&self, reference: &SecretRef) -> Result<SecretValue>;

    /// Check if the provider is available/configured.
    /// Called at startup to give clear errors early.
    fn is_available(&self) -> Result<bool>;
}
```

Providers are stateless and composable. Each one knows how to talk to its backend:

| Provider | Scheme | How It Works |
|----------|--------|-------------|
| `EnvProvider` | `env://` | Reads `std::env::var(name)` |
| `VaultProvider` | `vault://` | Calls `vault kv get` CLI or HTTP API. Requires `VAULT_ADDR` + `VAULT_TOKEN` (or other auth method) |
| `AwsProvider` | `aws://` | Calls `aws secretsmanager get-secret-value` CLI or SDK. Uses default credential chain |
| `GcpProvider` | `gcp://` | Calls `gcloud secrets versions access` or API |
| `AzureProvider` | `azure://` | Calls `az keyvault secret show` or API |
| `OnePasswordProvider` | `op://` | Calls `op read` CLI (1Password) |
| `FileProvider` | `file://` | Reads a local file (e.g., service account JSON). File permissions validated |

### Secret Bindings (`.aiki/secrets.yaml`)

Project-level file that maps logical secret names to provider-specific references:

```yaml
# .aiki/secrets.yaml
# Maps logical names → provider references
# This file is committed. It contains NO secret values.

secrets:
  # Simple env var
  github_token:
    ref: env://GITHUB_TOKEN

  # HashiCorp Vault
  database_password:
    ref: vault://secret/data/myapp/db#field=password

  # AWS Secrets Manager
  stripe_key:
    ref: aws://us-east-1/prod/stripe#key=api_key

  # 1Password
  slack_bot_token:
    ref: op://Development/Slack Bot/credential

  # Different bindings per environment
  api_key:
    ref: vault://secret/data/myapp/api-key
    env:
      staging: vault://secret/data/staging/api-key
      production: vault://secret/data/prod/api-key
```

This file is safe to commit — it contains only references, never values.

### Plugin Secret Declarations

Plugins declare what secrets they need in their `hooks.yaml`:

```yaml
# ~/.aiki/plugins/acme/jira-sync/hooks.yaml
name: acme/jira-sync
description: Sync aiki tasks with Jira

secrets:
  jira_api_token:
    description: "Jira API token for issue sync"
    required: true
  jira_base_url:
    description: "Jira instance URL"
    required: true

on:
  task.closed:
    - shell:
        command: |
          curl -X POST {{secrets.jira_base_url}}/rest/api/3/issue \
            -H "Authorization: Bearer {{secrets.jira_api_token}}" \
            -d '{"summary": "{{event.task.name}}"}'
```

When the plugin is installed, `aiki plugin install acme/jira-sync` checks if all required secrets have bindings in `.aiki/secrets.yaml`. If not, it prompts:

```
Plugin acme/jira-sync requires 2 secrets:
  jira_api_token — Jira API token for issue sync
  jira_base_url  — Jira instance URL

Add bindings to .aiki/secrets.yaml:
  aiki vault bind jira_api_token vault://secret/data/jira/token
  aiki vault bind jira_base_url  env://JIRA_BASE_URL
```

### Injection Into Hook Engine

The `VariableResolver` gains a new `secrets` namespace alongside the existing `event.*` vars:

```
{{secrets.github_token}}       → resolved value
{{secrets.database_password}}  → resolved value
```

Resolution is **lazy** — secrets are fetched only when first accessed, not eagerly at session start. This avoids unnecessary vault calls for secrets that aren't used in a given flow path.

For shell actions, secrets are injected as `AIKI_SECRET_*` environment variables:

```yaml
on:
  turn.started:
    - shell:
        command: my-tool --token $AIKI_SECRET_GITHUB_TOKEN
```

The `AIKI_SECRET_` prefix:
- Makes it obvious these come from aiki's vault integration
- Avoids collisions with existing env vars
- Makes grep-ability easy for security audits

### Secret Caching

Secrets are cached **per session** in memory. Never written to disk. Cache lifetime:

| Scope | Lifetime | Rationale |
|-------|----------|-----------|
| Interactive session | Until session ends | Avoid repeated vault calls across turns |
| Background session | Until task completes | Same agent, same credentials |
| `aiki task run` (spawned) | Fresh resolution | Different process, different credential scope |

Cache is a `HashMap<String, SecretValue>` held in the `HookEngine` or `AikiState`. Cleared on process exit (no persistence).

### Audit Logging

Every secret access generates an event:

```
event: secret.accessed
data:
  secret_name: github_token
  provider: vault
  reference: vault://secret/data/myapp/token  (ref, not value)
  session_id: <uuid>
  agent: claude-code
  task_id: <task-id>
  timestamp: <rfc3339>
```

These events flow through the existing event system and can be observed via OTel export (for teams using centralized logging). **Secret values are never logged.**

### User-Level Overrides (`~/.aiki/secrets.yaml`)

Users can provide personal secret bindings that supplement or override project bindings:

```yaml
# ~/.aiki/secrets.yaml (user-level, NOT committed)
secrets:
  github_token:
    ref: op://Personal/GitHub Token/credential
```

Resolution priority:
1. User-level `~/.aiki/secrets.yaml` (personal overrides)
2. Project-level `.aiki/secrets.yaml` (team defaults)
3. Plugin defaults (if any)

This mirrors the hook resolution pattern (user overrides project).

---

## CLI Commands

### `aiki vault`

```bash
# Check which providers are available
aiki vault status
# Output:
#   env     ✓ available (always)
#   vault   ✓ available (VAULT_ADDR=https://vault.example.com)
#   aws     ✓ available (region=us-east-1, profile=default)
#   gcp     ✗ not configured (gcloud not found)
#   op      ✓ available (1Password CLI v2.30)
#   azure   ✗ not configured

# Bind a secret name to a provider reference
aiki vault bind github_token env://GITHUB_TOKEN
aiki vault bind db_password vault://secret/data/myapp/db#field=password

# Unbind
aiki vault unbind db_password

# List bindings for current project
aiki vault list
# Output:
#   github_token      env://GITHUB_TOKEN                           (project)
#   db_password       vault://secret/data/myapp/db#field=password  (project)
#   slack_token       op://Dev/Slack/credential                    (user)

# Test that a secret can be resolved (shows success/failure, never the value)
aiki vault test github_token
# Output:
#   github_token  ✓ resolved (env://GITHUB_TOKEN, 40 chars)

# Test all bindings
aiki vault test --all

# Check what secrets a plugin requires
aiki vault check acme/jira-sync
# Output:
#   jira_api_token  ✗ not bound (required)
#   jira_base_url   ✗ not bound (required)

# Resolve a secret reference ad-hoc (prints value — use with care)
aiki vault get vault://secret/data/myapp/token
```

### `aiki doctor` Integration

`aiki doctor` already checks plugin health. It should also check secret bindings:

```
Secrets:
  ✓ github_token — bound, resolvable
  ✗ jira_api_token — required by acme/jira-sync, not bound
  ⚠ db_password — bound, but vault unreachable
```

---

## Implementation Plan

### Phase 1: Core Abstractions (Foundation)

- Define `SecretRef` parser (URI parsing with scheme, path, fragment params)
- Define `SecretProvider` trait
- Implement `EnvProvider` (simplest, always available)
- Implement `FileProvider` (read from local files)
- Define `.aiki/secrets.yaml` schema and parser
- Add `secrets` namespace to `VariableResolver`
- Lazy resolution in `create_resolver()`
- Unit tests for ref parsing, env provider, variable resolution

### Phase 2: HashiCorp Vault + AWS

- Implement `VaultProvider` (CLI-based: `vault kv get`)
  - Auth methods: token (`VAULT_TOKEN`), AppRole, Kubernetes
  - KV v1 and v2 support
  - Field extraction from JSON response
- Implement `AwsProvider` (CLI-based: `aws secretsmanager get-secret-value`)
  - Uses default credential chain (env vars, ~/.aws/credentials, IAM role)
  - Region from ref URI or `AWS_DEFAULT_REGION`
- `aiki vault status` command
- `aiki vault test` command
- Integration tests (mock providers)

### Phase 3: Plugin Declaration + Binding UX

- Add `secrets:` block parsing to hook loader
- `aiki vault bind` / `aiki vault unbind` commands
- `aiki vault list` command
- `aiki vault check <plugin>` command
- `aiki plugin install` warns about unbound secrets
- `aiki doctor` checks secret bindings
- `AIKI_SECRET_*` env var injection for shell actions
- Session-scoped in-memory cache

### Phase 4: Audit + Additional Providers

- `secret.accessed` event emission
- OTel export of secret access events
- Implement `OnePasswordProvider` (`op://` scheme)
- Implement `GcpProvider` (`gcp://` scheme)
- Implement `AzureProvider` (`azure://` scheme)
- Per-environment binding overrides (`env:` key in secrets.yaml)
- User-level `~/.aiki/secrets.yaml` support

### Phase 5: Advanced Features (Future)

- Secret rotation awareness (TTL-based cache invalidation)
- Dynamic secrets (Vault dynamic database creds, AWS STS)
- Per-agent secret scoping (agent X can access secrets A,B but not C)
- Secret injection into `aiki task run` spawned agents
- Plugin secret composition (plugin A provides secrets for plugin B)

---

## Security Considerations

### What Must Never Happen

1. **Secret values in JJ/Git history** — values never written to tracked files
2. **Secret values in task descriptions** — refs only, never values
3. **Secret values in logs** — audit events log ref + metadata, never the value
4. **Secret values in hook YAML** — always use `{{secrets.name}}`, never inline
5. **Secrets leaking to untrusted plugins** — future: per-plugin ACLs on which secrets it can access

### Defense in Depth

- `SecretValue` is a newtype with no `Display`/`Debug` impl (prevents accidental logging)
- `secrets.yaml` has a `.gitignore`-style validation: if it contains anything that looks like a raw secret (high entropy strings, `Bearer `, `sk-`), `aiki doctor` warns
- Shell commands that receive `AIKI_SECRET_*` env vars have those vars scrubbed from `PostToolUse` hook payloads
- `aiki vault get` (the only command that reveals values) requires `--yes-i-know` flag when stdout is a terminal

### Threat Model

| Threat | Mitigation |
|--------|-----------|
| Secrets in git history | Values never written to tracked files; `secrets.yaml` contains only refs |
| Agent leaks secret in output | Out of scope for vault integration — handled by existing content filters |
| Malicious plugin reads secrets | Phase 5: per-plugin ACL on secret names |
| Vault token compromise | Use short-lived tokens (AppRole, K8s auth); token management is user's responsibility |
| Secret in process memory | Cleared on session end; no disk persistence. Standard for in-process secret handling |
| Provider CLI not installed | `aiki vault status` checks availability; clear error on resolution failure |

---

## Open Questions

1. **Should `secrets.yaml` be committed?** It contains only refs, not values. Committing enables team consistency. But some teams may consider even the ref structure sensitive. Recommendation: commit by default, document how to `.gitignore` it.

2. **Native SDK vs CLI for providers?** CLI-based providers (`vault kv get`, `aws secretsmanager`) are simpler and inherit the user's auth context. Native SDK (via Rust crates) would avoid CLI dependency and be faster. Recommendation: start with CLI, add SDK option later if performance matters.

3. **Should secrets be available in `context:` actions?** If a hook injects `{{secrets.api_key}}` into the agent's prompt via `context:`, the secret appears in the LLM context window. This may be necessary (agent needs the key to make API calls) but increases exposure. Recommendation: allow it, but warn in docs.

4. **How does this interact with MCP servers?** MCP servers that need credentials (e.g., a database MCP server) could benefit from vault integration. The MCP server config could reference `aiki vault get` for credential resolution. This is a future extension point.

5. **Per-environment bindings — how to select?** The `env:` key in `secrets.yaml` allows different refs per environment, but how does aiki know which environment is active? Options: `AIKI_ENV` env var, `.aiki/config.toml` setting, CLI flag. Recommendation: `AIKI_ENV` env var (simplest, most flexible).

---

## Relationship to Existing Systems

| System | Relationship |
|--------|-------------|
| **Plugin system** | Plugins declare `secrets:` requirements. Vault resolves them |
| **Hook engine** | `{{secrets.*}}` vars injected via `VariableResolver`. `AIKI_SECRET_*` env vars for shell actions |
| **Agent sessions** | Secrets resolved at `session.started`, cached for session lifetime |
| **Task system** | `secret.accessed` events linked to task IDs for audit |
| **Skill injection** | Skills could declare secret requirements similar to plugins |
| **OTel export** | Secret access audit events exported alongside existing telemetry |
| **`aiki doctor`** | Checks provider availability and binding completeness |

---

## Research Takeaways

Full research at `ops/research/vaults.md`. Eight OSS tools were analyzed: Kubernetes ESO, Terraform, 1Password CLI, Doppler, Infisical, Mozilla SOPS, Ansible Lookup Plugins, and Docker Compose Secrets.

### What the research validates

Our existing design already aligns with the strongest patterns across the ecosystem:

| Design Decision | Validated By |
|----------------|-------------|
| URI-style secret references (`vault://`, `aws://`, `env://`) | 1Password (`op://vault/item/field`), ESO |
| Provider trait with `resolve()` | Ansible `LookupBase.run()`, ESO SecretStore, Terraform data sources |
| `AIKI_SECRET_*` env var injection for shell actions | Doppler `run`, 1Password `op run`, Infisical `run` |
| `{{secrets.name}}` template interpolation | Ansible `{{ lookup(...) }}`, 1Password `op inject`, GitHub Actions `${{ secrets.* }}` |
| Separate auth config from bindings | ESO (SecretStore vs ExternalSecret), Terraform (provider block vs data source) |
| Per-environment overrides | Doppler (project > env > config), SOPS (path-based KMS rules) |
| Session-scoped in-memory cache | Infisical SDK, Doppler |
| Audit logging without values | HashiCorp Vault, Infisical |

### What we should add to the design

**1. File-mount injection mode** (from Docker Compose)

Some tools expect secrets as files, not env vars (e.g., GCP service account JSON, TLS certs). Add an `inject: file` option:

```yaml
secrets:
  service_account:
    ref: vault://secret/data/myapp/gcp-sa
    inject: file  # writes to tmpfile, sets AIKI_SECRET_FILE_SERVICE_ACCOUNT=/tmp/aiki-secret-xxxxx
```

Tmpfile is created with `0600` permissions, deleted on session end.

**2. `aiki vault run -- command`** (from Doppler / 1Password / Infisical)

A subprocess wrapper for ad-hoc use outside of hooks:

```bash
# Resolves all bindings, injects as AIKI_SECRET_* env vars, runs the command
aiki vault run -- aws s3 ls
aiki vault run -- ./deploy.sh
```

Every major tool in this space provides a `run` command. It's the simplest onramp — works without configuring hooks.

**3. Shared auth module** (from Ansible `community.hashi_vault`)

The Ansible team learned the hard way that each provider re-implementing auth leads to inconsistency. Factor auth into a shared module:

- Token-based auth (universal)
- IAM role / instance profile (AWS, GCP)
- AppRole (Vault)
- OIDC (Vault, cloud providers)

Providers call `auth.resolve_token(config)` rather than implementing their own auth flows.

**4. Deep provider health checks** (from Doppler / ESO)

`aiki vault status` should go beyond "CLI exists" to verify actual connectivity:

```
aiki vault status
  env     ✓ available (always)
  vault   ✓ connected (https://vault.example.com, unsealed)
  aws     ✓ authenticated (arn:aws:iam::123456:user/dev, us-east-1)
  gcp     ✗ auth expired (run: gcloud auth login)
  op      ✓ signed in (user@example.com)
```

### What we should NOT do (anti-patterns from research)

1. **Don't write resolved secrets to disk** — Terraform's state file problem. Secrets stay in memory only.
2. **Don't cram all params into one string** — Ansible deprecated this ("term string" anti-pattern). Our URI-with-fragment approach is better.
3. **Don't build monolithic providers** — Ansible moved from one big `hashi_vault` plugin to tightly-scoped individual plugins. Keep providers small and focused.
4. **Don't require encryption for the binding file** — SOPS shows encryption adds complexity. Since `secrets.yaml` contains only refs (never values), encryption is unnecessary.

### Updated implementation phases

Based on research, two additions to the phasing:

- **Phase 2** should include `aiki vault run -- command` (it's the simplest user-facing feature and every comparable tool has it)
- **Phase 3** should include `inject: file` support (needed for GCP service accounts, TLS certs, and other file-based secrets)

---

## Comparable Systems

| System | How It Works | Aiki Analog |
|--------|-------------|-------------|
| **K8s ESO** | SecretStore + ExternalSecret CRDs → syncs to K8s Secrets | `~/.aiki/secrets.yaml` (provider config) + `.aiki/secrets.yaml` (bindings) |
| **Terraform** | `provider {}` auth config + `data` sources fetch secrets at plan time | Provider trait auth + `SecretRef` resolution |
| **1Password CLI** | `op://` URI refs, `op run` subprocess wrapper, `op inject` templating | `vault://` refs, `aiki vault run`, `{{secrets.*}}` |
| **Doppler** | `doppler run -- cmd`, project/env hierarchy, env var injection | `aiki vault run`, `env:` overrides, `AIKI_SECRET_*` |
| **Infisical** | `infisical run`, Agent sidecar, SDK caching with TTL refresh | `aiki vault run`, future agent refresh, session cache |
| **SOPS** | Encrypts values in-place, keys in cleartext, git-friendly diffs | `secrets.yaml` refs in cleartext (simpler — no encryption needed) |
| **Ansible** | `LookupBase.run()` provider pattern, shared auth, template syntax | `SecretProvider.resolve()`, shared auth module, `{{secrets.*}}` |
| **Docker Compose** | File-mount at `/run/secrets/`, `*_FILE` convention, per-container ACLs | `inject: file` mode, per-agent scoping (Phase 5) |
| **GitHub Actions** | `${{ secrets.MY_SECRET }}` in workflow YAML, org/repo scoping | `{{secrets.name}}` in hook YAML, project/user scoping |
