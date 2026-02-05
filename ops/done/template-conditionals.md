# Template Conditionals

**Date**: 2026-02-04
**Status**: Implemented
**Purpose**: Add conditional logic to task templates for dynamic content based on context

**Related Documents**:
- [Task Templates](../done/task-templates.md) - Base template system (implemented)
- [Declarative Subtasks](../done/declarative-subtasks.md) - Iteration over data sources (implemented)
- [Spec-Aware Review](spec-aware-review.md) - Uses conditionals for unified review template

---

## Executive Summary

Add conditional blocks to task templates, enabling a single template to adapt its content based on runtime context. This is the foundation for a unified review template that handles both code and spec reviews.

**This spec adopts full Tera syntax conventions:**
- **Control flow:** `{% if condition %}...{% endif %}`
- **Variable substitution:** `{{variable}}` (changed from `{variable}`)

**Syntax example:**
```markdown
{% if data.target_type == "file" %}
Review the document at {{data.path}} for completeness and clarity.
{% else %}
Review the code changes in {{data.scope}} for bugs and quality.
{% endif %}
```

**⚠️ Breaking Change:** This requires updating variable syntax from `{var}` → `{{var}}` throughout all templates and documentation.

---

## Motivation

The current template system supports variable substitution (`{data.key}`) but no conditional logic. This forces separate templates for variations of the same workflow:

**Current approach (multiple templates):**
```
.aiki/templates/aiki/
├── review.md           # Code review
├── review-spec.md      # Spec/document review
├── review-security.md  # Security-focused review
```

**With conditionals (single adaptive template):**
```
.aiki/templates/aiki/
└── review.md           # Adapts based on data.target_type
```

---

## Design

### Conditional Syntax

Use Jinja2/Tera-style syntax for clarity and consistency with Rust ecosystem:

```markdown
{% if <condition> %}
  content when true
{% else %}
  content when false (optional)
{% endif %}
```

**Why full Tera syntax:**
- **Consistency** - Tera is the dominant Rust template engine, natural fit for Aiki
- **Clear delimiters** - `{% %}` for control flow, `{{}}` for values
- **Standard conventions** - Matches what Rust developers already know
- **No ambiguity** - Double braces clearly distinguish variables from markdown
- **Future-proof** - Could potentially swap in Tera crate if needed

### Supported Conditions

**Equality:**
```markdown
{% if data.target_type == "file" %}
{% if assignee == "codex" %}
{% if priority == "p0" %}
```

**Inequality:**
```markdown
{% if data.target_type != "task" %}
```

**Numeric comparisons:**
```markdown
{% if data.priority_level > 2 %}
{% if data.count >= 10 %}
{% if data.score < 50 %}
{% if data.percentage <= 100 %}
```

**Existence (truthy):**
```markdown
{% if data.custom_field %}
  Field exists and is not empty/false/null/0
{% endif %}
```

**Negation:**
```markdown
{% if not data.skip_security %}
  Security checks enabled
{% endif %}
```

**Else-if chaining:**
```markdown
{% if data.target_type == "file" %}
  Document review
{% elif data.target_type == "task" %}
  Task review
{% else %}
  Code review
{% endif %}
```

### Nesting

Conditionals can be nested:

```markdown
{% if data.target_type == "file" %}
  {% if data.file_type == "spec" %}
    Review for implementability.
  {% else %}
    Review for clarity.
  {% endif %}
{% endif %}
```

### Boolean Operators

Support `and`, `or` for combining conditions:

```markdown
<!-- Both conditions must be true -->
{% if data.type == "file" and data.subtype == "spec" %}
  Spec file review
{% endif %}

<!-- Either condition can be true -->
{% if priority == "p0" or priority == "p1" %}
  High priority task
{% endif %}

<!-- Complex combinations -->
{% if data.type == "file" and (priority == "p0" or priority == "p1") %}
  High priority file review
{% endif %}
```

**Operator precedence:**
1. Parentheses `()`
2. `not`
3. `and`
4. `or`

**Nesting alternative:**
While `and`/`or` are supported, nesting is still valid for complex cases:

```markdown
{% if data.a == "x" %}
  {% if data.b == "y" %}
    Both conditions true
  {% endif %}
{% endif %}
```

---

## Examples

### Unified Review Template

```markdown
---
version: 1.0.0
type: review
---

# Review: {{data.target_name}}

Review the target for quality and readiness.

# Subtasks

## Understand what you're reviewing

{% if data.target_type == "file" %}
Read the file at `{{data.path}}` to understand:
- What is this document about?
- What is its purpose?
- Who is the audience?
{% else %}
Examine the code changes:
1. `aiki task show {{data.task_id}} --with-source` - Understand intent
2. `aiki task diff {{data.task_id}}` - View changes
{% endif %}

## Evaluate quality

{% if data.target_type == "file" %}
**For documents/specs, check:**
- **Completeness** - All sections filled, no TODOs or placeholders
- **Clarity** - Unambiguous requirements, clear acceptance criteria
- **Implementability** - Can be decomposed into tasks, sufficient detail
- **UX** - User experience considered, intuitive design
{% else %}
**For code, check:**
- **Correctness** - Logic errors, edge cases, bugs
- **Quality** - Error handling, resource leaks, code clarity
- **Security** - Injection, auth issues, data exposure
- **Performance** - Inefficient algorithms, resource usage
{% endif %}

For each issue, add a comment:

aiki task comment {{task.parent_id}} \
  --data severity=high|medium|low \
  --data category=<category> \
  "<description and suggested fix>"
```

### Optional Sections

```markdown
## Security audit

{% if data.include_security %}
Perform deep security analysis:
- Authentication flows
- Data validation
- Cryptographic usage
{% else %}
Basic security check only (use `--data include_security=true` for full audit).
{% endif %}
```

### Multi-way Branching with Elif

```markdown
## Review focus

{% if data.review_type == "security" %}
**Security-focused review:**
- Authentication and authorization
- Input validation
- Data exposure risks
{% elif data.review_type == "performance" %}
**Performance-focused review:**
- Algorithm complexity
- Resource usage
- Caching opportunities
{% elif data.review_type == "ux" %}
**UX-focused review:**
- User flow clarity
- Error messages
- Accessibility
{% else %}
**General code review:**
- Correctness, quality, and maintainability
{% endif %}
```

### Boolean Logic with And/Or

```markdown
## Priority handling

{% if priority == "p0" or priority == "p1" %}
**High priority** - Address immediately

{% if data.type == "security" and priority == "p0" %}
🚨 **CRITICAL SECURITY ISSUE** - Drop everything and fix now
{% endif %}

{% elif priority == "p2" %}
Standard priority - Address in normal workflow
{% else %}
Low priority - Consider for future work
{% endif %}
```

### Complex Conditions with Parentheses

```markdown
## Conditional review depth

{% if (data.type == "file" and data.subtype == "spec") or priority == "p0" %}
**Deep review required:**
- All specs get deep review
- All p0 tasks get deep review regardless of type

{% if data.type == "file" %}
Review for completeness, clarity, and implementability.
{% else %}
Review for correctness, security, and quality.
{% endif %}
{% else %}
**Standard review:**
Basic checks for correctness and quality.
{% endif %}
```

### Numeric Comparisons

```markdown
## Test coverage requirements

{% if data.coverage >= 80 %}
✅ **Excellent coverage** ({{data.coverage}}%)
Code coverage meets quality standards.
{% elif data.coverage >= 60 %}
⚠️ **Acceptable coverage** ({{data.coverage}}%)
Consider adding tests for critical paths.
{% else %}
❌ **Insufficient coverage** ({{data.coverage}}%)
Add tests before merging.
{% endif %}

## Priority-based workflow

{% if data.issue_count > 10 %}
**High issue volume** - {{data.issue_count}} issues found

{% if data.critical_count > 0 %}
🚨 {{data.critical_count}} critical issues require immediate attention
{% endif %}

Break into smaller review chunks.
{% elif data.issue_count > 0 %}
**Standard review** - {{data.issue_count}} issues to address
{% else %}
**Clean review** - No issues found!
{% endif %}
```

---

## Conditionals with Iteration

When using `subtasks:` to iterate over a data source (see [Declarative Subtasks](../done/declarative-subtasks.md)), conditionals are evaluated **once per item**. This enables filtering and customizing subtasks based on item properties.

### Variable Scopes in Iteration

Inside an iterated subtask template:

| Variable | Source | Example |
|----------|--------|---------|
| `item.*` | Current iteration item | `item.severity`, `item.category` |
| `data.*` | Parent template data | `data.target_type` |
| `parent.*` | Parent task fields | `parent.id`, `parent.assignee` |

### Example: Filter Subtasks by Severity

Only create subtasks for high-severity issues:

```markdown
---
version: 1.0.0
subtasks: source.comments
---

# Followup: {{source.name}}

Fix high-severity issues from review.

# Subtasks

{% if item.severity == "high" %}
## Fix: {{item.file}}:{{item.line}}

**Severity**: {{item.severity}}
**Category**: {{item.category}}

{{item.text}}
{% endif %}
```

**Result:** Only comments with `severity == "high"` become subtasks. Others are skipped entirely.

### Example: Conditional Content per Subtask

Vary subtask instructions based on item properties:

```markdown
---
version: 1.0.0
subtasks: source.comments
---

# Followup: {{source.name}}

# Subtasks

## Fix: {{item.category}} issue in {{item.file}}

{% if item.category == "security" %}
**SECURITY ISSUE** - Priority fix required.

Review authentication and authorization in this area.
Ensure no data exposure or injection vulnerabilities.
{% elif item.category == "performance" %}
**Performance issue** - Profile before and after fix.

Measure impact with benchmarks if possible.
{% else %}
Standard fix - follow code review feedback.
{% endif %}

**File**: {{item.file}}:{{item.line}}
**Description**: {{item.text}}
```

### Example: Combine Parent and Item Conditions

Use both parent-level data and item properties:

```markdown
---
version: 1.0.0
subtasks: source.comments
---

# Followup: {{source.name}}

# Subtasks

## {{item.category}}: {{item.file}}

{% if data.strict_mode %}
  {% if item.severity == "low" %}
  Even low-severity issues must be fixed in strict mode.
  {% endif %}
{% else %}
  {% if item.severity == "low" %}
  Low severity - consider fixing if time permits.
  {% endif %}
{% endif %}

{{item.text}}
```

### Evaluation Order with Iteration

When `subtasks:` is specified:

1. Parse frontmatter, extract `subtasks: <source>`
2. Load data source (e.g., fetch comments from task)
3. **For each item in source:**
   a. Set `item.*` variables from current item
   b. Parse and evaluate conditionals in subtask template
   c. If entire subtask is inside a false conditional, skip it
   d. Substitute remaining variables
   e. Add to subtask list
4. Create parent task + all non-skipped subtasks

### Subtask Skip Conditions

**Simple rule:** A subtask is skipped (not created) when its `## ` heading is inside a conditional block that evaluates to false.

```markdown
{% if item.severity == "high" %}
## Critical: {{item.file}}

{{item.text}}
{% endif %}
```

If `item.severity` is not `"high"`, this entire subtask is skipped.

**What does NOT skip a subtask:**
- Conditional content before the heading (notes, comments)
- Conditional content after the heading
- Whitespace or blank lines before the heading inside the conditional

**Examples:**

```markdown
# Subtasks

<!-- This subtask is SKIPPED if severity != "high" -->
{% if item.severity == "high" %}
## Critical: {{item.file}}
{{item.text}}
{% endif %}

<!-- This subtask is ALWAYS created (heading outside conditional) -->
## Fix: {{item.file}}
{% if item.severity == "high" %}
**Priority:** Address immediately.
{% endif %}
{{item.text}}
```

**Parsing rule:**

When parsing subtasks inside a `subtasks:` template:
1. Split template by `## ` heading markers (outside fenced code blocks)
2. For each subtask section:
   a. Check if section starts with `{% if ... %}` (after stripping leading whitespace)
   b. If yes, evaluate condition
   c. If false AND the `## ` heading is inside this conditional block, skip the subtask
3. For non-skipped subtasks, evaluate remaining conditionals normally

**Note:** `## ` markers inside fenced code blocks (``` or ~~~) are not treated as subtask headings. The parser only recognizes headings at the top level of the template.

---

## Implementation

### Phase 1: Parser

**Deliverables:**
- Parse `{% if %}`, `{% elif %}`, `{% else %}`, `{% endif %}` blocks
- Support comparison operators: `==`, `!=`, `>`, `<`, `>=`, `<=`
- Support existence checks (truthy)
- Support negation (`not`)
- Support else-if chaining
- Support boolean operators (`and`, `or`)
- Support parentheses for grouping

**Files:**
- `cli/src/tasks/templates/conditionals.rs` - Conditional parser and evaluator

**Tokenizer Stage:**

The parser uses a two-phase approach with explicit lexical boundaries:

**Phase 1a: Tokenization**
Scan input left-to-right, recognizing tokens by delimiter priority:
1. `{%` ... `%}` → `ControlBlock` token (greedy match to closing `%}`)
2. `{{` ... `}}` → `Variable` token (greedy match to closing `}}`)
3. Everything else → `Text` token

**Line tracking:** The tokenizer tracks the current line number as it scans. When errors occur (mismatched delimiters, unclosed blocks), the line number where the problem started is included in the error message.

**Token stream example:**
```
Input:  "{% if data.x %}{{data.y}}{% endif %}"
Tokens: [ControlBlock("if data.x"), Variable("data.y"), ControlBlock("endif")]
```

**Delimiter rules:**
- Delimiters are recognized by longest-match: `{{` is one token, not two `{`
- Nested braces inside delimiters are not special: `{{foo.{bar}}}` is invalid
- Unclosed delimiters are parse errors, not text

**Phase 1b: Parsing**
Build AST from token stream:
1. `ControlBlock` tokens → parse condition expressions
2. `Variable` tokens → variable references (substituted later)
3. `Text` tokens → literal text

**Parsing approach:**
1. Tokenize template into text segments and conditional blocks
2. Build AST of conditional nodes
3. Evaluate conditions against data context
4. Render selected branches

```rust
enum TemplateNode {
    Text(String),
    Conditional {
        condition: Condition,
        if_branch: Vec<TemplateNode>,
        elif_branches: Vec<(Condition, Vec<TemplateNode>)>,  // else-if chains
        else_branch: Option<Vec<TemplateNode>>,
    },
}

enum Condition {
    Equals { left: String, right: String },
    NotEquals { left: String, right: String },
    GreaterThan { left: String, right: String },
    LessThan { left: String, right: String },
    GreaterOrEqual { left: String, right: String },
    LessOrEqual { left: String, right: String },
    Exists(String),                          // data.foo is truthy
    Not(Box<Condition>),                     // not data.foo
    And(Box<Condition>, Box<Condition>),     // cond1 and cond2
    Or(Box<Condition>, Box<Condition>),      // cond1 or cond2
}
```

### Phase 2: Evaluator

**Deliverables:**
- Evaluate conditions against template data
- Resolve variable references in conditions
- Handle missing variables gracefully (treat as falsy)

**Evaluation rules:**
- `data.key == "value"` - String equality (case-sensitive)
- `data.key != "value"` - String inequality
- `data.key > value` - Numeric greater than (values coerced to numbers)
- `data.key < value` - Numeric less than
- `data.key >= value` - Numeric greater than or equal
- `data.key <= value` - Numeric less than or equal
- `data.key` (truthy) - Exists and is not: `null`, `false`, `""`, `0`, empty array/object
- `not data.key` - Does not exist or is falsy
- `cond1 and cond2` - Both conditions must be true (short-circuit evaluation)
- `cond1 or cond2` - At least one condition must be true (short-circuit evaluation)
- `(condition)` - Parentheses for explicit precedence

**Type coercion for numeric comparisons:**
- String numbers are converted: `"42" > "10"` → `42 > 10` → `true`
- Floats are supported: `"3.14" > "2.5"` → `3.14 > 2.5` → `true`
- Non-numeric strings compare as `0`: `"abc" > 5` → `0 > 5` → `false`
- Empty strings compare as `0`: `"" > 5` → `0 > 5` → `false`
- Whitespace-only strings compare as `0`: `"  " > 5` → `0 > 5` → `false`
- Booleans: `true` = `1`, `false` = `0`

**Undefined variables:** Following industry standard practice (Handlebars, Jinja2, Tera, Liquid, Go templates), undefined variables evaluate as **falsy**, not as errors. This allows optional feature flags without requiring every variable to be present.

Example:
```markdown
{% if data.include_security %}
  Security checks enabled
{% endif %}
```

If `include_security` is not passed, the block is silently skipped (evaluates to false).

**Warnings for undefined variables:**

To help catch typos and mistakes, the evaluator emits warnings (not errors) when:
1. A variable in a conditional is undefined: `Warning: 'data.incldue_security' is undefined (treating as falsy) at line N`
2. A numeric comparison uses a non-numeric value: `Warning: 'data.count' has non-numeric value "abc" (treating as 0) at line N`

**Line number precision:** Warning line numbers refer to the **control block start line** (the `{% if %}` line), not the exact position of the variable within the condition expression. For example, in `{% if a and b %}` on line 5, a warning about undefined `b` would report "at line 5". This is a v1 simplification; future versions may add column-level precision for complex expressions.

**Strictness levels (v1):**

| Level | Undefined vars | Non-numeric coercion | Default |
|-------|---------------|---------------------|---------|
| `warn` | Warning, treat as falsy | Warning, treat as 0 | ✓ |
| `strict` | Error | Error | |

Set via frontmatter:
```yaml
---
version: 1.0.0
template_strict: true
---
```

**Rationale:** Default `warn` balances flexibility with discoverability. Users can opt into `strict` mode for critical templates where silent failures are unacceptable.

### Phase 3: Integration

**Deliverables:**
- Integrate with existing variable substitution
- Conditionals evaluated before variable substitution
- Error messages for malformed conditionals

**Processing order:**

For non-iterating templates:
1. Parse frontmatter (no substitution)
2. Parse and evaluate conditionals (removes unselected branches)
3. Substitute variables in remaining text
4. Parse markdown structure

For iterating templates (`subtasks:` specified):
1. Parse frontmatter, extract data source
2. Load data source items
3. For parent section: evaluate conditionals → substitute variables
4. For each item:
   a. Set `item.*` context
   b. Evaluate conditionals in subtask template
   c. If subtask heading is inside false conditional, skip entirely
   d. Substitute variables in remaining text
5. Create parent + non-skipped subtasks

---

## Error Handling

| Error | Message |
|-------|---------|
| Unclosed `{% if %}` | `Error: Unclosed conditional block starting at line N` |
| `{% else %}` without `{% if %}` | `Error: Unexpected {% else %} without matching {% if %}` |
| `{% elif %}` without `{% if %}` | `Error: Unexpected {% elif %} without matching {% if %}` |
| `{% endif %}` without `{% if %}` | `Error: Unexpected {% endif %} without matching {% if %}` |
| Invalid condition syntax | `Error: Invalid condition: '<condition>'. Expected: 'var == "value"', 'var', or 'not var'` |
| Unknown item field in iteration | `Warning: Unknown field 'item.foo'. Treating as undefined (falsy).` |

**Note:** Unknown variables are treated as falsy (warnings only), not errors. This matches standard templating system behavior.

---

## Comparison with Jinja2/Tera

This is a **subset** of Jinja2/Tera, not full implementation:

| Feature | Supported | Notes |
|---------|-----------|-------|
| `{% if %}` | Yes | Basic conditionals with comparisons |
| `{% elif %}` | Yes | Else-if chaining |
| `{% else %}` | Yes | Else branch |
| `{% endif %}` | Yes | Close block |
| `not` negation | Yes | `{% if not var %}` |
| `and`, `or` | Yes | `{% if a and b %}`, short-circuit evaluation |
| Parentheses | Yes | `{% if (a or b) and c %}` |
| Comparison operators | Yes | `==`, `!=`, `>`, `<`, `>=`, `<=` with type coercion |
| `{% for %}` | No | Use `subtasks:` in frontmatter |
| `{% include %}` | No | Future: template inheritance |
| Filters | No | Keep templates simple |
| Tests (`is defined`) | No (v1) | Undefined vars are falsy; explicit test in v2 |
| Complex expressions | No | Use nesting |

**Why not full Jinja2/Tera?**
- Simpler than full Tera (no filters, tests, macros, inheritance)
- Templates stay readable as documentation
- Avoids Turing-complete template logic
- Includes most commonly-needed features (conditionals, comparisons, boolean logic)
- Can add more features in v2 if needed (`contains`, `is defined`, whitespace control, etc.)

---

## Future Enhancements (v2+)

### Array/Collection Operators

Support `contains` for checking membership:

```markdown
{% if data.tags contains "security" %}
  Security-focused review
{% endif %}
```

Alternative syntax (Python-style):
```markdown
{% if "security" in data.tags %}
```

Recommendation: Use Liquid-style `contains` for readability in markdown context.

### Whitespace Control (v2)

Add Jinja2/Liquid-style whitespace trimming:

```markdown
{%- if condition -%}
  Content without surrounding blank lines
{%- endif -%}
```

- `{%-` removes whitespace before
- `-%}` removes whitespace after

**Use case:** Prevent conditional blocks from creating blank lines in rendered output.

---

## Whitespace Handling (v1)

Since explicit whitespace control (`{%-`, `-%}`) is deferred to v2, v1 uses a simple automatic strategy:

**Rule: Collapse conditional-introduced blank lines**

When a conditional block evaluates to false (or true but empty), remove:
1. The entire line containing `{% if/elif/else/endif %}` (if that line contains only the tag + whitespace)
2. Up to one blank line immediately following a removed block

**Example:**
```markdown
Before:
Some text.

{% if data.show_note %}
**Note:** Important info here.
{% endif %}

More text.

After (if show_note is false):
Some text.

More text.
```

**Why this works:**
- Control tags on their own lines are removed entirely (no leftover blank lines)
- One trailing blank line is absorbed to prevent double-spacing
- Inline conditionals (text on same line as tag) preserve the line structure

**Edge cases:**
- Multiple consecutive false conditionals: each absorbs one blank line
- Nested conditionals: outer removal takes precedence
- Inline usage (`text {% if x %}more{% endif %} text`): no line removal, just content removal

**Limitations (addressed in v2):**
- Cannot preserve intentional blank lines inside conditional blocks
- Cannot force blank line removal when content exists on same line as tag
- Complex nesting may still produce extra blank lines in rare cases

### Explicit Defined Tests

Add Jinja2-style `is defined` test:

```markdown
{% if data.optional_field is defined %}
  Field was explicitly provided
{% endif %}
```

**Difference from truthy check:** `is defined` returns true even if value is `false`, `""`, or `0`.

---

## Delimiter Considerations

Aiki template syntax uses full Tera conventions:
- `{{var}}` - Variable substitution (double braces)
- `{% if %}` - Control flow (brace-percent)

This provides:
- **Standard Tera syntax** - Matches what Rust developers expect
- **Clear separation** - `{{}}` for values, `{% %}` for control flow
- **No ambiguity** - Parser rules match Tera semantics
- **Future compatibility** - Could potentially use Tera crate directly

**Delimiter rules:**
- `{{data.key}}` → variable substitution
- `{% if data.key %}` → conditional
- `{data.key}` → parse error (single brace not valid)

---

## Migration from Single-Brace Syntax

**Breaking Change:** This spec updates variable syntax from `{var}` to `{{var}}` to align with full Tera conventions.

### What Needs Updating

All templates and documentation that reference variables must be updated:

**Files requiring updates:**
1. **Specs and documentation:**
   - `ops/done/task-templates.md` - Base template system spec
   - `ops/done/declarative-subtasks.md` - Subtask iteration spec
   - `ops/now/spec-aware-review.md` - Review system spec
   - Any other docs with template examples

2. **Template files:**
   - `.aiki/templates/aiki/review.md` - Built-in review template
   - `.aiki/templates/aiki/spec.md` - Spec template (if exists)
   - Any custom user templates in `.aiki/templates/`

3. **Implementation references:**
   - Code comments or examples in `cli/src/tasks/templates/`

### Syntax Changes

| Old Syntax | New Syntax | Usage |
|------------|------------|-------|
| `{data.key}` | `{{data.key}}` | Variable substitution |
| `{item.field}` | `{{item.field}}` | Iteration item fields |
| `{source.name}` | `{{source.name}}` | Source reference |
| `{task.id}` | `{{task.id}}` | Task fields |
| `{parent.id}` | `{{parent.id}}` | Parent task fields |

### Migration Strategy

**Phase 1: Documentation (Completed)**
- ✅ `template-conditionals.md` - Updated to use `{{var}}` syntax with migration guide

**Phase 2: Specs (To Do)**
- ⬜ `ops/done/task-templates.md` - Base template system spec (defines variable syntax)
- ⬜ `ops/done/declarative-subtasks.md` - Subtask iteration with `{{item.*}}` variables
- ⬜ `ops/now/spec-aware-review.md` - Review system using templates

**Phase 3: Template Files (Completed)**
- ✅ `.aiki/templates/aiki/review.md` - Built-in review template
- ✅ `.aiki/templates/aiki/fix.md` - Fix template
- ✅ `.aiki/templates/aiki/spec.md` - Spec template

**Phase 4: Implementation (Completed)**
- ✅ Create `conditionals.rs` module with tokenizer, parser, and evaluator
- ✅ Update `variables.rs` to recognize `{{var}}` delimiters
- ✅ Add error for single-brace syntax with helpful migration message
- ✅ Update tests to use new syntax

**Completion Criteria:**
- All specs use `{{var}}` consistently
- All templates render correctly with new parser
- Error messages guide users away from old `{var}` syntax

### Migration Validation

**Automated validation tests:**

1. **Syntax scanner** - Find old syntax in templates:
   ```bash
   # Should find NO matches after migration
   rg '\{[a-z_]+(\.[a-z_]+)*\}' .aiki/templates/ --type md
   ```

2. **Template render tests** - Each template file gets a test case:
   ```rust
   #[test]
   fn test_review_template_renders() {
       let template = load_template(".aiki/templates/aiki/review.md");
       let data = json!({
           "data": {"target_type": "task", "task_id": "abc123"},
           "task": {"parent_id": "def456"}
       });
       let result = render_template(&template, &data);
       assert!(result.is_ok());
       assert!(!result.unwrap().contains("{data.")); // No unsubstituted vars
   }
   ```

3. **Round-trip test** - Parse → render → parse should be stable:
   ```rust
   #[test]
   fn test_template_roundtrip() {
       let original = load_template("review.md");
       let rendered = render_template(&original, &test_data());
       // Re-parsing rendered output shouldn't find template syntax
       assert!(!rendered.contains("{{"));
       assert!(!rendered.contains("{%"));
   }
   ```

4. **Error message test** - Old syntax produces helpful error:
   ```rust
   #[test]
   fn test_old_syntax_error_message() {
       let bad_template = "Hello {name}!";
       let err = parse_template(bad_template).unwrap_err();
       assert!(err.contains("single-brace syntax"));
       assert!(err.contains("{{name}}"));  // Suggests fix
   }
   ```

5. **Spec consistency test** - All spec files use new syntax:
   ```bash
   # Run as CI check
   ! rg '\{[a-z_]+\}' ops/**/*.md || echo "Found old syntax in specs"
   ```

**Manual validation checklist:**
- [ ] `aiki review <task-id>` works with updated review.md template
- [ ] Conditional branches render correctly (test both true/false paths)
- [ ] Nested conditionals render correctly
- [ ] Iteration with conditionals produces expected subtasks
- [ ] Warning messages appear for undefined variables
- [ ] `template_strict: true` produces errors as expected

### Backward Compatibility

**No backward compatibility** - This is a breaking change. Single-brace syntax will not be supported once conditionals are implemented. This is acceptable because:

1. Template system is not yet in production use
2. Cleaner to make the break now than support two syntaxes
3. Migration is straightforward (find/replace `{` → `{{` and `}` → `}}`)

**Migration error message:**

When the parser encounters single-brace syntax, it produces a helpful error:
```
Error: Invalid template syntax at line 5: '{data.name}'

  Single-brace variable syntax is no longer supported.
  Please update to double-brace syntax: '{{data.name}}'

  Quick fix: sed -i 's/{\([a-z_][a-z0-9_.]*\)}/{{\1}}/g' your-template.md
```

---

## Summary

Template conditionals enable:
- **Single templates** that adapt to context
- **Unified workflows** (one review template for code and specs)
- **Optional sections** based on flags
- **Filtered iteration** (skip subtasks based on item properties)
- **Per-item customization** (vary subtask content based on item data)
- **Cleaner template organization** (fewer files)
- **Tera compatibility** (Rust-native templating semantics)

This is a foundational feature for the spec-aware review system and works seamlessly with the existing `subtasks:` iteration feature.

**Key design decisions:**
1. **Full Tera syntax** - `{% %}` for control flow, `{{}}` for variables (breaking change from `{var}`)
2. **Comprehensive v1 feature set** - All comparison operators, boolean logic (`and`/`or`), parentheses
3. **Undefined variables are falsy** - Not errors, following industry standards for flexibility
4. **Else-if chaining** - Essential for readable multi-way branching
5. **Type coercion** - Automatic for numeric comparisons with clear semantics
6. **Simple subset of Tera** - No filters, tests, or macros - keeps templates readable as documentation
