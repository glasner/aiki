# Aiki - AI Code Provenance Tracking

Aiki is an AI-aware workflow layer for codebases: provenance-first, multi-agent orchestration, and automatic review/fix loops.

## What it helps with (for developers)

**Aiki turns AI coding into an engineer-controlled workflow.**
It helps teams ship faster and safer by turning planning, execution, and review into one repeatable loop:

- **Ship quickly:** move from plan to implementation with coordinated task execution and parallelizable lanes.
- **Keep quality in the loop:** automated review and fix loops catch issues before they become technical debt.
- **Preserve context:** task history + provenance show what changed, why it changed, and who was involved.
- **Reduce friction:** encode team conventions in events/flows so AI behavior stays consistent across editors and sessions.

## Quick Start

For setup and first run, use:

- [Getting Started](cli/docs/getting-started.md)

## Reference Docs

- [SDLC: Plan, Build, Review, Fix](cli/docs/sdlc.md)
- [Customizing Defaults](cli/docs/customizing-defaults.md)
- [Creating Plugins](cli/docs/creating-plugins.md)
- [Task Types and Links](cli/docs/tasks/kinds.md)
- [Session Isolation Workflow](cli/docs/session-isolation-workflow.md)




## Delegating Documentation Work

Use this template for doc updates when you want a dedicated pass:

```bash
aiki task start --template aiki/docs-writer
```

Pass scope to focus the work, for example with --data scope=getting-started or --data scope=sdlc.

The technical writer persona is tuned for:
- concise developer-first prose
- avoiding duplicated setup guidance
- consistent terminology (plan -> build -> review -> fix)
- accurate command snippets


## Need Deep Dives

- [Contribution Guide](cli/docs/contributing.md)
- [Plan](cli/docs/sdlc/plan.md), [Build](cli/docs/sdlc/build.md), [Decompose](cli/docs/sdlc/decompose.md), [Loop](cli/docs/sdlc/loop.md)
- [Review](cli/docs/sdlc/review.md), [Fix](cli/docs/sdlc/fix.md)
