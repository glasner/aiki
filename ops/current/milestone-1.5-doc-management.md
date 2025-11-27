# Milestone 1.5: Doc Management Action Type

This document outlines the implementation plan for the Doc Management action system (Milestone 1.5).

See [milestone-1.md](./milestone-1.md) for the full Milestone 1 overview.

---

## Overview

The Doc Management action allows flows to create, update, append to, and query structured markdown documentation within the `.aiki/` directory.

**Key Capabilities:**
- Create/update/append to markdown docs
- Operations: `create`, `update`, `append`, `query`
- Automatic directory creation
- Path validation for security

---

## Core Features

### 1. Doc Management Operations

Four operations for managing docs:

**Create** - Create new document (error if exists):
```yaml
doc_management:
  operation: create
  path: .aiki/tasks/my-feature/plan.md
  content: |
    # Feature Plan
    Implement user authentication
```

**Update** - Overwrite entire document:
```yaml
doc_management:
  operation: update
  path: .aiki/tasks/my-feature/plan.md
  content: |
    # Feature Plan (Updated)
    Implement OAuth2 authentication
```

**Append** - Add content to end of document:
```yaml
doc_management:
  operation: append
  path: .aiki/tasks/my-feature/plan.md
  content: |
    
    ## Progress Update
    - Database schema complete
    - API endpoints in progress
```

**Query** - Read document content into variable:
```yaml
- doc_management:
    operation: query
    path: .aiki/tasks/my-feature/plan.md
    variable: plan_content

- if: $plan_content contains "OAuth2"
  then:
    - shell: echo "Using OAuth2 authentication"
```

### 2. Automatic Directory Creation

Parent directories created automatically:

```yaml
doc_management:
  operation: create
  path: .aiki/tasks/deep/nested/path/doc.md  # Creates all parent dirs
  content: "Content"
```

### 3. Path Validation

Only paths within `.aiki/` are allowed:

```yaml
# ✅ Valid
doc_management:
  operation: create
  path: .aiki/tasks/plan.md

# ❌ Invalid - outside .aiki/
doc_management:
  operation: create
  path: /etc/passwd  # ERROR: Path must be within .aiki/

# ❌ Invalid - path traversal attempt
doc_management:
  operation: create
  path: .aiki/../../../etc/passwd  # ERROR: Path traversal detected
```

### 4. Atomic Writes

All write operations are atomic:

```rust
// Write to temp file first
let temp_path = path.with_extension("tmp");
fs::write(&temp_path, content)?;

// Then rename (atomic on most filesystems)
fs::rename(temp_path, path)?;
```

---

## Use Cases

### Use Case 1: Task Documentation

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

### Use Case 2: Architecture Discovery

```yaml
PostResponse:
  # Agent discovers new architecture pattern
  - let: pattern_description = $event.response extract_pattern()
  
  - doc_management:
      operation: append
      path: .aiki/arch/patterns/discovered.md
      content: |
        
        ## Pattern Discovered - $timestamp
        $pattern_description
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
PrePromit:
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

---

## Implementation Tasks

### Core Action

- [ ] Create `cli/src/flows/actions/doc_management.rs`
  - [ ] Parse `doc_management` action
  - [ ] Implement `create` operation
  - [ ] Implement `update` operation
  - [ ] Implement `append` operation
  - [ ] Implement `query` operation
  - [ ] Path validation logic
  - [ ] Atomic write implementation

### Path Security

- [ ] Validate paths are within `.aiki/`
  - [ ] Check path starts with `.aiki/`
  - [ ] Detect path traversal attempts (`..`)
  - [ ] Resolve symlinks and check final path
  - [ ] Error on invalid paths

### Engine Integration

- [ ] Register `doc_management` action in flow parser
- [ ] Add action executor to engine
- [ ] Variable assignment for `query` operation
- [ ] Error handling and reporting

### Testing

- [ ] Unit tests: Create operation
- [ ] Unit tests: Update operation
- [ ] Unit tests: Append operation
- [ ] Unit tests: Query operation
- [ ] Unit tests: Path validation
- [ ] Unit tests: Path traversal detection
- [ ] Unit tests: Atomic writes
- [ ] Integration tests: Multiple operations in sequence
- [ ] Integration tests: Variable assignment from query
- [ ] E2E tests: Real doc management workflows

### Documentation

- [ ] Tutorial: "Managing Documentation with Flows"
- [ ] Cookbook: Common patterns (task docs, architecture discovery)
- [ ] Reference: Doc management syntax
- [ ] Examples: Real-world doc management flows

---

## Success Criteria

✅ Can create new documents  
✅ Can update existing documents  
✅ Can append to documents  
✅ Can query document content into variables  
✅ Parent directories created automatically  
✅ Path validation prevents security issues  
✅ Path traversal attempts blocked  
✅ Atomic writes prevent corruption  
✅ Clear error messages for invalid operations  

---

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

### Operation Implementations

```rust
impl DocManagementAction {
    pub fn execute(&self, context: &mut FlowContext) -> Result<()> {
        let validated_path = validate_path(&self.path)?;
        
        match self.operation {
            DocOperation::Create => {
                if validated_path.exists() {
                    return Err(AikiError::DocAlreadyExists {
                        path: self.path.clone(),
                    });
                }
                atomic_write(&validated_path, self.content.as_ref().unwrap())?;
            }
            
            DocOperation::Update => {
                atomic_write(&validated_path, self.content.as_ref().unwrap())?;
            }
            
            DocOperation::Append => {
                let mut existing = if validated_path.exists() {
                    fs::read_to_string(&validated_path)?
                } else {
                    String::new()
                };
                existing.push_str(self.content.as_ref().unwrap());
                atomic_write(&validated_path, &existing)?;
            }
            
            DocOperation::Query => {
                if !validated_path.exists() {
                    return Err(AikiError::DocNotFound {
                        path: self.path.clone(),
                    });
                }
                let content = fs::read_to_string(&validated_path)?;
                
                if let Some(var_name) = &self.variable {
                    context.set_variable(var_name, content);
                }
            }
        }
        
        Ok(())
    }
}
```

---

## Example Workflows

### Workflow 1: Complete Task Documentation

```yaml
# When task starts
PrePrompt:
  - doc_management:
      operation: create
      path: .aiki/tasks/$task_id/plan.md
      content: |
        # Task: $task_name
        Started: $timestamp
  
  - doc_management:
      operation: create
      path: .aiki/tasks/$task_id/notes.md
      content: "# Session Notes\n"

# During work
PostResponse:
  - doc_management:
      operation: append
      path: .aiki/tasks/$task_id/notes.md
      content: |
        - $timestamp: $event.response summary()

# When task completes
PostCommit:
  - doc_management:
      operation: append
      path: .aiki/tasks/$task_id/plan.md
      content: |
        
        ## Completed
        - Finished: $timestamp
        - Commits: $commit_count
```

### Workflow 2: Architecture Cache

```yaml
PostResponse:
  # Query existing architecture doc
  - doc_management:
      operation: query
      path: .aiki/arch/structure/backend/index.md
      variable: existing_arch
  
  # Check if new pattern is already documented
  - if: $existing_arch not_contains "$new_pattern"
    then:
      - doc_management:
          operation: append
          path: .aiki/arch/structure/backend/index.md
          content: |
            
            ## $new_pattern_name
            $new_pattern_description
```

### Workflow 3: Daily Summaries

```yaml
PostSession:
  - let: today = $timestamp format("%Y-%m-%d")
  - let: summary = self.generate_session_summary()
  
  - doc_management:
      operation: append
      path: .aiki/logs/daily/$today.md
      content: |
        
        ## Session $session_id
        $summary
```

---

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

---

## Expected Timeline

**Week 3**

- Days 1-2: Core operations, path validation
- Days 3-4: Engine integration, variable assignment
- Day 5: Testing and documentation

---

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

### 2. Doc Queries

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

---

## References

- [milestone-1.md](./milestone-1.md) - Milestone 1 overview
- [ROADMAP.md](../ROADMAP.md) - Strategic context
