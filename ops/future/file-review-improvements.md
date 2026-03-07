# File Review Improvements

**Date**: 2026-02-04
**Status**: Future Ideas
**Purpose**: Potential enhancements for file-based reviews

**Related Documents**:
- [Spec-Aware Review](../now/spec-aware-review.md) - Base file review system
- [Template Conditionals](../now/template-conditionals.md) - Conditional template logic

---

## Overview

This document captures potential future enhancements for file-based reviews in Aiki. These ideas extend the spec-aware review system to handle more file types and provide smarter detection.

---

## File Type Detection

**Problem:** Currently, file reviews treat all files the same. Different file types (specs, documentation, readmes) have different review criteria.

**Solution:** Detect document type from content/path and adapt review accordingly.

### Path-Based Detection

```rust
fn detect_file_type(path: &Path) -> FileType {
    if path.starts_with("ops/now/") || path.starts_with("ops/future/") {
        FileType::Spec
    } else if path.starts_with("docs/") {
        FileType::Documentation
    } else if path.file_name() == Some("README.md") {
        FileType::Readme
    } else if path.extension() == Some("md") {
        FileType::Document
    } else {
        FileType::Unknown
    }
}
```

### Content-Based Detection

Could analyze file content for markers:

```rust
fn detect_from_content(content: &str) -> Option<FileType> {
    // Check frontmatter
    if let Some(frontmatter) = parse_frontmatter(content) {
        if frontmatter.contains("Status:") && frontmatter.contains("Purpose:") {
            return Some(FileType::Spec);
        }
    }
    
    // Check structure
    if content.contains("## Installation") && content.contains("## Usage") {
        return Some(FileType::Readme);
    }
    
    None
}
```

### Template Integration

Pass detected type to review template:

```bash
# Auto-detected
aiki review ops/now/feature.md
# → data.file_type = "spec"

# Auto-detected
aiki review README.md
# → data.file_type = "readme"
```

Template adapts based on type:

```markdown
{% if data.file_type == "spec" %}
**Spec review criteria:**
- Completeness - All sections filled
- Clarity - Unambiguous requirements
- Implementability - Can be decomposed into tasks
{% elif data.file_type == "readme" %}
**README review criteria:**
- Installation instructions present
- Usage examples clear
- Links working
{% else %}
**General document review:**
- Grammar and spelling
- Formatting consistency
- Content clarity
{% endif %}
```

---

## Non-Markdown File Reviews

**Problem:** Currently only markdown files can be reviewed. Other file types (code, config) could benefit from file-based review.

**Solution:** Support reviewing non-markdown files with appropriate criteria.

### Supported File Types

| Extension | Review Type | Focus Areas |
|-----------|-------------|-------------|
| `.rs`, `.ts`, `.py`, `.go` | Code review | Structure, patterns, potential issues (no git diff) |
| `.yaml`, `.json`, `.toml` | Config review | Validity, security, best practices |
| `.sh`, `.bash` | Script review | Safety, error handling, portability |
| `.sql` | Query review | Performance, injection risks |

### Code File Review (Without Diff)

Review code files directly without needing a git change:

```bash
# Review a specific file's code quality
aiki review src/auth.rs

# Template receives:
# - data.target_type = "file"
# - data.file_type = "code"
# - data.language = "rust"
# - data.path = "src/auth.rs"
```

Template adapts:

```markdown
{% if data.file_type == "code" %}
## Review {{data.path}}

Review the code for quality and potential issues:

**Focus areas for {{data.language}}:**
{% if data.language == "rust" %}
- Idiomatic Rust patterns
- Error handling (Result/Option usage)
- Unsafe code blocks
- Lifetime annotations clarity
{% elif data.language == "typescript" %}
- Type safety
- Async/await patterns
- Error handling
- Null safety
{% endif %}

**General code quality:**
- Function complexity
- Code clarity and readability
- Potential bugs or edge cases
- Security issues
{% endif %}
```

### Config File Review

```bash
# Review configuration safety
aiki review config/production.yaml

# Template receives:
# - data.target_type = "file"
# - data.file_type = "config"
# - data.format = "yaml"
```

Template checks:

```markdown
{% if data.file_type == "config" %}
## Review {{data.path}} configuration

**Security checks:**
- No hardcoded secrets or passwords
- Appropriate permissions/access controls
- No sensitive data exposure

**Validity checks:**
- {{data.format | upper}} syntax valid
- Required fields present
- Value types correct

**Best practices:**
- Environment-specific overrides clear
- Defaults appropriate
- Comments explain non-obvious settings
{% endif %}
```

---

## Smart Review Scope

**Problem:** When reviewing a file, might want to review related files too (e.g., implementation + tests).

**Solution:** Auto-detect and include related files in review.

### Related File Detection

```bash
# Review src/auth.rs and auto-include tests
aiki review src/auth.rs --with-related

# Finds and includes:
# - src/auth.rs (main file)
# - src/auth/tests.rs (test file)
# - docs/auth.md (documentation)
```

Template receives list of related files:

```markdown
## Review: {{data.primary_file}}

{% if data.related_files %}
**Related files included:**
{% for file in data.related_files %}
- `{{file.path}}` ({{file.type}})
{% endfor %}
{% endif %}
```

---

## Review History and Trends

**Problem:** No way to see review patterns over time (common issues, quality trends).

**Solution:** Track review metrics and provide insights.

### Metrics Tracking

```bash
# Show review statistics
aiki review stats

# Output:
Reviews completed: 47
Average issues per review: 3.2
Most common issue categories:
  1. Security (18 issues)
  2. Error handling (12 issues)
  3. Documentation (9 issues)

File types reviewed:
  - Code files: 32
  - Specs: 10
  - Documentation: 5
```

### Quality Trends

```bash
# Show quality trend for a file
aiki review history src/auth.rs

# Output:
Review history for src/auth.rs:

2026-01-15: 8 issues found (4 security, 2 quality, 2 performance)
2026-01-22: 3 issues found (1 security, 2 quality)
2026-02-04: 1 issue found (1 quality)

Trend: ✅ Improving (fewer issues over time)
```

---

## Batch File Reviews

**Problem:** Reviewing multiple related files requires multiple commands.

**Solution:** Support glob patterns and batch reviews.

### Glob Pattern Support

```bash
# Review all specs in ops/now/
aiki review 'ops/now/*.md'

# Review all Rust files in src/
aiki review 'src/**/*.rs'

# Template receives:
# - data.target_type = "batch"
# - data.files = ["ops/now/a.md", "ops/now/b.md", ...]
```

### Batch Review Template

```markdown
{% if data.target_type == "batch" %}
# Batch Review: {{data.files | length}} files

Review all files for consistency and quality.

# Subtasks

{% for file in data.files %}
## Review: {{file}}

Review `{{file}}` individually.
{% endfor %}

## Cross-file consistency

Check for consistency across all reviewed files:
- Naming conventions
- Style consistency
- Duplicate content
{% endif %}
```

---

## Interactive Review Mode

**Problem:** Reviews are fire-and-forget. No way to ask clarifying questions or iterate.

**Solution:** Interactive mode where agent can ask questions.

### Interactive Flow

```bash
# Start interactive review
aiki review --interactive ops/now/feature.md

# Agent reviews, then:
Agent: "I found a potential ambiguity in the API design section.
Should the 'limit' parameter default to 10 or 100?"

User: "Default to 100"

Agent: "Thanks! I'll note that in my review comment."
```

Could use flow system hooks to enable user prompts during review tasks.

---

## Review Templates per Directory

**Problem:** Different projects/directories might want different review criteria.

**Solution:** Allow per-directory review template overrides.

### Directory-Specific Templates

```
.aiki/
├── templates/
│   └── aiki/
│       └── review.md          # Default review template
└── review-templates/
    ├── src/security/review.md  # Security-focused for security/ dir
    ├── docs/review.md          # Documentation review for docs/
    └── ops/review.md           # Spec review for ops/
```

When reviewing a file, check for closest directory-specific template:

```bash
aiki review src/security/auth.rs
# Uses .aiki/review-templates/src/security/review.md if exists
# Falls back to .aiki/templates/review.md
```

---

## AI-Suggested Improvements

**Problem:** Reviews identify issues but don't suggest concrete fixes.

**Solution:** Agent suggests specific improvements with diffs.

### Suggested Fix Format

Review comments include suggested changes:

```markdown
**Issue**: Function is too complex (cyclomatic complexity: 12)

**Suggested fix**:
Extract validation logic into separate function:

\`\`\`rust
// Before
fn process_request(req: Request) -> Result<Response> {
    // 50 lines of mixed validation and processing
}

// After
fn process_request(req: Request) -> Result<Response> {
    validate_request(&req)?;
    execute_request(req)
}

fn validate_request(req: &Request) -> Result<()> {
    // Validation logic here
}
\`\`\`
```

Could even generate a task to apply the fix:

```bash
# Agent creates followup task with suggested diff
aiki task show <followup-task-id>
# Shows: "Apply suggested refactoring to process_request()"
```

---

## Summary

These future enhancements would make file reviews more powerful:

1. **Smart detection** - Automatic file type detection adapts review criteria
2. **More file types** - Code, config, scripts beyond just markdown
3. **Related files** - Include tests/docs automatically
4. **History tracking** - See trends and patterns over time
5. **Batch reviews** - Review multiple files with glob patterns
6. **Interactive mode** - Agent asks clarifying questions
7. **Per-directory templates** - Different criteria for different parts of codebase
8. **Suggested fixes** - Concrete improvement recommendations with diffs

Most of these build on the template system and can be added incrementally as needed.
