# Aiki MVP: Simplified Specification with Idiomatic JJ Integration

## Core Value Proposition

**Pre-compute code reviews in background using JJ's operation log, return instant results at commit time.**

**Time savings: 2-5 seconds per commit → 10-50ms**

**Key insight:** Use JJ's canonical integration pattern (watch `.jj/repo/op_heads/heads`) instead of polling for zero idle CPU usage and instant response.

**Implementation approach:** Uses jj-lib v0.35.0 crate directly (no external JJ binary required).

Agent attribution is deferred to Phase 2. MVP focuses purely on making reviews instant.

---

## I. What We're Building (MVP Scope)

### Three Components

```
┌─────────────────────────────────────────────────────────────┐
│  1. JJ op_heads Watcher (Event-Driven)                      │
│     • Watches .jj/repo/op_heads/heads file                  │
│     • Detects new operations instantly (<5ms)               │
│     • Extracts changed files from operations                 │
│     • Zero CPU when idle                                     │
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

## II. Architecture: Event-Driven JJ Integration

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
│         • Updates: .jj/repo/op_heads/heads ← WATCH THIS!     │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ File change event (FSEvents/inotify)
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         op_heads File Watcher                                │
│         • Watch .jj/repo/op_heads/heads                      │
│         • Instant notification (<5ms)                        │
│         • Zero CPU when idle                                 │
│         • Debounce rapid changes (300ms)                     │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Operation detected!
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Operation Handler                                    │
│         • Load repo at new operation                         │
│         • Extract changed files                              │
│         • Queue for review                                   │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Changed files
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

## III. Why Watch op_heads Instead of Polling?

### The op_heads Directory

From the JJ FAQ, this is the canonical way to detect new operations:

```bash
watchexec --quiet --clear --restart \
  --watch=.jj/repo/op_heads/heads \
  --ignore-nothing --wrap-process=none \
  -- jj --ignore-working-copy log
```

**What `.jj/repo/op_heads/heads` contains:**
- Current operation head ID(s)
- Updated atomically whenever any JJ operation completes
- Lock-free (safe to read while JJ is writing)

**Why it's perfect:**
1. **Event-driven** - No polling overhead
2. **Instant** - Notified immediately when operations happen (<5ms vs 150ms avg)
3. **Idiomatic** - This is how JJ tools are meant to integrate (used by `gg`, `jj-fzf`)
4. **Efficient** - Single file watch instead of directory recursion
5. **Canonical** - JJ guarantees this file updates on every operation
6. **Zero idle CPU** - CPU sleeps when no operations happening
7. **Battery friendly** - No periodic wake-ups

### Comparison: Polling vs Watching

| Metric | Polling (300ms) | Watching (Idiomatic) |
|--------|----------------|---------------------|
| Idle CPU | ~2-5% | ~0% |
| Response time | 150ms avg | <5ms |
| Battery impact | Moderate | Minimal |
| Scales with frequency | No | Yes |
| Code complexity | Moderate | Low |
| Idiomatic for JJ | No | Yes |

**Decision: Use file watching (idiomatic JJ integration)**

---

## IV. Implementation Details

### A. Watch op_heads (Event-Driven)

```rust
use notify::{Watcher, RecursiveMode, Event, EventKind};
use tokio::sync::mpsc;

pub struct OpHeadsWatcher {
    repo_path: PathBuf,
    event_tx: mpsc::Sender<OpHeadEvent>,
}

impl OpHeadsWatcher {
    pub fn new(repo_path: PathBuf) -> Result<Self> {
        let (event_tx, _event_rx) = mpsc::channel(100);
        
        Ok(Self {
            repo_path,
            event_tx,
        })
    }
    
    pub async fn watch(&self) -> Result<()> {
        let op_heads_path = self.repo_path
            .join(".jj")
            .join("repo")
            .join("op_heads")
            .join("heads");
        
        if !op_heads_path.exists() {
            return Err(anyhow!("Not a JJ repository: {:?}", self.repo_path));
        }
        
        // Create file watcher
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;
        
        // Watch the op_heads file specifically
        watcher.watch(&op_heads_path, RecursiveMode::NonRecursive)?;
        
        println!("Watching: {}", op_heads_path.display());
        
        // Convert sync receiver to async
        loop {
            match rx.recv() {
                Ok(event) => {
                    if self.should_process_event(&event) {
                        self.handle_op_head_change().await?;
                    }
                }
                Err(_) => break,
            }
        }
        
        Ok(())
    }
    
    fn should_process_event(&self, event: &Event) -> bool {
        match event.kind {
            // File modified (JJ wrote new op head)
            EventKind::Modify(_) => true,
            // File created (rare, but possible)
            EventKind::Create(_) => true,
            // Ignore other events
            _ => false,
        }
    }
    
    async fn handle_op_head_change(&self) -> Result<()> {
        // Load repo at new operation head
        // IMPORTANT: Use --ignore-working-copy to avoid snapshotting
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        let repo = workspace.repo_loader().load_at_head()?;
        
        // Get operation ID
        let op_id = repo.op_id().clone();
        
        println!("New operation: {}", op_id.hex());
        
        // Extract changed files
        let changed_files = self.extract_changed_files(&repo)?;
        
        // Queue for review
        for file in changed_files {
            self.event_tx.send(OpHeadEvent::FilesChanged {
                op_id: op_id.clone(),
                files: vec![file],
            }).await?;
        }
        
        Ok(())
    }
    
    fn extract_changed_files(&self, repo: &ReadonlyRepo) -> Result<Vec<PathBuf>> {
        // Get working copy commit
        let view = repo.view();
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

### B. Debouncing (Handle Rapid Operations)

Even with event-driven, we need debouncing because JJ might trigger multiple file events for rapid operations:

```rust
pub struct DebouncedOpWatcher {
    watcher: OpHeadsWatcher,
    debounce_duration: Duration,
    pending_event: Option<Instant>,
}

impl DebouncedOpWatcher {
    pub fn new(watcher: OpHeadsWatcher, debounce_duration: Duration) -> Self {
        Self {
            watcher,
            debounce_duration,
            pending_event: None,
        }
    }
    
    pub async fn watch(&mut self) -> Result<()> {
        let mut event_rx = self.watcher.start()?;
        
        loop {
            tokio::select! {
                // New event from watcher
                Some(event) = event_rx.recv() => {
                    // Record event time
                    self.pending_event = Some(Instant::now());
                }
                
                // Debounce timer
                _ = self.wait_for_debounce(), if self.pending_event.is_some() => {
                    // Debounce period elapsed - process event
                    self.process_pending_event().await?;
                    self.pending_event = None;
                }
            }
        }
    }
    
    async fn wait_for_debounce(&self) -> () {
        if let Some(event_time) = self.pending_event {
            let elapsed = event_time.elapsed();
            if elapsed < self.debounce_duration {
                tokio::time::sleep(self.debounce_duration - elapsed).await;
            }
        }
    }
    
    async fn process_pending_event(&mut self) -> Result<()> {
        // Process the accumulated changes
        // (All rapid operations will be batched)
        self.watcher.handle_op_head_change().await
    }
}
```

### C. Review Cache (Simplified)

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

### D. Commit Interceptor (Same as Original)

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

## V. What Gets Simpler Without Agent Attribution

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
async fn handle_op_head_change(&self) {
    let files = self.extract_changed_files(&repo)?;
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
  debounce_ms: 300
  cache_size_mb: 100
```

---

## VI. Edge Cases & Robustness

### A. Concurrent Operations

JJ supports concurrent operations. The op_heads file might update rapidly:

```rust
impl DebouncedOpWatcher {
    // Debouncing handles this naturally
    // Example: 5 operations in 100ms
    // Event 1: 0ms   - Start debounce timer
    // Event 2: 20ms  - Reset timer
    // Event 3: 40ms  - Reset timer
    // Event 4: 60ms  - Reset timer
    // Event 5: 80ms  - Reset timer
    // Process: 380ms - Debounce elapsed, process final state
    
    // We'll see the cumulative changes from all 5 operations
}
```

### B. Divergent Operations

JJ operation log can diverge (concurrent operations on different machines):

```rust
impl OpHeadsWatcher {
    async fn handle_divergent_ops(&self, repo: &ReadonlyRepo) -> Result<()> {
        // JJ may have multiple op heads during divergence
        let op_heads = repo.view().op_heads();
        
        if op_heads.len() > 1 {
            println!("Warning: {} divergent operations detected", op_heads.len());
            
            // Process each head
            for op_id in op_heads {
                let repo_at_op = self.repo_loader.load_at(op_id)?;
                self.process_operation(&repo_at_op).await?;
            }
        } else {
            // Normal case: single op head
            self.process_operation(&repo).await?;
        }
        
        Ok(())
    }
}
```

### C. Missed Events (Recovery)

What if we miss a file system event?

```rust
impl OpHeadsWatcher {
    pub async fn watch_with_recovery(&self) -> Result<()> {
        let mut last_op_id = None;
        let mut missed_event_check = tokio::time::interval(Duration::from_secs(5));
        
        loop {
            tokio::select! {
                // Normal event handling
                event = self.event_rx.recv() => {
                    if let Some(event) = event {
                        last_op_id = Some(self.handle_event(event).await?);
                    }
                }
                
                // Periodic check (every 5 seconds)
                _ = missed_event_check.tick() => {
                    // Check if op head changed without us seeing an event
                    let current_op = self.get_current_op()?;
                    
                    if Some(&current_op) != last_op_id.as_ref() {
                        println!("Warning: Missed event, recovering...");
                        last_op_id = Some(self.handle_missed_operation(current_op).await?);
                    }
                }
            }
        }
    }
}
```

### D. Use `--ignore-working-copy`

When we load the repo to process operations, we should avoid snapshotting:

```rust
impl OpHeadsWatcher {
    fn load_repo_safely(&self) -> Result<ReadonlyRepo> {
        // Don't snapshot working copy
        // (avoids conflicts with user's concurrent JJ commands)
        
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        
        // Load at head without snapshotting
        // This is equivalent to: jj --ignore-working-copy
        let repo = workspace.repo_loader().load_at_head()?;
        
        Ok(repo)
    }
}
```

**Why this matters:**
- Without `--ignore-working-copy`: Aiki's repo load snapshots working copy
- User's concurrent `jj` command also snapshots
- Creates divergent operations
- User sees "divergent changes" warnings

**With `--ignore-working-copy`:**
- Aiki reads repo state without snapshotting
- No interference with user's commands
- Clean operation log

---

## VII. MVP User Experience

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
# ... Aiki detects change INSTANTLY, reviews in background ...

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
# ... Aiki reviews amendments INSTANTLY ...

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

## VIII. MVP Success Metrics

### Technical Metrics

- [ ] **Operation watching works reliably** (99%+ uptime)
- [ ] **Cache hit rate >80%** during typical development
- [ ] **Commit intercept latency <50ms** (P95)
- [ ] **Background CPU usage <1%** (idle)
- [ ] **Memory usage <200MB**
- [ ] **Response time <5ms** (operation detection)

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

## IX. What We Deliver in MVP

### Week 1-2: JJ Integration (Event-Driven)

```
✅ Setup file watcher on .jj/repo/op_heads/heads
✅ Handle file change events
✅ Debounce rapid operations (300ms)
✅ Load repo at new operation
✅ Extract changed files from operations
✅ Recovery mechanism (periodic check)
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

## X. What We Defer to Phase 2

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

## XI. Key Decisions

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

### Decision: Watch op_heads (Not Poll)

**Use JJ's canonical integration pattern: watch `.jj/repo/op_heads/heads`**

**Rationale:**
- Idiomatic (how JJ tools integrate: `gg`, `jj-fzf`)
- Zero idle CPU (vs 2-5% for polling)
- Instant response (<5ms vs 150ms avg for polling)
- Battery efficient (no periodic wake-ups)
- Used by existing JJ ecosystem

**Trade-off:**
- Slightly more complex (file watching)
- But: `notify` crate makes it cross-platform
- And: more efficient in every way

### Decision: 300ms Debounce Duration

**Debounce rapid file changes for 300ms**

**Rationale:**
- Handles rapid JJ operations gracefully
- Low enough latency (sub-second background review)
- High enough to batch multiple rapid changes

**Trade-off:**
- Not truly instant (300ms delay)
- But: doesn't need to be (background review)

---

## XII. Minimal Viable Setup

### Files to Create

```
aiki/
├── src/
│   ├── main.rs              # Entry point
│   ├── watcher.rs           # op_heads file watcher
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
jj-lib = "0.35.0"            # Direct JJ integration (no external binary needed)
clap = { version = "4.5", features = ["derive", "cargo"] }
tokio = { version = "1", features = ["full"] }
notify = "6"                  # File watching
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"

[dev-dependencies]
tempfile = "3.0"
assert_cmd = "2.0"
predicates = "3.0"
```

### Installation Script

```bash
#!/bin/bash
# install.sh

# Build Aiki (no external JJ binary needed - using jj-lib)
cargo build --release

# Install binary
cp target/release/aiki /usr/local/bin/

# Initialize in current repo
aiki init

echo "✓ Aiki installed and initialized"
echo "  Note: No external JJ binary required - using jj-lib crate"
```

---

## XIII. The `aiki init` Command

### Overview

The `aiki init` command sets up Aiki in the current repository. It:
1. Initializes JJ repository using jj-lib crate (colocated with Git)
2. Creates `.aiki/` directory structure
3. Installs Git pre-commit hook
4. Creates default configuration
5. Starts background watcher (optional daemon mode)

**Implementation Note:** Uses jj-lib v0.35.0 directly, no external JJ binary required.

### Command Interface

```bash
# Basic usage
aiki init

# With options
aiki init --daemon          # Start watcher as background daemon
aiki init --no-hook         # Skip pre-commit hook installation
aiki init --config FILE     # Use custom config
```

### Implementation

```rust
// src/cli/init.rs

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct InitCommand {
    repo_path: PathBuf,
    install_hook: bool,
    start_daemon: bool,
}

impl InitCommand {
    pub fn new(repo_path: PathBuf) -> Self {
        Self {
            repo_path,
            install_hook: true,
            start_daemon: false,
        }
    }
    
    pub fn run(&self) -> Result<()> {
        println!("Initializing Aiki in: {}", self.repo_path.display());
        
        // Step 1: Check/initialize JJ
        self.ensure_jj_initialized()?;
        
        // Step 2: Create .aiki directory structure
        self.create_aiki_directory()?;
        
        // Step 3: Install pre-commit hook
        if self.install_hook {
            self.install_git_hook()?;
        }
        
        // Step 4: Create default config
        self.create_default_config()?;
        
        // Step 5: Initialize cache
        self.initialize_cache()?;
        
        // Step 6: Start watcher (optional)
        if self.start_daemon {
            self.start_watcher_daemon()?;
        } else {
            println!("\nTo start watching for changes, run:");
            println!("  aiki watch");
        }
        
        println!("\n✓ Aiki initialized successfully!");
        println!("\nNext steps:");
        println!("  1. Start the watcher: aiki watch");
        println!("  2. Make changes to your code");
        println!("  3. Commit as usual: git commit -m \"message\"");
        
        Ok(())
    }
    
    fn ensure_jj_initialized(&self) -> Result<()> {
        let jj_dir = self.repo_path.join(".jj");
        
        if jj_dir.exists() {
            println!("✓ JJ already initialized");
            return Ok(());
        }
        
        println!("Initializing JJ (colocated with Git)...");
        
        // Check if git repo exists
        let git_dir = self.repo_path.join(".git");
        if !git_dir.exists() {
            return Err(anyhow::anyhow!(
                "Not a Git repository. Run 'git init' first."
            ));
        }
        
        // Use jj-lib to initialize colocated repository
        use jj_lib::config::StackedConfig;
        use jj_lib::settings::UserSettings;
        use jj_lib::workspace::Workspace;
        
        let config = StackedConfig::with_defaults();
        let settings = UserSettings::from_config(config)
            .context("Failed to create user settings")?;
        
        let (_workspace, _repo) = Workspace::init_colocated_git(&settings, &self.repo_path)
            .context("Failed to initialize JJ repository")?;
        
        println!("✓ JJ initialized (colocated mode)");
        Ok(())
    }
    
    fn create_aiki_directory(&self) -> Result<()> {
        let aiki_dir = self.repo_path.join(".aiki");
        
        if aiki_dir.exists() {
            println!("✓ .aiki directory already exists");
            return Ok(());
        }
        
        println!("Creating .aiki directory structure...");
        
        // Create directory structure
        fs::create_dir_all(aiki_dir.join("cache"))?;
        fs::create_dir_all(aiki_dir.join("logs"))?;
        fs::create_dir_all(aiki_dir.join("tmp"))?;
        
        // Add to .gitignore
        self.update_gitignore()?;
        
        println!("✓ Created .aiki/");
        println!("  ├── cache/  (review cache)");
        println!("  ├── logs/   (watcher logs)");
        println!("  └── tmp/    (temporary files)");
        
        Ok(())
    }
    
    fn update_gitignore(&self) -> Result<()> {
        let gitignore_path = self.repo_path.join(".gitignore");
        let aiki_entry = "\n# Aiki review cache\n.aiki/\n";
        
        if gitignore_path.exists() {
            let content = fs::read_to_string(&gitignore_path)?;
            if content.contains(".aiki/") {
                return Ok(()); // Already in .gitignore
            }
            
            // Append to existing .gitignore
            fs::write(&gitignore_path, format!("{}{}", content, aiki_entry))?;
        } else {
            // Create new .gitignore
            fs::write(&gitignore_path, aiki_entry)?;
        }
        
        println!("✓ Added .aiki/ to .gitignore");
        Ok(())
    }
    
    fn install_git_hook(&self) -> Result<()> {
        let hooks_dir = self.repo_path.join(".git").join("hooks");
        let hook_path = hooks_dir.join("pre-commit");
        
        println!("Installing Git pre-commit hook...");
        
        // Create hooks directory if it doesn't exist
        fs::create_dir_all(&hooks_dir)?;
        
        // Hook script
        let hook_script = r#"#!/bin/bash
# Aiki pre-commit hook
# Returns cached review results instantly

set -e

# Run Aiki commit interceptor
aiki intercept

# Exit code from interceptor determines if commit proceeds
exit $?
"#;
        
        if hook_path.exists() {
            // Backup existing hook
            let backup_path = hooks_dir.join("pre-commit.backup");
            fs::copy(&hook_path, &backup_path)?;
            println!("  ℹ Backed up existing hook to pre-commit.backup");
        }
        
        // Write hook
        fs::write(&hook_path, hook_script)?;
        
        // Make executable (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms)?;
        }
        
        println!("✓ Installed .git/hooks/pre-commit");
        Ok(())
    }
    
    fn create_default_config(&self) -> Result<()> {
        let config_path = self.repo_path.join(".aiki").join("config.toml");
        
        if config_path.exists() {
            println!("✓ Config already exists");
            return Ok(());
        }
        
        println!("Creating default configuration...");
        
        let default_config = r#"# Aiki Configuration

[review]
# Debounce duration for rapid file changes (milliseconds)
debounce_ms = 300

# Cache size limit (megabytes)
cache_size_mb = 100

# Enable AI review (requires API key)
ai_review_enabled = false

[workers]
# Run static analysis (clippy, eslint, etc.)
static_analysis = true

# Run type checking (tsc, rust-analyzer, etc.)
type_checking = true

# Number of parallel review workers
parallelism = 4

[git]
# Block commits on critical issues
block_on_critical = true

# Block commits on warnings
block_on_warnings = false

# Auto-escalate to human after N failed attempts
auto_escalate_after = 3
"#;
        
        fs::write(&config_path, default_config)?;
        println!("✓ Created .aiki/config.toml");
        
        Ok(())
    }
    
    fn initialize_cache(&self) -> Result<()> {
        let cache_path = self.repo_path.join(".aiki").join("cache");
        
        // Create empty cache index
        let index_path = cache_path.join("index.json");
        if !index_path.exists() {
            fs::write(&index_path, "{}")?;
            println!("✓ Initialized review cache");
        }
        
        Ok(())
    }
    
    fn start_watcher_daemon(&self) -> Result<()> {
        println!("Starting watcher daemon...");
        
        let log_path = self.repo_path.join(".aiki").join("logs").join("watcher.log");
        
        // Start watcher in background
        Command::new("aiki")
            .arg("watch")
            .arg("--daemon")
            .current_dir(&self.repo_path)
            .stdout(std::fs::File::create(&log_path)?)
            .stderr(std::fs::File::create(&log_path)?)
            .spawn()
            .context("Failed to start watcher daemon")?;
        
        println!("✓ Watcher started (logs: .aiki/logs/watcher.log)");
        
        Ok(())
    }
}
```

### Usage Examples

**Example 1: Basic initialization**
```bash
$ cd my-project
$ aiki init

Initializing Aiki in: /Users/me/my-project
✓ JJ already initialized
Creating .aiki directory structure...
✓ Created .aiki/
  ├── cache/  (review cache)
  ├── logs/   (watcher logs)
  └── tmp/    (temporary files)
✓ Added .aiki/ to .gitignore
Installing Git pre-commit hook...
✓ Installed .git/hooks/pre-commit
Creating default configuration...
✓ Created .aiki/config.toml
✓ Initialized review cache

✓ Aiki initialized successfully!

Next steps:
  1. Start the watcher: aiki watch
  2. Make changes to your code
  3. Commit as usual: git commit -m "message"
```

**Example 2: Initialize with daemon mode**
```bash
$ aiki init --daemon

Initializing Aiki in: /Users/me/my-project
✓ JJ already initialized
✓ .aiki directory already exists
✓ Config already exists
✓ Installed .git/hooks/pre-commit
Starting watcher daemon...
✓ Watcher started (logs: .aiki/logs/watcher.log)

✓ Aiki initialized successfully!

Watcher is running in background. To check status:
  aiki status
```

**Example 3: Fresh project (no JJ yet)**
```bash
$ cd new-project
$ git init
$ aiki init

Initializing Aiki in: /Users/me/new-project
Initializing JJ (colocated with Git)...
✓ JJ initialized (colocated mode)
Creating .aiki directory structure...
✓ Created .aiki/
...
✓ Aiki initialized successfully!
```

### Directory Structure After Init

```
my-project/
├── .git/                    # Git repository
│   └── hooks/
│       └── pre-commit       # Aiki hook
├── .jj/                     # JJ repository (colocated)
│   └── repo/
│       └── op_heads/
│           └── heads        # File we watch
├── .aiki/                 # Aiki directory
│   ├── cache/               # Review cache
│   │   └── index.json
│   ├── logs/                # Watcher logs
│   │   └── watcher.log
│   ├── tmp/                 # Temporary files
│   └── config.toml          # Configuration
├── .gitignore               # Updated with .aiki/
└── src/                     # Your code
```

### Error Handling

```rust
impl InitCommand {
    fn run(&self) -> Result<()> {
        // Check if JJ is installed
        if !self.is_jj_installed()? {
            return Err(anyhow::anyhow!(
                "JJ (Jujutsu) is not installed.\n\
                Install it with: brew install jj-cli\n\
                Or visit: https://github.com/martinvonz/jj"
            ));
        }
        
        // Check if we're in a git repo
        if !self.repo_path.join(".git").exists() {
            return Err(anyhow::anyhow!(
                "Not a Git repository.\n\
                Initialize Git first: git init"
            ));
        }
        
        // Check for write permissions
        if !self.has_write_permission()? {
            return Err(anyhow::anyhow!(
                "No write permission in: {}\n\
                Check directory permissions.",
                self.repo_path.display()
            ));
        }
        
        // Continue with initialization...
        Ok(())
    }
    
    fn is_jj_installed(&self) -> Result<bool> {
        Ok(Command::new("jj")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false))
    }
    
    fn has_write_permission(&self) -> Result<bool> {
        // Try to create a temp file
        let test_path = self.repo_path.join(".aiki_init_test");
        match fs::write(&test_path, "") {
            Ok(_) => {
                let _ = fs::remove_file(&test_path);
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
}
```

### CLI Integration

```rust
// src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(name = "aiki")]
#[clap(about = "AI code review aiki", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize Aiki in current repository
    Init {
        /// Start watcher as background daemon
        #[clap(long)]
        daemon: bool,
        
        /// Skip pre-commit hook installation
        #[clap(long)]
        no_hook: bool,
        
        /// Custom config file
        #[clap(long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
    
    /// Watch for changes and review in background
    Watch {
        /// Run as background daemon
        #[clap(long)]
        daemon: bool,
    },
    
    /// Intercept commit and return cached review
    Intercept,
    
    /// Check Aiki status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Init { daemon, no_hook, config } => {
            let repo_path = std::env::current_dir()?;
            let mut cmd = InitCommand::new(repo_path);
            cmd.install_hook = !no_hook;
            cmd.start_daemon = daemon;
            cmd.run()?;
        }
        
        Commands::Watch { daemon } => {
            let repo_path = std::env::current_dir()?;
            let watcher = OpHeadsWatcher::new(repo_path)?;
            if daemon {
                watcher.run_daemon().await?;
            } else {
                watcher.watch().await?;
            }
        }
        
        Commands::Intercept => {
            let repo_path = std::env::current_dir()?;
            let interceptor = CommitInterceptor::new(repo_path)?;
            interceptor.intercept().await?;
        }
        
        Commands::Status => {
            // Show watcher status, cache stats, etc.
            print_status()?;
        }
    }
    
    Ok(())
}
```

### Week 1 Deliverable

In Week 1-2, we implement:
- ✅ `aiki init` command
- ✅ `.aiki/` directory creation
- ✅ JJ initialization check/setup
- ✅ Git pre-commit hook installation
- ✅ Default configuration generation
- ✅ Cache initialization

This provides a solid foundation for the rest of the MVP implementation.

---

## XIV. Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_op_heads_change_detected() {
        let temp_repo = setup_jj_repo();
        let watcher = OpHeadsWatcher::new(temp_repo.path())?;
        
        // Start watcher in background
        let handle = tokio::spawn(async move {
            watcher.watch().await
        });
        
        // Make a JJ operation
        Command::new("jj")
            .arg("describe")
            .arg("-m")
            .arg("test commit")
            .current_dir(temp_repo.path())
            .output()?;
        
        // Wait for event
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Verify event was processed
        assert!(event_received);
    }
    
    #[tokio::test]
    async fn test_debouncing() {
        // Make 5 rapid operations
        // Verify only 1 review triggered after debounce
    }
    
    #[tokio::test]
    async fn test_divergent_ops() {
        // Create divergent operations
        // Verify both heads are processed
    }
}
```

### Integration Tests

```bash
# Test with real JJ repository
cd test-repo
jj git init --colocate

# Start Aiki in background
aiki watch &

# Make changes
echo "test" >> file.txt
jj describe -m "test"

# Verify Aiki processed the operation
# Check logs, review cache, etc.
```

---

## XIV. Open Questions

1. **How do we handle repos that already use JJ?**
   - Just run `aiki init` (skip `jj git init`)
   - Detect if JJ already initialized

2. **What if user has uncommitted changes when running `aiki init`?**
   - JJ automatically commits working copy
   - Should be fine, but need to test

3. **Do we need a daemon or can watcher run in terminal?**
   - Start simple: terminal process
   - Add daemon mode in Phase 2 if needed

4. **How do we persist cache across restarts?**
   - Disk cache in `.aiki/cache/`
   - LRU eviction policy

5. **What happens if operation log grows very large?**
   - JJ handles this (designed for Google scale)
   - We only look at current op head

6. **Cross-platform file watching differences?**
   - Use `notify` crate (handles FSEvents/inotify/Windows)
   - Test on macOS, Linux, Windows

---

## XV. Success Criteria for MVP

**Technical Success:**
- ✅ Watcher detects operations reliably
- ✅ Reviews cached and reused
- ✅ Commit intercept returns results in <50ms
- ✅ Works with Cursor, Copilot, human edits
- ✅ Zero idle CPU usage

**User Success:**
- ✅ Setup in <5 minutes
- ✅ Works without configuration
- ✅ Catches real bugs pre-commit
- ✅ Feels instant at commit time
- ✅ No perceived performance impact

**Market Success:**
- ✅ 10 users actively using daily
- ✅ Positive feedback on time savings
- ✅ No major bugs or reliability issues
- ✅ Users recommend to others

**If these pass: Move to Phase 2 (agent attribution + coordination)**

---

## XVI. Implementation Priority

### Critical Path (Must Work)

1. JJ op_heads file watching
2. Event handling + debouncing
3. Operation detection
4. Changed file extraction
5. Review execution
6. Review caching
7. Commit interception
8. Result display

**These must work reliably or MVP fails**

### Important (Should Work)

9. Recovery mechanism (missed events)
10. Cache persistence
11. Error handling
12. Performance optimization
13. User documentation

**These improve UX but aren't blocking**

### Nice to Have (Can Defer)

14. Detailed logging
15. Configuration options
16. Advanced analytics
17. Pretty terminal UI

**These can come in Phase 2**

---

## XVII. Summary

**MVP Scope:**
- ✅ JJ op_heads file watching (idiomatic, event-driven)
- ✅ Background review aiki
- ✅ Review caching by (change_id, commit_id)
- ✅ Commit-time interception
- ✅ Same UX as original doc

**MVP Deferrals:**
- ❌ Agent attribution
- ❌ Agent-specific features
- ❌ Advanced provenance
- ❌ Multi-agent coordination

**Key Insights:**
1. **Agent attribution is analytics, not core functionality** - We get all the value (instant reviews, AI self-correction) without the complexity
2. **Watch op_heads, don't poll** - Idiomatic JJ integration is more efficient and follows established patterns (`gg`, `jj-fzf`)
3. **Event-driven architecture** - Zero idle CPU, instant response, battery efficient

**Delivery:** 12 weeks to production-ready MVP that validates the core value proposition using idiomatic JJ integration patterns.

**This is the correct way to build on JJ.**
