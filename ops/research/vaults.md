# Vault Integration Research: OSS Secret Manager Patterns

Research into how popular open-source tools integrate with secret managers (HashiCorp Vault, AWS Secrets Manager, etc.) to inform aiki's vault integration design.

---

## 1. Kubernetes External Secrets Operator (ESO)

The closest architectural match to aiki's vault design. ESO uses a **two-resource pattern**:

- **`SecretStore`** — defines *how* to connect to a provider (Vault, AWS SM, GCP, Azure KV). Cluster-wide or namespace-scoped. Contains auth config but not secrets.
- **`ExternalSecret`** — defines *what* to fetch. Maps external secret paths to local secret names. Declarative, committed to git.

The controller watches `ExternalSecret` resources, resolves them against the `SecretStore`, and creates native K8s `Secret` objects. Periodic re-sync keeps values fresh.

**Key takeaways:**
- Separating "provider config" from "secret bindings" is a proven pattern — maps cleanly to `~/.aiki/secrets.yaml` (provider config) + `.aiki/secrets.yaml` (bindings)
- Multi-tenancy via scoped stores — analogous to per-agent secret access control
- ESO supports 20+ providers — starting with 2-3 and growing is fine

**Sources:**
- https://external-secrets.io/latest/introduction/overview/
- https://github.com/external-secrets/external-secrets

---

## 2. Terraform Provider Pattern

Terraform's approach is **data sources** that fetch secrets at plan/apply time:

```hcl
data "vault_generic_secret" "db" {
  path = "secret/data/myapp/db"
}

resource "aws_instance" "app" {
  user_data = data.vault_generic_secret.db.data["password"]
}
```

Each provider (`vault`, `aws`, `google`) has its own authentication config in `provider {}` blocks, and secrets are referenced as data source attributes.

**Key takeaways:**
- **Separate auth from reference** — provider block handles auth, data source handles path
- **Dynamic secrets** — Vault can generate ephemeral credentials (DB creds, AWS STS tokens) that auto-expire
- **State file risk** — Terraform writes resolved secrets to state files (a known weakness). Aiki must avoid this by keeping secrets only in memory.

**Sources:**
- https://developer.hashicorp.com/terraform/tutorials/secrets/secrets-vault
- https://blog.gruntwork.io/a-comprehensive-guide-to-managing-secrets-in-your-terraform-code-1d586955ace1

---

## 3. 1Password CLI (`op run` / `op read` / `op inject`)

1Password's developer tooling is the best example of **secret reference syntax** done well:

- **`op://vault/item/field`** — URI-style references that are readable and greppable
- **`op run`** — scans env vars for `op://` references, resolves them, passes to subprocess as env vars
- **`op inject`** — replaces `op://` references in template files, outputs resolved files
- **`op read`** — resolves a single reference to stdout

**Key takeaways:**
- The URI reference pattern (`op://vault/item/field`) is proven and developer-friendly — validates our `vault://`, `aws://`, `op://` scheme design
- `op run` as a subprocess wrapper is exactly what `AIKI_SECRET_*` env var injection does
- `op inject` for config file templating — aiki gets this for free via `{{secrets.name}}` in hook YAML
- Shell plugins that integrate `op` with tools like `gh`, `aws`, `docker` — shows how plugins can declare secret requirements

**Sources:**
- https://developer.1password.com/docs/cli/secret-references/
- https://developer.1password.com/docs/cli/secrets-scripts/

---

## 4. Doppler

Doppler's design centers on **env var injection as the universal interface**:

- **`doppler run -- command`** — fetches all project secrets, injects as env vars, runs command
- **Projects + Configs + Environments** — hierarchical organization (project > environment > config)
- **`--watch` flag** — live-reloads secrets when they change (useful for long-running processes)
- SDKs are secondary; env vars are the primary injection mechanism (language-agnostic)

**Key takeaways:**
- Env vars as the universal injection mechanism is the right default — `AIKI_SECRET_*` pattern aligns
- Environment-based overrides (dev/staging/prod) are important — validates the `env:` key in `secrets.yaml`
- Doppler's caching with scheduled refreshes maps to per-session cache design

**Sources:**
- https://medium.com/4th-coffee/doppler-a-brief-introduction-to-secrets-managers-e779b48fac1b
- https://thenewstack.io/secrets-management-doppler-or-hashicorp-vault/

---

## 5. Infisical

Open-source (MIT) platform with the richest agent/SDK story:

- **Infisical Agent** — a sidecar process that injects secrets without modifying app code (similar to Vault Agent)
- **SDKs** (Node, Python, Go, Ruby, Java, .NET) — fetch secrets at runtime, with built-in caching
- **`infisical run`** — like `doppler run`, injects secrets as env vars
- **K8s Operator** — syncs secrets into K8s every 60s, auto-reloads deployments on change
- **.NET SDK has `SetSecretsAsEnvironmentVariables`** — one call to dump all secrets into env

**Key takeaways:**
- The Agent pattern (background process managing secrets) could be a future aiki feature for long-running sessions
- SDK-level caching with periodic refresh — worth considering TTL-based cache invalidation (Phase 5)
- `SetSecretsAsEnvironmentVariables` is exactly what shell action injection does

**Sources:**
- https://github.com/Infisical/infisical
- https://infisical.com/blog/open-source-secrets-management-devops

---

## 6. Mozilla SOPS

Different approach — **encrypted-at-rest files** rather than runtime resolution:

- Encrypts *values* but keeps *keys* in cleartext (YAML/JSON tree walking)
- Each value gets unique AES256_GCM IV; single data key per file
- Master keys from AWS KMS, GCP KMS, Azure KV, age, PGP
- MAC integrity protection prevents secret addition/removal
- Git-friendly — diffs show which keys changed, even though values are encrypted

**Key takeaways:**
- SOPS solves a different problem (secrets in git) but shows the value of keeping keys/refs in cleartext
- `secrets.yaml` (refs only, no values) is spiritually similar but avoids encryption complexity entirely
- SOPS `.sops.yaml` path-based rules for applying different KMS keys — maps to per-environment binding overrides

**Sources:**
- https://github.com/getsops/sops
- https://blog.gitguardian.com/a-comprehensive-guide-to-sops/

---

## 7. Ansible Lookup Plugins

Ansible's **`LookupBase` pattern** is the closest to our `SecretProvider` trait design:

- Each provider implements a `run()` method (= our `fn resolve()`)
- Shared auth utilities across all plugins — auth is factored out and reused
- Template syntax: `{{ lookup('community.hashi_vault.vault_kv2_get', 'secret/data/myapp') }}`
- Plugin discovery by directory convention

**Anti-patterns identified by the Ansible team:**
- Putting all params in a single string ("term string") was deprecated — validates our URI-with-fragment approach over cramming everything into one string
- Monolithic plugins that do everything → moving to tightly-scoped individual plugins
- The `community.hashi_vault` collection evolved shared auth utilities so all plugins and modules share auth logic consistently — validates factoring auth out of individual providers

**Sources:**
- https://docs.ansible.com/projects/ansible/latest/collections/community/hashi_vault/hashi_vault_lookup.html
- https://docs.ansible.com/projects/ansible/latest/plugins/lookup.html

---

## 8. Docker Compose Secrets

Docker's model is **file-mount based** rather than env-var based:

- Secrets mounted at `/run/secrets/<name>` as files (tmpfs, in-memory only)
- Applications read from files, not env vars
- `*_FILE` convention — e.g., `MYSQL_ROOT_PASSWORD_FILE=/run/secrets/db_password`
- Secrets encrypted at rest in Swarm Raft log, decrypted only in container memory

**Key takeaways:**
- File-mount pattern is an alternative to env vars — some tools prefer reading secrets from files
- Could support a `file_path` injection mode alongside env vars for tools that expect file-based secrets
- Docker's explicit "only containers that declare the secret can access it" — validates per-agent scoping in Phase 5

**Sources:**
- https://spacelift.io/blog/docker-secrets
- https://docs.docker.com/engine/swarm/secrets/

---

## Cross-Cutting Patterns Summary

| Pattern | Used By | Aiki Mapping |
|---------|---------|--------------|
| URI-style secret references | 1Password, ESO | `vault://`, `aws://`, `env://` refs in `secrets.yaml` |
| Provider trait / interface | Ansible, ESO, Terraform | `SecretProvider` trait with `resolve()` |
| Env var injection via subprocess | Doppler, 1Password, Infisical | `AIKI_SECRET_*` vars for shell actions |
| Template interpolation | Ansible, 1Password (`op inject`) | `{{secrets.name}}` in hook YAML |
| Separate auth config from bindings | ESO (SecretStore/ExternalSecret), Terraform | `~/.aiki/secrets.yaml` vs `.aiki/secrets.yaml` |
| Per-environment overrides | Doppler, SOPS, 1Password | `env:` key in binding declarations |
| Session-scoped caching | Infisical SDK, Doppler | In-memory cache cleared on session end |
| Audit logging of access | Vault, Infisical, Ansible | `secret.accessed` events via OTel |
| Shared auth utilities | Ansible `community.hashi_vault` | Common auth handling in provider implementations |
| File-mount injection | Docker Compose | Potential `file_path` injection mode |
| Subprocess wrapper (`run`) | 1Password, Doppler, Infisical | Potential `aiki vault run -- command` |
| Sidecar/agent for refresh | Infisical, Vault Agent | Future long-running session secret refresh |

---

## New Design Ideas from Research

### 1. File-mount injection mode (from Docker)
Some tools expect secrets as files, not env vars. Add a `file_path` injection option:
```yaml
secrets:
  service_account:
    ref: vault://secret/data/myapp/gcp-sa
    inject: file  # writes to a temp file, sets AIKI_SECRET_FILE_SERVICE_ACCOUNT=/tmp/aiki-secret-xxxxx
```

### 2. `aiki vault run -- command` (from Doppler / 1Password)
A subprocess wrapper for ad-hoc use outside of hooks:
```bash
aiki vault run -- aws s3 ls
# Resolves all bindings, injects as AIKI_SECRET_* env vars, runs the command
```

### 3. Secret refresh for long sessions (from Infisical Agent / Vault Agent)
For sessions lasting hours, secrets may expire or rotate. A future sidecar pattern could:
- Watch for TTL expiry on dynamic secrets
- Re-resolve and update env vars in-process
- Emit `secret.rotated` events

### 4. Shared auth module (from Ansible)
Factor auth handling out of individual providers into a shared module:
- Token-based auth (all providers)
- IAM role auth (AWS, GCP)
- AppRole auth (Vault)
- OIDC auth (Vault, cloud providers)
This prevents each provider from re-implementing auth and ensures consistent behavior.

### 5. Provider availability as health checks (from Doppler / ESO)
`aiki vault status` should not just check if a CLI exists, but actually verify connectivity:
- Vault: check `vault status` (sealed/unsealed)
- AWS: check `aws sts get-caller-identity`
- GCP: check `gcloud auth print-identity-token`
- 1Password: check `op whoami`
