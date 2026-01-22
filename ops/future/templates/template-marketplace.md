# Template Marketplace

**Date**: 2026-01-21
**Status**: Future Enhancement
**Related**: [Task Templates](../../now/task-templates.md)

---

## Overview

Allow users to install and share templates from external sources (GitHub, package registries, etc.).

## Syntax

```bash
# Install community templates
aiki task template install github:org/repo/templates/advanced-review.md

# Install with custom namespace
aiki task template install github:org/repo/templates/advanced-review.md --as myorg/advanced-review

# Install all templates from a repository
aiki task template install github:org/aiki-templates

# Update templates
aiki task template update

# Update specific template
aiki task template update myorg/advanced-review

# List installed templates with sources
aiki task template list --show-source
```

## Template Sources

### GitHub

```bash
# Single file
aiki task template install github:acme/templates/security.md

# Directory
aiki task template install github:acme/templates/reviews/

# Specific commit/tag
aiki task template install github:acme/templates@v1.2.3
```

### HTTP/HTTPS

```bash
# Direct URL
aiki task template install https://example.com/templates/security.md
```

### Local File System

```bash
# Copy from local path
aiki task template install file:///home/user/my-templates/review.md --as myorg/review
```

## Template Registry

Optional central registry for discovering templates:

```bash
# Search registry
aiki task template search security

# Output:
# aiki/security (built-in) - Comprehensive security review
# acme/security-advanced - Extended security checks
# company/pci-compliance - PCI-DSS compliance review

# Install from registry
aiki task template install acme/security-advanced
```

## Security Considerations

- **Verification**: Verify checksums/signatures for downloaded templates
- **Sandboxing**: Templates can't execute arbitrary code (just markdown + YAML)
- **Review before install**: Show diff before installing/updating
- **Source pinning**: Lock to specific versions/commits

## Storage

```
.aiki/
├── templates/
│   ├── aiki/              # Built-in templates
│   ├── myorg/             # User templates
│   └── community/         # Installed from marketplace
└── template-lock.json     # Track installed template sources and versions
```

## Benefits

- **Community sharing**: Share templates across projects and teams
- **Version control**: Track template versions and updates
- **Discoverability**: Find templates for common workflows
- **Best practices**: Learn from community templates
