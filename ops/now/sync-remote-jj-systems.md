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

## Deep Dive: Native JJ Remotes (The Real Solution)

### The Insight

Git didn't become ubiquitous because of GitHub. Git became ubiquitous because **git itself** has native remote support. GitHub just provided hosting.

```bash
# Git's killer feature: native remote protocol
git remote add origin git@github.com:user/repo.git
git push origin main
git fetch origin
```

JJ currently piggybacks on Git remotes via `jj git push/fetch`. But this only syncs the **Git layer**, not JJ's native concepts:

| Concept | Syncs via Git? | Problem |
|---------|---------------|---------|
| File content | ✅ Yes | - |
| Git commits | ✅ Yes | - |
| **Change IDs** | ❌ No | Regenerated on import |
| **Operation log** | ❌ No | Local only |
| **Change descriptions** | ⚠️ Partial | Only if committed to Git |
| **Bookmarks** | ⚠️ Partial | Maps to Git branches |

**What we want: native JJ remotes that sync JJ-native concepts.**

### What "JJ Remotes" Would Look Like

```bash
# Native JJ remote support (doesn't exist yet)
jj remote add origin jj://cloud.aiki.dev/user/repo
jj push origin @
jj fetch origin
jj pull origin  # fetch + merge
```

What syncs:
- ✅ Change IDs (stable across machines)
- ✅ Change descriptions (including `[aiki]` metadata)
- ✅ Operation log (full history)
- ✅ Bookmarks (native, not mapped to Git branches)
- ✅ File content (via content-addressed store)

### Protocol Design: JJ Remote Protocol

```
┌─────────────────────────────────────────────────────────────────┐
│                    JJ Remote Protocol (v1)                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Transport: HTTPS + optional SSH                                │
│  Encoding: Protobuf or MessagePack                              │
│  Auth: API keys, SSH keys, or OAuth                             │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Endpoints                               │  │
│  ├───────────────────────────────────────────────────────────┤  │
│  │                                                            │  │
│  │  GET  /refs                                                │  │
│  │       → { bookmarks: [...], tags: [...], heads: [...] }   │  │
│  │                                                            │  │
│  │  GET  /ops?since=<op_id>                                  │  │
│  │       → [Operation, Operation, ...]                       │  │
│  │                                                            │  │
│  │  GET  /changes/<change_id>                                │  │
│  │       → { change_id, commit_ids: [...], description }     │  │
│  │                                                            │  │
│  │  GET  /commits/<commit_id>                                │  │
│  │       → { tree_id, parent_ids, ... }                      │  │
│  │                                                            │  │
│  │  GET  /trees/<tree_id>                                    │  │
│  │       → { entries: [{ name, blob_id, mode }, ...] }       │  │
│  │                                                            │  │
│  │  GET  /blobs/<blob_id>                                    │  │
│  │       → <raw bytes>                                        │  │
│  │                                                            │  │
│  │  POST /push                                                │  │
│  │       ← { ops: [...], changes: [...], commits: [...],     │  │
│  │           trees: [...], blobs: [...] }                    │  │
│  │       → { ok: true } or { conflict: ... }                 │  │
│  │                                                            │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Data Model: What Gets Synced

```
┌─────────────────────────────────────────────────────────────────┐
│                    JJ Repository Structure                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Operation Log (append-only)                                    │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  op_001 → op_002 → op_003 → ... → op_head               │   │
│  │                                                          │   │
│  │  Each operation records:                                 │   │
│  │  - Parent operation(s)                                   │   │
│  │  - Mutation (what changed)                               │   │
│  │  - Timestamp, hostname, user                             │   │
│  │  - View (snapshot of refs at that point)                 │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  View (current state)                                           │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  heads: [change_A, change_B, ...]                       │   │
│  │  bookmarks: { main: change_X, feature: change_Y }       │   │
│  │  tags: { v1.0: change_Z }                               │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  Changes (mutable, identified by stable change_id)              │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  change_A:                                               │   │
│  │    change_id: "abc123..."  (STABLE - never changes)     │   │
│  │    commit_ids: ["def456..."]  (changes on rewrite)      │   │
│  │    description: "Fix bug\n\n[aiki]\nagent=claude\n..."  │   │
│  │    parents: [change_B]                                   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  Commits (immutable, content-addressed)                         │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  commit_def456:                                          │   │
│  │    tree_id: "789abc..."                                  │   │
│  │    parent_commit_ids: [...]                              │   │
│  │    author, committer, etc.                               │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  Trees + Blobs (immutable, content-addressed)                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Same as Git - content-addressed storage                 │   │
│  │  Can literally use Git packfiles for efficiency          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### The Key Insight: Operations Are The Unit of Sync

Git syncs **commits**. JJ should sync **operations**.

```
Machine A                          Machine B
    │                                  │
    │  op_001: create change_X         │
    │  op_002: describe change_X       │
    │  op_003: create change_Y         │
    │                                  │
    │ ─────── jj push ───────────────► │
    │                                  │  (imports operations)
    │                                  │  op_001, op_002, op_003
    │                                  │
    │                                  │  op_004: rebase change_Y
    │                                  │
    │ ◄─────── jj fetch ────────────── │
    │                                  │
    │  (imports op_004)                │
    │  change_Y now rebased here too   │
```

**Why this works:**
- Operations are append-only (like Git commits)
- Operations can have multiple heads (like Git branches)
- Merge = reconcile divergent operation heads
- Change IDs remain stable because they're part of the operation

### Sync Semantics

#### Push

```rust
fn push(local: &Repo, remote: &Remote) -> Result<()> {
    // 1. Find operations remote doesn't have
    let remote_ops = remote.fetch_op_heads()?;
    let missing_ops = local.ops_not_in(&remote_ops);

    // 2. Collect all objects referenced by missing ops
    let mut objects = ObjectSet::new();
    for op in &missing_ops {
        objects.extend(op.referenced_changes());
        objects.extend(op.referenced_commits());
        objects.extend(op.referenced_trees());
        objects.extend(op.referenced_blobs());
    }

    // 3. Push objects then operations
    remote.push_objects(&objects)?;
    remote.push_ops(&missing_ops)?;

    Ok(())
}
```

#### Fetch

```rust
fn fetch(local: &mut Repo, remote: &Remote) -> Result<()> {
    // 1. Get remote operation heads
    let remote_ops = remote.fetch_op_heads()?;
    let missing_ops = remote.ops_not_in(&local.op_heads())?;

    // 2. Fetch operations (with referenced objects)
    for op in missing_ops {
        let objects = remote.fetch_objects_for_op(&op)?;
        local.import_objects(&objects)?;
        local.import_op(&op)?;
    }

    // 3. If divergent, create merge operation
    if local.op_heads().len() > 1 {
        local.merge_op_heads()?;
    }

    Ok(())
}
```

#### Conflicts

What happens when two machines make conflicting changes?

```
Machine A: op_001 → op_002 (edit file.rs)
Machine B: op_001 → op_003 (also edit file.rs)

After sync:
           ┌─ op_002 (A's edit)
op_001 ───┤
           └─ op_003 (B's edit)
                    ↓
              op_004 (merge)
```

JJ already handles this! Conflicts become first-class citizens in the working copy. The user resolves them, and resolution becomes a new operation.

### Implementation Path

#### Phase 1: Protocol Specification (4 weeks)

Deliverables:
- Wire protocol spec (protobuf schemas)
- Authentication model
- Conflict resolution semantics
- Reference implementation in Rust

##### Week 1-2: Core Protocol Design

**Wire Format (Protobuf)**

```protobuf
syntax = "proto3";
package jj.remote.v1;

// === Core Identifiers ===

message ChangeId {
  bytes id = 1;  // 32 bytes, hex-encoded in UI
}

message CommitId {
  bytes id = 1;  // SHA-256 hash
}

message OperationId {
  bytes id = 1;  // SHA-256 hash of operation content
}

message TreeId {
  bytes id = 1;
}

message BlobId {
  bytes id = 1;
}

// === Repository State ===

message RepoView {
  repeated ChangeId head_ids = 1;
  map<string, ChangeId> bookmarks = 2;  // name -> change_id
  map<string, ChangeId> tags = 3;
  repeated OperationId op_heads = 4;
}

message Change {
  ChangeId change_id = 1;
  repeated CommitId commit_ids = 2;  // History of commits for this change
  string description = 3;
  repeated ChangeId parent_ids = 4;
}

message Commit {
  CommitId commit_id = 1;
  TreeId tree_id = 2;
  repeated CommitId parent_ids = 3;
  Signature author = 4;
  Signature committer = 5;
  string description = 6;  // May differ from Change description
}

message Signature {
  string name = 1;
  string email = 2;
  Timestamp timestamp = 3;
}

message Timestamp {
  int64 seconds = 1;
  int32 nanos = 2;
  int32 tz_offset_minutes = 3;
}

message TreeEntry {
  string name = 1;
  oneof value {
    BlobId blob_id = 2;
    TreeId subtree_id = 3;
    ConflictId conflict_id = 4;
  }
  bool executable = 5;
  bool symlink = 6;
}

message Tree {
  TreeId tree_id = 1;
  repeated TreeEntry entries = 2;
}

// === Operations ===

message Operation {
  OperationId id = 1;
  repeated OperationId parent_ids = 2;
  OperationMetadata metadata = 3;
  RepoView view = 4;  // State after this operation

  oneof mutation {
    CreateChange create_change = 10;
    UpdateChange update_change = 11;
    AbandonChange abandon_change = 12;
    SetBookmark set_bookmark = 13;
    DeleteBookmark delete_bookmark = 14;
    SetTag set_tag = 15;
    // ... more mutation types
  }
}

message OperationMetadata {
  Timestamp timestamp = 1;
  string hostname = 2;
  string username = 3;
  string description = 4;  // Human-readable description of operation
  map<string, string> tags = 5;  // Extensible metadata
}

message CreateChange {
  ChangeId change_id = 1;
  repeated ChangeId parent_ids = 2;
  CommitId initial_commit_id = 3;
}

message UpdateChange {
  ChangeId change_id = 1;
  optional string new_description = 2;
  optional CommitId new_commit_id = 3;
  repeated ChangeId new_parent_ids = 4;
}

message AbandonChange {
  ChangeId change_id = 1;
}

message SetBookmark {
  string name = 1;
  ChangeId target = 2;
}

message DeleteBookmark {
  string name = 1;
}

// === RPC Messages ===

message GetRefsRequest {}
message GetRefsResponse {
  RepoView view = 1;
}

message FetchOpsRequest {
  repeated OperationId known_ops = 1;  // Client's current op heads
  uint32 max_ops = 2;  // Limit for pagination
}

message FetchOpsResponse {
  repeated Operation operations = 1;
  bool has_more = 2;
}

message FetchObjectsRequest {
  repeated ChangeId change_ids = 1;
  repeated CommitId commit_ids = 2;
  repeated TreeId tree_ids = 3;
  repeated BlobId blob_ids = 4;
}

message FetchObjectsResponse {
  repeated Change changes = 1;
  repeated Commit commits = 2;
  repeated Tree trees = 3;
  map<string, bytes> blobs = 4;  // blob_id (hex) -> content
}

message PushRequest {
  repeated Operation operations = 1;
  repeated Change changes = 2;
  repeated Commit commits = 3;
  repeated Tree trees = 4;
  map<string, bytes> blobs = 5;

  // Expected state (for optimistic concurrency)
  repeated OperationId expected_op_heads = 6;
}

message PushResponse {
  bool success = 1;
  optional PushConflict conflict = 2;
  repeated OperationId new_op_heads = 3;
}

message PushConflict {
  enum ConflictType {
    STALE_OP_HEAD = 0;      // Remote has newer ops
    BOOKMARK_CONFLICT = 1;  // Bookmark moved
    PERMISSION_DENIED = 2;
  }
  ConflictType type = 1;
  string message = 2;
  repeated OperationId current_op_heads = 3;  // Actual remote state
}
```

##### Week 3: Authentication Model

```
┌─────────────────────────────────────────────────────────────────┐
│                    Authentication Flow                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Option A: API Key (simple, for agents)                         │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Authorization: Bearer <api_key>                         │   │
│  │  X-Aiki-Agent-Id: <agent_uuid>                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  Option B: SSH Keys (for developers)                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  jj+ssh://cloud.aiki.dev/user/repo                      │   │
│  │  Uses ~/.ssh/id_ed25519 or ssh-agent                    │   │
│  │  Server validates against authorized_keys               │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  Option C: OAuth (for web-based flows)                          │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  jj auth login cloud.aiki.dev                           │   │
│  │  Opens browser → OAuth flow → stores token locally      │   │
│  │  Token stored in ~/.jj/credentials.toml                 │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  Permission Model:                                               │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  repo:read   - Fetch operations and objects             │   │
│  │  repo:write  - Push operations                          │   │
│  │  repo:admin  - Manage permissions, delete repo          │   │
│  │  bookmark:*  - Per-bookmark write permissions           │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

##### Week 4: Conflict Resolution Semantics

```
┌─────────────────────────────────────────────────────────────────┐
│                    Conflict Scenarios                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Scenario 1: Divergent Operation Heads                          │
│  ─────────────────────────────────────                          │
│  Machine A: op_1 → op_2                                         │
│  Machine B: op_1 → op_3                                         │
│                                                                  │
│  Resolution: Both op_2 and op_3 become op heads.                │
│  On fetch, client creates merge operation:                      │
│                                                                  │
│       op_2 ──┐                                                  │
│              ├──► op_4 (merge)                                  │
│       op_3 ──┘                                                  │
│                                                                  │
│  If op_2 and op_3 modified same file → conflict in working copy │
│  If op_2 and op_3 modified different files → auto-merge         │
│                                                                  │
│  ─────────────────────────────────────────────────────────────  │
│                                                                  │
│  Scenario 2: Bookmark Moved                                     │
│  ──────────────────────────                                     │
│  Machine A: bookmark 'main' → change_X                          │
│  Machine B: bookmark 'main' → change_Y                          │
│                                                                  │
│  Resolution options:                                             │
│  a) Last-writer-wins (default)                                  │
│  b) Reject push, require explicit --force                       │
│  c) Create conflict marker bookmark                             │
│                                                                  │
│  Default: (b) Reject, show divergent state:                     │
│  $ jj push                                                      │
│  Error: bookmark 'main' has diverged                            │
│    local:  main → change_X                                      │
│    remote: main → change_Y                                      │
│  Use 'jj push --force' to overwrite, or 'jj fetch' first        │
│                                                                  │
│  ─────────────────────────────────────────────────────────────  │
│                                                                  │
│  Scenario 3: Change Description Conflict                        │
│  ───────────────────────────────────────                        │
│  Machine A: describe change_X "Fix: auth bug"                   │
│  Machine B: describe change_X "Fix: login bug"                  │
│                                                                  │
│  Resolution: Last-writer-wins for descriptions.                 │
│  Both descriptions are in op log for history.                   │
│  UI can show "description changed from X to Y"                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

#### Phase 2: Server Implementation (8 weeks)

##### Week 5-6: Storage Backend

```rust
//! Server storage architecture

use std::path::PathBuf;
use tokio::sync::RwLock;

/// Content-addressed store for immutable objects
pub trait ObjectStore: Send + Sync {
    async fn get_commit(&self, id: &CommitId) -> Result<Option<Commit>>;
    async fn get_tree(&self, id: &TreeId) -> Result<Option<Tree>>;
    async fn get_blob(&self, id: &BlobId) -> Result<Option<Vec<u8>>>;

    async fn put_commit(&self, commit: &Commit) -> Result<CommitId>;
    async fn put_tree(&self, tree: &Tree) -> Result<TreeId>;
    async fn put_blob(&self, content: &[u8]) -> Result<BlobId>;

    /// Check which objects exist (for negotiation)
    async fn has_objects(&self, ids: &ObjectIds) -> Result<ObjectIds>;
}

/// Store for mutable data (changes, operations)
pub trait MetadataStore: Send + Sync {
    // Changes
    async fn get_change(&self, id: &ChangeId) -> Result<Option<Change>>;
    async fn put_change(&self, change: &Change) -> Result<()>;
    async fn list_changes(&self) -> Result<Vec<ChangeId>>;

    // Operations
    async fn get_operation(&self, id: &OperationId) -> Result<Option<Operation>>;
    async fn put_operation(&self, op: &Operation) -> Result<()>;
    async fn get_op_heads(&self) -> Result<Vec<OperationId>>;
    async fn set_op_heads(&self, heads: &[OperationId]) -> Result<()>;

    // Refs (bookmarks, tags)
    async fn get_refs(&self) -> Result<RepoView>;
    async fn update_refs(&self, updates: &RefUpdates) -> Result<()>;
}

/// Storage backend options
pub enum StorageBackend {
    /// Local filesystem (for single-server deployment)
    /// Objects: content-addressed files in objects/
    /// Metadata: SQLite database
    LocalFs {
        root: PathBuf,
    },

    /// Cloud storage (for scalable deployment)
    /// Objects: S3/GCS/R2
    /// Metadata: PostgreSQL
    Cloud {
        object_store: Box<dyn ObjectStore>,
        metadata_store: Box<dyn MetadataStore>,
    },

    /// Git backend (reuse existing Git infrastructure)
    /// Objects: Git packfiles (via gitoxide)
    /// Metadata: Custom refs + loose files
    GitBacked {
        git_dir: PathBuf,
    },
}

// Concrete implementations

/// S3-compatible object store
pub struct S3ObjectStore {
    client: aws_sdk_s3::Client,
    bucket: String,
    prefix: String,
}

/// PostgreSQL metadata store
pub struct PostgresMetadataStore {
    pool: sqlx::PgPool,
}

// SQL schema for metadata
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS changes (
    id BYTEA PRIMARY KEY,
    data BYTEA NOT NULL,  -- Protobuf-encoded Change
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS operations (
    id BYTEA PRIMARY KEY,
    parent_ids BYTEA[] NOT NULL,
    data BYTEA NOT NULL,  -- Protobuf-encoded Operation
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS op_heads (
    repo_id UUID NOT NULL,
    op_id BYTEA NOT NULL,
    PRIMARY KEY (repo_id, op_id)
);

CREATE TABLE IF NOT EXISTS refs (
    repo_id UUID NOT NULL,
    name VARCHAR(255) NOT NULL,
    ref_type VARCHAR(20) NOT NULL,  -- 'bookmark', 'tag'
    target_change_id BYTEA NOT NULL,
    PRIMARY KEY (repo_id, name, ref_type)
);

CREATE INDEX idx_operations_parents ON operations USING GIN (parent_ids);
"#;
```

##### Week 7-8: HTTP API Server

```rust
//! HTTP API server using axum

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

pub struct ServerState {
    pub repos: Arc<dyn RepoManager>,
    pub auth: Arc<dyn AuthProvider>,
}

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        // Repository management
        .route("/api/v1/repos", post(create_repo))
        .route("/api/v1/repos/:owner/:name", get(get_repo_info))

        // JJ Remote Protocol
        .route("/api/v1/repos/:owner/:name/refs", get(get_refs))
        .route("/api/v1/repos/:owner/:name/ops", get(fetch_ops))
        .route("/api/v1/repos/:owner/:name/objects", post(fetch_objects))
        .route("/api/v1/repos/:owner/:name/push", post(push))

        // Blob streaming (large files)
        .route("/api/v1/repos/:owner/:name/blobs/:id", get(get_blob))

        .with_state(Arc::new(state))
}

async fn get_refs(
    State(state): State<Arc<ServerState>>,
    Path((owner, name)): Path<(String, String)>,
    auth: AuthenticatedUser,
) -> Result<Json<GetRefsResponse>, ApiError> {
    let repo = state.repos.get(&owner, &name).await?;
    auth.require_permission(&repo, Permission::Read)?;

    let view = repo.metadata.get_refs().await?;
    Ok(Json(GetRefsResponse { view }))
}

async fn fetch_ops(
    State(state): State<Arc<ServerState>>,
    Path((owner, name)): Path<(String, String)>,
    Query(params): Query<FetchOpsParams>,
    auth: AuthenticatedUser,
) -> Result<Json<FetchOpsResponse>, ApiError> {
    let repo = state.repos.get(&owner, &name).await?;
    auth.require_permission(&repo, Permission::Read)?;

    // Find operations the client doesn't have
    let client_ops: HashSet<_> = params.known_ops.iter().collect();
    let all_ops = repo.metadata.list_ops_since(&params.known_ops).await?;

    let missing_ops: Vec<_> = all_ops
        .into_iter()
        .filter(|op| !client_ops.contains(&op.id))
        .take(params.max_ops.unwrap_or(1000) as usize)
        .collect();

    Ok(Json(FetchOpsResponse {
        operations: missing_ops,
        has_more: all_ops.len() > params.max_ops.unwrap_or(1000) as usize,
    }))
}

async fn push(
    State(state): State<Arc<ServerState>>,
    Path((owner, name)): Path<(String, String)>,
    auth: AuthenticatedUser,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, ApiError> {
    let repo = state.repos.get(&owner, &name).await?;
    auth.require_permission(&repo, Permission::Write)?;

    // Optimistic concurrency check
    let current_heads = repo.metadata.get_op_heads().await?;
    if current_heads != req.expected_op_heads {
        return Ok(Json(PushResponse {
            success: false,
            conflict: Some(PushConflict {
                conflict_type: ConflictType::StaleOpHead,
                message: "Remote has newer operations".into(),
                current_op_heads: current_heads,
            }),
            new_op_heads: vec![],
        }));
    }

    // Store objects (content-addressed, idempotent)
    for (id, blob) in &req.blobs {
        repo.objects.put_blob(blob).await?;
    }
    for tree in &req.trees {
        repo.objects.put_tree(tree).await?;
    }
    for commit in &req.commits {
        repo.objects.put_commit(commit).await?;
    }

    // Store changes and operations
    for change in &req.changes {
        repo.metadata.put_change(change).await?;
    }

    // Validate operation chain
    validate_operation_chain(&req.operations, &current_heads)?;

    for op in &req.operations {
        repo.metadata.put_operation(op).await?;
    }

    // Update op heads
    let new_heads = compute_new_heads(&current_heads, &req.operations);
    repo.metadata.set_op_heads(&new_heads).await?;

    // Broadcast to connected clients (WebSocket)
    state.broadcast_ops(&owner, &name, &req.operations).await;

    Ok(Json(PushResponse {
        success: true,
        conflict: None,
        new_op_heads: new_heads,
    }))
}
```

##### Week 9-10: WebSocket for Real-time Sync

```rust
//! WebSocket handler for real-time operation streaming

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;

pub struct RepoSubscription {
    pub owner: String,
    pub name: String,
    pub tx: broadcast::Sender<Operation>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
    Path((owner, name)): Path<(String, String)>,
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, owner, name, auth))
}

async fn handle_socket(
    socket: WebSocket,
    state: Arc<ServerState>,
    owner: String,
    name: String,
    auth: AuthenticatedUser,
) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to operations for this repo
    let mut rx = state.subscribe_to_repo(&owner, &name);

    // Send operations to client
    let send_task = tokio::spawn(async move {
        while let Ok(op) = rx.recv().await {
            let msg = serde_json::to_string(&WsMessage::Operation(op)).unwrap();
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Receive messages from client
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Handle client messages (e.g., request specific ops)
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
}

#[derive(Serialize, Deserialize)]
enum WsMessage {
    Operation(Operation),
    RefUpdate { name: String, old: ChangeId, new: ChangeId },
    Ping,
    Pong,
}
```

#### Phase 3: Client Implementation (6 weeks)

##### Week 11-12: RemoteBackend Trait

```rust
//! Client-side remote protocol implementation

use async_trait::async_trait;
use reqwest::Client;
use url::Url;

/// Trait for remote backends (pluggable implementations)
#[async_trait]
pub trait RemoteBackend: Send + Sync {
    /// Get current refs (bookmarks, tags, heads)
    async fn fetch_refs(&self) -> Result<RepoView>;

    /// Fetch operations since known heads
    async fn fetch_ops_since(&self, known: &[OperationId]) -> Result<Vec<Operation>>;

    /// Fetch specific objects
    async fn fetch_objects(&self, ids: &ObjectIds) -> Result<Objects>;

    /// Push operations and objects
    async fn push(&self, bundle: PushBundle) -> Result<PushResult>;

    /// Subscribe to real-time updates (returns stream)
    async fn subscribe(&self) -> Result<Box<dyn Stream<Item = Operation>>>;
}

/// HTTP(S) remote implementation
pub struct HttpRemote {
    client: Client,
    base_url: Url,
    auth: AuthMethod,
}

impl HttpRemote {
    pub fn new(url: &str, auth: AuthMethod) -> Result<Self> {
        let base_url = Url::parse(url)?;
        let client = Client::builder()
            .user_agent("jj-remote/0.1")
            .build()?;

        Ok(Self { client, base_url, auth })
    }
}

#[async_trait]
impl RemoteBackend for HttpRemote {
    async fn fetch_refs(&self) -> Result<RepoView> {
        let url = self.base_url.join("refs")?;
        let resp = self.client
            .get(url)
            .header("Authorization", self.auth.header())
            .send()
            .await?;

        let body: GetRefsResponse = resp.json().await?;
        Ok(body.view)
    }

    async fn fetch_ops_since(&self, known: &[OperationId]) -> Result<Vec<Operation>> {
        let url = self.base_url.join("ops")?;
        let resp = self.client
            .get(url)
            .query(&[("known", &serialize_op_ids(known))])
            .header("Authorization", self.auth.header())
            .send()
            .await?;

        let body: FetchOpsResponse = resp.json().await?;
        Ok(body.operations)
    }

    async fn push(&self, bundle: PushBundle) -> Result<PushResult> {
        let url = self.base_url.join("push")?;
        let resp = self.client
            .post(url)
            .header("Authorization", self.auth.header())
            .json(&PushRequest::from(bundle))
            .send()
            .await?;

        let body: PushResponse = resp.json().await?;
        if body.success {
            Ok(PushResult::Success { new_heads: body.new_op_heads })
        } else {
            Ok(PushResult::Conflict(body.conflict.unwrap()))
        }
    }

    async fn subscribe(&self) -> Result<Box<dyn Stream<Item = Operation>>> {
        let ws_url = self.base_url
            .to_string()
            .replace("https://", "wss://")
            .replace("http://", "ws://");

        let (ws, _) = tokio_tungstenite::connect_async(&ws_url).await?;
        let stream = ws.map(|msg| {
            // Parse WebSocket messages into Operations
            // ...
        });

        Ok(Box::new(stream))
    }
}

/// SSH remote implementation
pub struct SshRemote {
    session: ssh2::Session,
    repo_path: String,
}

#[async_trait]
impl RemoteBackend for SshRemote {
    // Similar implementation using SSH channels
    // Protocol is the same, just different transport
}
```

##### Week 13-14: Integration with jj-lib

```rust
//! Integration with jj-lib's existing abstractions

use jj_lib::repo::Repo;
use jj_lib::workspace::Workspace;

/// Extension trait for Repo to add remote operations
pub trait RepoRemoteExt {
    fn add_remote(&mut self, name: &str, url: &str) -> Result<()>;
    fn remove_remote(&mut self, name: &str) -> Result<()>;
    fn list_remotes(&self) -> Result<Vec<RemoteInfo>>;
    fn get_remote(&self, name: &str) -> Result<Box<dyn RemoteBackend>>;
}

impl RepoRemoteExt for Repo {
    fn add_remote(&mut self, name: &str, url: &str) -> Result<()> {
        let config_path = self.repo_path().join("remotes.toml");
        let mut config: RemotesConfig = load_or_default(&config_path)?;

        config.remotes.insert(name.to_string(), RemoteConfig {
            url: url.to_string(),
            push_bookmark: None,
            fetch_bookmark: None,
        });

        save(&config_path, &config)?;
        Ok(())
    }

    fn get_remote(&self, name: &str) -> Result<Box<dyn RemoteBackend>> {
        let config = self.get_remote_config(name)?;
        let url = &config.url;

        // Dispatch based on URL scheme
        if url.starts_with("jj://") || url.starts_with("https://") {
            let auth = self.get_auth_for_remote(name)?;
            Ok(Box::new(HttpRemote::new(url, auth)?))
        } else if url.starts_with("jj+ssh://") {
            Ok(Box::new(SshRemote::connect(url)?))
        } else if url.starts_with("/") || url.starts_with("file://") {
            Ok(Box::new(LocalRemote::new(url)?))
        } else {
            Err(anyhow!("Unknown remote URL scheme: {}", url))
        }
    }
}

/// High-level fetch operation
pub async fn fetch(repo: &mut Repo, remote_name: &str) -> Result<FetchResult> {
    let remote = repo.get_remote(remote_name)?;

    // 1. Get remote refs
    let remote_view = remote.fetch_refs().await?;

    // 2. Find missing operations
    let local_ops = repo.op_heads();
    let missing_ops = remote.fetch_ops_since(&local_ops).await?;

    if missing_ops.is_empty() {
        return Ok(FetchResult::UpToDate);
    }

    // 3. Fetch objects referenced by missing operations
    let needed_objects = collect_object_ids(&missing_ops);
    let objects = remote.fetch_objects(&needed_objects).await?;

    // 4. Import objects into local store
    for commit in objects.commits {
        repo.store().write_commit(&commit)?;
    }
    for tree in objects.trees {
        repo.store().write_tree(&tree)?;
    }
    for (id, blob) in objects.blobs {
        repo.store().write_blob(&id, &blob)?;
    }

    // 5. Import operations
    for op in missing_ops {
        repo.op_store().write_operation(&op)?;
    }

    // 6. Merge op heads if diverged
    let new_heads = repo.op_heads();
    if new_heads.len() > 1 {
        let merge_op = create_merge_operation(repo, &new_heads)?;
        repo.op_store().write_operation(&merge_op)?;
        repo.set_op_heads(&[merge_op.id])?;
    }

    Ok(FetchResult::Updated {
        ops_imported: missing_ops.len(),
        new_head: repo.op_heads()[0].clone(),
    })
}

/// High-level push operation
pub async fn push(
    repo: &Repo,
    remote_name: &str,
    options: PushOptions,
) -> Result<PushResult> {
    let remote = repo.get_remote(remote_name)?;

    // 1. Get remote state
    let remote_view = remote.fetch_refs().await?;

    // 2. Find local operations to push
    let local_heads = repo.op_heads();
    let remote_heads = remote_view.op_heads;

    let ops_to_push = find_ops_to_push(repo, &local_heads, &remote_heads)?;

    if ops_to_push.is_empty() {
        return Ok(PushResult::UpToDate);
    }

    // 3. Collect objects referenced by operations
    let objects = collect_objects_for_ops(repo, &ops_to_push)?;

    // 4. Build push bundle
    let bundle = PushBundle {
        operations: ops_to_push,
        changes: objects.changes,
        commits: objects.commits,
        trees: objects.trees,
        blobs: objects.blobs,
        expected_op_heads: remote_heads,
    };

    // 5. Push to remote
    match remote.push(bundle).await? {
        PushResult::Success { new_heads } => {
            // Update remote tracking refs
            repo.set_remote_heads(remote_name, &new_heads)?;
            Ok(PushResult::Success { new_heads })
        }
        PushResult::Conflict(conflict) => {
            if options.force {
                // Retry with force
                // ...
            } else {
                Ok(PushResult::Conflict(conflict))
            }
        }
    }
}
```

##### Week 15-16: CLI Commands

```rust
//! CLI commands for remote operations

use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum RemoteCommands {
    /// Add a new remote
    Add {
        /// Name for the remote
        name: String,
        /// URL of the remote repository
        url: String,
    },

    /// Remove a remote
    Remove {
        /// Name of the remote to remove
        name: String,
    },

    /// List configured remotes
    List,

    /// Show remote details
    Show {
        /// Name of the remote
        name: String,
    },

    /// Set remote URL
    SetUrl {
        /// Name of the remote
        name: String,
        /// New URL
        url: String,
    },
}

#[derive(Parser)]
pub struct FetchArgs {
    /// Remote to fetch from (default: origin)
    #[arg(default_value = "origin")]
    remote: String,

    /// Fetch all remotes
    #[arg(long)]
    all: bool,
}

#[derive(Parser)]
pub struct PushArgs {
    /// Remote to push to (default: origin)
    #[arg(default_value = "origin")]
    remote: String,

    /// Bookmark to push
    #[arg(short, long)]
    bookmark: Option<String>,

    /// Push all bookmarks
    #[arg(long)]
    all: bool,

    /// Force push (overwrite remote state)
    #[arg(short, long)]
    force: bool,

    /// Delete remote bookmark
    #[arg(short, long)]
    delete: Option<String>,
}

#[derive(Parser)]
pub struct PullArgs {
    /// Remote to pull from (default: origin)
    #[arg(default_value = "origin")]
    remote: String,

    /// Rebase local changes instead of merge
    #[arg(long)]
    rebase: bool,
}

// Implementation

pub fn cmd_remote_add(name: &str, url: &str) -> Result<()> {
    let workspace = Workspace::load_current()?;
    let mut repo = workspace.repo_mut();

    repo.add_remote(name, url)?;

    println!("Added remote '{}' at {}", name, url);
    Ok(())
}

pub async fn cmd_fetch(args: FetchArgs) -> Result<()> {
    let workspace = Workspace::load_current()?;
    let mut repo = workspace.repo_mut();

    let remotes = if args.all {
        repo.list_remotes()?
    } else {
        vec![repo.get_remote_info(&args.remote)?]
    };

    for remote_info in remotes {
        print!("Fetching from '{}'...", remote_info.name);
        std::io::stdout().flush()?;

        match fetch(&mut repo, &remote_info.name).await? {
            FetchResult::UpToDate => {
                println!(" up to date");
            }
            FetchResult::Updated { ops_imported, new_head } => {
                println!(" {} operation(s) imported", ops_imported);
            }
        }
    }

    Ok(())
}

pub async fn cmd_push(args: PushArgs) -> Result<()> {
    let workspace = Workspace::load_current()?;
    let repo = workspace.repo();

    let options = PushOptions {
        force: args.force,
        bookmark: args.bookmark,
        all_bookmarks: args.all,
        delete: args.delete,
    };

    print!("Pushing to '{}'...", args.remote);
    std::io::stdout().flush()?;

    match push(&repo, &args.remote, options).await? {
        PushResult::UpToDate => {
            println!(" up to date");
        }
        PushResult::Success { new_heads } => {
            println!(" done");
        }
        PushResult::Conflict(conflict) => {
            println!(" FAILED");
            eprintln!("Error: {}", conflict.message);
            match conflict.conflict_type {
                ConflictType::StaleOpHead => {
                    eprintln!("Remote has newer changes. Run 'jj fetch' first.");
                }
                ConflictType::BookmarkConflict => {
                    eprintln!("Bookmark has diverged. Use --force to overwrite.");
                }
                _ => {}
            }
            std::process::exit(1);
        }
    }

    Ok(())
}

pub async fn cmd_pull(args: PullArgs) -> Result<()> {
    // Fetch
    let fetch_args = FetchArgs {
        remote: args.remote.clone(),
        all: false,
    };
    cmd_fetch(fetch_args).await?;

    // Merge or rebase
    let workspace = Workspace::load_current()?;
    let mut repo = workspace.repo_mut();

    if args.rebase {
        // Rebase local changes on top of remote
        // ...
    } else {
        // Merge (already handled by fetch if ops diverged)
    }

    Ok(())
}
```

#### Phase 4: Testing & Hardening (4 weeks)

##### Week 17-18: Test Suite

```rust
//! Integration tests for JJ remote protocol

#[tokio::test]
async fn test_basic_push_fetch() {
    // Setup: two local repos with a shared server
    let server = TestServer::start().await;
    let repo_a = TestRepo::new("repo_a");
    let repo_b = TestRepo::new("repo_b");

    // Add same remote to both
    repo_a.add_remote("origin", &server.url()).unwrap();
    repo_b.add_remote("origin", &server.url()).unwrap();

    // Create change in repo_a
    repo_a.create_file("hello.txt", "Hello, world!");
    let change_id = repo_a.describe("Add greeting");

    // Push from repo_a
    let result = push(&repo_a, "origin", Default::default()).await.unwrap();
    assert!(matches!(result, PushResult::Success { .. }));

    // Fetch in repo_b
    let result = fetch(&mut repo_b, "origin").await.unwrap();
    assert!(matches!(result, FetchResult::Updated { ops_imported: 1, .. }));

    // Verify change exists in repo_b with same change_id
    let change = repo_b.get_change(&change_id).unwrap();
    assert_eq!(change.description, "Add greeting");

    // Verify file content
    let content = repo_b.read_file(&change_id, "hello.txt").unwrap();
    assert_eq!(content, "Hello, world!");
}

#[tokio::test]
async fn test_concurrent_push_conflict() {
    let server = TestServer::start().await;
    let repo_a = TestRepo::with_remote("origin", &server.url());
    let repo_b = TestRepo::with_remote("origin", &server.url());

    // Both start from same state
    repo_a.create_file("file.txt", "initial");
    push(&repo_a, "origin", Default::default()).await.unwrap();
    fetch(&mut repo_b, "origin").await.unwrap();

    // Both make changes
    repo_a.edit_file("file.txt", "from A");
    repo_b.edit_file("file.txt", "from B");

    // A pushes first - succeeds
    let result = push(&repo_a, "origin", Default::default()).await.unwrap();
    assert!(matches!(result, PushResult::Success { .. }));

    // B pushes - should conflict
    let result = push(&repo_b, "origin", Default::default()).await.unwrap();
    assert!(matches!(result, PushResult::Conflict(PushConflict {
        conflict_type: ConflictType::StaleOpHead,
        ..
    })));

    // B fetches, resolves, pushes again
    fetch(&mut repo_b, "origin").await.unwrap();
    // (resolve conflict)
    let result = push(&repo_b, "origin", Default::default()).await.unwrap();
    assert!(matches!(result, PushResult::Success { .. }));
}

#[tokio::test]
async fn test_description_with_aiki_metadata() {
    let server = TestServer::start().await;
    let repo_a = TestRepo::with_remote("origin", &server.url());
    let repo_b = TestRepo::with_remote("origin", &server.url());

    // Create change with [aiki] metadata
    repo_a.create_file("main.rs", "fn main() {}");
    let description = r#"Add main function

[aiki]
agent=claude-code
session=sess_abc123
confidence=high
[/aiki]"#;
    let change_id = repo_a.describe(description);

    // Push
    push(&repo_a, "origin", Default::default()).await.unwrap();

    // Fetch in repo_b
    fetch(&mut repo_b, "origin").await.unwrap();

    // Verify metadata preserved
    let change = repo_b.get_change(&change_id).unwrap();
    assert!(change.description.contains("[aiki]"));
    assert!(change.description.contains("agent=claude-code"));
    assert!(change.description.contains("session=sess_abc123"));
}

#[tokio::test]
async fn test_large_repo_performance() {
    let server = TestServer::start().await;
    let repo = TestRepo::with_remote("origin", &server.url());

    // Create 10,000 files
    for i in 0..10_000 {
        repo.create_file(&format!("file_{}.txt", i), &format!("content {}", i));
    }
    repo.describe("Add many files");

    // Measure push time
    let start = Instant::now();
    push(&repo, "origin", Default::default()).await.unwrap();
    let push_duration = start.elapsed();

    // Should complete in reasonable time (< 30 seconds)
    assert!(push_duration < Duration::from_secs(30));

    // Measure fetch time
    let repo_b = TestRepo::with_remote("origin", &server.url());
    let start = Instant::now();
    fetch(&mut repo_b, "origin").await.unwrap();
    let fetch_duration = start.elapsed();

    assert!(fetch_duration < Duration::from_secs(30));
}
```

##### Week 19-20: Edge Cases & Documentation

**Edge cases to handle:**
1. Network failures mid-push (retry logic, idempotency)
2. Server restart during WebSocket connection
3. Very large files (streaming, chunking)
4. Many small operations (batching)
5. Clock skew between machines
6. Repository corruption recovery
7. Backwards compatibility with older clients

**Documentation:**
1. Protocol specification document
2. Server deployment guide
3. Client integration guide
4. Migration guide from `jj git push/fetch`
5. Troubleshooting guide

### Total Timeline Summary

| Phase | Weeks | Key Deliverables |
|-------|-------|------------------|
| Protocol Specification | 4 | Protobuf schemas, auth model, conflict semantics |
| Server Implementation | 6 | HTTP API, storage backend, WebSocket |
| Client Implementation | 6 | RemoteBackend trait, jj-lib integration, CLI |
| Testing & Hardening | 4 | Test suite, edge cases, documentation |
| **Total** | **20** | Production-ready native JJ remotes |

### Next Steps

1. **Week 0**: Draft RFC for JJ community discussion
2. **Week 1**: Begin protocol specification
3. **Ongoing**: Engage with JJ maintainers on design decisions

### Relationship to JJ Upstream

**Option A: Contribute to JJ core**
- Best for ecosystem
- JJ maintainers must approve design
- Slower, but sustainable

**Option B: Fork or extension**
- Faster to ship
- Risk of divergence
- May not get upstream adoption

**Option C: Hybrid**
- Build server + protocol ourselves
- Propose client changes to JJ upstream
- If rejected, maintain as extension

**Recommendation:** Start with Option C. Build the server, prove it works, then propose to JJ.

---

## Research: Existing JJ Community Plans & Discussions

### Key Finding: Google Already Has This

From the [JJ Development Roadmap](https://jj-vcs.github.io/jj/latest/roadmap/):

> **Google has an internal Jujutsu server backed by a database.** This server allows commits and repos (operation logs) to be stored in the cloud (i.e. the database). Working copies can still be stored locally.
>
> In order to reduce latency, there is a **local daemon process that caches reads and writes**. It also prefetches objects it thinks the client might ask for next. It also helps with write latency by **optimistically answering write requests** (it therefore needs to know the server's hashing scheme so it can return the right IDs).
>
> We (the project, not necessarily Google) **want to provide a similar experience for all users**. We would therefore like to create a similar server and daemon.

**This validates our entire design direction.** Google has already built what we're designing. The JJ project wants to open-source it.

### JJ Roadmap Alignment

| JJ Roadmap Item | Our Design | Alignment |
|-----------------|------------|-----------|
| Open-source cloud-based repos | Aiki Cloud / JJ Remote Server | ✅ Direct match |
| RPC API for tools | Our protocol design | ✅ Complementary |
| Local daemon for caching | Client proxy with cache | ✅ Same approach |
| VFS for large repos | Out of scope (future) | - |

### Operation Log: Already Designed for Distributed Sync

From [JJ Concurrency Docs](https://jj-vcs.github.io/jj/latest/technical/concurrency/):

> The most important piece in the lock-free design is the "operation log". That is what allows us to detect and merge divergent operations.

Key points that validate our design:

1. **Operations are content-addressed** (like Git commits) - safe to write without locking
2. **Automatic 3-way merge** of divergent operation heads - already implemented
3. **Designed for distributed filesystems** - "Unlike other DVCSs, Jujutsu treats concurrent edits the same whether they're made locally or remotely"
4. **View objects** contain snapshot of refs at each operation - exactly what we need to sync

**Implication:** JJ's operation log is *already* designed for the sync semantics we need. We're not inventing new concepts - we're exposing existing JJ capabilities over the network.

### Discussion #2425: "Remote-Backed Commit Store"

From [Working branches and the JJ "way"](https://github.com/martinvonz/jj/discussions/2425):

> With a remote-backed commit store (with a local layer), you'd never have to push at all unless interacting with git repos — "that'd be basically a **jj-aware server**."

This is exactly what we're proposing. The community has discussed this idea.

### RPC API Design Doc

The JJ team has a [design doc for the RPC API](https://docs.google.com/document/d/1rOKvutee5TVYpFhh_UDNZDxfUKyrJ8rjCNpFaNHOHwU/edit):

> One problem with writing tools using the Rust API is that they will only work with the backends they were compiled with... We want to provide an **RPC API** for tools that want to work with an unknown build of jj.
>
> The RPC API will probably be at a **higher abstraction level** than the Rust API.

**Implication:** Our protocol should consider compatibility with JJ's planned RPC API.

### Native Backend Status

From various discussions:

> The native backend is currently very naive and slow. The git backend is used not only because of GitHub, but because it's just a much better backend so far, mostly thanks to the packfile format.

> In the longer term, the hope is that clearer transactional semantics will make it possible for a native jj backend to do things that are hard to do with the git backend.

**Implication:** The native backend exists but isn't optimized. However, the *storage abstraction* is solid. Our server can use Git packfiles for efficient storage (like JJ does internally).

### Issue #5685: Library Reliability for Servers

From [Improve jj library's reliability](https://github.com/jj-vcs/jj/issues/5685):

> In their specific use case, a custom heads implementation involved an RPC call that could fail, leading to an unavoidable panic.

**Implication:** Others are already using jj-lib in server contexts. We should contribute to improving library reliability.

### Issue #3577: Generalized Hook Support with gRPC

From [FR: Generalized hook support](https://github.com/jj-vcs/jj/issues/3577):

> Hook binaries would be passed a file descriptor with a connection to the **jj grpc server**.

**Implication:** The community is thinking about gRPC integration. Our protocol could align with this.

### What's NOT in JJ Yet (Our Opportunity)

Based on research, these are gaps we'd be filling:

| Gap | Status | Our Solution |
|-----|--------|--------------|
| Native remote protocol | Not implemented | jj:// protocol |
| Public cloud server | Google's is internal | Aiki Cloud |
| Operation log sync over network | Only shared filesystem | HTTP/WebSocket push/fetch |
| Cross-machine change ID stability | Broken via Git roundtrip | Native protocol preserves IDs |

### Community Sentiment

From [Lobsters discussion](https://lobste.rs/s/rojoz1/jujutsu_jj_git_compatible_vcs):

> jj has two backends: the native backend and the git backend. While the native backend is tested for use in jj, given the world of tooling and hosting options out there, it's assumed that you'll be using the git backend.

The community accepts Git as the practical backend for now, but there's appetite for native solutions.

### Recommendations Based on Research

1. **Engage with JJ maintainers early** - They want to open-source Google's cloud backend. We could collaborate rather than duplicate.

2. **Align with planned RPC API** - Our protocol should be compatible with or build on JJ's RPC plans.

3. **Use Git packfiles for storage** - JJ already does this internally. Don't reinvent object storage.

4. **Leverage existing operation log merge** - JJ's 3-way merge of views is battle-tested at Google scale.

5. **Contribute improvements upstream** - Issues like #5685 show others need jj-lib to be server-ready.

6. **Start with local daemon** - Google's architecture has a local daemon for latency. This matches our "client proxy with cache" design.

### Strategic Options (Updated)

**Option A: Wait for JJ to open-source Google's server**
- Pro: No duplication of effort
- Con: Unknown timeline, may never happen publicly
- Risk: We're blocked on JJ team's priorities

**Option B: Build compatible implementation, propose to JJ**
- Pro: We control timeline, can contribute upstream
- Con: More effort, risk of divergence
- Opportunity: **Become the de facto JJ cloud solution**

**Option C: Build Aiki-specific layer on top**
- Pro: Ship faster, Aiki-specific features
- Con: Not reusable by broader JJ community
- Risk: Maintenance burden if JJ releases official solution

**Recommendation:** Option B. Build a server that:
1. Implements our protocol using jj-lib internally
2. Uses Git packfiles for efficient storage
3. Could be contributed to JJ project
4. Has Aiki-specific extensions (provenance, tasks) as optional layers

---

### Why This Beats "GitHub for Agents"

The previous design (Aiki Cloud) was:
- A **layer on top of Git** for coordination
- Git still handles file sync
- Metadata syncs separately

Native JJ remotes are:
- **Fundamental infrastructure** like Git itself
- Everything syncs natively
- No split between "code sync" and "metadata sync"

```
GitHub for Agents (v1)           Native JJ Remotes (v2)
─────────────────────────        ─────────────────────────
      Aiki Cloud                       Aiki Cloud
   (tasks, provenance)               (hosting only)
          │                                │
          ▼                                ▼
      Git Remote                       JJ Remote
   (files, commits)               (everything syncs)
          │                                │
          ▼                                ▼
    Local .git/                       Local .jj/
    Local .jj/                    (single source of truth)
```

**The JJ remote approach is cleaner because there's one sync mechanism, not two.**

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
