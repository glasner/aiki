# Plugin Search Command

**Status**: Next
**Priority**: P2
**Depends On**: [Remote Plugins](../now/remote-plugins.md), [Plugin Registry](../now/registry.md)

---

## Problem

`aiki plugin install owner/repo` requires users to already know the exact plugin reference. There's no way to discover plugins from the CLI — users must learn about them through word of mouth, READMEs, or external links.

---

## Summary

Add `aiki plugin search` and `aiki plugin info` CLI commands that query the plugin registry to help users discover plugins. Plugins remain hosted on GitHub — the registry is a read-only discovery index.

---

## Commands

### `aiki plugin search <query>`

Search plugins by keyword or category.

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
```

### `aiki plugin info <reference>`

Show detailed plugin metadata.

```bash
aiki plugin info acme/security
#  acme/security
#  Security-focused code review templates
#
#  Author:     acme
#  Repository: github.com/acme/security
#  Categories: security, review
#  Templates:  audit, vulnerability-scan, dependency-check
#  Hooks:      turn.completed (security scan)
#
#  Install: aiki plugin install acme/security
```

---

## Implementation Notes

### Registry API

The CLI calls the registry's public read-only endpoints:

```
GET /plugins?q={query}&category={category}&sort={sort}&limit={limit}&offset={offset}
GET /plugins/{namespace}/{name}
```

The registry URL is hardcoded in the CLI. No user configuration needed.

### Dependencies

Will need an HTTP client (e.g. `reqwest` with blocking or `ureq`) added to `cli/Cargo.toml`.

### Output

Results go to stdout as human-readable text. Follow existing CLI output conventions (stderr for status messages, stdout for data).

---

## Open Questions

1. **Offline fallback** — Should the CLI cache search results locally? Or just fail gracefully when the registry is unreachable?
2. **Registry URL** — What's the actual registry endpoint? Blocked on registry service deployment.
3. **Pagination** — Should `search` support `--limit` / `--offset`, or just return a reasonable default (e.g. 20 results)?
