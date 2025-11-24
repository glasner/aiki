# The Aiki Way: aiki/default Flow

> Aiki's recommended opinionated flow for AI-assisted development, inspired by 6 months of production use pushing Claude Code to its limits.

## What is aiki/default?

`aiki/default` is Aiki's comprehensive flow that implements battle-tested patterns for working with AI coding agents. While `aiki/core` provides minimal provenance tracking, `aiki/default` adds intelligent automation to prevent common pitfalls and maximize code quality.

**Installation:**
```bash
aiki flows install aiki/default
```

**Key Difference:**
- `aiki/core` - Minimal provenance tracking (always runs)
- `aiki/default` - Comprehensive quality automation (opt-in)

---

## Implementation Roadmap

The `aiki/default` flow implements four key patterns through six implementation milestones:

### Milestone 1: Core Extensions (2-3 weeks)
**Goal:** Add event types and capabilities needed for all patterns.

**What gets built:**
- PrePrompt event (fires before agent sees prompt)
- PostResponse event (fires after agent responds)
- Flow composition (`flow:` action, `includes:` directive)
- doc_management action type
- Session state persistence

**Success metric:** Can compose flows and track session state

---

### Milestone 2: Auto Architecture Documentation (1-2 weeks)
**Goal:** Cache architecture exploration so agents don't re-discover the same patterns repeatedly.

**The Problem:**
Agents frequently grep entire directories to understand "how does the backend work?" or "where are components organized?" This is expensive (tokens, time) and repetitive.

**The Solution:**
Automatically generate and cache architecture documentation when agents explore code. Next time, they read the cached markdown instantly.

**What gets built:**
- Exploration detection (track when agents read 5+ files in a directory)
- Auto-summarization (extract patterns, key files, organization)
- Shadow directory structure (`.aiki/arch/structure/` mirrors codebase)
- Staleness tracking (invalidate when key files change)
- Auto-regeneration (recreate docs when cache is stale)
- CLI commands (aiki arch show/refresh/clear)

**Directory structure:**
```
.aiki/arch/
├── structure/              # Mirrors your codebase
│   ├── src/
│   │   ├── components/
│   │   │   └── index.md
│   │   └── services/
│   │       └── index.md
│   └── backend/
│       └── index.md
└── .metadata/              # Tracking info
    └── cache-manifest.json
```

**Each cached doc includes:**
```markdown
<!-- 
Generated: 2024-01-18T14:30:00Z
Files analyzed: 47
Key files: Auth/LoginForm.tsx, Common/Button.tsx
Confidence: high
-->

# Components Architecture

## Organization
Components are organized by feature...

## Key Patterns
- React 19 with TypeScript
- Props interfaces end with *Props

## Important Files
- `Auth/` - Authentication components
- `Layout/` - Page layout components
```

**How it works:**
1. **Detection:** Agent reads 5+ files in `src/components/` within one response
2. **Capture:** PostResponse hook detects exploration pattern
3. **Generate:** Summarize findings into `.aiki/arch/structure/src/components/index.md`
4. **Track:** Record analyzed files and their timestamps
5. **Reuse:** PrePrompt checks for cached docs and injects them
6. **Invalidate:** If `Auth/LoginForm.tsx` changes, mark cache stale
7. **Regenerate:** Next exploration auto-updates the doc

**Integration with Skills:**
Skills can reference arch docs for instant context:
```yaml
PrePrompt:
  - shell: |
      if [ -f ".aiki/arch/structure/backend/index.md" ]; then
        cat .aiki/arch/structure/backend/index.md
      fi
```

**Version control:**
- `.gitignore` the `.aiki/arch/` directory (personal cache)
- Track in JJ (change descriptions note when arch docs were updated)
- Teams can optionally commit for shared knowledge

**Success metric:** Agents can answer "how does X work?" instantly from cache instead of exploring 20+ files

---

### Milestone 3: Skills Auto-Activation (2-3 weeks)
**Goal:** Implement automatic guideline injection.

**What gets built:**
- Pattern matching engine (keywords, files, content)
- Skill configuration format (skill-rules.yaml)
- PrePrompt flow implementation
- Example skills (backend, frontend, database)
- CLI commands (aiki skills list/show/create)

**Success metric:** 90%+ of relevant prompts trigger correct skills

---

### Milestone 4: Multi-Stage Pipeline (1-2 weeks)
**Goal:** Zero errors left behind.

**The Problem:**
Agents make changes but don't validate them. Two hours later you discover TypeScript errors, broken builds, failing tests. Errors compound - one mistake becomes ten.

**The Solution:**
Automatic quality checks after every agent response. Catch errors immediately while context is hot.

**What gets built:**
- Session state tracking (edited files, affected repos)
- PostResponse hook that runs builds automatically
- Error parsing (TypeScript, Rust, ESLint, etc.)
- Pattern detection (missing error handling, async without try-catch)
- Gentle reminder system (non-blocking suggestions)
- Integration with provenance (record build status in change descriptions)

**How it works:**

**Stage 1: Track Edits (PostToolUse)**
```yaml
PostToolUse:
  - shell: |
      # Track which files were edited
      echo "$event.file_path" >> .aiki/.session-state/edited-files.log
      echo "$event.timestamp|$event.file_path" >> .aiki/.session-state/edit-log.jsonl
  - let: repo = self.determine_repo_for_file
  - shell: echo "$repo" >> .aiki/.session-state/affected-repos.log
```

**Stage 2: Run Builds (PostResponse)**
```yaml
PostResponse:
  - let: repos = self.get_affected_repos
  - shell: |
      for repo in $repos; do
        echo "🔨 Building $repo..."
        cd $repo && npm run build 2>&1 | tee .aiki/.session-state/build-$repo.log
      done
  - let: error_count = self.count_build_errors
```

**Stage 3: Show Errors**
```yaml
PostResponse:
  - shell: |
      if [ $error_count -gt 0 ] && [ $error_count -lt 5 ]; then
        echo "⚠️  Found $error_count errors:"
        cat .aiki/.session-state/build-*.log | grep "error TS"
      elif [ $error_count -ge 5 ]; then
        echo "⚠️  Found $error_count errors. Consider: aiki agent run error-resolver"
      fi
```

**Stage 4: Pattern Detection**
```yaml
PostResponse:
  - let: risky_patterns = self.detect_risky_patterns
  - shell: |
      if echo "$risky_patterns" | grep -q "try-catch"; then
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo "📋 ERROR HANDLING SELF-CHECK"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo ""
        echo "⚠️  Try-catch blocks detected"
        echo ""
        echo "❓ Did you add Sentry.captureException()?"
        echo "❓ Are error messages user-friendly?"
        echo ""
      fi
```

**Patterns detected:**
- Try-catch blocks without error tracking
- Async functions without error handling
- Database operations without transactions
- API endpoints without input validation
- React hooks with missing dependency arrays

**Session state structure:**
```
.aiki/.session-state/
├── edited-files.log           # List of files edited
├── affected-repos.log         # List of repos touched
├── edit-log.jsonl             # Detailed edit log
├── build-frontend.log         # Build output
├── build-backend.log          # Build output
└── last-check-timestamp       # When last check ran
```

**Expected user experience:**

**Without Pipeline:**
```
Agent: "I've implemented the authentication service"
You: "Great! Now let's add the frontend"
[30 minutes of frontend work]
You: [Manually runs build]
Build: "ERROR: Type 'string | undefined' is not assignable..."
Build: "ERROR: Property 'userId' does not exist..."
You: "Ugh, let me go back and fix the backend first"
```

**With Pipeline:**
```
Agent: "I've implemented the authentication service"
[Pipeline runs automatically]
Pipeline: "⚠️  Found 3 TypeScript errors in backend/src/services/AuthService.ts"
Agent: "Let me fix those immediately"
[Agent fixes errors]
Pipeline: "✅ Build passed"
You: "Great! Now let's add the frontend"
```

**Success metric:** Zero undetected errors left behind

---

### Milestone 5: Dev Docs System (1-2 weeks)
**Goal:** Structured task management.

**What gets built:**
- Task directory structure (.aiki/tasks/)
- Doc management operations (create/update/query)
- Session resumption (auto-load active tasks)
- Task tracking in change descriptions
- CLI commands (aiki tasks list/resume/complete)

**Success metric:** Can resume tasks across sessions without losing context

---

### Milestone 6: Process Management (2 weeks)
**Goal:** Background service integration.

**What gets built:**
- Process action type (start/stop/logs/status)
- Process configuration format (.aiki/processes.yaml)
- Log aggregation and correlation
- Health monitoring
- CLI commands (aiki process start/stop/logs)

**Success metric:** Agents can autonomously debug using logs

---

**Total Timeline:** 10-14 weeks

---

## Dogfooding Strategy

Use aiki/default to build aiki/default:

**After Milestone 2 (Auto Architecture):**
- Cache Aiki's own architecture (flow system, event bus, handlers)
- Agents instantly understand Aiki's structure
- Test with: "How does the flow executor work?"

**After Milestone 3 (Skills):**
- Create `rust-guidelines` skill for Aiki development
- Create `jj-integration` skill for change-centric patterns
- Create `aiki-architecture` skill for flow system patterns

**After Milestone 4 (Dev Docs):**
- Use task system for Milestone 5 and Milestone 6 implementation
- Track progress in tasks.md
- Document decisions in context.md

**After Milestone 5 (Pipeline):**
- Run `cargo check` on PostResponse
- Run `cargo clippy` for linting
- Detect missing error handling

**After Milestone 6 (Process Management):**
- Manage integration test processes
- Debug test failures using log correlation

---

## Configuration Example

Once implemented, here's what configuring aiki/default looks like:

### Skill Configuration
```yaml
# .aiki/skill-rules.yaml
skills:
  backend-guidelines:
    priority: high
    enforcement: suggest
    triggers:
      keywords: ["backend", "API", "controller", "service"]
      file_patterns: ["backend/src/**/*.ts"]
      content_patterns: ["router\\.", "export.*Controller"]
  
  frontend-guidelines:
    priority: high
    enforcement: suggest
    triggers:
      keywords: ["frontend", "component", "React"]
      file_patterns: ["src/components/**/*.tsx"]
      content_patterns: ["import.*React"]
```

### Process Configuration
```yaml
# .aiki/processes.yaml
processes:
  frontend:
    command: "npm run dev"
    cwd: "./frontend"
    healthcheck:
      url: "http://localhost:3000/health"
    logs:
      stdout: ".aiki/logs/frontend.out.log"
      stderr: ".aiki/logs/frontend.err.log"
  
  backend:
    command: "npm start"
    cwd: "./backend"
    healthcheck:
      url: "http://localhost:3001/api/health"
    logs:
      stdout: ".aiki/logs/backend.out.log"
      stderr: ".aiki/logs/backend.err.log"
```

### Flow Structure
```yaml
# .aiki/flows/aiki/default/flow.yaml
name: "aiki/default"
version: "1"

includes:
  - aiki/quick-lint
  - aiki/build-check

PrePrompt:
  - let: skills = self.analyze_and_activate_skills
  - log: "Skills activated: $skills"

PostChange:
  - flow: quick-lint

PostResponse:
  - flow: build-check
  - let: errors = self.detect_patterns
  - shell: |
      if [ -n "$errors" ]; then
        echo "⚠️  Reminders: $errors"
      fi

PreCommit:
  - flow: comprehensive-check
```

---

## Why These Patterns Work

### Inspiration: 6 Months of Production Use

These four patterns come from an engineer who solo-rewrote 300k+ LOC using Claude Code. Key insights:

**Context is King:**
- AI quality is proportional to context quality
- Manual context injection is tedious and forgotten
- Automatic injection ensures consistency

**Memory Across Sessions:**
- Large features take multiple sessions
- Context compaction loses the plot
- Structured docs maintain continuity

**Immediate Feedback:**
- One error today becomes ten tomorrow
- Catching errors while context is hot prevents compounding
- Gentle reminders prevent repetition

**Specialized Tools:**
- Quick checks during iteration
- Thorough checks before commits
- Right tool for the right moment

**Observable Systems:**
- Multi-service debugging requires log visibility
- Correlating errors with changes aids debugging
- Agents can debug autonomously with log access

---

## Aiki-Specific Advantages

Why Aiki is uniquely suited for these patterns:

### Change-Centric Model
- Stable change_ids persist across rewrites
- Skills can reference past changes by change_id
- Task metadata survives rebases

### JJ Integration
- Query past patterns: `jj log -r 'description("pattern=controller")'`
- Find task changes: `jj log -r 'description("task=user-auth")'`
- Time-travel debugging with change history

### Provenance Tracking
- Record which skills were activated
- Track build status in change descriptions
- Correlate process errors with changes
- Analyze patterns over time

### Type-Safe Flows
- Rust's guarantees throughout
- Structured error types
- Native functions for performance

---

## Commands (Future)

Once implemented:

```bash
# Skills
aiki skills list                    # Show available skills
aiki skills show backend-guidelines # Show skill details
aiki skills create my-skill         # Create new skill

# Tasks
aiki tasks create feature-name      # Create task docs
aiki tasks resume feature-name      # Resume task
aiki tasks show feature-name        # Show task status

# Processes
aiki process start backend          # Start service
aiki process logs backend --errors  # Show error logs
aiki process status                 # All services status

# Flows
aiki flows list                     # Available flows
aiki flows install aiki/default     # Install this flow
aiki flows show aiki/default        # Show flow details
```

---

## Directory Structure (Future)

```
.aiki/
├── flows/
│   └── aiki/
│       └── default/              # This flow
├── skills/
│   ├── skill-rules.yaml          # Skill configuration
│   ├── backend-guidelines/
│   ├── frontend-guidelines/
│   └── database-verification/
├── arch/                         # Auto architecture docs
│   ├── structure/                # Mirrors codebase
│   │   ├── src/
│   │   │   ├── components/
│   │   │   │   └── index.md
│   │   │   └── services/
│   │   │       └── index.md
│   │   └── backend/
│   │       └── index.md
│   └── .metadata/
│       └── cache-manifest.json
├── tasks/
│   ├── .active                   # Current task
│   └── user-authentication/
│       ├── plan.md
│       ├── context.md
│       └── tasks.md
├── logs/
│   ├── frontend.out.log
│   └── backend.err.log
├── .session-state/
│   ├── edited-files.log
│   └── affected-repos.log
└── processes.yaml                # Process configuration
```

---

## Getting Started (After Implementation)

1. **Install the flow:**
   ```bash
   aiki flows install aiki/default
   ```

2. **Create your first skill:**
   ```bash
   aiki skills create my-project-patterns
   ```

3. **Configure processes (if multi-service):**
   ```yaml
   # .aiki/processes.yaml
   processes:
     my-service:
       command: "npm start"
       cwd: "./service"
   ```

4. **Start working:**
   ```bash
   aiki tasks create my-feature
   # Files created in .aiki/tasks/my-feature/
   ```

5. **Let the flow work:**
   - Skills auto-activate based on what you're doing
   - Builds run automatically after responses
   - Tasks track progress across sessions
   - Processes are monitored and debuggable

---

## Status

**Current:** Vision document  
**Milestone 1 Start:** TBD  
**Expected Completion:** 8-12 weeks from Milestone 1 start

This document will be updated as implementation progresses.

---

## Related Documentation

- `ops/ROADMAP.md` - Overall Aiki roadmap
- `ops/phase-5.md` - Current phase (flow system)
- `cli/src/flows/core/flow.yaml` - Minimal aiki/core flow
- `CLAUDE.md` - Development guidelines for Aiki itself
