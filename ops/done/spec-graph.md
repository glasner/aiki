# SpecGraph: First-Class Spec Management

**Dependencies**: This spec builds on [spec-file-frontmatter.md](./spec-file-frontmatter.md) for the `draft` field. The spec-to-plan link comes from the TaskGraph (task edges), not frontmatter.

## Vision

Currently, specs (design documents) are **implicit nodes** in the TaskGraph's edge index:
- Specs referenced as `file:` URIs in task edges
- `graph.edges.referrers(&target, "implements")` finds tasks implementing a spec
- This creates an **implicit graph**: `Spec ← implements ← Plan ← subtask ← Tasks`

**Make this explicit with SpecGraph:**

```
SpecGraph:
  specs: map[spec_path -> Spec]
  spec_to_tasks: map[spec_path -> [task_ids]]  // O(1) reverse index

Spec:
  path: "file:ops/now/foo.md"
  title: extracted from markdown H1
  description: first paragraph
  created_at: timestamp
  draft: boolean (from frontmatter)
  status: Draft | Planned | Implementing | Implemented (inferred from TaskGraph)
```

**Spec relationships:**
- `implements` - which tasks implement this spec (from TaskGraph edges)
- `refines` - spec B refines/extends spec A
- `depends-on` - spec needs another spec implemented first
- `supersedes` - newer spec replaces older one

**Benefits:**
- O(1) lookup: "what tasks implement this spec?"
- Query "what specs have no implementations?"
- Query "what specs depend on this one?"
- Track spec lifecycle (draft → planned → implementing → implemented) by inferring from TaskGraph + draft flag
- Filter out drafts (specs not ready for implementation)
- Validate circular dependencies between specs
- Enable spec versioning/evolution
- Visualize the spec dependency tree

## Phase 0: Build SpecGraph Foundation

Build the SpecGraph abstraction first, then use it for all spec operations.

**Prerequisites**: Spec file frontmatter must be implemented first (see [spec-file-frontmatter.md](./spec-file-frontmatter.md)). The frontmatter utility (`cli/src/frontmatter.rs`) is used to read the `draft` field.

### Implementation Plan

**1. Create SpecGraph module structure**

```
cli/src/specs/
  mod.rs        # Module exports
  graph.rs      # SpecGraph struct and queries
  parser.rs     # Parse metadata from markdown files
```

**2. Build SpecGraph from filesystem + TaskGraph**

```
SpecGraph.build(cwd, task_graph):
  specs = {}
  spec_to_tasks = {}
  
  # Scan filesystem for spec files
  for file in find_markdown_files("ops/now/", "ops/done/"):
    spec = parse_spec_from_markdown(file)
    specs[spec.path] = spec
  
  # Build reverse index: spec -> implementing tasks
  # This comes from TaskGraph edges, NOT frontmatter
  for task_id, task in task_graph.tasks:
    for edge in task.outgoing_edges where edge.type == "implements":
      if edge.target.starts_with("file:"):
        spec_to_tasks[edge.target].append(task_id)
  
  # Infer status for all specs
  for spec in specs.values():
    spec.status = infer_status(spec, spec_to_tasks, task_graph)
  
  return SpecGraph(specs, spec_to_tasks)
```

**3. Core query operations**

```
# O(1) lookup - what tasks implement this spec?
SpecGraph.implementing_tasks(spec_path, task_graph):
  task_ids = spec_to_tasks[normalize(spec_path)]
  return [task_graph.tasks[id] for id in task_ids]

# Find most recent valid plan for a spec
SpecGraph.find_plan_for_spec(spec_path, task_graph):
  candidates = implementing_tasks(spec_path, task_graph)
  
  # Close invalid plans (no subtasks)
  for plan in candidates:
    if plan.subtasks.is_empty() and plan.status != Closed:
      close_as_invalid(plan, reason="no subtasks")
  
  # Return most recent valid plan
  valid_plans = [p for p in candidates 
                 if p.has_subtasks() and p.status != Closed]
  return max(valid_plans, key=lambda p: p.created_at)

# What specs have no implementation?
SpecGraph.unimplemented():
  return [spec for spec in specs.values() 
          if spec_to_tasks[spec.path].is_empty()]

# What specs are drafts?
SpecGraph.drafts():
  return [spec for spec in specs.values() if spec.draft]

# What specs are ready for implementation?
SpecGraph.ready():
  return [spec for spec in specs.values() 
          if not spec.draft and spec.status == Draft]
```

**4. Parse spec metadata from markdown**

```
parse_spec_from_markdown(file_path):
  content = read_file(file_path)
  
  # Read frontmatter (requires cli/src/frontmatter.rs from spec-file-frontmatter.md)
  (frontmatter, body) = read_frontmatter(file_path)
  
  spec.path = "file:" + file_path
  spec.title = extract_first_h1(body)
  spec.description = extract_first_paragraph_after_h1(body)
  spec.created_at = file_creation_time(file_path)
  
  # Read draft flag from frontmatter (only field in frontmatter)
  spec.draft = frontmatter.get("draft").unwrap_or(false)
  
  # Status is inferred later from TaskGraph
  spec.status = None  # Will be set by infer_status()
  
  return spec

extract_first_h1(content):
  for line in content.lines():
    if line.starts_with("# "):
      return line.remove_prefix("# ").trim()

extract_first_paragraph_after_h1(content):
  skip_until_after_h1()
  collect_lines_until_empty_line()
  return joined_paragraph
```

**Status Inference Logic:**

Status is **inferred from the TaskGraph** combined with the `draft` flag:

```
infer_status(spec, spec_to_tasks, task_graph):
  # Check draft flag first
  if spec.draft:
    return Draft  # Explicitly marked as draft, ignore task state
  
  # Find implementing tasks from TaskGraph edges
  task_ids = spec_to_tasks[spec.path]
  if task_ids.is_empty():
    return Draft  # No plan exists
  
  # Get implementing tasks (plans)
  plans = [task_graph.tasks[id] for id in task_ids]
  
  # Find most recent open plan
  open_plans = [p for p in plans if p.status != Closed]
  if open_plans.is_empty():
    # All plans are closed - check if any succeeded
    succeeded = [p for p in plans if p.status == Closed and p.close_reason != WontDo]
    if succeeded:
      return Implemented  # At least one plan succeeded
    else:
      return Draft  # All plans failed/wont_do, back to draft
  
  # Found open plan(s) - use most recent
  plan = max(open_plans, key=lambda p: p.created_at)
  
  # Check plan status
  if plan.status == InProgress:
    return Implementing  # Actively being worked on
  else:
    return Planned  # Plan exists but not started yet
```

**Status meanings:**
- `Draft` - `draft: true` in frontmatter, OR no plan exists (or all plans closed as wont_do/failed)
- `Planned` - Plan exists and is open (not started)
- `Implementing` - Plan is in_progress
- `Implemented` - Plan is closed (successfully)

**Why use TaskGraph edges instead of frontmatter:**
- **TaskGraph is the source of truth** for task relationships
- **No duplication**: Don't store the same link in two places
- **Always consistent**: Can't have stale frontmatter pointing to wrong task
- **Existing infrastructure**: `implements` edges already exist
- **Frontmatter stays minimal**: Only `draft` flag, nothing else

### Benefits of Phase 0 First

1. **Better abstraction**: All spec operations go through SpecGraph
2. **Performance**: O(1) lookups from the start
3. **Extensibility**: Easy to add new queries later
4. **Consistency**: Single source of truth (TaskGraph for links + minimal frontmatter for draft flag)
5. **Testing**: Can test SpecGraph independently
6. **Separation of concerns**: Status and links from TaskGraph, draft flag from frontmatter
7. **Draft filtering**: Can easily filter out specs not ready for implementation

### Migration Strategy

**Step 1: Build SpecGraph module** (no behavior changes)
- Create `cli/src/specs/` module
- Implement `SpecGraph::build()` and basic queries
- Use `frontmatter.rs` to read `draft` field only
- Build spec_to_tasks index from TaskGraph edges
- Add tests

**Step 2: Migrate existing code** (refactor, same behavior)

*Codebase exploration identified 9 locations using spec/plan mapping:*

**Critical: Duplicate `find_plan_for_spec()` functions**
- `plan.rs:292` - Uses `graph.edges.referrers("file:<spec>", "implements")`
- `build.rs:429` - Identical duplicate of above
- **Action**: Replace both with `SpecGraph::find_plan_for_spec()`

**Other locations to refactor:**
- `plan.rs:find_created_plan()` - Finds plans by `data.spec` + source matching
- `build.rs:cleanup_stale_builds()` - Matches `data.spec` on orchestrator tasks
- `build.rs:run_show()` - Matches `data.spec` on orchestrator tasks
- `review.rs` - `ReviewScopeKind::Spec` resolves via `task.data.get("spec")`
- `status_monitor.rs:299` - Uses `data.get("plan")` to render plan subtrees
- `graph.rs:467-478` - Edge synthesis: `data.get("spec")` → `implements` edge

**Migration tasks:**
- [ ] Update `plan.rs` to use SpecGraph (remove duplicate at line 292)
- [ ] Update `build.rs` to use SpecGraph (remove duplicate at line 429)
- [ ] Update `build.rs:cleanup_stale_builds()` to use SpecGraph
- [ ] Update `build.rs:run_show()` to use SpecGraph
- [ ] Update `review.rs` to use SpecGraph for spec scope resolution
- [ ] Consider migrating `plan.rs:find_created_plan()` to SpecGraph
- [ ] Verify `status_monitor.rs` still works correctly
- [ ] Keep `graph.rs` edge synthesis for backward compatibility

**Step 3: Verify** (same behavior, better foundation)
- All existing commands work identically
- Performance improvement on multi-spec operations
- Foundation ready for Phase 1


## Phase 1: Deterministic Plan Lookup (Build on SpecGraph)

Now that SpecGraph is in place, make `aiki plan <spec>` deterministic.

### Problem

`aiki plan <spec>` has **interactive prompts** that block automation:

**Current behavior:**
- No plan exists → creates new plan ✅
- Incomplete plan exists → **shows interactive prompt** ❌
- In non-interactive context (piped) → **errors** ❌

**This breaks templates:**
```bash
# Build template needs this workaround:
PLAN=$(aiki plan show {{data.spec}} --output id || aiki plan {{data.spec}} --output id)
```

### Solution

Make `aiki plan <spec>` deterministic (find-or-create):

```
aiki_plan(spec_path, restart_flag):
  if restart_flag:
    return create_new_plan(spec_path)
  
  # Find existing valid plan
  spec_graph = SpecGraph.build()
  
  # Check if spec is a draft
  spec = spec_graph.specs.get(spec_path)
  if spec and spec.draft:
    error("Cannot create plan for draft spec. Remove draft: true from frontmatter first.")
  
  existing_plan = spec_graph.find_plan_for_spec(spec_path)
  
  if existing_plan:
    print(existing_plan.id)
    return existing_plan
  else:
    return create_new_plan(spec_path)

# No interactive prompts - just do the right thing!
```

**New behavior:**
- Spec is marked `draft: true` → error (can't plan a draft)
- No plan exists → create new plan
- Valid incomplete plan exists → return it (no prompt)
- Invalid plan exists (no subtasks) → close as wont_do, then create new plan
- Closed plan exists → create new plan
- `--restart` flag → always create new plan

### Benefits

1. **Simpler templates**: `aiki plan {{data.spec}} --output id` (no fallback needed)
2. **Deterministic**: No interactive prompts, works in scripts/automation
3. **Validates quality**: Invalid plans (no subtasks) are rejected
4. **Draft protection**: Can't accidentally plan a draft spec
5. **Clear errors**: Tell users how to recover (`--restart`)
6. **Backward compatible**: `--restart` flag unchanged
7. **Built on SpecGraph**: Reuses O(1) lookups from Phase 0

### Plan Validation

```
validate_plan(plan):
  subtasks = get_subtasks(plan)
  
  if subtasks.is_empty():
    # Invalid - planning agent failed to create subtasks
    close_plan_as_wont_do(plan, summary="No subtasks created")
    return Invalid
  
  return Valid

# Called automatically in find_plan_for_spec()
```

**Why close instead of error:**
- Invalid plans indicate planning agent failed
- Automatically recover by closing failed plan and creating fresh one
- No manual intervention required from user

### Edge Cases

1. **Multiple plans for same spec**: Returns most recent valid plan
2. **Closed plans**: Find-or-create creates new plan (doesn't reuse closed)
3. **Invalid plan (no subtasks, still open)**: Auto-closed as wont_do, new plan created
4. **Invalid plan (no subtasks, already closed)**: Silently skipped, new plan created
5. **Plan has only closed subtasks**: Still valid (user might have completed them all)
6. **Draft spec**: Error with helpful message to remove `draft: true` first

## Roadmap

**Phase 0** (Foundation): Build SpecGraph ← **Do this first**
- **Prerequisites**: Complete spec-file-frontmatter.md implementation first (for `draft` field)
- Create `cli/src/specs/` module
- Implement SpecGraph with O(1) reverse index
- Parse spec metadata from markdown (using frontmatter for `draft` only)
- Build spec-to-task index from TaskGraph edges (not frontmatter)
- Infer status from TaskGraph (not frontmatter)
- Migrate existing code to use SpecGraph

**Phase 1** (Deterministic): Make `aiki plan <spec>` deterministic
- Implement find-or-create behavior using SpecGraph
- Add plan validation (no subtasks = invalid)
- Add draft protection (can't plan a draft)
- Remove interactive prompts
- Simplify templates

**Phase 2** (Extended): Add rich spec commands ← **Future (see ops/future/)**
- `aiki spec list --ready` (exclude drafts)
- `aiki spec list --drafts` (show only drafts)
- `aiki spec graph` (visualization)
- Spec dependencies (`depends-on`, `refines`)
- Circular dependency detection
