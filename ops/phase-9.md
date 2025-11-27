# Plan: Phase 9 - Doc Management Action Type

## Problem

Flows need a way to create, update, and query structured documentation within the `.aiki/` directory. Phase 8's "Aiki Way" patterns (architecture caching, task documentation, session notes) all require persistent document storage that flows can interact with programmatically.

**Current situation (without doc management):**
- No way to cache architecture discoveries in flows
- Task documentation must be manual
- Session notes can't be auto-generated  
- Architecture patterns can't be stored and queried
- Each feature would need its own custom file I/O implementation

**This blocks Phase 8 milestones:**
- Milestone 2 (Auto Architecture Docs) needs to store discovered patterns
- Milestone 5 (Dev Docs System) needs to create/update task docs
- Session notes and other patterns need persistent storage

## Solution

Implement the `doc_management` action type that allows flows to create, update, append to, and query markdown documents. This provides a secure, consistent interface for document operations across all flows.

**Key capabilities:**
- **Create** - Create new documents (error if exists)
- **Update** - Overwrite entire documents
- **Append** - Add content to end of documents  
- **Query** - Read document content into variables

**Security built-in:**
- All operations restricted to `.aiki/` directory
- Path traversal detection (blocks `..` attempts)
- Atomic writes (temp file + rename)
- Clear error messages

## What We Build

### 1. Doc Management Action Type

Four operations for managing docs in flows:

**Create** - Create new document (error if exists):
```yaml
doc_management:
  operation: create
  path: .aiki/arch/structure/backend/index.md
  content: |
    # Backend Architecture
    Discovered patterns...
```

**Update** - Overwrite entire document:
```yaml
doc_management:
  operation: update
  path: .aiki/tasks/current/status.md
  content: "Status: In Progress"
```

**Append** - Add content to end of document:
```yaml
doc_management:
  operation: append
  path: .aiki/sessions/notes.md
  content: |
    - Completed auth implementation
```

**Query** - Read document content into variable:
```yaml
- doc_management:
    operation: query
    path: .aiki/arch/structure/backend/index.md
    variable: backend_arch

- if: $backend_arch contains "OAuth2"
  then:
    prompt: "Remember we use OAuth2 for auth"
```

### 2. Automatic Directory Creation

Parent directories created automatically:

```yaml
doc_management:
  operation: create
  path: .aiki/tasks/deep/nested/path/doc.md  # Creates all parent dirs
  content: "Content"
```

### 3. Path Security

**Valid paths** (within `.aiki/`):
```yaml
doc_management:
  operation: create
  path: .aiki/tasks/plan.md
```

**Invalid paths** (blocked):
```yaml
# Outside .aiki/
doc_management:
  operation: create
  path: /etc/passwd  # ERROR: Path must be within .aiki/

# Path traversal attempt
doc_management:
  operation: create
  path: .aiki/../../../etc/passwd  # ERROR: Path traversal detected
```

### 4. Atomic Writes

All write operations are atomic to prevent corruption:

```rust
// Write to temp file first
let temp_path = path.with_extension("tmp");
fs::write(&temp_path, content)?;

// Then rename (atomic on most filesystems)
fs::rename(temp_path, path)?;
```

## Example Use Cases

### Use Case 1: Architecture Caching (Phase 8, Milestone 2)

```yaml
PostResponse:
  # Detect architecture exploration
  - if: self.files_read_in_directory("src/") > 5
    then:
      - let: pattern_description = self.extract_pattern()
      - doc_management:
          operation: append
          path: .aiki/arch/structure/backend/index.md
          content: |
            
            ## Pattern Discovered - $timestamp
            $pattern_description
```

### Use Case 2: Task Documentation (Phase 8, Milestone 5)

```yaml
PrePrompt:
  # Create task plan when user starts new task
  - doc_management:
      operation: create
      path: .aiki/tasks/$task_id/plan.md
      content: |
        # Task: $task_name
        
        ## Goal
        $task_description
        
        ## Created
        $timestamp

PostResponse:
  # Update progress as work proceeds
  - doc_management:
      operation: append
      path: .aiki/tasks/$task_id/plan.md
      content: |
        
        ## Progress Update - $timestamp
        - $completed_work
```

### Use Case 3: Session Notes

```yaml
PostResponse:
  # Record what was accomplished
  - doc_management:
      operation: append
      path: .aiki/sessions/$session_id/notes.md
      content: |
        - $timestamp: $event.response summary()
```

### Use Case 4: Checklist Management

```yaml
PrePrompt:
  # Read checklist
  - doc_management:
      operation: query
      path: .aiki/tasks/current/checklist.md
      variable: checklist
  
  # Check if all items completed
  - if: $checklist not_contains "[ ]"
    then:
      - shell: echo "All checklist items complete!"
```

## Commands Delivered

**No new CLI commands** - This is a flow action type only. Flows use it programmatically.

Example usage in flows:
```yaml
PostResponse:
  - doc_management:
      operation: append
      path: .aiki/arch/patterns/discovered.md
      content: |
        ## Pattern: $pattern_name
        $pattern_description
```

## Value Delivered

### For Phase 8 Milestones
- ✅ **Enables Milestone 2** - Architecture docs cached via doc_management
- ✅ **Enables Milestone 5** - Task docs created and updated automatically  
- ✅ **Enables session notes** - Track work across sessions in `.aiki/sessions/`

### For Flow Authors
- **Persistent state** - Store data across flow executions
- **Queryable docs** - Load and check existing documentation
- **Safe operations** - Security built-in, no path traversal risks
- **Consistent API** - Same interface for all document operations

### For Aiki
- **Single implementation** - All document features use the same code path
- **Secure by default** - Path validation prevents common vulnerabilities
- **Composable** - Works with other flow actions (if/then, let, etc.)

## Technical Components

| Component | Complexity | Priority | Timeline |
|-----------|------------|----------|----------|
| Doc management action parser | Low | High | 1 day |
| Path validation & security | Medium | High | 1 day |
| Create/update/append operations | Low | High | 1 day |
| Query operation with variables | Low | High | 1 day |
| Atomic write implementation | Low | High | 1 day |
| Unit tests | Medium | High | 1 day |
| Integration tests | Medium | High | 1 day |
| Documentation & examples | Low | Medium | 1 day |

## Implementation Tasks

### Core Action (3 days)

- [ ] Create `cli/src/flows/actions/doc_management.rs`
  - [ ] `DocManagementAction` struct with operation enum
  - [ ] Parse `doc_management` action from YAML
  - [ ] Implement `create` operation
  - [ ] Implement `update` operation
  - [ ] Implement `append` operation
  - [ ] Implement `query` operation
  - [ ] Path validation logic
  - [ ] Atomic write implementation

### Path Security (1 day)

- [ ] Validate paths are within `.aiki/`
  - [ ] Check path starts with `.aiki/`
  - [ ] Detect path traversal attempts (`..`)
  - [ ] Resolve symlinks and check final path
  - [ ] Error on invalid paths with clear messages

### Engine Integration (1 day)

- [ ] Register `doc_management` action in flow parser
- [ ] Add action executor to engine
- [ ] Variable assignment for `query` operation
- [ ] Error handling and reporting

### Testing (2 days)

**Unit tests:**
- [ ] Create operation (new file, error if exists)
- [ ] Update operation (overwrite file)
- [ ] Append operation (add to end)
- [ ] Query operation (read into variable)
- [ ] Path validation (valid paths accepted)
- [ ] Path traversal detection (invalid paths blocked)
- [ ] Atomic writes (temp file + rename)

**Integration tests:**
- [ ] Multiple operations in sequence
- [ ] Variable assignment from query
- [ ] Real doc management workflows

**E2E tests:**
- [ ] Architecture caching workflow
- [ ] Task documentation workflow
- [ ] Session notes workflow

### Documentation (1 day)

- [ ] Tutorial: "Managing Documentation with Flows"
- [ ] Cookbook: Common patterns (task docs, architecture discovery)
- [ ] Reference: Doc management syntax
- [ ] Examples: Real-world doc management flows

## Technical Design

### Action Structure

```rust
pub struct DocManagementAction {
    pub operation: DocOperation,
    pub path: PathBuf,
    pub content: Option<String>,
    pub variable: Option<String>,  // For query operation
}

pub enum DocOperation {
    Create,
    Update,
    Append,
    Query,
}
```

### Path Validation

```rust
pub fn validate_path(path: &Path) -> Result<PathBuf> {
    // Ensure path starts with .aiki/
    if !path.starts_with(".aiki/") {
        return Err(AikiError::InvalidDocPath {
            path: path.to_path_buf(),
            reason: "Path must be within .aiki/ directory".to_string(),
        });
    }
    
    // Resolve to absolute path
    let absolute = workspace_root()?.join(path);
    let canonical = absolute.canonicalize()?;
    
    // Ensure resolved path is still within .aiki/
    let aiki_dir = workspace_root()?.join(".aiki").canonicalize()?;
    if !canonical.starts_with(&aiki_dir) {
        return Err(AikiError::PathTraversalDetected {
            path: path.to_path_buf(),
        });
    }
    
    Ok(canonical)
}
```

### Atomic Write

```rust
pub fn atomic_write(path: &Path, content: &str) -> Result<()> {
    // Create parent directories
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Write to temp file
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, content)?;
    
    // Atomic rename
    fs::rename(&temp_path, path)?;
    
    Ok(())
}
```

## Success Criteria

- ✅ Can create new documents from flows
- ✅ Can update existing documents
- ✅ Can append to documents
- ✅ Can query document content into variables
- ✅ Parent directories created automatically
- ✅ Path validation prevents security issues
- ✅ Path traversal attempts blocked
- ✅ Atomic writes prevent corruption
- ✅ Clear error messages for invalid operations
- ✅ All operations work in real flows
- ✅ Integration with Phase 8 milestones validated

## Error Handling

### Document Already Exists

```
Error: Document already exists

Path: .aiki/tasks/my-feature/plan.md

Use operation 'update' to overwrite or 'append' to add content.
```

### Document Not Found (Query)

```
Error: Document not found

Path: .aiki/tasks/missing/plan.md

Create the document first with operation 'create'.
```

### Invalid Path

```
Error: Invalid document path

Path: /etc/passwd

Doc management paths must be within .aiki/ directory for security.
```

### Path Traversal Detected

```
Error: Path traversal detected

Path: .aiki/../../../etc/passwd
Resolved to: /etc/passwd

This path escapes the .aiki/ directory and is not allowed.
```

## Timeline

**Estimated: 1 week**

| Day | Focus |
|-----|-------|
| 1-2 | Core operations, path validation |
| 3 | Engine integration, variable assignment |
| 4-5 | Testing (unit, integration, E2E) |
| 6 | Documentation and examples |
| 7 | Code review and polish |

## Why This Enables Phase 8

Phase 8's "Aiki Way" patterns fundamentally depend on doc_management:

1. **Architecture Caching (Milestone 2)** - Needs to store discovered patterns in `.aiki/arch/`
2. **Task Documentation (Milestone 5)** - Needs to create/update task docs in `.aiki/tasks/`
3. **Session Notes** - Needs to append session progress to `.aiki/sessions/`
4. **Skills** - May need to query existing documentation for context

Without doc_management, each of these features would need custom file I/O implementations, leading to:
- Inconsistent security checks
- Duplicated atomic write logic
- Different error handling patterns
- More code to maintain

By implementing doc_management first, we provide a secure, tested foundation for all document operations in flows.

## Future Enhancements

### 1. Template Support

Use templates for common doc types:

```yaml
doc_management:
  operation: create_from_template
  template: aiki/task-plan
  path: .aiki/tasks/$task_id/plan.md
  variables:
    task_name: "User Authentication"
    priority: "High"
```

### 2. Doc Queries with Selectors

Query document structure:

```yaml
- doc_management:
    operation: query
    path: .aiki/tasks/current/plan.md
    selector: "## Progress"  # Extract specific section
    variable: progress
```

### 3. Batch Operations

Operate on multiple docs:

```yaml
doc_management:
  operation: update_all
  pattern: ".aiki/tasks/*/status.md"
  content: "Status: In Progress"
```

## Phase Dependencies

**Depends on:**
- Phase 5 (Internal Flow Engine) - Provides flow action infrastructure

**Enables:**
- Phase 8, Milestone 2 (Auto Architecture Docs) - Uses doc_management for caching
- Phase 8, Milestone 5 (Dev Docs System) - Uses doc_management for task docs
- Future phases that need persistent document storage

## References

- `ops/ROADMAP.md` - Overall phase plan
- `ops/current/milestone-1.4-doc-management.md` - Original milestone document (deprecated, now part of Phase 9)
- `ops/the-aiki-way.md` - Phase 8 patterns that use doc_management
