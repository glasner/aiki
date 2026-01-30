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

## Related Documentation

- `ops/ROADMAP.md` - Phase 21: Shared JJ Brain & Team Coordination
- `ops/later/WORKSPACE_AUTOMATION.md` - Local multi-agent workspaces
- `ops/later/AIKI_TWIN.md` - Personalized review agents
- `AGENTS.md` - Task system and aiki CLI usage
