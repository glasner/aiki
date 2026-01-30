# Sync Remote JJ Systems for Centralized Control Plane

## Problem Statement

For commercial success, Aiki needs a centralized control plane that can coordinate AI agents across multiple remote machines. The biggest blocker: **how do we sync our JJ-based provenance and task systems across machines?**

Current state:
- JJ stores data locally in `.jj/` directory
- Aiki metadata lives in JJ change descriptions (`[aiki]` blocks)
- Tasks are stored on the `aiki/tasks` branch
- No native multi-machine sync mechanism exists

---

## Why This Is Hard

JJ is designed as a local-first VCS with Git as its storage backend:

1. **JJ ≠ Git remote model**: JJ uses `jj git push/fetch` but this syncs *content*, not JJ-specific metadata like change IDs
2. **Change IDs are local**: JJ's stable change IDs are generated locally and may differ across clones
3. **Tasks branch is metadata**: The `aiki/tasks` branch holds task state that needs real-time sync
4. **Provenance needs replication**: `[aiki]` blocks in change descriptions must be visible everywhere

---

## Options Analysis

### Option 1: JJ Git Remote Sync (Native Approach)

**How it works:**
- Use JJ's built-in Git interop (`jj git push`, `jj git fetch`)
- Push JJ bookmarks to Git branches on a central remote
- Remote machines fetch and import those branches

**Pros:**
- Uses JJ's native capabilities
- No new infrastructure needed
- Git remotes are well-understood

**Cons:**
- Change IDs don't survive round-trip (regenerated on import)
- Provenance in change descriptions survives, but linking breaks
- Tasks branch sync requires manual coordination
- No real-time sync (poll-based)

**Verdict:** Good for code sync, insufficient for control plane.

---

### Option 2: Shared JJ Repository Server

**How it works:**
- Central server hosts a JJ repository
- Remote machines connect as JJ workspaces
- All machines share the same change graph

**Pros:**
- Change IDs are consistent (single source of truth)
- Provenance visible across all machines
- Tasks branch accessible everywhere
- True multi-agent coordination

**Cons:**
- JJ not designed for networked/server mode
- Would require significant JJ core contributions
- Latency-sensitive (every operation hits server)
- Single point of failure

**Architecture sketch:**
```
┌─────────────────────────────────────────────────────────┐
│                 Central JJ Server                        │
│  ┌───────────────────────────────────────────────────┐  │
│  │               Shared JJ Repository                 │  │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐  │  │
│  │  │  ws-1   │ │  ws-2   │ │  ws-3   │ │  ws-n   │  │  │
│  │  │(mach-A) │ │(mach-B) │ │(mach-C) │ │  ...    │  │  │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘  │  │
│  │                 Common Change Graph                │  │
│  │              aiki/tasks branch (shared)            │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
         ↑              ↑              ↑
         │ Network      │ Network      │ Network
         ▼              ▼              ▼
   ┌──────────┐   ┌──────────┐   ┌──────────┐
   │ Machine A │   │ Machine B │   │ Machine C │
   │  (agent)  │   │  (agent)  │   │  (human)  │
   └──────────┘   └──────────┘   └──────────┘
```

**Verdict:** Ideal semantics but requires JJ core changes. Long-term goal.

---

### Option 3: Git + Metadata Overlay Service

**How it works:**
- Use Git remotes for code sync (standard workflow)
- Separate service syncs Aiki metadata:
  - Tasks state (CRUD operations)
  - Session information
  - Agent assignments
  - Provenance mappings (change_id → metadata)

**Pros:**
- Decouples code sync from metadata sync
- Can use existing infrastructure (PostgreSQL, Redis, etc.)
- Real-time updates possible (WebSocket, SSE)
- Easier to build incrementally

**Cons:**
- Two sources of truth (Git + metadata service)
- Must reconcile metadata with Git state
- Change IDs still problematic across clones
- More moving parts

**Architecture sketch:**
```
┌─────────────────────────────────────────────────────────┐
│                   Aiki Control Plane                     │
│  ┌───────────────────┐     ┌────────────────────────┐   │
│  │   Git Remote       │     │   Metadata Service     │   │
│  │  (code changes)    │     │   ┌──────────────┐    │   │
│  │   origin/main      │     │   │ Tasks        │    │   │
│  │   origin/feature/* │     │   │ Sessions     │    │   │
│  └───────────────────┘     │   │ Provenance   │    │   │
│                             │   │ Assignments  │    │   │
│                             │   └──────────────┘    │   │
│                             │   WebSocket/SSE API   │   │
│                             └────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
         ↑           ↑                    ↑
         │ git       │ git                │ HTTP/WS
         │ push/pull │ push/pull          │ real-time
         ▼           ▼                    ▼
   ┌──────────┐   ┌──────────┐   ┌──────────┐
   │ Machine A │   │ Machine B │   │ Machine C │
   │  .git/    │   │  .git/    │   │  .git/    │
   │  .jj/     │   │  .jj/     │   │  .jj/     │
   │  aiki CLI │   │  aiki CLI │   │  aiki CLI │
   └──────────┘   └──────────┘   └──────────┘
```

**Verdict:** Pragmatic, but loses JJ's benefits for metadata.

---

### Option 4: CRDT-Based Event Sync

**How it works:**
- Every Aiki operation emits an event (task created, provenance added, etc.)
- Events are CRDTs (Conflict-free Replicated Data Types)
- Events sync to central store and broadcast to peers
- Each machine rebuilds local state from event log

**Pros:**
- Eventually consistent by design
- Handles network partitions gracefully
- Can work offline, sync when connected
- Audit log built-in

**Cons:**
- Complex implementation
- State reconstruction can be expensive
- CRDT semantics may not fit all operations
- Harder to debug

**Architecture sketch:**
```
┌─────────────────────────────────────────────────────────┐
│                    Event Store                           │
│  ┌───────────────────────────────────────────────────┐  │
│  │   Event Log (append-only, CRDT)                   │  │
│  │   ┌─────────────────────────────────────────────┐ │  │
│  │   │ [task.created] [prov.added] [session.start] │ │  │
│  │   │ [task.closed]  [agent.assign] [file.edit]   │ │  │
│  │   └─────────────────────────────────────────────┘ │  │
│  └───────────────────────────────────────────────────┘  │
│                    ↕ Pub/Sub                            │
└─────────────────────────────────────────────────────────┘
         ↑              ↑              ↑
         │ Subscribe    │ Subscribe    │ Subscribe
         │ Publish      │ Publish      │ Publish
         ▼              ▼              ▼
   ┌──────────┐   ┌──────────┐   ┌──────────┐
   │ Machine A │   │ Machine B │   │ Machine C │
   │ Local JJ  │   │ Local JJ  │   │ Local JJ  │
   │ Event Ctl │   │ Event Ctl │   │ Event Ctl │
   └──────────┘   └──────────┘   └──────────┘
```

**Verdict:** Elegant for sync, but significant engineering investment.

---

### Option 5: Hybrid Git-Native Approach (Recommended)

**How it works:**
- **Code sync**: JJ → Git remote (standard `jj git push/fetch`)
- **Tasks sync**: Push `aiki/tasks` branch to Git remote
- **Provenance**: Stored in Git commit trailers (via prepare-commit-msg hook)
- **Control plane**: Lightweight coordinator that watches Git remote

**Key insight**: Instead of fighting Git, embrace it as the universal sync layer.

**Changes required:**
1. Tasks branch pushed to Git remote (not just local JJ branch)
2. Provenance extracted to Git commit trailers on push
3. Control plane watches Git remote for changes
4. Real-time coordination via separate channel (WebSocket for agent orchestration)

**Architecture sketch:**
```
┌─────────────────────────────────────────────────────────┐
│                   Aiki Control Plane                     │
│  ┌───────────────────────────────────────────────────┐  │
│  │                   Git Remote                       │  │
│  │   origin/main        (code)                       │  │
│  │   origin/aiki/tasks  (task state)                 │  │
│  │   origin/aiki/meta   (provenance index)           │  │
│  └───────────────────────────────────────────────────┘  │
│                         ↑                                │
│  ┌───────────────────────────────────────────────────┐  │
│  │              Coordinator Service                   │  │
│  │   - Watches Git for changes                        │  │
│  │   - Assigns tasks to agents                        │  │
│  │   - Tracks session state                           │  │
│  │   - Provides WebSocket API for real-time           │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
         ↑              ↑              ↑
         │ git+ws       │ git+ws       │ git+ws
         ▼              ▼              ▼
   ┌──────────┐   ┌──────────┐   ┌──────────┐
   │ Machine A │   │ Machine B │   │ Machine C │
   │ .jj/      │   │ .jj/      │   │ .jj/      │
   │ git push  │   │ git push  │   │ git push  │
   │ aiki CLI  │   │ aiki CLI  │   │ aiki CLI  │
   └──────────┘   └──────────┘   └──────────┘
```

**Pros:**
- Git is already distributed and well-understood
- Tasks branch syncs like any other branch
- Provenance travels with commits (via trailers)
- Coordinator adds real-time without replacing Git
- Incremental: start with just tasks sync, add more later

**Cons:**
- Git round-trip for task updates (not instant)
- Need to handle merge conflicts on tasks branch
- Coordinator is additional infrastructure
- JJ change IDs still local (but less critical with this design)

**Verdict:** Best balance of practicality and capability.

---

## Recommended Path Forward

### Phase 1: Tasks Branch Sync (MVP)

**Goal:** Enable multiple machines to share task state via Git.

**Implementation:**
1. Modify `aiki task` commands to push tasks branch after mutations
2. Add `aiki sync` command to pull latest tasks from remote
3. Handle merge conflicts on tasks branch (last-writer-wins or merge)
4. Test with 2 machines sharing tasks

**Commands:**
```bash
# Push local tasks to remote
aiki sync push

# Pull remote tasks to local
aiki sync pull

# Auto-sync (push after every task mutation)
aiki config set sync.auto true
```

**Scope:** ~2-3 weeks

---

### Phase 2: Provenance in Git Trailers

**Goal:** Ensure provenance travels with Git commits across machines.

**Implementation:**
1. Enhance prepare-commit-msg hook to include provenance trailers
2. Add `Aiki-Provenance: <json>` trailer to commits
3. Create `aiki import-provenance` to extract trailers on pull
4. Index provenance from Git history, not just JJ

**Example commit:**
```
feat: Add user authentication

Co-authored-by: Claude Code <claude-code@anthropic.ai>
Aiki-Session: session-abc123
Aiki-Agent: claude-code
Aiki-Confidence: high
```

**Scope:** ~2 weeks

---

### Phase 3: Coordinator Service

**Goal:** Real-time agent coordination across machines.

**Implementation:**
1. Lightweight HTTP/WebSocket service
2. Watches Git remote for changes (webhooks or polling)
3. Maintains in-memory session/agent state
4. Broadcasts events to connected agents
5. Provides REST API for task assignment, status

**API sketch:**
```
POST /api/v1/sessions           # Register agent session
GET  /api/v1/tasks/available    # Get tasks available for work
POST /api/v1/tasks/{id}/claim   # Claim a task
WS   /api/v1/events             # Real-time event stream
```

**Scope:** ~4-6 weeks

---

### Phase 4: Enterprise Control Plane

**Goal:** Dashboard, policies, and multi-tenant support.

**Implementation:**
1. Web dashboard for viewing all agents, tasks, provenance
2. Policy engine (who can claim which tasks, review gates)
3. Multi-repo support (manage multiple projects)
4. SSO/auth integration
5. Audit logging and compliance reports

**Scope:** ~8-12 weeks

---

## Change ID Challenge Deep Dive

The hardest problem: **JJ change IDs are local**.

When machine A creates change `abc123` and pushes to Git, machine B imports and gets a *different* change ID `xyz789` for the same content.

**Why this matters:**
- Provenance links to change IDs
- Tasks reference change IDs
- Blame queries use change IDs

**Solutions:**

1. **Use Git commit SHAs as universal IDs**
   - Change IDs are JJ-local, commit SHAs are Git-global
   - Store both in provenance: `change_id=abc123 commit_sha=def456`
   - Cross-machine queries use commit SHA
   - Downside: SHA changes on rebase (but so does content)

2. **Generate deterministic change IDs**
   - Hash content + metadata to get same ID everywhere
   - Requires JJ core changes
   - Would enable true distributed JJ

3. **Provenance by content hash**
   - Store provenance keyed by content hash, not change ID
   - Content hash survives Git round-trip
   - Query: "who wrote these exact bytes?"

**Recommendation:** Use Git commit SHAs as the cross-machine identifier, keep change IDs for local JJ operations. The `[aiki]` block stores both:

```
[aiki]
agent=claude-code
change_id=abc123        # Local JJ change ID
commit_sha=def456       # Git commit SHA (stable across machines)
session=session-xyz
[/aiki]
```

---

## Open Questions

1. **Conflict resolution for tasks branch?**
   - CRDT-style merge (all task mutations commute)?
   - Last-writer-wins with conflict markers?
   - Coordinator as arbiter?

2. **Real-time requirements?**
   - How fast must task updates propagate?
   - Is Git push/pull latency acceptable (~seconds)?
   - Need WebSocket for sub-second updates?

3. **Multi-repo coordination?**
   - Can one control plane manage multiple repos?
   - Per-repo vs global task namespace?

4. **Offline mode?**
   - How do agents work when disconnected?
   - Queue operations for later sync?

5. **Security model?**
   - Who can push to tasks branch?
   - Agent authentication/authorization?
   - Signed tasks/provenance?

---

## Success Criteria

- ✅ Two machines can share tasks via Git remote
- ✅ Provenance survives Git round-trip (via trailers)
- ✅ Coordinator tracks active sessions across machines
- ✅ Agent on machine A can see task created on machine B
- ✅ Real-time event stream for connected agents
- ✅ Less than 5 second latency for task sync
- ✅ Works with standard Git hosting (GitHub, GitLab, etc.)

---

---

## Deep Dive: Option 2 - Shared JJ Repository Server

### Why This Is The Ideal Solution

JJ workspaces already solve multi-agent coordination *locally*:
- Multiple workspaces share one `.jj/` directory
- All agents see the same change graph
- Change IDs are consistent (single repo)
- Provenance visible everywhere

**The gap:** JJ assumes local filesystem access. Remote machines can't share `.jj/`.

### What Would It Take?

#### Layer 1: JJ Operation Protocol

JJ operations are transactional. Each operation:
1. Reads repo state
2. Computes new state
3. Writes atomically to operation log

We'd need a **wire protocol** for these operations:

```
┌─────────────────────────────────────────────────────────┐
│                  JJ Operation Protocol                   │
├─────────────────────────────────────────────────────────┤
│  Request Types:                                          │
│  - ReadView(op_id) → RepoView                           │
│  - WriteOp(parent_op, mutation) → OpResult              │
│  - WatchOps(since_op) → Stream<Op>                      │
│                                                          │
│  Mutation Types:                                         │
│  - CreateChange(parent, tree)                           │
│  - UpdateDescription(change_id, desc)                   │
│  - RebaseChange(change_id, new_parent)                  │
│  - AbandonChange(change_id)                             │
│  - SetBookmark(name, change_id)                         │
│                                                          │
│  Data Types:                                             │
│  - RepoView: heads, bookmarks, tags, operations         │
│  - Change: id, commit_id, description, tree_id          │
│  - Tree: file entries with hashes                       │
│  - Blob: file content                                   │
└─────────────────────────────────────────────────────────┘
```

**Estimated effort:** 4-6 weeks to design, 8-12 weeks to implement.

#### Layer 2: Server Component

A service that:
1. Hosts the authoritative `.jj/` repository
2. Handles concurrent operations (locking/OCC)
3. Serves blobs on demand (lazy fetch)
4. Broadcasts operations to connected clients

```rust
// Pseudocode for JJ server
struct JJServer {
    repo: Arc<RwLock<JJRepository>>,
    clients: HashMap<ClientId, ClientConnection>,
    op_log: OperationLog,
}

impl JJServer {
    async fn handle_write_op(&self, req: WriteOpRequest) -> Result<OpResult> {
        let mut repo = self.repo.write().await;

        // Optimistic concurrency: check parent op is still head
        if repo.op_heads() != vec![req.parent_op] {
            return Err(ConflictError::StaleParent);
        }

        // Apply mutation
        let new_op = repo.apply_mutation(req.mutation)?;

        // Broadcast to other clients
        self.broadcast_op(&new_op).await;

        Ok(OpResult::Success(new_op.id))
    }

    async fn handle_read_blob(&self, hash: BlobHash) -> Result<Blob> {
        self.repo.read().await.store().get_blob(&hash)
    }
}
```

**Estimated effort:** 6-10 weeks.

#### Layer 3: Client Proxy

Local `aiki` CLI talks to server instead of local `.jj/`:

```rust
// Client wraps JJ operations to go over network
struct RemoteJJClient {
    server_url: Url,
    local_cache: BlobCache,  // Cache blobs locally
    current_op: OpId,        // Last known op head
}

impl RemoteJJClient {
    fn describe(&self, change_id: &ChangeId, desc: &str) -> Result<()> {
        let mutation = Mutation::UpdateDescription {
            change_id: change_id.clone(),
            description: desc.to_string(),
        };

        self.send_write_op(mutation)?;
        Ok(())
    }

    fn get_file(&self, change_id: &ChangeId, path: &Path) -> Result<Vec<u8>> {
        // Try local cache first
        if let Some(blob) = self.local_cache.get(change_id, path) {
            return Ok(blob);
        }

        // Fetch from server
        let blob = self.fetch_blob(change_id, path)?;
        self.local_cache.insert(change_id, path, &blob);
        Ok(blob)
    }
}
```

**Estimated effort:** 4-6 weeks.

#### Layer 4: Working Copy Sync

The hardest part: keeping local files in sync with remote repo state.

Options:
1. **Full sync**: Download entire tree on every operation (slow)
2. **Lazy sync**: Only fetch files when accessed (complex)
3. **Virtual filesystem**: FUSE mount (OS-specific, complex)
4. **Hybrid**: Sync active files, lazy fetch rest

```
┌──────────────────────────────────────────────────────────┐
│                    Working Copy Sync                      │
├──────────────────────────────────────────────────────────┤
│                                                           │
│  Local Machine                    JJ Server              │
│  ┌─────────────────┐             ┌─────────────────┐     │
│  │ ./src/main.rs   │◄───fetch────│ Tree: abc123    │     │
│  │ ./Cargo.toml    │             │  src/main.rs    │     │
│  │ ...             │             │  Cargo.toml     │     │
│  └─────────────────┘             │  ...            │     │
│         │                        └─────────────────┘     │
│         │ edit                           ▲               │
│         ▼                                │               │
│  ┌─────────────────┐                     │               │
│  │ Modified file   │─────upload──────────┘               │
│  └─────────────────┘                                     │
│                                                           │
│  Challenge: How to detect local edits?                   │
│  - inotify/FSEvents (OS-specific)                        │
│  - Polling (slow, battery drain)                         │
│  - Editor hooks (limited)                                │
│                                                           │
└──────────────────────────────────────────────────────────┘
```

**Estimated effort:** 8-12 weeks (this is the hard part).

### Total Effort Estimate

| Component | Weeks | Complexity |
|-----------|-------|------------|
| Protocol design | 4-6 | Medium |
| Server implementation | 6-10 | High |
| Client proxy | 4-6 | Medium |
| Working copy sync | 8-12 | Very High |
| Testing & hardening | 4-6 | Medium |
| **Total** | **26-40 weeks** | - |

That's **6-10 months** of focused engineering for a proper implementation.

### Shortcuts That Could Reduce Scope

#### Shortcut A: Server-Side Execution Only

Don't sync working copy. All file operations happen on server:

```
Client                          Server
  │                               │
  │  aiki describe "fix bug"      │
  │ ────────────────────────────► │
  │                               │  Execute in server workspace
  │  ◄──────────────────────────  │
  │       Result: OK              │
```

**Pros:** No working copy sync problem
**Cons:** Can't run local tools, editors don't work
**Effort reduction:** -8 weeks (skip Layer 4)
**Use case:** CI/CD, automated agents, not human developers

#### Shortcut B: Git as Transport, JJ as Index

Keep Git for file sync. Use JJ server only for:
- Change ID registry (assign globally unique IDs)
- Provenance metadata
- Task state

```
┌─────────────────────────────────────────────────────────┐
│                    Hybrid Architecture                   │
├─────────────────────────────────────────────────────────┤
│                                                          │
│   Git Remote (files)         JJ Server (metadata)       │
│   ┌─────────────────┐       ┌─────────────────────┐     │
│   │ origin/main     │       │ Change Registry      │     │
│   │ origin/feature  │       │  change_id → commit  │     │
│   └─────────────────┘       │  provenance[sha]     │     │
│          ▲                  │  tasks               │     │
│          │                  └─────────────────────┘     │
│          │ git push/pull           ▲                    │
│          │                         │ gRPC/REST          │
│          │                         │                    │
│   ┌──────┴─────────────────────────┴────────┐          │
│   │              Local Machine               │          │
│   │   .git/  (files)    aiki (metadata)     │          │
│   │   .jj/   (local JJ for change tracking) │          │
│   └─────────────────────────────────────────┘          │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

**Pros:**
- Git handles file sync (solved problem)
- JJ server only handles lightweight metadata
- Local JJ still works for change tracking

**Cons:**
- Change IDs still diverge (but we map them via registry)
- Two systems to understand

**Effort reduction:** -12 weeks (skip Layers 1, 4)
**Effective effort:** 10-16 weeks

This is essentially **Option 5 (Hybrid) with a change ID registry**.

#### Shortcut C: JJ-over-SSH

Use SSH as transport. Server exposes JJ commands over SSH:

```bash
# On client
ssh jj-server "jj describe -r @- -m 'fix bug'"
```

**Pros:**
- Simple to implement
- SSH handles auth, encryption
- Can use existing JJ CLI

**Cons:**
- Latency on every operation
- Working copy still local (same sync problem)
- Not a real API

**Effort:** 2-4 weeks for basic version
**Use case:** Quick hack for small teams

### Recommendation: Staged Approach

Instead of building full Option 2 upfront:

**Stage 1: Hybrid (Option 5)** - 3-4 months
- Git for file sync
- Lightweight metadata service (tasks, provenance mapping)
- Real-time coordination via WebSocket
- *This unblocks commercial use*

**Stage 2: Change ID Registry** - 2-3 months
- Central service assigns globally unique change IDs
- Maps Git commits ↔ JJ change IDs
- Enables cross-machine provenance queries

**Stage 3: Full JJ Server** - 6-10 months (if needed)
- Only build if Stage 1-2 prove insufficient
- By then, JJ community may have native solution
- Consider contributing to JJ core instead

### JJ Community Considerations

Before building our own:

1. **Check JJ roadmap**: Is remote/server mode planned?
2. **Discuss with JJ maintainers**: Would they accept server-mode PRs?
3. **Look at existing attempts**: Anyone else building this?

If JJ adds native server support, our effort would be wasted. Better to:
- Build the lightweight hybrid first (Option 5)
- Monitor JJ development
- Contribute to JJ if we need server mode

---

## Related Documentation

- `ops/ROADMAP.md` - Phase 21: Shared JJ Brain & Team Coordination
- `ops/later/WORKSPACE_AUTOMATION.md` - Local multi-agent workspaces
- `ops/later/AIKI_TWIN.md` - Personalized review agents
- `AGENTS.md` - Task system and aiki CLI usage
