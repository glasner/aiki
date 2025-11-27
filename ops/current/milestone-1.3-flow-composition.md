# Milestone 1.3: Flow Composition

This document outlines the implementation plan for the Flow Composition system (Milestone 1.3).

See [milestone-1.md](./milestone-1.md) for the full Milestone 1 overview.

---

## Overview

Flow Composition allows flows to include and reuse other flows, enabling modular, composable workflow design.

**Key Capabilities:**
- Include other flows via `includes:` directive
- Invoke flows inline with `flow:` action
- Flow resolution (aiki/*, vendor/*, local paths)
- Circular dependency detection

---

## Core Features

### 1. Flow Includes Directive

Include other flows at the top level:

```yaml
name: "My Workflow"
version: "1.0"

includes:
  - aiki/quick-lint
  - aiki/build-check
  - vendor/security-scan
  - ./local-checks.yml

PreCommit:
  - shell: echo "Running checks..."
```

**How it works:**
- Included flows are loaded and merged before parent flow executes
- Included flow actions execute before parent flow actions
- All events from included flows are available

### 2. Flow Action (Inline Invocation)

Invoke flows inline during execution:

```yaml
PostResponse:
  - let: error_count = self.count_errors
  
  - if: $error_count > 0
    then:
      - flow: aiki/quick-lint  # Invoke flow inline
  
  - shell: echo "All checks passed"
```

**Difference from includes:**
- `includes:` - Merge at parse time, always executes
- `flow:` action - Invoke at runtime, conditionally

### 3. Flow Resolution

Flows are resolved in this order:

1. **Built-in flows:** `aiki/*` → `~/.aiki/flows/aiki/`
2. **Vendor flows:** `vendor/*` → `~/.aiki/flows/vendor/`
3. **Local flows:** `./` or absolute paths → relative to current flow

**Examples:**
```yaml
includes:
  - aiki/quick-lint           # ~/.aiki/flows/aiki/quick-lint.yml
  - vendor/eslint             # ~/.aiki/flows/vendor/eslint.yml
  - ./my-checks.yml           # .aiki/flows/my-checks.yml
  - /abs/path/checks.yml      # Absolute path
```

### 4. Circular Dependency Detection

Prevent infinite loops:

```yaml
# flow-a.yml
includes:
  - ./flow-b.yml

# flow-b.yml
includes:
  - ./flow-a.yml  # ERROR: Circular dependency detected
```

**Detection mechanism:**
- Track flow call stack during loading
- Error if flow appears twice in stack
- Clear error message with cycle path

---

## Use Cases

### Use Case 1: Reusable Lint Checks

```yaml
# aiki/quick-lint.yml
name: "Quick Lint"
PostResponse:
  - let: lint_errors = self.count_lint_errors
  - if: $lint_errors > 0
    then:
      autoreply: "Fix $lint_errors linting issues"

# User's flow
includes:
  - aiki/quick-lint  # Reuse lint checks
```

### Use Case 2: Conditional Flow Invocation

```yaml
PostResponse:
  - let: files = self.get_edited_files
  
  - if: $files contains ".ts"
    then:
      - flow: aiki/typescript-check
  
  - if: $files contains ".rs"
    then:
      - flow: aiki/rust-check
```

### Use Case 3: Multi-Layer Composition

```yaml
# aiki/default.yml
includes:
  - aiki/quick-lint
  - aiki/build-check
  - aiki/test-runner

# User's custom-workflow.yml
includes:
  - aiki/default  # Include everything from aiki/default
  - ./company-policies.yml
```

### Use Case 4: Vendor-Specific Workflows

```yaml
# vendor/github/pr-checks.yml
name: "GitHub PR Checks"
PostResponse:
  - flow: aiki/quick-lint
  - flow: aiki/test-runner
  - shell: gh pr review --approve

# User includes vendor workflow
includes:
  - vendor/github/pr-checks
```

---

## Implementation Tasks

### Core Parser

- [ ] Add `includes:` directive to flow schema
- [ ] Parse `includes` list in `cli/src/flows/parser.rs`
- [ ] Add `flow:` action to flow DSL
- [ ] Parse `flow:` action in `cli/src/flows/parser.rs`

### Flow Loader

- [ ] Implement `cli/src/flows/loader.rs`
  - [ ] Load flow from path
  - [ ] Resolve flow paths (aiki/*, vendor/*, local)
  - [ ] Recursive loading for includes
  - [ ] Circular dependency detection
  - [ ] Flow caching (avoid reloading same flow)

### Flow Resolver

- [ ] Implement `cli/src/flows/resolver.rs`
  - [ ] Resolve `aiki/*` to `~/.aiki/flows/aiki/`
  - [ ] Resolve `vendor/*` to `~/.aiki/flows/vendor/`
  - [ ] Resolve relative paths
  - [ ] Resolve absolute paths
  - [ ] Error handling (flow not found)

### Flow Merger

- [ ] Implement flow merging logic
  - [ ] Merge included flow actions before parent
  - [ ] Preserve event order
  - [ ] Handle name conflicts
  - [ ] Merge metadata (name, version, etc.)

### Engine Integration

- [ ] Add `flow:` action executor to `cli/src/flows/engine.rs`
- [ ] Load and execute referenced flow
- [ ] Pass current event context to invoked flow
- [ ] Return control after flow completes

### Testing

- [ ] Unit tests: Flow path resolution
- [ ] Unit tests: Circular dependency detection
- [ ] Unit tests: Flow merging logic
- [ ] Integration tests: `includes:` directive
- [ ] Integration tests: `flow:` action
- [ ] Integration tests: Multi-level composition
- [ ] E2E tests: Real flows with includes

### Documentation

- [ ] Tutorial: "Composing Flows"
- [ ] Cookbook: Common patterns (reusable checks, vendor workflows)
- [ ] Reference: Flow composition syntax
- [ ] Examples: Real-world composed flows

---

## Success Criteria

✅ Can include flows via `includes:` directive  
✅ Can invoke flows via `flow:` action  
✅ Flow paths resolve correctly (aiki/*, vendor/*, local)  
✅ Circular dependencies are detected and rejected  
✅ Included flow actions execute in correct order  
✅ Flow caching prevents redundant loads  
✅ Clear error messages for missing flows  
✅ Multi-level composition works (flow includes flow includes flow)  

---

## Technical Design

### Flow Structure

```rust
pub struct Flow {
    pub name: String,
    pub version: String,
    pub includes: Vec<String>,           // Flow paths to include
    pub events: HashMap<EventType, Vec<Action>>,
}
```

### Flow Loader

```rust
pub struct FlowLoader {
    cache: HashMap<PathBuf, Flow>,       // Loaded flows cache
    call_stack: Vec<PathBuf>,            // For circular detection
}

impl FlowLoader {
    pub fn load(&mut self, path: &str) -> Result<Flow> {
        let resolved_path = FlowResolver::resolve(path)?;
        
        // Check circular dependency
        if self.call_stack.contains(&resolved_path) {
            return Err(AikiError::CircularDependency {
                path: path.to_string(),
                stack: self.call_stack.clone(),
            });
        }
        
        // Check cache
        if let Some(flow) = self.cache.get(&resolved_path) {
            return Ok(flow.clone());
        }
        
        // Load flow
        self.call_stack.push(resolved_path.clone());
        let flow = self.load_from_file(&resolved_path)?;
        self.call_stack.pop();
        
        // Cache and return
        self.cache.insert(resolved_path, flow.clone());
        Ok(flow)
    }
}
```

### Flow Resolver

```rust
pub struct FlowResolver;

impl FlowResolver {
    pub fn resolve(path: &str) -> Result<PathBuf> {
        if path.starts_with("aiki/") {
            // ~/.aiki/flows/aiki/quick-lint.yml
            Ok(home_dir()?.join(".aiki/flows").join(path).with_extension("yml"))
        } else if path.starts_with("vendor/") {
            // ~/.aiki/flows/vendor/eslint.yml
            Ok(home_dir()?.join(".aiki/flows").join(path).with_extension("yml"))
        } else if path.starts_with("./") {
            // Relative to current flow
            Ok(current_flow_dir()?.join(path))
        } else {
            // Absolute path
            Ok(PathBuf::from(path))
        }
    }
}
```

### Flow Merger

```rust
pub struct FlowMerger;

impl FlowMerger {
    pub fn merge(parent: &Flow, includes: Vec<Flow>) -> Flow {
        let mut merged = parent.clone();
        
        for included in includes {
            // Merge each event type
            for (event_type, actions) in included.events {
                merged.events
                    .entry(event_type)
                    .or_insert_with(Vec::new)
                    .splice(0..0, actions);  // Prepend included actions
            }
        }
        
        merged
    }
}
```

---

## Example Execution

Given these flows:

```yaml
# aiki/quick-lint.yml
PostResponse:
  - let: lint_errors = self.count_lint_errors
  - if: $lint_errors > 0
    then:
      autoreply: "Fix linting"

# my-workflow.yml
includes:
  - aiki/quick-lint

PostResponse:
  - shell: echo "Custom check"
```

**Execution order:**

1. Load `my-workflow.yml`
2. Load `aiki/quick-lint.yml` (from includes)
3. Merge flows:
   ```
   PostResponse:
     # From aiki/quick-lint (included)
     - let: lint_errors = self.count_lint_errors
     - if: $lint_errors > 0
       then:
         autoreply: "Fix linting"
     
     # From my-workflow.yml (parent)
     - shell: echo "Custom check"
   ```
4. Execute merged PostResponse

---

## Error Handling

### Flow Not Found

```
Error: Flow not found: 'aiki/missing-flow'

Searched locations:
  - ~/.aiki/flows/aiki/missing-flow.yml
  - ~/.aiki/flows/aiki/missing-flow.yaml

Available flows:
  - aiki/quick-lint
  - aiki/build-check
  - aiki/test-runner
```

### Circular Dependency

```
Error: Circular dependency detected

Flow include chain:
  my-workflow.yml
  → aiki/shared.yml
  → vendor/checks.yml
  → aiki/shared.yml  ← Circular!

Remove the circular include to fix this.
```

---

## Expected Timeline

**Week 2**

- Days 1-2: Parser, loader, resolver
- Days 3-4: Merger, engine integration
- Day 5: Testing and documentation

---

## Future Enhancements

### 1. Flow Parameters

Pass parameters to included flows:

```yaml
includes:
  - flow: aiki/lint-check
    params:
      max_warnings: 10
      auto_fix: true
```

### 2. Conditional Includes

Include flows based on conditions:

```yaml
includes:
  - if: $project.language == "typescript"
    then:
      - aiki/typescript-check
```

### 3. Flow Registry

Central registry of available flows:

```bash
aiki flows list
aiki flows search "lint"
aiki flows info aiki/quick-lint
```

---

## References

- [milestone-1.md](./milestone-1.md) - Milestone 1 overview
- [ROADMAP.md](../ROADMAP.md) - Strategic context
