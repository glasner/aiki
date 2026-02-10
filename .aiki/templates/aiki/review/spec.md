# Understand the plan described in spec

Read the spec file at {{data.scope.id}} to understand plan.

If this spec was created from a task, check the task for additional context:

```bash
aiki task show --source file:{{data.scope.id}} --with-instructions
```

When done close this task with a summary of your understanding:

```bash
aiki task close {{id}} --summary <your summary here>                # View all code changes
```
