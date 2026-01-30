# Sync Remote JJ Systems for Centralized Control Plane

## Vision: GitHub for Agents

**What GitHub did for human developers, Aiki does for AI agents.**

GitHub transformed software development by providing:
- Central place to store and share code
- Collaboration primitives (PRs, issues, reviews, discussions)
- Identity and attribution (who contributed what)
- Complete audit trail (git history)
- Discoverability (repos, packages, developers)

**Aiki as "GitHub for Agents" provides:**
- Central place for agents to push work and coordinate
- Agent collaboration primitives (tasks, handoffs, reviews)
- Agent identity and provenance (who wrote what, when, how confident)
- Complete audit trail (JJ change history with `[aiki]` metadata)
- Agent discoverability (which agents, what capabilities, track records)

```
┌─────────────────────────────────────────────────────────────────┐
│                     AIKI CLOUD (v0)                              │
│           "GitHub for Agents" - Centralized Control Plane        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                    Agent Registry                        │   │
│   │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐       │   │
│   │  │ Claude  │ │ Cursor  │ │ Copilot │ │ Custom  │       │   │
│   │  │  Code   │ │  Agent  │ │   X     │ │ Agent   │       │   │
│   │  └─────────┘ └─────────┘ └─────────┘ └─────────┘       │   │
│   └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                  Coordination Layer                      │   │
│   │  • Task assignment & handoffs                            │   │
│   │  • Real-time status (WebSocket)                          │   │
│   │  • Conflict detection                                    │   │
│   │  • Review gates                                          │   │
│   └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                  Provenance Layer                        │   │
│   │  • Change attribution (agent, session, confidence)       │   │
│   │  • Cross-repo provenance queries                         │   │
│   │  • Audit trail & compliance                              │   │
│   │  • Quality metrics per agent                             │   │
│   └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                   Storage Layer                          │   │
│   │  • Git remotes (code)                                    │   │
│   │  • JJ metadata (changes, descriptions)                   │   │
│   │  • Tasks branch sync                                     │   │
│   │  • Blob storage (artifacts, logs)                        │   │
│   └─────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
           ▲              ▲              ▲              ▲
           │              │              │              │
     ┌─────┴─────┐  ┌─────┴─────┐  ┌─────┴─────┐  ┌─────┴─────┐
     │ Dev       │  │ CI/CD     │  │ Review    │  │ Prod      │
     │ Machine   │  │ Runner    │  │ Agent     │  │ Server    │
     │ (Claude)  │  │ (Codex)   │  │ (Aiki)    │  │ (Custom)  │
     └───────────┘  └───────────┘  └───────────┘  └───────────┘
```

### Why "GitHub for Agents" Wins

1. **Network effects**: More agents → more value → more agents
2. **Data moat**: Provenance data across millions of agent sessions
3. **Platform lock-in**: Agents integrate with Aiki, not just Git
4. **Enterprise value**: Governance, compliance, audit trails
5. **Pricing leverage**: Per-agent-seat, per-repo, per-API-call

### Competitive Landscape

| Player | Focus | Gap |
|--------|-------|-----|
| GitHub Copilot | Code completion | No agent coordination |
| Cursor | IDE-integrated agent | Single-machine only |
| Devin | Autonomous agent | No provenance/audit |
| Replit Agent | Cloud dev environment | No multi-agent |
| **Aiki Cloud** | Agent coordination + provenance | This is the gap |

---

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

## Aiki Cloud MVP: The v0

### What Ships in v0

**Core thesis:** Ship the minimum that enables multi-machine agent coordination with provenance. Everything else can wait.

#### Must Have (MVP)

1. **Agent Registration**
   - Agents authenticate with API key
   - Track agent type, version, capabilities
   - Assign globally unique agent IDs

2. **Session Tracking**
   - Register session start/end
   - Track which agent, which repo, which machine
   - Real-time session status

3. **Task Sync**
   - Push/pull tasks to central store
   - Task assignment to specific agents
   - Task state visible across all agents

4. **Provenance Ingestion**
   - Accept `[aiki]` metadata on push
   - Index by repo, agent, session, file
   - Query: "who wrote this code?"

5. **Git Integration**
   - Webhook on push to trigger provenance ingestion
   - Read Git commits, extract trailers
   - Link Git SHAs to provenance records

#### Nice to Have (v0.1+)

- Web dashboard
- Agent quality metrics
- Review workflows
- Conflict detection
- Multi-repo support

### API Design (v0)

```yaml
# OpenAPI 3.0 sketch
openapi: 3.0.0
info:
  title: Aiki Cloud API
  version: 0.1.0
  description: GitHub for Agents - Centralized Control Plane

paths:
  # === Agent Registry ===
  /api/v1/agents:
    post:
      summary: Register a new agent
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                agent_type: { type: string, example: "claude-code" }
                version: { type: string, example: "1.0.32" }
                capabilities: { type: array, items: { type: string } }
      responses:
        201:
          description: Agent registered
          content:
            application/json:
              schema:
                type: object
                properties:
                  agent_id: { type: string, format: uuid }
                  api_key: { type: string }

  # === Sessions ===
  /api/v1/sessions:
    post:
      summary: Start a new agent session
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                agent_id: { type: string, format: uuid }
                repo_url: { type: string }
                machine_id: { type: string }
      responses:
        201:
          description: Session started
          content:
            application/json:
              schema:
                type: object
                properties:
                  session_id: { type: string }

  /api/v1/sessions/{session_id}:
    patch:
      summary: Update session (heartbeat, status)
    delete:
      summary: End session

  # === Tasks ===
  /api/v1/repos/{repo}/tasks:
    get:
      summary: List tasks for a repo
      parameters:
        - name: status
          in: query
          schema: { type: string, enum: [pending, in_progress, completed] }
        - name: assigned_to
          in: query
          schema: { type: string, format: uuid }
    post:
      summary: Create a new task

  /api/v1/repos/{repo}/tasks/{task_id}:
    get:
      summary: Get task details
    patch:
      summary: Update task (status, assignee, comments)

  /api/v1/repos/{repo}/tasks/{task_id}/claim:
    post:
      summary: Claim a task for an agent
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                agent_id: { type: string, format: uuid }
                session_id: { type: string }

  # === Provenance ===
  /api/v1/repos/{repo}/provenance:
    post:
      summary: Ingest provenance record
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                commit_sha: { type: string }
                change_id: { type: string }
                agent_id: { type: string, format: uuid }
                session_id: { type: string }
                files: { type: array, items: { type: string } }
                confidence: { type: string, enum: [low, medium, high] }
                metadata: { type: object }

  /api/v1/repos/{repo}/provenance/query:
    post:
      summary: Query provenance
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                file_path: { type: string }
                commit_sha: { type: string }
                agent_id: { type: string }
                since: { type: string, format: date-time }

  # === Real-time ===
  /api/v1/ws:
    get:
      summary: WebSocket for real-time events
      description: |
        Events streamed:
        - session.started
        - session.ended
        - task.created
        - task.claimed
        - task.completed
        - provenance.ingested
```

### Data Model

```sql
-- Core entities for Aiki Cloud

-- Agents (registered AI coding assistants)
CREATE TABLE agents (
    id UUID PRIMARY KEY,
    agent_type VARCHAR(50) NOT NULL,  -- 'claude-code', 'cursor', etc.
    version VARCHAR(20),
    api_key_hash VARCHAR(64) NOT NULL,
    capabilities JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ
);

-- Sessions (agent working sessions)
CREATE TABLE sessions (
    id VARCHAR(64) PRIMARY KEY,
    agent_id UUID REFERENCES agents(id),
    repo_url VARCHAR(500) NOT NULL,
    machine_id VARCHAR(100),
    started_at TIMESTAMPTZ DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    status VARCHAR(20) DEFAULT 'active'
);

-- Tasks (work items for agents)
CREATE TABLE tasks (
    id VARCHAR(32) PRIMARY KEY,  -- aiki task ID format
    repo_url VARCHAR(500) NOT NULL,
    content TEXT NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',
    priority VARCHAR(10) DEFAULT 'p2',
    assigned_to UUID REFERENCES agents(id),
    assigned_session VARCHAR(64) REFERENCES sessions(id),
    parent_id VARCHAR(32) REFERENCES tasks(id),
    source JSONB,  -- { type: 'file', path: 'ops/now/plan.md' }
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Task comments
CREATE TABLE task_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id VARCHAR(32) REFERENCES tasks(id),
    agent_id UUID REFERENCES agents(id),
    session_id VARCHAR(64) REFERENCES sessions(id),
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Provenance records
CREATE TABLE provenance (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repo_url VARCHAR(500) NOT NULL,
    commit_sha VARCHAR(40) NOT NULL,
    change_id VARCHAR(64),  -- JJ change ID (local to originating machine)
    agent_id UUID REFERENCES agents(id),
    session_id VARCHAR(64) REFERENCES sessions(id),
    file_path VARCHAR(1000) NOT NULL,
    confidence VARCHAR(10),
    method VARCHAR(20),  -- 'hook', 'manual', 'inferred'
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_provenance_repo_file ON provenance(repo_url, file_path);
CREATE INDEX idx_provenance_commit ON provenance(commit_sha);
CREATE INDEX idx_provenance_agent ON provenance(agent_id);
CREATE INDEX idx_tasks_repo_status ON tasks(repo_url, status);
CREATE INDEX idx_sessions_agent ON sessions(agent_id);
```

### CLI Integration

The `aiki` CLI gets new commands for cloud sync:

```bash
# Configure cloud connection
aiki cloud login                    # Authenticate with Aiki Cloud
aiki cloud status                   # Show connection status

# Sync operations
aiki sync                           # Pull + push tasks and provenance
aiki sync push                      # Push local state to cloud
aiki sync pull                      # Pull cloud state to local

# Auto-sync mode (background daemon)
aiki sync watch                     # Continuous sync in background

# Task operations (enhanced for cloud)
aiki task list --cloud              # Show tasks from cloud (not just local)
aiki task claim <id>                # Claim a task from cloud
aiki task handoff <id> --to <agent> # Hand off task to another agent
```

### Sync Protocol

```
┌─────────────────────────────────────────────────────────────────┐
│                     Sync Protocol Flow                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Local Machine                        Aiki Cloud                │
│       │                                    │                     │
│       │  1. POST /sessions (start)         │                     │
│       │ ─────────────────────────────────► │                     │
│       │                                    │                     │
│       │  2. GET /tasks (pull)              │                     │
│       │ ─────────────────────────────────► │                     │
│       │  ◄───────────────────────────────  │                     │
│       │     [task list]                    │                     │
│       │                                    │                     │
│       │  3. Agent does work locally        │                     │
│       │     (creates changes in JJ)        │                     │
│       │                                    │                     │
│       │  4. git push origin main           │                     │
│       │ ─────────────────────────────────► │  (Git remote)      │
│       │                                    │                     │
│       │  5. POST /provenance (sync)        │                     │
│       │ ─────────────────────────────────► │                     │
│       │     [commit_sha, change_id,        │                     │
│       │      agent_id, files, metadata]    │                     │
│       │                                    │                     │
│       │  6. PATCH /tasks/{id} (update)     │                     │
│       │ ─────────────────────────────────► │                     │
│       │                                    │                     │
│       │  7. WebSocket: task.claimed        │                     │
│       │  ◄─────────────────────────────── │  (to other agents) │
│       │                                    │                     │
│       │  8. DELETE /sessions (end)         │                     │
│       │ ─────────────────────────────────► │                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Infrastructure (v0)

Keep it simple:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Aiki Cloud Infrastructure                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                   Load Balancer                          │   │
│   │              (Cloudflare / AWS ALB)                      │   │
│   └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                   API Server (Rust)                      │   │
│   │   • axum for HTTP                                        │   │
│   │   • tokio-tungstenite for WebSocket                     │   │
│   │   • sqlx for PostgreSQL                                  │   │
│   │   • 2-3 instances for HA                                │   │
│   └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│   ┌──────────────────┐   ┌──────────────────┐                  │
│   │   PostgreSQL     │   │   Redis          │                  │
│   │   (RDS / Neon)   │   │   (session cache,│                  │
│   │                  │   │    pub/sub)      │                  │
│   └──────────────────┘   └──────────────────┘                  │
│                                                                  │
│   Estimated monthly cost (low traffic): $50-100                 │
│   Estimated monthly cost (production): $300-500                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Development Timeline

| Phase | Scope | Duration |
|-------|-------|----------|
| **v0.0** | API design, data model, CLI skeleton | 2 weeks |
| **v0.1** | Agent registration, session tracking | 2 weeks |
| **v0.2** | Task sync (CRUD, claim, handoff) | 3 weeks |
| **v0.3** | Provenance ingestion, basic queries | 3 weeks |
| **v0.4** | WebSocket real-time events | 2 weeks |
| **v0.5** | CLI integration, `aiki sync` | 2 weeks |
| **v0.6** | Git webhook integration | 1 week |
| **v0.7** | Testing, docs, hardening | 2 weeks |
| **Total** | MVP ready for beta | **~17 weeks** |

### Success Metrics for v0

- [ ] Two machines can share tasks in real-time
- [ ] Provenance survives git push/pull cycle
- [ ] Agent A sees changes from Agent B within 5 seconds
- [ ] 99.9% API uptime
- [ ] <100ms P95 latency for task operations
- [ ] 5+ beta users running multi-agent workflows

---

## Related Documentation

- `ops/ROADMAP.md` - Phase 21: Shared JJ Brain & Team Coordination
- `ops/later/WORKSPACE_AUTOMATION.md` - Local multi-agent workspaces
- `ops/later/AIKI_TWIN.md` - Personalized review agents
- `AGENTS.md` - Task system and aiki CLI usage
