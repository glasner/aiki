# Understand the implementation of spec

Explore the codebase to understand the **current implementation** of plan in {{data.scope.id}}

When done with all subtasks close this task with a summary of your understanding:

```bash
aiki task close {{id}} --summary <your summary here>
```

# Subtasks

{% subtask aiki/review/spec %}

## Explore the codebase to understand implementation

Search for files, modules, and functions described in the spec. Read the actual code to understand the current state.

### Review Criteria

Evaluate the implementation against these categories:

**Spec Coverage**
- All requirements from the spec exist in the codebase
- No missing features or unimplemented sections
- No scope creep beyond what the spec describes

**Code Quality**
- Logic errors, incorrect assumptions, edge cases
- Error handling and resource management
- Code clarity and maintainability

**Security**
- Injection vulnerabilities (command, SQL, XSS)
- Authentication and authorization issues
- Data exposure or crypto misuse

**Spec Alignment**
- UX matches spec design (commands, flags, output format)
- Architecture follows spec's prescribed approach
- Acceptance criteria from spec are met
