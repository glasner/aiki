# Aiki Twin: Personalized Human Review Impersonation

## Overview

Aiki Twin is a personal AI agent that learns from your individual interactions with AI agents (Claude Code, Cursor, etc.) and can impersonate your human review style when you're unavailable. It acts as your proxy in the review loop, applying your standards, preferences, and patterns to AI-generated code.

**Core Insight:** Every human reviewer has patterns — code style preferences, security concerns they always catch, types of feedback they consistently give. Aiki Twin learns these patterns from your real review history and can apply them automatically.

---

## Problem

### The Review Bottleneck

Autonomous AI agents can generate code faster than humans can review it. This creates a fundamental bottleneck:

- **Async work suffers** — AI generates code at 2am, sits idle until human reviews at 9am
- **Multiple agents blocked** — 5 agents waiting on 1 human reviewer
- **Review fatigue** — Humans start rubber-stamping to keep up
- **Inconsistent standards** — Rushed reviews miss things the reviewer would normally catch

### Human Review Has Personal Patterns

Every reviewer has a fingerprint:
- **Style preferences** — "Always use early returns", "Never use `any` in TypeScript"
- **Security awareness** — "Always check for SQL injection", "Validate all user input"
- **Architecture opinions** — "Services should be stateless", "Prefer composition over inheritance"
- **Common feedback** — "Add error handling here", "This needs a test", "Document the why"

These patterns are consistent but currently can't be automated because they're locked in the reviewer's head.

---

## Solution

### Build a Personal Twin from Real Interactions

Capture every human-AI interaction in a repository:
- Code reviews (approve, reject, request changes)
- Inline comments and suggestions
- Chat conversations about code decisions
- Edit patterns when fixing AI code
- Reasoning explanations for rejections

Train a personalized model (or prompt profile) that captures:
- What you approve without comment
- What triggers you to request changes
- How you phrase your feedback
- What you catch that agents miss

### Deploy the Twin as a Review Proxy

When the human is unavailable, the twin:
1. Receives the AI-generated change
2. Reviews it using the human's learned patterns
3. Either approves (confident match to approval patterns) or queues for human review (uncertainty)
4. Provides feedback in the human's voice

**Conservative by Default:** The twin should have high precision — it only auto-approves when very confident. When uncertain, it queues for human review with a summary of concerns.

---

## Data Model

### Interaction Corpus

Every interaction between a human and AI agent is recorded:

```rust
struct TwinInteraction {
    interaction_id: String,
    timestamp: DateTime<Utc>,
    interaction_type: InteractionType,

    // The AI's output being reviewed
    ai_change: ChangeContext,

    // The human's response
    human_response: HumanResponse,

    // Derived patterns
    extracted_patterns: Vec<Pattern>,
}

enum InteractionType {
    CodeReview,          // Approve/reject/comment on change
    InlineEdit,          // Human edits AI-generated code
    ChatCorrection,      // "No, do X instead of Y"
    ExplicitInstruction, // "I prefer X style"
    ImplicitApproval,    // No comment = tacit approval
}

struct ChangeContext {
    change_id: ChangeId,       // JJ change
    files_modified: Vec<PathBuf>,
    diff: String,
    agent: AgentType,
    task_context: Option<String>,
}

struct HumanResponse {
    verdict: ReviewVerdict,
    comments: Vec<ReviewComment>,
    edits: Vec<HumanEdit>,       // If human modified the code
    time_spent: Duration,        // How long human spent reviewing
}

enum ReviewVerdict {
    Approved,
    ApprovedWithComments,
    RequestedChanges,
    Rejected,
}
```

### Learned Patterns

Patterns extracted from the interaction corpus:

```rust
struct Pattern {
    pattern_id: String,
    category: PatternCategory,
    confidence: f64,           // How confident are we this is a real pattern
    frequency: u32,            // How often this pattern appears
    examples: Vec<String>,     // Example instances

    // The pattern itself (depends on category)
    rule: PatternRule,
}

enum PatternCategory {
    StylePreference,     // "Use early returns"
    SecurityConcern,     // "Validate user input"
    ArchitectureRule,    // "Services should be stateless"
    TestingRequirement,  // "All public functions need tests"
    DocumentationStandard, // "Document the why, not the what"
    ErrorHandling,       // "Always handle errors explicitly"
    NamingConvention,    // "Use camelCase for functions"
}

enum PatternRule {
    MustContain(String),     // Code must contain this pattern
    MustNotContain(String),  // Code must not contain this pattern
    PreferredAlternative {
        original: String,
        preferred: String,
    },
    RequiresCompanion {      // If X exists, Y must also exist
        trigger: String,
        requirement: String,
    },
    ContextualRule {         // In context X, rule Y applies
        context: String,
        rule: Box<PatternRule>,
    },
}
```

### Twin Profile

The aggregated profile that represents the human:

```rust
struct TwinProfile {
    owner_id: String,
    created_at: DateTime<Utc>,
    last_updated: DateTime<Utc>,

    // Learned patterns by category
    patterns: HashMap<PatternCategory, Vec<Pattern>>,

    // Review behavior model
    review_style: ReviewStyle,

    // Confidence thresholds
    auto_approve_threshold: f64,  // Confidence to auto-approve
    flag_threshold: f64,          // Confidence to flag for human

    // Statistics
    total_interactions: u32,
    accuracy_score: f64,          // How well twin matches human decisions
}

struct ReviewStyle {
    verbosity: Verbosity,         // Terse, normal, detailed
    tone: Tone,                   // Direct, gentle, encouraging
    focus_areas: Vec<PatternCategory>,  // What they care most about
    response_patterns: Vec<String>,     // Common phrases they use
}
```

---

## How the Twin Learns

### 1. Passive Learning from Reviews

Every code review interaction is captured:

```yaml
PreReview:
  - capture_review_context:
      change_id: $change_id
      files: $modified_files
      diff: $diff
      agent: $agent_type

PostReview:
  - capture_review_outcome:
      verdict: $review_verdict
      comments: $review_comments
      time_spent: $review_duration

  - extract_patterns:
      interaction: $captured_interaction
      existing_patterns: $twin_profile.patterns
```

### 2. Explicit Training

Human can explicitly teach the twin:

```bash
# Add a rule directly
aiki twin teach "Always use const instead of let for immutable values"

# Show example of good/bad code
aiki twin example bad "any" "Don't use 'any' in TypeScript"
aiki twin example good "unknown" "Use 'unknown' for truly unknown types"

# Import preferences from linter configs
aiki twin import-rules .eslintrc.json
aiki twin import-rules rustfmt.toml
```

### 3. Feedback Loop

When the twin makes a decision human would override:

```bash
# Human reviews twin's auto-approval
$ aiki twin review abc123

Twin auto-approved change abc123:
  Files: src/auth.rs
  Reason: Matches pattern "simple-refactor" (confidence: 0.92)

Do you agree? [y/n/comment]: n
What should the twin have caught?
> Missing error handling for token expiration

# Twin learns from correction
Pattern extracted: RequiresCompanion {
    trigger: "token validation",
    requirement: "expiration handling"
}
```

### 4. Retroactive Learning

Analyze past Git history to bootstrap the twin:

```bash
$ aiki twin bootstrap --since 2024-01-01

Analyzing 847 commits...
Found 234 with AI attribution
Analyzed 189 subsequent human edits

Extracted patterns:
- 12 style preferences
- 8 security concerns
- 5 testing requirements
- 3 documentation standards

Twin profile created with 67% initial confidence.
Run `aiki twin validate` to improve accuracy.
```

---

## How the Twin Reviews

### Review Pipeline

```
┌─────────────────┐
│ AI Change       │
│ (from agent)    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Pattern Matcher │  Check against all learned patterns
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Confidence      │  Calculate confidence in approval
│ Scorer          │
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
┌───────┐ ┌───────────────┐
│ HIGH  │ │ LOW           │
│ conf  │ │ confidence    │
└───┬───┘ └───────┬───────┘
    │             │
    ▼             ▼
┌───────────┐ ┌───────────────┐
│ Auto-     │ │ Queue for     │
│ Approve   │ │ Human Review  │
└───────────┘ └───────────────┘
```

### Review Output

```bash
$ aiki twin review abc123

Change abc123: src/api/users.rs (+45, -12)
Agent: claude-code
Task: "Add user deletion endpoint"

Pattern Analysis:
  ✓ Style: Early returns used (0.95)
  ✓ Error handling: All paths covered (0.88)
  ⚠ Security: No rate limiting on delete (0.72)
  ✓ Testing: Unit test added (0.91)

Twin Confidence: 0.76 (below auto-approve threshold: 0.85)

Recommendation: QUEUE FOR HUMAN REVIEW
Reason: Security concern about rate limiting needs human judgment

Suggested comment (your style):
  "This looks good, but we should add rate limiting to the delete
   endpoint to prevent abuse. See src/api/posts.rs:45 for our
   standard rate limiting pattern."
```

---

## Privacy & Security

### Data Stays Local

All twin data is stored locally:
- `~/.aiki/twin/profile.toml` — Twin profile and patterns
- `~/.aiki/twin/interactions/` — Interaction history
- `~/.aiki/twin/feedback/` — Human corrections

**No cloud sync by default.** Optional encrypted sync for multi-device use.

### Pattern Anonymization

When sharing patterns (e.g., team templates):
- Code examples are anonymized
- File paths are genericized
- Only pattern rules are shared, not raw interactions

### Access Control

**Prereq:** [user-settings](user-settings.md) — twin config lives in the `twin` section of `~/.aiki/config.yaml`.

```yaml
# ~/.aiki/config.yaml — twin section
twin:
  permissions:
    auto_approve: true           # Allow twin to auto-approve
    auto_comment: true           # Allow twin to add comments
    auto_reject: false           # Never auto-reject (always human)
    max_auto_approve_per_hour: 10  # Rate limit
    require_mfa_for_settings: true
```

---

## Integration Points

### With JJ Provenance

Twin decisions are recorded in change descriptions:

```
[aiki]
agent=claude-code
session=abc123
tool=Edit
reviewed_by=twin
twin_confidence=0.89
twin_patterns_matched=["early-return", "error-handling", "unit-test"]
review_verdict=approved
[/aiki]
```

### With Flows

```yaml
PostToolUse:
  # After every AI edit, run twin review
  - if: $event.tool matches "Edit|Write"
    then:
      - twin_review:
          change_id: $current_change_id
          auto_approve_threshold: 0.85
          on_low_confidence: queue_for_human
```

### With CI/CD

```yaml
# .github/workflows/aiki-twin.yml
on:
  pull_request:
    types: [opened, synchronize]

jobs:
  twin-review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run Aiki Twin Review
        run: |
          aiki twin review --ci --output-format json > twin-review.json
      - name: Comment on PR
        uses: actions/github-script@v7
        with:
          script: |
            const review = require('./twin-review.json')
            // Post twin's review as PR comment
```

---

## Commands

```bash
# Profile management
aiki twin init              # Create twin profile
aiki twin status            # Show twin profile summary
aiki twin patterns          # List learned patterns
aiki twin stats             # Show accuracy statistics

# Teaching
aiki twin teach <rule>      # Add explicit rule
aiki twin example <good|bad> <code> <reason>
aiki twin import-rules <file>  # Import from linter config
aiki twin bootstrap         # Learn from git history

# Review
aiki twin review <change_id>     # Review specific change
aiki twin review --pending       # Review all pending changes
aiki twin explain <change_id>    # Explain why twin made a decision

# Feedback
aiki twin correct <change_id>    # Correct a twin decision
aiki twin validate               # Interactive validation session

# Export/Import
aiki twin export --format=json   # Export profile
aiki twin import profile.json    # Import profile
aiki twin share <pattern_id>     # Share pattern to team
```

---

## Technical Components

| Component | Complexity | Priority |
|-----------|------------|----------|
| Interaction capture (flows) | Medium | High |
| Pattern extraction engine | High | High |
| Twin profile storage | Low | High |
| Review pipeline | Medium | High |
| Confidence scoring | High | High |
| Feedback loop | Medium | Medium |
| Bootstrap from history | High | Medium |
| CLI commands | Medium | Medium |
| CI/CD integration | Low | Low |
| Team pattern sharing | Medium | Low |

---

## Success Criteria

- ✅ Twin learns from 100+ interactions with >80% pattern accuracy
- ✅ Auto-approve decisions match human decisions >95% of the time
- ✅ False positive rate <5% (twin approves something human would reject)
- ✅ Reduces human review time by >50% through auto-approval
- ✅ Feedback loop improves accuracy over time
- ✅ No private data leaves local machine without explicit consent
- ✅ Twin explains its decisions in human's voice/style

---

## Open Questions

1. **Local vs. cloud model?**
   - Local: Privacy, no latency, works offline
   - Cloud: More powerful models, shared learning
   - Hybrid: Local profile, cloud inference?

2. **Per-repo or global twin?**
   - Per-repo: Different patterns for different codebases
   - Global: Consistent personal style across projects
   - Layered: Global base + repo-specific overrides?

3. **Team twins?**
   - Should teams share a combined twin?
   - How to merge individual preferences?
   - Conflict resolution when patterns disagree?

4. **Twin versioning?**
   - How to handle twin evolution over time?
   - Rollback if twin "learns" bad patterns?
   - A/B testing of twin versions?

5. **Accountability?**
   - Who's responsible for twin-approved code?
   - Audit trail requirements?
   - Regulatory compliance for auto-review?

---

## Future Enhancements

### Multi-Modal Learning

Learn from more than just code reviews:
- Slack/chat conversations about code
- PR discussions and debates
- Documentation preferences
- Meeting notes mentioning code decisions

### Twin Collaboration

When human is unavailable:
- Twin A reviews change from Agent 1
- Twin B (different human) validates Twin A's decision
- Only auto-approve if both twins agree

### Proactive Suggestions

Twin notices patterns and suggests:
- "You often add logging here, should I suggest it to agents?"
- "This is the 5th time you've added rate limiting, want to make it a rule?"
- "Your recent reviews suggest you now prefer X over Y, update pattern?"

### Style Transfer

Apply one human's patterns to another's codebase:
- "Review this code as if you were Senior Dev Alice"
- Mentorship: junior devs learn patterns from senior twin
- Consistency: all team code follows lead architect's patterns

---

## Related Documentation

- `ops/ROADMAP.md` - Phase 13: Autonomous Review Flow (foundation for twin)
- `ops/later/WORKSPACE_AUTOMATION.md` - Multi-agent workspaces
- `ops/review.md` - Current review system design
- `CLAUDE.md` - JJ change provenance (twin decisions recorded here)
