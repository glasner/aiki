# Aiki: Idiomatic JJ Integration (Watch op_heads)

## Key Discovery

**JJ has a canonical way to detect new operations: watch `.jj/repo/op_heads/heads`**

From the JJ FAQ:
```bash
watchexec --quiet --clear --restart \
  --watch=.jj/repo/op_heads/heads \
  --ignore-nothing --wrap-process=none \
  -- jj --ignore-working-copy log
```

This is how GUIs like `gg` and tools like `jj-fzf` stay synchronized with JJ operations.

---

## I. Why Watch op_heads Instead of Polling?

### The op_heads Directory

```
.jj/repo/op_heads/
├── heads              # ← WATCH THIS FILE
└── (internal state)
```

**What it contains:**
- Current operation head ID(s)
- Updated atomically whenever any JJ operation completes
- Lock-free (safe to read while JJ is writing)

**Why it's perfect:**
1. **Event-driven** - No polling overhead
2. **Instant** - Notified immediately when operations happen
3. **Idiomatic** - This is how JJ tools are meant to integrate
4. **Efficient** - Single file watch instead of directory recursion
5. **Canonical** - JJ guarantees this file updates on every operation

### Comparison

**Polling (our original plan):**
```rust
loop {
    let current_op = repo.op_id();
    if current_op != last_op {
        // Handle new operation
    }
    sleep(300ms); // ← Wasted CPU cycles
}
```

**Watching op_heads (idiomatic):**
```rust
let watcher = FileWatcher::new(".jj/repo/op_heads/heads");
for event in watcher {
    // Operation happened - handle it immediately
}
```

**Benefits:**
- ✅ Zero CPU when idle
- ✅ Instant response (no 300ms delay)
- ✅ Event-driven architecture
- ✅ Scales to any operation frequency

---

## II. Revised Architecture

```
┌──────────────────────────────────────────────────────────────┐
│         Developer/AI Edits Files                             │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ Save files
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Jujutsu Automatically Snapshots                      │
│         (working copy → operation log)                       │
│         Updates: .jj/repo/op_heads/heads                     │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ File change event
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Aiki File Watcher                                  │
│         Watch: .jj/repo/op_heads/heads                       │
│         (FSEvents on macOS, inotify on Linux)                │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        │ op_heads changed!
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Operation Handler                                    │
│         • Load repo at new operation                         │
│         • Extract changed files                              │
│         • Queue for review                                   │
└───────────────────────┬──────────────────────────────────────┘
                        │
                        ↓
┌──────────────────────────────────────────────────────────────┐
│         Review Aiki                                        │
│         (same as before)                                     │
└──────────────────────────────────────────────────────────────┘
```

---

## III. Implementation

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
        let (event_tx, event_rx) = mpsc::channel(100);
        
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
}
```

### B. Debouncing (Still Useful)

Even with event-driven, we need debouncing because JJ might trigger multiple file events:

```rust
pub struct DebouncedOpWatcher {
    watcher: OpHeadsWatcher,
    debounce_duration: Duration,
    pending_event: Option<Instant>,
}

impl DebouncedOpWatcher {
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
}
```

### C. Simplified Main Loop

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let repo_path = std::env::current_dir()?;
    
    // Create watcher
    let mut watcher = DebouncedOpWatcher::new(
        repo_path,
        Duration::from_millis(300), // Debounce period
    )?;
    
    // Start watching (blocks)
    watcher.watch().await?;
    
    Ok(())
}
```

**Much simpler than polling!**

---

## IV. Advantages Over Polling

### 1. Zero Idle CPU Usage

**Polling:**
```
CPU: ████░░░░████░░░░████░░░░████░░░░
      ↑ poll  ↑ poll  ↑ poll  ↑ poll
```

**Event-driven:**
```
CPU: ░░░░░░░░░░░░░████░░░░░░░░░░░░░░
                  ↑ only when operation happens
```

### 2. Instant Response

**Polling:**
- Worst case: 300ms delay (if operation happens right after poll)
- Average: 150ms delay

**Event-driven:**
- Worst case: <10ms (file system notification latency)
- Average: <5ms

### 3. Scales to Any Frequency

**Polling:**
- High frequency (100ms) = high CPU
- Low frequency (500ms) = slow response
- Trade-off between CPU and responsiveness

**Event-driven:**
- No trade-off needed
- Always instant, always efficient
- Works perfectly whether operations are rare or frequent

### 4. Battery Friendly

**Polling:**
- Wakes CPU every 300ms
- Prevents deep sleep
- Drains laptop battery

**Event-driven:**
- CPU sleeps when idle
- Only wakes on actual operations
- Minimal battery impact

---

## V. Edge Cases & Robustness

### A. Concurrent Operations

JJ supports concurrent operations. The op_heads file might update rapidly:

```rust
impl OpHeadsWatcher {
    async fn handle_rapid_updates(&self) -> Result<()> {
        // Debouncing handles this naturally
        // We'll process the final state after debounce period
        
        // Example: 5 operations in 100ms
        // Event 1: 0ms   - Start debounce timer
        // Event 2: 20ms  - Reset timer
        // Event 3: 40ms  - Reset timer
        // Event 4: 60ms  - Reset timer
        // Event 5: 80ms  - Reset timer
        // Process: 380ms - Debounce elapsed, process final state
        
        // We'll see the cumulative changes from all 5 operations
        Ok(())
    }
}
```

### B. Divergent Operations

JJ operation log can diverge (concurrent operations on different machines):

```rust
impl OpHeadsWatcher {
    fn handle_divergent_ops(&self, repo: &ReadonlyRepo) -> Result<()> {
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

### C. Missed Events

What if we miss a file system event?

```rust
impl OpHeadsWatcher {
    async fn watch_with_recovery(&self) -> Result<()> {
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

---

## VI. Integration with JJ Commands

### Important: Use `--ignore-working-copy`

When we load the repo to process operations, we should use `--ignore-working-copy`:

```rust
impl OpHeadsWatcher {
    fn load_repo_safely(&self) -> Result<ReadonlyRepo> {
        // Don't snapshot working copy
        // (avoids conflicts with user's concurrent JJ commands)
        
        let workspace = Workspace::load(&self.repo_path, &default_loader())?;
        
        // Load at head without snapshotting
        let repo = workspace.repo_loader().load_at_head()?;
        
        // This is equivalent to: jj --ignore-working-copy
        Ok(repo)
    }
}
```

**Why this matters:**

Without `--ignore-working-copy`:
- Aiki's repo load snapshots working copy
- User's concurrent `jj` command also snapshots
- Creates divergent operations
- User sees "divergent changes" warnings

With `--ignore-working-copy`:
- Aiki reads repo state without snapshotting
- No interference with user's commands
- Clean operation log

---

## VII. Comparison: Polling vs Watching

### Code Complexity

**Polling (original):**
```rust
// ~150 lines
pub struct OperationPoller {
    repo_path: PathBuf,
    last_op_id: Option<OperationId>,
    poll_interval: Duration,
}

impl OperationPoller {
    pub async fn run(&mut self) {
        loop {
            self.poll_once().await;
            tokio::time::sleep(self.poll_interval).await;
        }
    }
    
    async fn poll_once(&mut self) {
        // Load repo
        // Check if op changed
        // Get ops since last poll
        // Process each op
    }
}
```

**Watching (idiomatic):**
```rust
// ~100 lines
pub struct OpHeadsWatcher {
    watcher: notify::RecommendedWatcher,
    repo_path: PathBuf,
}

impl OpHeadsWatcher {
    pub async fn watch(&mut self) {
        // Setup file watcher on .jj/repo/op_heads/heads
        // Receive events
        // Process when file changes
    }
}
```

**Less code, more efficient!**

### Resource Usage

| Metric | Polling (300ms) | Watching |
|--------|----------------|----------|
| Idle CPU | ~2-5% | ~0% |
| Response time | 150ms avg | <5ms |
| Battery impact | Moderate | Minimal |
| Scales with ops | No | Yes |

### Behavior

| Scenario | Polling | Watching |
|----------|---------|----------|
| User edits file | Wait up to 300ms | Instant (<5ms) |
| Rapid edits (5/sec) | Process every 300ms | Debounce (process once) |
| No activity | Continuous polling | Sleep until event |
| High frequency ops | May miss some | Catches all |

---

## VIII. Revised MVP Implementation

### Week 1: File Watching (2-3 days)

```rust
// Day 1: Basic watcher
let watcher = OpHeadsWatcher::new(repo_path)?;
watcher.watch().await?;

// Day 2: Debouncing
let debounced = DebouncedOpWatcher::new(watcher, Duration::from_millis(300));

// Day 3: Error handling + recovery
debounced.watch_with_recovery().await?;
```

### Week 2: Operation Processing (4-5 days)

```rust
// Extract changed files from operation
// Queue for review
// Handle divergent operations
// Integration tests
```

### Weeks 3-12: Same as Before

Review aiki, caching, commit interceptor, etc.

**Net change: Week 1 is simpler and more efficient**

---

## IX. Testing Strategy

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
        // Verify only 1 review triggered
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

## X. Migration from Polling Design

### What Changes

**File to modify:**
```
src/poller.rs → src/watcher.rs
```

**Old interface:**
```rust
pub struct OperationPoller {
    pub async fn run(&mut self) {
        loop {
            self.poll_once().await;
            sleep(300ms).await;
        }
    }
}
```

**New interface:**
```rust
pub struct OpHeadsWatcher {
    pub async fn watch(&mut self) {
        // Event-driven, no sleep
        for event in self.events {
            self.handle_event(event).await;
        }
    }
}
```

### What Stays the Same

✅ Review cache
✅ Review aiki  
✅ Commit interceptor
✅ Changed file extraction
✅ All downstream logic

**Only the "trigger mechanism" changes!**

---

## XI. Recommended Approach

### Phase 1: Simple File Watcher (Week 1)

Start with basic file watching, no debouncing:

```rust
use notify::Watcher;

let watcher = OpHeadsWatcher::new(repo_path)?;
watcher.watch().await?; // Blocks, handles events
```

### Phase 2: Add Debouncing (Week 1)

Add debouncing to handle rapid operations:

```rust
let debounced = DebouncedOpWatcher::new(
    watcher,
    Duration::from_millis(300),
);
```

### Phase 3: Add Recovery (Week 2)

Add periodic checks in case we miss events:

```rust
watcher.watch_with_recovery().await?;
```

### Phase 4: Optimize (Later)

After MVP works, optimize if needed:
- Tune debounce duration
- Add smarter batching
- Profile performance

---

## XII. Decision: Watch vs Poll

### Watch (Recommended)

**Pros:**
- ✅ Idiomatic (how JJ tools integrate)
- ✅ Zero idle CPU
- ✅ Instant response
- ✅ Battery efficient
- ✅ Scales naturally
- ✅ Less code

**Cons:**
- ❌ Slightly more complex (file watching)
- ❌ Platform-specific (FSEvents vs inotify)

**Mitigation:** Use `notify` crate (cross-platform)

### Poll (Alternative)

**Pros:**
- ✅ Simpler (just a loop)
- ✅ Platform-independent

**Cons:**
- ❌ Not idiomatic
- ❌ Wastes CPU when idle
- ❌ 150ms average delay
- ❌ More code
- ❌ Battery drain

---

## XIII. Final Recommendation

**Use file watching on `.jj/repo/op_heads/heads`**

**Rationale:**
1. This is how JJ expects tools to integrate
2. Much more efficient (zero idle CPU)
3. Faster response (<5ms vs 150ms avg)
4. Used by existing JJ tools (gg, jj-fzf)
5. Only slightly more complex than polling
6. `notify` crate makes it cross-platform

**Implementation:**
- Week 1: Basic file watcher + debouncing
- Week 2: Test with JJ operations
- Weeks 3-12: Same as before (review aiki, cache, etc.)

**This is the idiomatic JJ way.**
