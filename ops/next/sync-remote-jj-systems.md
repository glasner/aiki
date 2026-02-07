# Native JJ Remote Sync for Aiki

## 1. Vision

JJ's operation log is already designed for distributed sync — operations are content-addressed, append-only, and support automatic 3-way merge of divergent heads. The missing piece is a network protocol to expose these capabilities across machines.

**Goal:** Build native JJ remote sync that preserves change IDs, operation history, and `[aiki]` provenance metadata across machines. This eliminates the split between "code sync" and "metadata sync" that plagues Git-based approaches.

```
Git-Based Sync (current)            Native JJ Remote Sync (target)
─────────────────────────           ──────────────────────────────
   Aiki Cloud                            Aiki Cloud
 (tasks, provenance)                    (hosting only)
        │                                     │
        ▼                                     ▼
    Git Remote                            JJ Remote
 (files, commits)                     (everything syncs)
        │                                     │
        ▼                                     ▼
  Local .git/                           Local .jj/
  Local .jj/                       (single source of truth)
```

**What syncs natively:**
- Change IDs (stable across machines)
- Change descriptions (including `[aiki]` metadata blocks)
- Operation log (full history of mutations)
- Bookmarks (native, not mapped to Git branches)
- File content (via content-addressed store)

---

## 2. Problem Statement

### What JJ Syncs via Git Today

JJ uses `jj git push/fetch` to sync through Git remotes. This works for file content but loses JJ-specific concepts:

| Concept | Syncs via Git? | Notes |
|---------|---------------|-------|
| File content | Yes | Standard Git |
| Git commits | Yes | Standard Git |
| Change IDs | Partial | `git.write-change-id-header` preserves IDs as Git commit headers, but only for the Git backend |
| Operation log | No | Local only — this is the biggest gap |
| Change descriptions | Partial | Survives if committed to Git, but JJ-local descriptions don't sync |
| Bookmarks | Partial | Maps to Git branches, but loses JJ-native bookmark semantics |

### Corrected: Change IDs and `git.write-change-id-header`

The original analysis overstated the change ID problem. JJ's `git.write-change-id-header` setting (enabled by default) writes change IDs into Git commit headers:

```
tree abc123
parent def456
author Alice <alice@example.com> 1234567890 +0000
committer Alice <alice@example.com> 1234567890 +0000
change-id iabcdefghijklmnopqrstuvwxyzabcd

Fix the auth bug
```

This means change IDs **do survive** Git round-trips for the common case. However, this doesn't solve:

1. **Operation log sync** — The full mutation history (who described what, when, rebases, abandons) is completely local. Another machine importing from Git only sees the final state, not the path there.
2. **Mutable state** — JJ allows rewriting changes. The operation log records this history, but Git only sees the latest commit.
3. **Bookmark semantics** — Git branches are a poor model for JJ bookmarks. JJ bookmarks track which change they point to across rewrites; Git branches are just pointers to commit SHAs.
4. **Concurrent agent coordination** — The `aiki/tasks` branch requires atomic updates that Git's push model can't guarantee (see Section 8).

### What We Actually Need

The core gap isn't change IDs — it's the **operation log**. Operations are JJ's unit of state change, and without syncing them, remote machines lose:
- Who made what change and when
- The sequence of mutations (rebases, describes, abandons)
- Divergent-head detection and automatic merge
- Provenance metadata embedded in operation context

---

## 3. Design Decisions

### Why Native JJ Remotes Over Hybrid Git

**Rejected: Hybrid Git-Native (push tasks branch to Git remote)**

The hybrid approach uses Git for file sync and a coordinator service for metadata. Problems:
- Two sync mechanisms to maintain and debug
- Tasks branch conflicts require manual coordination or last-writer-wins
- No operation log sync means losing mutation history
- The coordinator becomes a single point of failure separate from the data

**Chosen: Native JJ protocol that syncs operations**

A single sync mechanism that covers everything JJ knows about. Simpler to reason about, and JJ's existing merge semantics handle conflicts.

### Why Operations Are the Unit of Sync

Git syncs **commits** (content snapshots). JJ should sync **operations** (state mutations).

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

Operations are:
- Append-only (like Git commits) — safe to replicate
- Content-addressed — deduplication is free
- Support multiple heads — divergence is expected, not an error
- Already have 3-way merge — JJ merges divergent operation heads automatically

### Why NOT a Shared JJ Server

A shared server where all machines connect as JJ workspaces would give ideal semantics (single source of truth for change IDs, provenance visible everywhere). But:
- JJ isn't designed for networked/server mode today
- Every operation would hit the network (latency-sensitive)
- Single point of failure for all writes
- Requires JJ core changes we can't control the timeline for

The native remote approach gives the same sync guarantees without requiring every operation to go through a central server. Machines work locally and sync when ready.

### Why NOT Aiki Cloud First

An approach that uses Git for file sync plus a separate metadata service (PostgreSQL for tasks, provenance, sessions) has appeal for shipping quickly. But:
- Splits truth between Git and the metadata service — reconciliation is an ongoing problem
- Change IDs still diverge unless we build a change ID registry (which is half the work of native remotes anyway)
- Locks us into a design that's harder to migrate away from

Native remotes are more work upfront but eliminate the metadata split entirely.

### Relationship to CRDTs

The CRDT approach (every operation emits conflict-free replicated events) was dismissed as too complex. In hindsight, Aiki's existing task model is already 80% CRDT:
- Tasks are stored as append-only events
- Events are identified by change IDs (content-addressed)
- State is materialized by replaying the event log

The native remote protocol we're designing is effectively a transport layer for these CRDT-like operations. We don't need a separate CRDT library — JJ's operation log already provides the merge semantics.

---

## 4. Protocol Design

### Wire Format (Protobuf)

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

message ConflictId {
  bytes id = 1;  // Content-addressed hash of conflict description
}

// === Repository State ===

message RepoView {
  repeated ChangeId head_ids = 1;
  map<string, ChangeId> bookmarks = 2;
  map<string, ChangeId> tags = 3;
  repeated OperationId op_heads = 4;
}

message Change {
  ChangeId change_id = 1;
  repeated CommitId commit_ids = 2;
  string description = 3;
  repeated ChangeId parent_ids = 4;
}

message Commit {
  CommitId commit_id = 1;
  TreeId tree_id = 2;
  repeated CommitId parent_ids = 3;
  Signature author = 4;
  Signature committer = 5;
  string description = 6;
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
  // File mode as an enum — avoids illegal states from
  // independent bool flags (e.g. executable + symlink simultaneously)
  FileMode mode = 5;
}

enum FileMode {
  FILE_MODE_UNSPECIFIED = 0;
  FILE_MODE_NORMAL = 1;
  FILE_MODE_EXECUTABLE = 2;
  FILE_MODE_SYMLINK = 3;
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
  }
}

message OperationMetadata {
  Timestamp timestamp = 1;
  string hostname = 2;
  string username = 3;
  string description = 4;
  map<string, string> tags = 5;
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

message SetTag {
  string name = 1;
  ChangeId target = 2;
}

// === RPC Messages ===

message GetRefsRequest {}
message GetRefsResponse {
  RepoView view = 1;
}

message FetchOpsRequest {
  repeated OperationId known_ops = 1;
  uint32 max_ops = 2;
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
  map<string, bytes> blobs = 4;
}

message PushRequest {
  repeated Operation operations = 1;
  repeated Change changes = 2;
  repeated Commit commits = 3;
  repeated Tree trees = 4;
  map<string, bytes> blobs = 5;
  repeated OperationId expected_op_heads = 6;
}

message PushResponse {
  bool success = 1;
  optional PushConflict conflict = 2;
  repeated OperationId new_op_heads = 3;
}

message PushConflict {
  enum ConflictType {
    CONFLICT_TYPE_UNSPECIFIED = 0;
    STALE_OP_HEAD = 1;
    BOOKMARK_CONFLICT = 2;
    PERMISSION_DENIED = 3;
  }
  ConflictType type = 1;
  string message = 2;
  repeated OperationId current_op_heads = 3;
}
```

### Endpoints (HTTP + WebSocket)

```
Transport: HTTPS (primary), SSH (optional)
Encoding: Protobuf (wire), JSON (debug/dev mode)

GET  /api/v1/repos/:owner/:name/refs
     → GetRefsResponse (current bookmarks, tags, heads, op heads)

GET  /api/v1/repos/:owner/:name/ops?known=<op_ids>&max=<limit>
     → FetchOpsResponse (operations the client doesn't have)

POST /api/v1/repos/:owner/:name/objects
     ← FetchObjectsRequest (specific object IDs needed)
     → FetchObjectsResponse (changes, commits, trees, blobs)

POST /api/v1/repos/:owner/:name/push
     ← PushRequest (operations + objects + expected op heads)
     → PushResponse (success or conflict details)

GET  /api/v1/repos/:owner/:name/blobs/:id
     → Raw blob bytes (streaming for large files)

WS   /api/v1/repos/:owner/:name/ws
     → Real-time operation stream (bidirectional)
```

### Conflict Resolution Semantics

**Scenario 1: Divergent Operation Heads**

```
Machine A: op_1 → op_2 (edit file.rs)
Machine B: op_1 → op_3 (also edit file.rs)

After sync:
         ┌─ op_2 (A's edit)
op_1 ───┤
         └─ op_3 (B's edit)
                  ↓
            op_4 (merge)
```

JJ handles this natively. Both op_2 and op_3 become op heads. The client creates a merge operation. If the same file was edited, JJ's first-class conflict markers appear in the working copy.

**Scenario 2: Bookmark Moved Concurrently**

Default behavior: reject push, require explicit `--force`:
```
$ jj push
Error: bookmark 'main' has diverged
  local:  main → change_X
  remote: main → change_Y
Use 'jj push --force' to overwrite, or 'jj fetch' first
```

**Scenario 3: Change Description Conflict**

Last-writer-wins for descriptions. Both descriptions are in the operation log, so history is preserved.

---

## 5. Server Implementation

### Storage Backend

```rust
/// Content-addressed store for immutable objects
pub trait ObjectStore: Send + Sync {
    async fn get_commit(&self, id: &CommitId) -> Result<Option<Commit>>;
    async fn get_tree(&self, id: &TreeId) -> Result<Option<Tree>>;
    async fn get_blob(&self, id: &BlobId) -> Result<Option<Vec<u8>>>;

    async fn put_commit(&self, commit: &Commit) -> Result<CommitId>;
    async fn put_tree(&self, tree: &Tree) -> Result<TreeId>;
    async fn put_blob(&self, content: &[u8]) -> Result<BlobId>;

    async fn has_objects(&self, ids: &ObjectIds) -> Result<ObjectIds>;
}

/// Store for mutable data (changes, operations)
pub trait MetadataStore: Send + Sync {
    async fn get_change(&self, id: &ChangeId) -> Result<Option<Change>>;
    async fn put_change(&self, change: &Change) -> Result<()>;

    async fn get_operation(&self, id: &OperationId) -> Result<Option<Operation>>;
    async fn put_operation(&self, op: &Operation) -> Result<()>;
    async fn get_op_heads(&self) -> Result<Vec<OperationId>>;
    async fn set_op_heads(&self, heads: &[OperationId]) -> Result<()>;
    async fn list_ops_since(&self, known: &[OperationId]) -> Result<Vec<Operation>>;

    async fn get_refs(&self) -> Result<RepoView>;
    async fn update_refs(&self, updates: &RefUpdates) -> Result<()>;
}

/// Storage backend options
pub enum StorageBackend {
    /// Local filesystem — objects as content-addressed files, metadata in SQLite
    LocalFs { root: PathBuf },

    /// Cloud — objects in S3/GCS/R2, metadata in PostgreSQL
    Cloud {
        object_store: Box<dyn ObjectStore>,
        metadata_store: Box<dyn MetadataStore>,
    },

    /// Git-backed — objects as Git packfiles (via gitoxide), metadata as custom refs
    GitBacked { git_dir: PathBuf },
}
```

PostgreSQL schema for cloud deployment:

```sql
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
```

### HTTP API Server

```rust
use axum::{
    extract::{Path, Query, State},
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
        .route("/api/v1/repos", post(create_repo))
        .route("/api/v1/repos/:owner/:name", get(get_repo_info))
        .route("/api/v1/repos/:owner/:name/refs", get(get_refs))
        .route("/api/v1/repos/:owner/:name/ops", get(fetch_ops))
        .route("/api/v1/repos/:owner/:name/objects", post(fetch_objects))
        .route("/api/v1/repos/:owner/:name/push", post(push))
        .route("/api/v1/repos/:owner/:name/blobs/:id", get(get_blob))
        .with_state(Arc::new(state))
}

async fn fetch_ops(
    State(state): State<Arc<ServerState>>,
    Path((owner, name)): Path<(String, String)>,
    Query(params): Query<FetchOpsParams>,
    auth: AuthenticatedUser,
) -> Result<Json<FetchOpsResponse>, ApiError> {
    let repo = state.repos.get(&owner, &name).await?;
    auth.require_permission(&repo, Permission::Read)?;

    let client_ops: HashSet<_> = params.known_ops.iter().collect();
    let all_ops = repo.metadata.list_ops_since(&params.known_ops).await?;
    let max = params.max_ops.unwrap_or(1000) as usize;

    // Collect into vec first, then check total count
    let total_count = all_ops.len();
    let missing_ops: Vec<_> = all_ops
        .into_iter()
        .filter(|op| !client_ops.contains(&op.id))
        .take(max)
        .collect();

    Ok(Json(FetchOpsResponse {
        operations: missing_ops,
        has_more: total_count > max,
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

    // Atomic: read current heads, validate, write — all under lock.
    // The read-then-check-then-write must be serialized to avoid
    // TOCTOU races where two pushes both see the same heads.
    let mut heads_guard = repo.metadata.lock_op_heads().await?;
    let current_heads = heads_guard.current();

    if *current_heads != req.expected_op_heads {
        return Ok(Json(PushResponse {
            success: false,
            conflict: Some(PushConflict {
                conflict_type: ConflictType::StaleOpHead,
                message: "Remote has newer operations".into(),
                current_op_heads: current_heads.clone(),
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
    for change in &req.changes {
        repo.metadata.put_change(change).await?;
    }

    // Validate operation chain
    validate_operation_chain(&req.operations, current_heads)?;

    for op in &req.operations {
        repo.metadata.put_operation(op).await?;
    }

    // Update op heads atomically
    let new_heads = compute_new_heads(current_heads, &req.operations);
    heads_guard.set(&new_heads).await?;

    // Broadcast to connected WebSocket clients
    state.broadcast_ops(&owner, &name, &req.operations).await;

    Ok(Json(PushResponse {
        success: true,
        conflict: None,
        new_op_heads: new_heads,
    }))
}
```

### WebSocket Real-Time Streaming

```rust
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
    ws.on_upgrade(move |socket| handle_socket(socket, state, owner, name))
}

async fn handle_socket(
    socket: WebSocket,
    state: Arc<ServerState>,
    owner: String,
    name: String,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.subscribe_to_repo(&owner, &name);

    let send_task = tokio::spawn(async move {
        while let Ok(op) = rx.recv().await {
            let msg = serde_json::to_string(&WsMessage::Operation(op)).unwrap();
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

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

---

## 6. Client Implementation

### RemoteBackend Trait

```rust
use async_trait::async_trait;
use reqwest::Client;
use url::Url;

#[async_trait]
pub trait RemoteBackend: Send + Sync {
    async fn fetch_refs(&self) -> Result<RepoView>;
    async fn fetch_ops_since(&self, known: &[OperationId]) -> Result<Vec<Operation>>;
    async fn fetch_objects(&self, ids: &ObjectIds) -> Result<Objects>;
    async fn push(&self, bundle: PushBundle) -> Result<PushResult>;
    async fn subscribe(&self) -> Result<Box<dyn Stream<Item = Operation>>>;
}

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
        let resp = self.client.get(url)
            .header("Authorization", self.auth.header())
            .send().await?;
        let body: GetRefsResponse = resp.json().await?;
        Ok(body.view)
    }

    async fn fetch_ops_since(&self, known: &[OperationId]) -> Result<Vec<Operation>> {
        let url = self.base_url.join("ops")?;
        let resp = self.client.get(url)
            .query(&[("known", &serialize_op_ids(known))])
            .header("Authorization", self.auth.header())
            .send().await?;
        let body: FetchOpsResponse = resp.json().await?;
        Ok(body.operations)
    }

    async fn push(&self, bundle: PushBundle) -> Result<PushResult> {
        let url = self.base_url.join("push")?;
        let resp = self.client.post(url)
            .header("Authorization", self.auth.header())
            .json(&PushRequest::from(bundle))
            .send().await?;
        let body: PushResponse = resp.json().await?;
        if body.success {
            Ok(PushResult::Success { new_heads: body.new_op_heads })
        } else {
            Ok(PushResult::Conflict(body.conflict.unwrap()))
        }
    }

    async fn subscribe(&self) -> Result<Box<dyn Stream<Item = Operation>>> {
        let ws_url = self.base_url.to_string()
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        let (ws, _) = tokio_tungstenite::connect_async(&ws_url).await?;
        let stream = ws.map(|msg| {
            // Parse WebSocket messages into Operations
            todo!()
        });
        Ok(Box::new(stream))
    }
}
```

### jj-lib Integration

```rust
use jj_lib::repo::Repo;
use jj_lib::workspace::Workspace;

pub trait RepoRemoteExt {
    fn add_remote(&mut self, name: &str, url: &str) -> Result<()>;
    fn remove_remote(&mut self, name: &str) -> Result<()>;
    fn list_remotes(&self) -> Result<Vec<RemoteInfo>>;
    fn get_remote(&self, name: &str) -> Result<Box<dyn RemoteBackend>>;
}

impl RepoRemoteExt for Repo {
    fn get_remote(&self, name: &str) -> Result<Box<dyn RemoteBackend>> {
        let config = self.get_remote_config(name)?;
        let url = &config.url;

        if url.starts_with("jj://") || url.starts_with("https://") {
            let auth = self.get_auth_for_remote(name)?;
            Ok(Box::new(HttpRemote::new(url, auth)?))
        } else if url.starts_with("jj+ssh://") {
            Ok(Box::new(SshRemote::connect(url)?))
        } else if url.starts_with('/') || url.starts_with("file://") {
            Ok(Box::new(LocalRemote::new(url)?))
        } else {
            Err(anyhow!("Unknown remote URL scheme: {}", url))
        }
    }

    // ... other methods follow same pattern
}
```

### High-Level Fetch

```rust
pub async fn fetch(repo: &mut Repo, remote_name: &str) -> Result<FetchResult> {
    let remote = repo.get_remote(remote_name)?;

    let remote_view = remote.fetch_refs().await?;
    let local_ops = repo.op_heads();
    let missing_ops = remote.fetch_ops_since(&local_ops).await?;

    if missing_ops.is_empty() {
        return Ok(FetchResult::UpToDate);
    }

    // Fetch objects referenced by missing operations
    let needed_objects = collect_object_ids(&missing_ops);
    let objects = remote.fetch_objects(&needed_objects).await?;

    // Import into local store
    for commit in objects.commits {
        repo.store().write_commit(&commit)?;
    }
    for tree in objects.trees {
        repo.store().write_tree(&tree)?;
    }
    for (id, blob) in objects.blobs {
        repo.store().write_blob(&id, &blob)?;
    }

    // Record count before consuming the iterator
    let ops_count = missing_ops.len();
    for op in &missing_ops {
        repo.op_store().write_operation(op)?;
    }

    // Merge op heads if diverged
    let new_heads = repo.op_heads();
    if new_heads.len() > 1 {
        let merge_op = create_merge_operation(repo, &new_heads)?;
        repo.op_store().write_operation(&merge_op)?;
        repo.set_op_heads(&[merge_op.id])?;
    }

    Ok(FetchResult::Updated {
        ops_imported: ops_count,
        new_head: repo.op_heads()[0].clone(),
    })
}
```

### High-Level Push

```rust
pub async fn push(
    repo: &Repo,
    remote_name: &str,
    options: PushOptions,
) -> Result<PushResult> {
    let remote = repo.get_remote(remote_name)?;
    let remote_view = remote.fetch_refs().await?;

    let local_heads = repo.op_heads();
    let remote_heads = remote_view.op_heads;
    let ops_to_push = find_ops_to_push(repo, &local_heads, &remote_heads)?;

    if ops_to_push.is_empty() {
        return Ok(PushResult::UpToDate);
    }

    let objects = collect_objects_for_ops(repo, &ops_to_push)?;
    let bundle = PushBundle {
        operations: ops_to_push,
        changes: objects.changes,
        commits: objects.commits,
        trees: objects.trees,
        blobs: objects.blobs,
        expected_op_heads: remote_heads,
    };

    match remote.push(bundle).await? {
        PushResult::Success { new_heads } => {
            repo.set_remote_heads(remote_name, &new_heads)?;
            Ok(PushResult::Success { new_heads })
        }
        PushResult::Conflict(conflict) => {
            if options.force {
                // Retry with force — overwrite remote state
                todo!("force push implementation")
            } else {
                Ok(PushResult::Conflict(conflict))
            }
        }
    }
}
```

### CLI Commands

```rust
use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum RemoteCommands {
    Add { name: String, url: String },
    Remove { name: String },
    List,
    Show { name: String },
    SetUrl { name: String, url: String },
}

#[derive(Parser)]
pub struct FetchArgs {
    #[arg(default_value = "origin")]
    remote: String,
    #[arg(long)]
    all: bool,
}

#[derive(Parser)]
pub struct PushArgs {
    #[arg(default_value = "origin")]
    remote: String,
    #[arg(short, long)]
    bookmark: Option<String>,
    #[arg(long)]
    all: bool,
    #[arg(short, long)]
    force: bool,
    #[arg(short, long)]
    delete: Option<String>,
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

    eprint!("Pushing to '{}'...", args.remote);

    match push(&repo, &args.remote, options).await? {
        PushResult::UpToDate => {
            eprintln!(" up to date");
        }
        PushResult::Success { .. } => {
            eprintln!(" done");
        }
        PushResult::Conflict(conflict) => {
            eprintln!(" FAILED");
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
            return Err(AikiError::RemotePushFailed {
                remote: args.remote,
                reason: conflict.message,
            }.into());
        }
    }

    Ok(())
}
```

---

## 7. Security Model

### Agent Identity

Agent identity must be **cryptographic, not self-asserted**. An agent claiming to be "claude-code" isn't sufficient — the server must verify.

**Identity scheme:**

1. **Agent registration** generates an Ed25519 keypair. The public key is registered with the server; the private key stays on the agent's machine.
2. **Every push** includes a signature over the operation IDs being pushed. The server verifies against the registered public key.
3. **API keys** are acceptable for initial authentication but are supplemented by operation signing for non-repudiation.

```
Agent Registration:
  1. Agent generates Ed25519 keypair
  2. POST /api/v1/agents/register { public_key, agent_type, capabilities }
  3. Server returns agent_id + API key
  4. Private key stored in ~/.aiki/credentials.toml

Push Authentication:
  1. Sign(private_key, hash(operation_ids)) → signature
  2. Push request includes { operations, signature, agent_id }
  3. Server verifies signature against registered public key
  4. Operation metadata records verified agent_id
```

### Operation Signing

Every operation pushed to the server is signed by the agent that created it. This provides:

- **Non-repudiation**: You can prove which agent created which operations
- **Tamper detection**: Modified operations won't pass signature verification
- **Audit trail**: The signature chain forms a verifiable provenance record

Operations signed by unrecognized keys are rejected. Operations with missing signatures are accepted only in a degraded "unsigned mode" that the admin must explicitly enable.

### Per-Bookmark Permissions

Not all agents should be able to modify all bookmarks:

```
Permission Model:
  repo:read    — Fetch operations and objects
  repo:write   — Push operations (to permitted bookmarks)
  repo:admin   — Manage permissions, delete repo

  bookmark:<name>:write  — Modify specific bookmark
  bookmark:*:write       — Modify any bookmark (default for repo:write)

Example policy:
  agent "claude-code":
    - repo:write
    - bookmark:main:write      (can push to main)
    - bookmark:aiki/tasks:write (can push task changes)

  agent "junior-agent":
    - repo:write
    - bookmark:feature/*:write  (feature branches only)
    - NOT bookmark:main:write   (must go through review)
```

### Audit Trail

The server logs all operations with:
- Verified agent identity (from signature)
- Source IP address
- Timestamp (server-side, not client-claimed)
- Operation content hash

Audit logs are append-only and stored separately from the repository data. They survive repository deletion.

---

## 8. Tasks Branch Sync

### Current Concurrency Failure Modes

Aiki's task storage (`cli/src/tasks/storage.rs`) uses a 3-subprocess pattern to write events:

1. `jj new aiki/tasks --no-edit` — Create a child change
2. `jj log -r "children(aiki/tasks) & description(...)"` — Query for the new change ID
3. `jj bookmark set aiki/tasks -r <change_id>` — Move the bookmark forward

**Problem: This is not atomic.** Between steps 1 and 3, another process can interfere:

```
Process A: jj new aiki/tasks         → creates change A1
Process B: jj new aiki/tasks         → creates change B1
          (Both see old bookmark, both create children)
Process A: query children()           → finds A1
Process A: bookmark set ... A1        → succeeds, bookmark moves to A1
Process B: query children()           → finds A1, B1 (multiple children!)
          (--limit 1 picks first alphabetically, may not be B1)
Process B: bookmark set ... ??? → may overwrite A's work, or set wrong change
```

The `--limit 1` safeguard handles orphaned changes from previous failed runs, but it doesn't protect against concurrent writes.

### The Linear Chain Assumption

Task storage assumes events form a linear chain:

```
event_1 → event_2 → event_3 → event_4  (assumed)
```

But with concurrent agents, the reality is a DAG:

```
         ┌─ event_2a (agent A)
event_1 ─┤
         └─ event_2b (agent B)
```

JJ handles DAGs naturally (divergent operation heads), but the task storage code doesn't — it expects `children(aiki/tasks)` to return exactly one result.

### How Native Remotes Solve This

With native JJ remote sync, the `aiki/tasks` branch sync becomes a standard operation sync:

1. Agent A creates a task event → creates an operation
2. Agent A pushes → server accepts the operation
3. Agent B fetches → imports the operation, sees the new task
4. If both created concurrent events → operation heads diverge → JJ auto-merges

The 3-subprocess non-atomic pattern is replaced by JJ's own transactional operation model. The bookmark update is part of the operation, not a separate step.

### Atomic Bookmark Updates

In the native remote protocol, bookmark updates are embedded in operations:

```protobuf
message Operation {
  // ...
  oneof mutation {
    SetBookmark set_bookmark = 13;
    // ...
  }
}
```

The server accepts or rejects the entire operation atomically. No TOCTOU race between reading current heads and updating the bookmark.

### Event DAG Support

With native remotes, task events naturally form a DAG instead of requiring a linear chain. The `materialize_tasks()` function would walk the DAG (all ancestors of the current bookmark target) rather than a linear chain:

```rust
// Current: walks linear chain, breaks on concurrent writes
fn walk_events(branch_tip: &ChangeId) -> Vec<TaskEvent> {
    let mut events = vec![];
    let mut current = branch_tip;
    loop {
        events.push(read_event(current));
        current = parent(current); // assumes single parent
    }
    events
}

// With DAG support: walks all ancestors, handles merge points
fn walk_events_dag(branch_tip: &ChangeId) -> Vec<TaskEvent> {
    let mut events = vec![];
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([branch_tip.clone()]);
    while let Some(id) = queue.pop_front() {
        if !visited.insert(id.clone()) { continue; }
        events.push(read_event(&id));
        for parent in parents(&id) {
            queue.push_back(parent);
        }
    }
    // Sort by timestamp for deterministic ordering
    events.sort_by_key(|e| e.timestamp);
    events
}
```

---

## 9. Phased Implementation

### Phase 0: Task Branch Git Push/Pull MVP (2 weeks)

**Immediate value before native protocol work.** This is the "real MVP" — most of native remotes' value for Aiki comes from syncing the tasks branch.

**Deliverables:**
- `aiki sync push` — pushes `aiki/tasks` bookmark to Git remote via `jj git push`
- `aiki sync pull` — fetches `aiki/tasks` from Git remote via `jj git fetch`
- `aiki sync` — pull then push (bidirectional)
- Handle merge conflicts on tasks branch (detect divergence, auto-merge if possible)
- Test with 2 machines sharing tasks

**Limitations:** No operation log sync, change IDs may diverge for non-header-aware Git hosts, no real-time updates. But tasks sync, which is the immediate need.

### Phase 1: Protocol Specification (6 weeks)

**Deliverables:**
- Protobuf schemas (as defined in Section 4)
- Authentication model (API key + operation signing)
- Conflict resolution semantics document
- Reference implementation of push/fetch in Rust (client-side only, testing against mock server)

Includes 2 weeks of buffer vs. the original 4-week estimate, based on review feedback that protocol design always takes longer than expected.

### Phase 2: Server + Client Implementation (8 weeks)

**Deliverables:**
- HTTP API server (axum) with ObjectStore + MetadataStore backends
- WebSocket real-time operation streaming
- Client RemoteBackend trait + HttpRemote implementation
- jj-lib integration (RepoRemoteExt trait)
- CLI commands: `jj remote add/remove`, `jj push`, `jj fetch`, `jj pull`

### Phase 3: Testing, Security Hardening (6 weeks)

**Deliverables:**
- Integration test suite (basic push/fetch, concurrent push conflict, metadata preservation, performance)
- Agent identity and operation signing implementation
- Per-bookmark permission enforcement
- Audit trail storage and query
- Edge case handling: network failures mid-push, server restart during WebSocket, large files, clock skew, repository corruption recovery

### Timeline Summary

| Phase | Duration | Key Deliverables |
|-------|----------|------------------|
| Phase 0: Task Branch MVP | 2 weeks | `aiki sync push/pull` via Git |
| Phase 1: Protocol Spec | 6 weeks | Protobuf schemas, auth model, conflict semantics |
| Phase 2: Server + Client | 8 weeks | HTTP/WS server, client library, CLI |
| Phase 3: Security + Testing | 6 weeks | Agent identity, signing, tests, hardening |
| **Total** | **~22 weeks** | Production-ready native JJ remotes |

Phase 0 ships immediate value. Phases 1-3 can overlap (e.g., start client work in week 4 of protocol spec as schemas stabilize).

---

## 10. JJ Upstream Strategy

### Google's Internal Server

From the [JJ Development Roadmap](https://jj-vcs.github.io/jj/latest/roadmap/):

> Google has an internal Jujutsu server backed by a database. This server allows commits and repos (operation logs) to be stored in the cloud. Working copies can still be stored locally.
>
> In order to reduce latency, there is a local daemon process that caches reads and writes. It also prefetches objects it thinks the client might ask for next.
>
> We (the project, not necessarily Google) want to provide a similar experience for all users.

This validates our design direction. Google has already built what we're designing. The JJ project wants to open-source a similar experience.

### RPC API Alignment

The JJ team has a [design doc for the RPC API](https://docs.google.com/document/d/1rOKvutee5TVYpFhh_UDNZDxfUKyrJ8rjCNpFaNHOHwU/edit):

> We want to provide an RPC API for tools that want to work with an unknown build of jj. The RPC API will probably be at a higher abstraction level than the Rust API.

Our protocol should be compatible with JJ's planned RPC API where possible. If they ship a gRPC service definition, we should align our protobuf schemas.

### Contribution Strategy: Build, Prove, Propose (Option C)

1. **Build** the server ourselves using jj-lib internally
2. **Prove** it works with real multi-agent workflows
3. **Propose** to JJ upstream as a reference implementation

If JJ accepts, we become maintainers of the official cloud backend. If they don't (or build something different), we maintain our implementation as an extension with Aiki-specific layers (provenance, tasks) on top.

**Tactical steps:**
- Engage with JJ maintainers on design before Phase 1 coding begins
- Open an RFC discussion on the JJ repository
- Use Git packfiles for storage (matching JJ's internal approach)
- Contribute jj-lib reliability improvements (e.g., [issue #5685](https://github.com/jj-vcs/jj/issues/5685)) as goodwill
- Keep the server's core generic (JJ remote protocol) with Aiki features as optional layers

---

## 11. Open Questions

### Resolved by This Update

| Question | Resolution |
|----------|------------|
| Do change IDs survive Git round-trips? | Partially — `git.write-change-id-header` handles the common case. Operation log still doesn't sync. |
| Which approach to take? (5 competing options) | Native JJ remotes. Hybrid Git is Phase 0 stepping stone only. |
| Is CRDT applicable? | Yes — task events are already CRDT-like. JJ operations provide the merge semantics. |
| What's the real MVP? | Phase 0: task branch Git push/pull (2 weeks), not the full native protocol. |

### Still Open

1. **JJ maintainer response** — Will they accept an RFC for native remotes, or prefer to open-source Google's implementation?
2. **Local daemon architecture** — Google uses a local daemon for caching. Do we need one, or is direct HTTP sufficient for our scale?
3. **Working copy sync** — This design syncs the JJ store but not the working copy. Agents that need file access still need local checkouts. Is a VFS or lazy-fetch model worth exploring?
4. **Backwards compatibility** — How do we handle clients running different protocol versions? Negotiation at connection time, or strict versioning?
5. **Offline mode** — Agents that lose connectivity should queue operations. How long can they stay offline before divergence becomes unmergeable?
6. **Multi-repo coordination** — Can one server manage multiple repositories? What's the isolation model between repos?

---

## 12. Appendix: Research References

### JJ Project Links

- [JJ Development Roadmap](https://jj-vcs.github.io/jj/latest/roadmap/) — confirms Google's internal server and plans to open-source
- [JJ Concurrency Docs](https://jj-vcs.github.io/jj/latest/technical/concurrency/) — operation log design, automatic 3-way merge
- [RPC API Design Doc](https://docs.google.com/document/d/1rOKvutee5TVYpFhh_UDNZDxfUKyrJ8rjCNpFaNHOHwU/edit) — planned higher-level API for tools
- [Issue #5685: Library Reliability for Servers](https://github.com/jj-vcs/jj/issues/5685) — others using jj-lib in server contexts
- [Issue #3577: Generalized Hook Support](https://github.com/jj-vcs/jj/issues/3577) — gRPC integration discussion
- [Discussion #2425: Remote-Backed Commit Store](https://github.com/martinvonz/jj/discussions/2425) — community discussion of jj-aware servers

### Community Discussions

- [Lobsters: Jujutsu (jj), a Git-Compatible VCS](https://lobste.rs/s/rojoz1/jujutsu_jj_git_compatible_vcs) — community sentiment on native vs Git backends

### Related Aiki Documentation

- `ops/ROADMAP.md` — Phase 21: Shared JJ Brain & Team Coordination
- `ops/later/WORKSPACE_AUTOMATION.md` — Local multi-agent workspaces
- `ops/later/AIKI_TWIN.md` — Personalized review agents
