# Aiki Review System

**Status**: 🔴 Not Started  
**Priority**: High  
**Depends On**: Milestone 1.4 Task System

## Overview

The Review System provides code review capabilities integrated with Aiki's flow system. Reviews are stored on the `aiki/reviews` JJ branch as event-sourced data, separate from tasks.

**Key Architecture:**
- Reviews stored on `aiki/reviews` branch (separate from `aiki/tasks`)
- Event-sourced: review requests, completions, cancellations
- Reviews create tasks via `discovered_from` links
- Different lifecycle: reviews are one-time verifications, tasks are ongoing work

**Design Philosophy:**
- **Headless-first**: Reviews run in background, results available via flows
- **Agent-agnostic**: Works with any agent (Claude Code, Cursor, Codex, etc.)
- **Flow-integrated**: Review results trigger flow actions automatically
- **Flexible scoping**: Review working copy, staged, session, ranges, or specific files

---

## Table of Contents

1. [Core Concepts](#core-concepts)
2. [Data Model](#data-model)
3. [CLI Commands](#cli-commands)
4. [Flow Integration](#flow-integration)
5. [Use Cases](#use-cases)
6. [Implementation Plan](#implementation-plan)

---

## Core Concepts

### Review vs Task Separation

```
┌─────────────────────────────────────────────────────────────────┐
│  REVIEWS (aiki/reviews branch)                                  │
│  • One-time code verification                                   │
│  • Lifecycle: requested → completed                             │
│  • Results: approve/reject + issues found                       │
│  • Stored as ReviewEvent stream                                 │
└─────────────────────────────────────────────────────────────────┘
                          │
                          │ creates (if issues found)
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  TASKS (aiki/tasks branch)                                      │
│  • Ongoing work items                                           │
│  • Lifecycle: created → claimed → started → completed → closed  │
│  • Linked via discovered_from: "review:rev-456"                 │
│  • Stored as TaskEvent stream                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Review Scopes

- **working_copy** (`@`) - Current uncommitted changes
- **staged** - Files staged for commit
- **session** - All changes in current session
- **range** - JJ revset (e.g., `trunk()..@`)
- **files** - Specific file paths

### Review Agents

- **codex** - OpenAI Codex (default for v1)
- **claude-code** - Anthropic Claude (self-review)
- **cursor** - Cursor agent
- **human** - Human reviewer (async)

---

## Data Model

### Review Events

Stored on `aiki/reviews` branch:

```yaml
# Review requested
---
aiki_review_event: v1
review_id: rev-abc123
event: requested
timestamp: 2025-01-15T10:00:00Z
requested_by: claude-code
requested_from: codex
scope:
  type: working_copy
  files: ["src/auth.ts", "src/middleware.ts"]
context: "JWT authentication implementation"
prompt: default
---

# Review completed
---
aiki_review_event: v1
review_id: rev-abc123
event: completed
timestamp: 2025-01-15T10:05:00Z
reviewed_by: codex
outcome: rejected
issues_found: 2
issues:
  - severity: error
    message: "Potential null pointer dereference"
    file: src/auth.ts
    line: 42
    suggestion: "Add null check before accessing user.name"
  - severity: warning
    message: "JWT token should be validated"
    file: src/auth.ts
    lines: [67, 68, 69]
    suggestion: "Validate token signature before use"
tasks_created: ["task-def456", "task-ghi789"]
---
```

### Tasks Created from Reviews

When review finds issues, creates tasks on `aiki/tasks` branch:

```yaml
---
aiki_task_event: v1
task_id: task-def456
event: created
timestamp: 2025-01-15T10:05:01Z
agent_type: codex
task:
  name: "Fix: Potential null pointer dereference"
  type: issue
  body: |
    Found during code review: review:rev-abc123
    
    Potential null pointer dereference at src/auth.ts:42
    
    Suggestion: Add null check before accessing user.name
  scope:
    files:
      - path: src/auth.ts
        lines: [42]
  discovered_from: "review:rev-abc123"
  assignee: claude-code  # Authoring agent from JJ [aiki] metadata
---
```

### Review Outcomes

```rust
pub enum ReviewOutcome {
    Approved,
    ApprovedWithSuggestions { suggestions: Vec<String> },
    Rejected { issues: Vec<ReviewIssue> },
}

pub struct ReviewIssue {
    severity: IssueSeverity,  // Error, Warning, Suggestion
    message: String,
    file: PathBuf,
    line: Option<u32>,
    lines: Option<Vec<u32>>,
    suggestion: Option<String>,
}
```

---

## CLI Commands

### aiki review request

Request a code review:

```bash
# Review working copy with default agent (codex)
aiki review request @

# Review staged changes
aiki review request @ --scope staged

# Review with specific agent
aiki review request @ --from codex

# Self-review (same agent reviews own code)
aiki review request @ --self

# Review with context
aiki review request @ --context "Ready for merge, focus on error handling"

# Review specific files
aiki review request @ --files src/auth.ts src/middleware.ts

# Review with custom prompt
aiki review request @ --prompt security

# Quick mode (faster, less thorough)
aiki review request @ --quick
```

### aiki review list

List reviews:

```bash
# List pending reviews
aiki review list --pending

# List reviews for specific agent
aiki review list --for human

# List all reviews
aiki review list

# JSON output
aiki review list --pending --json
```

### aiki review show

Show review details:

```bash
# Show review
aiki review show rev-abc123

# Show with full issue details
aiki review show rev-abc123 --verbose

# JSON output
aiki review show rev-abc123 --json
```

### aiki review history

Show review history for changes:

```bash
# History for current change
aiki review history @

# History for range
aiki review history 'trunk()..@'

# History for specific file
aiki review history @ --file src/auth.ts
```

---

## Flow Integration

### review: Flow Action

Request a review from a flow:

```yaml
# Simple review
response.received:
  - review:
      scope: session

# Review with options
change.completed:
  - review:
      scope: working_copy
      from: codex
      prompt: default
      context: "Change completed, ready for review"

# Self-review
response.received:
  - review:
      scope: session
      self: true
      quick: true

# Review specific files
change.completed:
  - if: $event.files | contains("src/auth.ts")
    then:
      - review:
          scope: working_copy
          files: $event.files
          prompt: security
```

### review.completed Event

Fires when review finishes:

```yaml
review.completed:
  - if: $event.review.issues_found > 0
    then:
      - log: "⚠️  Review found ${event.review.issues_found} issue(s)"
      
      # Tasks already created automatically
      # Just notify user
      - log: "Created ${event.review.tasks_created.length} task(s)"
```

### Prompt-based Flow Filtering

Use `prompt` field to filter flows:

```yaml
# Security review handler
review.completed:
  - if: $event.review.prompt == "security" && $event.review.issues_found > 0
    then:
      - block: |
          🚨 SECURITY REVIEW FAILED
          
          Found ${event.review.issues_found} security issue(s).
          Fix before proceeding.

# Performance review handler  
review.completed:
  - if: $event.review.prompt == "performance"
    then:
      - log: "Performance review: ${event.review.outcome}"
```

---

## Use Cases

### Use Case 1: Pre-Commit Review

```yaml
# .aiki/flows/default.yml
name: "default"
version: "1"

commit_message.started:
  - log: "Running pre-commit review..."
  - review:
      scope: staged
      prompt: default
  
  - if: $review.issues_found > 0
    then:
      - block: |
          ❌ Review failed with ${review.issues_found} issue(s).
          
          Fix tasks created. Run `aiki task ready` to see them.
```

### Use Case 2: Critical File Security Review

```yaml
# .aiki/flows/security.yml
name: "security"
version: "1"

change.completed:
  - if: $event.files | any(f => f.path | contains("auth") || f.path | contains("crypto"))
    then:
      - log: "Security-sensitive file changed, requesting review..."
      - review:
          scope: working_copy
          files: $event.files
          prompt: security

review.completed:
  - if: $event.review.prompt == "security" && $event.review.issues_found > 0
    then:
      - block: |
          🚨 SECURITY REVIEW FAILED
          
          Security issues found in critical files.
          Review tasks: ${event.review.tasks_created}
```

### Use Case 3: Session Self-Review

```yaml
# .aiki/flows/self-review.yml
name: "self-review"
version: "1"

response.received:
  - if: $event.session.modified_files_count > 0
    then:
      - log: "Running self-review..."
      - review:
          scope: session
          self: true
          quick: true
      
      - if: $review.issues_found > 0
        then:
          - log: "⚠️  Found ${review.issues_found} issue(s) in self-review"
          - log: "Tasks created: ${review.tasks_created}"
```

### Use Case 4: Pre-Push Review

```yaml
# .aiki/flows/pre-push.yml
name: "pre-push"
version: "1"

shell.permission_asked:
  - if: $event.command | contains("git push")
    then:
      - log: "Running pre-push review..."
      - review:
          scope: staged
          prompt: default
      
      - if: $review.issues_found > 0
        then:
          - block: |
              ❌ Cannot push - review found issues
              
              Fix these tasks first:
              ${review.tasks_created | join(", ")}
```

---

## Implementation Plan

### Phase 1: Core Review System

**Deliverables:**
1. `aiki/reviews` branch management
2. ReviewEvent types and storage
3. `aiki review request` CLI command
4. Basic review execution (codex agent)
5. Task creation from review results

**Files:**
- `cli/src/reviews/types.rs` - ReviewEvent, ReviewOutcome, ReviewIssue
- `cli/src/reviews/manager.rs` - ReviewManager with JJ operations
- `cli/src/reviews/agents/codex.rs` - Codex agent implementation
- `cli/src/commands/review.rs` - CLI commands

**Timeline:** 2 weeks

### Phase 2: Flow Integration

**Deliverables:**
1. `review:` flow action
2. `review.completed` event
3. Prompt templates system
4. Self-review support

**Files:**
- `cli/src/flows/actions/review.rs` - Review flow action
- `cli/src/reviews/prompts/` - Prompt templates

**Timeline:** 1 week

### Phase 3: Additional Features

**Deliverables:**
1. Multiple review agents (claude-code, cursor)
2. Quick mode
3. Review history
4. Human review support (async)

**Timeline:** 2 weeks

---

## Success Criteria

### Must Have (Phase 1)

- ✅ Reviews stored on `aiki/reviews` branch
- ✅ `aiki review request @` works with codex
- ✅ Review results create tasks automatically
- ✅ Tasks linked via `discovered_from`
- ✅ Reviews and tasks completely separate

### Should Have (Phase 2)

- ✅ `review:` flow action
- ✅ `review.completed` event fires
- ✅ Prompt templates (default, security, performance)
- ✅ Self-review mode

### Nice to Have (Phase 3)

- ✅ Multiple agent support
- ✅ Quick mode
- ✅ Review history queries
- ✅ Human review workflow

---

## Open Questions

1. **Review agent configuration** - Hardcode codex for v1 or make configurable?
2. **Prompt storage** - YAML files in `.aiki/prompts/` or embedded in code?
3. **Review caching** - Cache review results to avoid re-reviewing same code?
4. **Concurrent reviews** - Allow multiple pending reviews or serialize?

---

## References

- Milestone 1.4: Task System
- [OpenAI Codex API](https://platform.openai.com/docs/guides/code)
- [Google's Critique/Oracle](https://research.google/blog/learning-to-generate-corrective-patches-using-neural-machine-translation/)
