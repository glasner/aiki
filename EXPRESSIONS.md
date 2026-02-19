# Expression Syntax

Aiki uses [Rhai](https://rhai.rs) for expression evaluation in hook conditions and template conditionals.

## Basics

Expressions evaluate to a boolean result. Any expression that evaluates to a truthy value is considered `true`.

```
event.task.type == "review"
count > 5
approved
```

## Operators

### Comparison
| Operator | Meaning |
|----------|---------|
| `==` | Equal |
| `!=` | Not equal |
| `>` | Greater than |
| `>=` | Greater than or equal |
| `<` | Less than |
| `<=` | Less than or equal |

### Logical
| Symbol | Word form | Meaning |
|--------|-----------|---------|
| `&&` | `and` | Logical AND |
| `\|\|` | `or` | Logical OR |
| `!` | `not` | Logical NOT |

Both symbol and word forms are supported. Word operators must be whole words (`band` is not rewritten).

```
x > 5 && approved
x > 5 and approved        # equivalent
!done
not done                   # equivalent
a > 1 || b > 1
a > 1 or b > 1            # equivalent
```

## Variable Access

### Simple variables
```
approved                   # boolean check
count > 5                  # numeric comparison
status == "done"           # string comparison
```

### Dotted field access
```
event.task.type == "review"
event.write
data.config.threshold >= 5
```

### Dollar-prefix syntax (deprecated)
The `$var` prefix is supported for backwards compatibility but deprecated:

```
# Deprecated:
$event.task.type == "review"

# Preferred:
event.task.type == "review"
```

## Truthiness

Values are coerced to boolean using these rules:

| Value | Result |
|-------|--------|
| `true` | true |
| `false` | false |
| `0` / `0.0` | false |
| Non-zero number | true |
| `""` (empty string) | false |
| `"false"`, `"0"`, `"null"` | false |
| Non-empty string | true |
| Undefined variable | false |
| Map (exists) | true |
| Empty array | false |
| Non-empty array | true |

## Type Coercion

String values from variables are automatically coerced:

- `"true"` / `"false"` → boolean
- Integer strings (e.g., `"42"`) → integer
- Float strings (e.g., `"3.14"`) → float
- Everything else → string

## Error Handling

Expression evaluation uses **lenient mode**: if an expression fails to parse or evaluate, it defaults to `false` and emits a warning:

```
[aiki] Warning: condition evaluation failed (defaulting to false): `bad expr` — ...
```

## Usage in Hooks

Hook conditions use the `if:` field:

```yaml
- if: event.task.type == "review"
  then:
    - log: "Review task detected"
```

Variables available in hooks come from the event context (e.g., `event.*`, `session.*`).

## Usage in Templates

Template conditionals use `{% if %}` blocks:

```markdown
{% if data.needs_review %}
Review required.
{% elif data.priority > 5 %}
High priority item.
{% else %}
No action needed.
{% endif %}
```

Variables available in templates come from the template's data context.
