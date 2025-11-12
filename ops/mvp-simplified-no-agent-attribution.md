# Aiki MVP: Simplified Specification (No Agent Attribution)

## Core Value Proposition

**Pre-compute code reviews in background using JJ's operation log, return instant results at commit time.**

**Time savings: 2-5 seconds per commit → 10-50ms**

Agent attribution is deferred to Phase 2. MVP focuses purely on making reviews instant.

---

## I. What We're Building (MVP Scope)

### Three Components

```
┌─────────────────────────────────────────────────────────────┐
│  1. JJ Operation Poller                                      │
│     • Polls operation log every 300ms                        │
│     • Detects new operations (any user/agent)                │
│     • Extracts changed files from operations                 │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│  2. Background Review Aiki                                 │
│     • Reviews changed files automatically                    │
│     • Caches results by change ID + commit ID               │
│     • Runs static analysis + type checking + AI review      │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│  3. Commit Interceptor                                       │
│     • Pre-commit hook (intercepts git commit)               │
│     • Returns cached review results (~10ms)                 │
│     • Same interface as original doc (options menu)         │
└─────────────────────────────────────────────────────────────┘
```

### What We're NOT Building (Deferred)

❌ **Agent detection/attribution** - All operations treated equally
❌ **Agent-specific feedback** - Generic feedback only
❌ **Agent trust scores** - Not needed for MVP
❌ **Process monitoring** - No attempt to detect Cursor vs Copilot
❌ **Agent analytics** - No tracking of which agent made what

**Rationale:** These are analytics/provenance features. They don't affect the core review loop or time savings.

---

## II. Simplified Architecture

```
┌──────────────────────────────────────────────────────────────┐
│         Developer + AI Agent (Any Tool)                      │
│         Working on code                                      │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Edits files
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Jujutsu Repository                                   │
│         • Working copy = commit @                            │
│         • Operation log records every change                 │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Aiki polls (300ms)
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Operation Poller                                     │
│         • Poll JJ operation log                              │
│         • Detect new operations                              │
│         • Extract changed files                              │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ New operations detected
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Review Queue                                         │
│         • Queue changed files for review                     │
│         • Priority: files about to be committed              │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Process queue
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Review Workers (Parallel)                            │
│         • Static analysis (clippy, eslint)                   │
│         • Type checking (rust-analyzer, tsc)                 │
│         • AI review (GPT-4/Claude)                           │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Review results
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Review Cache                                         │
│         • Key: (change_id, commit_id)                        │
│         • Value: ReviewResult                                │
│         • Invalidation: Check commit_id matches              │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ (Background loop continues)
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         git commit -m "..."                                  │
│         (Developer or AI attempts commit)                    │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Pre-commit hook triggered
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Commit Interceptor                                   │
│         • Lookup cached review (~10ms)                       │
│         • If stale: Quick re-review (~100ms)                 │
│         • Return results to committer                        │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Review results
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Display Results (Same as Original Doc)               │
│                                                              │
│  Review Status: FAILED (2 critical, 1 warning)              │
│                                                              │
│  Options:                                                    │
│  [r] Read and fix                                           │
│  [e] Escalate to human                                      │
│  [i] Ignore and commit                                      │
│  [c] Cancel                                                 │
└──────────────────────────────────────────────────────────────┘
```

---

## III. Implementation Details

### A. JJ Operation Poller

```rust
use jj_lib::repo::ReadonlyRepo;
use jj_lib::op_store::OperationId;
use std::time::Duration;

pub struct OperationPoller {
    repo_path: PathBuf,
    last_op_id: Option<OperationId>,
    poll_interval: Duration,
}

impl OperationPoller {
    pub fn new(repo_path: PathBuf) -> Self {
        Self {
            repo_path,
            last_op_id: None,
            poll_interval: Duration::from_millis(300),
        }
    }
    
    pub async fn run(&mut self) {
        loop {
            if let Ok(new_ops) = self.poll_once().await {
                for op in new_ops {
                    self.handle_operation(op).await;
                }
            }
            
            tokio::time::sleep(self.poll_interval).await;
        }
    }
    
    async fn poll_once(&mut self) -> Result<Vec<Operation>> {
        // Load repo at latest operation
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        let repo = workspace.repo_loader().load_at_head()?;
        
        let current_op_id = repo.op_id().clone();
        
        // First poll - just record current state
        if self.last_op_id.is_none() {
            self.last_op_id = Some(current_op_id);
            return Ok(vec![]);
        }
        
        // No change
        if current_op_id == *self.last_op_id.as_ref().unwrap() {
            return Ok(vec![]);
        }
        
        // Get operations since last poll
        let ops = self.get_ops_since(&self.last_op_id.as_ref().unwrap(), &current_op_id)?;
        self.last_op_id = Some(current_op_id);
        
        Ok(ops)
    }
    
    async fn handle_operation(&self, op: Operation) {
        // Get changed files
        let changed_files = self.extract_changed_files(&op)?;
        
        // Queue for review
        for file in changed_files {
            REVIEW_QUEUE.push(ReviewTask {
                file,
                change_id: self.get_change_id_for_file(&file)?,
                priority: Priority::Normal,
            }).await;
        }
    }
    
    fn extract_changed_files(&self, op: &Operation) -> Result<Vec<PathBuf>> {
        // Load repo at this operation
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        let repo = workspace.repo_loader().load_at(op)?;
        
        // Get working copy commit
        let view = op.view()?;
        let wc_commit_id = view.get_wc_commit_id(&WorkspaceId::default())?;
        let commit = repo.store().get_commit(wc_commit_id)?;
        
        // Get parent to compute diff
        if commit.parent_ids().is_empty() {
            return Ok(vec![]); // Root commit
        }
        
        let parent_id = &commit.parent_ids()[0];
        let parent = repo.store().get_commit(parent_id)?;
        
        // Compute diff
        let diff = parent.tree()?.diff(
            commit.tree()?,
            &repo.matcher_from_values(&[])?
        );
        
        // Extract paths
        let mut files = Vec::new();
        for (path, _) in diff {
            files.push(path);
        }
        
        Ok(files)
    }
}
```

### B. Review Cache (Simplified)

```rust
use jj_lib::backend::{ChangeId, CommitId};

pub struct ReviewCache {
    // Cache key: (change_id, commit_id)
    // We need both because:
    // - change_id: stable across amendments
    // - commit_id: validates exact revision
    cache: RwLock<HashMap<(ChangeId, CommitId), ReviewResult>>,
}

impl ReviewCache {
    pub async fn get_or_review(
        &self,
        change_id: &ChangeId,
        commit_id: &CommitId,
    ) -> Result<ReviewResult> {
        // 1. Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(cached) = cache.get(&(*change_id, *commit_id)) {
                return Ok(cached.clone()); // Exact match - cache hit!
            }
        }
        
        // 2. Cache miss - run review
        let result = self.run_review(commit_id).await?;
        
        // 3. Store in cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert((*change_id, *commit_id), result.clone());
        }
        
        Ok(result)
    }
    
    async fn run_review(&self, commit_id: &CommitId) -> Result<ReviewResult> {
        // Load commit
        let repo = self.load_repo()?;
        let commit = repo.store().get_commit(commit_id)?;
        let tree = commit.tree()?;
        
        // Run reviews in parallel
        let (static_result, type_result, ai_result) = tokio::join!(
            self.static_analysis(&tree),
            self.type_check(&tree),
            self.ai_review(&tree),
        );
        
        // Merge results
        let mut result = ReviewResult::new();
        result.merge(static_result?);
        result.merge(type_result?);
        result.merge(ai_result?);
        
        Ok(result)
    }
}
```

### C. Commit Interceptor (Same as Original)

```rust
pub struct CommitInterceptor {
    cache: Arc<ReviewCache>,
    repo_path: PathBuf,
}

impl CommitInterceptor {
    pub async fn intercept() -> Result<()> {
        // 1. Load current repo state
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        let repo = workspace.repo_loader().load_at_head()?;
        
        // 2. Get working copy commit
        let view = repo.view();
        let wc_commit_id = view.get_wc_commit_id(&WorkspaceId::default())?;
        let commit = repo.store().get_commit(wc_commit_id)?;
        
        // 3. Get change ID
        let change_id = commit.change_id();
        
        // 4. Lookup cached review (~10ms)
        let result = self.cache.get_or_review(change_id, wc_commit_id).await?;
        
        // 5. Display results (SAME AS ORIGINAL DOC)
        self.display_results(&result);
        
        // 6. Wait for user/agent response
        let choice = self.read_choice()?;
        
        match choice {
            Choice::ReadAndFix => {
                // Output structured feedback for agent/human
                self.output_feedback(&result);
                std::process::exit(1); // Block commit
            }
            Choice::Escalate => {
                println!("Escalating to developer...");
                std::process::exit(1);
            }
            Choice::Ignore => {
                println!("Ignoring issues, committing...");
                std::process::exit(0); // Allow commit
            }
            Choice::Cancel => {
                std::process::exit(1);
            }
        }
    }
    
    fn display_results(&self, result: &ReviewResult) {
        println!("\nAiki Review Results:");
        
        for issue in &result.issues {
            let icon = match issue.severity {
                Severity::Error => "❌",
                Severity::Warning => "⚠️",
                Severity::Info => "ℹ️",
            };
            
            println!("  {} {}:{} - {}", 
                icon,
                issue.file.display(),
                issue.line,
                issue.message
            );
        }
        
        if result.has_critical() {
            println!("\nReview Status: FAILED");
        } else {
            println!("\nReview Status: PASSED");
        }
    }
}
```

---

## IV. What Gets Simpler Without Agent Attribution

### Removed Complexity

**No longer need:**
```rust
// ❌ Agent detection
fn infer_agent(&self, op: &Operation) -> Option<Agent> { ... }

// ❌ Process monitoring
fn detect_agent_from_pid(&self, pid: u32) -> Option<Agent> { ... }

// ❌ Agent-specific feedback formatting
fn format_for_agent(&self, agent: &Agent) -> String { ... }

// ❌ Agent metadata injection
std::env::set_var("JJ_USER", "cursor-agent");

// ❌ Agent trust scoring
fn calculate_trust_score(&self, agent: &Agent) -> f64 { ... }
```

**Simplified to:**
```rust
// ✅ Just detect that change happened
fn handle_operation(&self, op: &Operation) {
    let files = self.extract_changed_files(&op)?;
    for file in files {
        self.queue_review(file).await;
    }
}
```

### Simpler Data Structures

**Before (with agent attribution):**
```rust
struct ReviewResult {
    issues: Vec<Issue>,
    agent: Option<Agent>,
    timestamp: Instant,
    iteration_count: usize,
    agent_trust_score: f64,
}

struct CachedReview {
    change_id: ChangeId,
    commit_id: CommitId,
    result: ReviewResult,
    reviewed_by_agent: Option<Agent>,
}
```

**After (without agent attribution):**
```rust
struct ReviewResult {
    issues: Vec<Issue>,
    timestamp: Instant,
}

struct CachedReview {
    change_id: ChangeId,
    commit_id: CommitId,
    result: ReviewResult,
}
```

### Simpler Configuration

**Before:**
```yaml
agents:
  cursor:
    trust_score: 0.85
    feedback_format: concise
  copilot:
    trust_score: 0.78
    feedback_format: structured
    
  detection:
    method: [env_vars, process_monitor, heuristics]
    fallback: manual_tag
```

**After:**
```yaml
review:
  poll_interval_ms: 300
  cache_size_mb: 100
```

---

## V. MVP User Experience

### Setup (One Time)

```bash
# 1. Install JJ
brew install jj-cli

# 2. Install Aiki
brew install aiki

# 3. Initialize in repository
cd my-repo
jj git init --colocate
aiki init

# Done! Both git and jj commands work
```

### Daily Usage

**Developer/AI edits code (Aiki works silently in background):**

```bash
# Work normally
vim auth.py
# ... Aiki detects change, reviews in background ...

# Commit when ready
git commit -m "Add authentication"

# Aiki intercepts (returns cached result in ~10ms)
Aiki Review Results:
  ❌ auth.py:45 - Missing return type annotation
  ❌ auth.py:67 - SQL injection risk

Review Status: FAILED

Options:
  [r] Read and fix
  [e] Escalate  
  [i] Ignore
  [c] Cancel

# AI reads feedback, fixes, tries again
# ... Aiki reviews amendments in background ...

git commit -m "Fix issues"

# Aiki intercepts (cached result, ~10ms)
Aiki Review Results:
  ✅ All checks passed

Review Status: PASSED
Committing...

[main abc1234] Fix issues
```

**No agent detection, no agent-specific behavior. Same experience regardless of who/what is making changes.**

---

## VI. MVP Success Metrics

### Technical Metrics

- [ ] **Operation polling works reliably** (99%+ uptime)
- [ ] **Cache hit rate >80%** during typical development
- [ ] **Commit intercept latency <50ms** (P95)
- [ ] **Background CPU usage <5%**
- [ ] **Memory usage <200MB**

### Product Metrics

- [ ] **Developers report catching bugs** they'd have missed
- [ ] **Perceived review speed improvement** (2-5 sec → instant)
- [ ] **False positive rate <15%** (issues flagged incorrectly)
- [ ] **Zero Git workflow disruption** (git commands work normally)

### User Experience Metrics

- [ ] **Setup time <5 minutes** (install + init)
- [ ] **Zero config required** (works out of box)
- [ ] **Works with any AI tool** (Cursor, Copilot, etc)

---

## VII. What We Deliver in MVP

### Week 1-2: JJ Integration

```
✅ Load JJ workspace
✅ Poll operation log (300ms)
✅ Extract changed files from operations
✅ Basic logging/debugging
```

### Week 3-4: Background Review

```
✅ Review queue
✅ Static analysis worker (clippy/eslint)
✅ Type checking worker (tsc/rust-analyzer)
✅ Review result aggregation
```

### Week 5-6: Review Cache

```
✅ Cache by (change_id, commit_id)
✅ Cache lookup
✅ Cache invalidation
✅ Disk persistence
```

### Week 7-8: Commit Interceptor

```
✅ Pre-commit hook registration
✅ Cached result lookup
✅ Result display (same as original doc)
✅ Option menu + input handling
```

### Week 9-10: AI Review + Polish

```
✅ AI review worker (GPT-4/Claude)
✅ Structured feedback output
✅ Error handling + graceful degradation
✅ Performance optimization
```

### Week 11-12: Testing + Documentation

```
✅ Integration tests
✅ Performance benchmarks
✅ User documentation
✅ Installation guide
```

**Total: 12 weeks to production-ready MVP**

---

## VIII. What We Defer to Phase 2

### Provenance & Analytics

- Agent attribution (which AI made what change)
- Agent performance metrics
- Trust scoring
- Historical analysis

**Why defer:** Not needed for core value loop. Adds complexity without improving time savings.

### Advanced Features

- Multi-agent coordination
- PR review for cloud agents
- Team-wide coordination
- Enterprise compliance

**Why defer:** Build on proven MVP first. Validate Phase 1 before expanding.

---

## IX. Key Decisions

### Decision: JJ Required for MVP

**Users must install JJ and run `jj git init --colocate`**

**Rationale:**
- Makes implementation 10x simpler
- Colocated mode = Git works normally (zero disruption)
- JJ is one command to install
- Can add Git fallback in Phase 2 if needed

**Trade-off:**
- Smaller initial market (must convince users to try JJ)
- But: targets early adopters (Cursor power users)
- And: JJ is easy sell (better UX than Git)

### Decision: No Agent Detection

**All operations treated equally regardless of source**

**Rationale:**
- Significantly simpler implementation
- Core value (time savings) works without it
- Can add in Phase 2 with zero user impact

**Trade-off:**
- No agent-specific analytics
- But: not needed for core loop

### Decision: 300ms Poll Interval

**Poll JJ operation log every 300ms**

**Rationale:**
- Fast enough (sub-second background review)
- Low CPU overhead
- Simple implementation

**Trade-off:**
- Not real-time (but doesn't need to be)
- Can optimize later if needed

---

## X. Minimal Viable Setup

### Files to Create

```
aiki/
├── src/
│   ├── main.rs              # Entry point
│   ├── poller.rs            # Operation log poller
│   ├── review.rs            # Review orchestration
│   ├── cache.rs             # Review cache
│   ├── interceptor.rs       # Commit hook
│   └── workers/
│       ├── static.rs        # Static analysis
│       ├── types.rs         # Type checking
│       └── ai.rs            # AI review
├── Cargo.toml               # Dependencies
└── README.md                # User docs

scripts/
└── pre-commit               # Git hook
```

### Dependencies (Cargo.toml)

```toml
[dependencies]
jj-lib = "0.32"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
```

### Installation Script

```bash
#!/bin/bash
# install.sh

# Check if JJ installed
if ! command -v jj &> /dev/null; then
    echo "Installing Jujutsu..."
    brew install jj-cli
fi

# Build Aiki
cargo build --release

# Install binary
cp target/release/aiki /usr/local/bin/

# Initialize in current repo
jj git init --colocate
aiki init

echo "✓ Aiki installed and initialized"
```

---

## XI. Open Questions

1. **How do we handle repos that already use JJ?**
   - Just run `aiki init` (skip `jj git init`)
   - Detect if JJ already initialized

2. **What if user has uncommitted changes when running `aiki init`?**
   - JJ automatically commits working copy
   - Should be fine, but need to test

3. **Do we need a daemon or can poller run in terminal?**
   - Start simple: terminal process
   - Add daemon mode in Phase 2 if needed

4. **How do we persist cache across restarts?**
   - Disk cache in `.aiki/cache/`
   - LRU eviction policy

5. **What happens if operation log grows very large?**
   - JJ handles this (designed for Google scale)
   - We only look at recent operations

---

## XII. Success Criteria for MVP

**Technical Success:**
- ✅ Poller detects operations reliably
- ✅ Reviews cached and reused
- ✅ Commit intercept returns results in <50ms
- ✅ Works with Cursor, Copilot, human edits

**User Success:**
- ✅ Setup in <5 minutes
- ✅ Works without configuration
- ✅ Catches real bugs pre-commit
- ✅ Feels instant at commit time

**Market Success:**
- ✅ 10 users actively using daily
- ✅ Positive feedback on time savings
- ✅ No major bugs or reliability issues
- ✅ Users recommend to others

**If these pass: Move to Phase 2 (agent attribution + coordination)**

---

## XIII. Implementation Priority

### Critical Path (Must Work)

1. JJ workspace loading
2. Operation log polling
3. Changed file extraction
4. Review execution
5. Review caching
6. Commit interception
7. Result display

**These must work reliably or MVP fails**

### Important (Should Work)

8. Cache persistence
9. Error handling
10. Performance optimization
11. User documentation

**These improve UX but aren't blocking**

### Nice to Have (Can Defer)

12. Detailed logging
13. Configuration options
14. Advanced analytics
15. Pretty terminal UI

**These can come in Phase 2**

---

## XIV. Summary

**MVP Scope:**
- ✅ JJ operation log polling
- ✅ Background review aiki
- ✅ Review caching by (change_id, commit_id)
- ✅ Commit-time interception
- ✅ Same UX as original doc

**MVP Deferrals:**
- ❌ Agent attribution
- ❌ Agent-specific features
- ❌ Advanced provenance
- ❌ Multi-agent coordination

**Key Insight:** Agent attribution is analytics, not core functionality. We get all the value (instant reviews, AI self-correction) without the complexity.

**Delivery:** 12 weeks to production-ready MVP that validates the core value proposition.
