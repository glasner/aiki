# Milestone 1.3: Flow Composition

This document outlines the implementation plan for the Flow Composition system (Milestone 1.3).

See [milestone-1.md](./milestone-1.md) for the full Milestone 1 overview.

---

## Overview

Flow Composition allows flows to include and reuse other flows, enabling modular, composable workflow design.

**Key Capabilities:**
- Include flows via `before:` (run before this flow) and `after:` (run after this flow) directives
- Invoke flows inline with `flow:` action (run at specific point in action list)
- Flow resolution (aiki/*, vendor/*, local paths)
- Circular dependency detection
- Atomic flow execution (each flow runs its own before/after internally)

---

## Core Features

### 1. Before/After Directives

Include flows that run before or after this flow:

```yaml
name: "My Workflow"
version: "1.0"

before:
  - aiki/quick-lint       # Runs before this flow's actions
  - aiki/security-scan

after:
  - aiki/cleanup          # Runs after this flow's actions
  - aiki/metrics

PostResponse:
  - shell: echo "My custom logic"
```

**Execution order:**
1. Flows in `before:` list (in order)
2. This flow's actions
3. Flows in `after:` list (in order)

**How it works:**
- Each included flow is executed atomically (runs its own before/after internally)
- Before flows run first, in the order listed
- After flows run last, in the order listed
- All flows share the same event context (e.g., PostResponse event)

### 2. Flow Action (Inline Invocation)

Invoke flows inline at a specific point in the action list:

```yaml
PostResponse:
  - let: error_count = self.count_errors
  
  - if: $error_count > 0
    then:
      - flow: aiki/detailed-lint  # Runs NOW (not in before/after)
  
  - shell: echo "All checks passed"
```

**Key behavior:**
- `flow:` action executes **the same event** from the referenced flow
- Example: `flow: aiki/quick-lint` inside `PostResponse` runs `quick-lint`'s `PostResponse` actions
- The invoked flow is atomic (runs its own before/after if it has them)

**Difference from before/after:**
- `before:` / `after:` - Always execute, fixed position
- `flow:` action - Executes at that point in the action list, can be conditional

### 3. Flow Resolution

Flows are resolved using different path prefixes, similar to TypeScript/Vite path mapping:

1. **Built-in flows:** `aiki/*` → `~/.aiki/flows/aiki/`
2. **Vendor flows:** `vendor/*` → `~/.aiki/flows/vendor/`
3. **Project-relative:** `@/` → Relative to project root (where `.aiki/` is)
4. **Flow-relative:** `./` → Relative to directory containing current flow file
5. **Absolute paths:** `/path/to/file` → As-is

**Examples:**
```yaml
before:
  - aiki/quick-lint               # Built-in: ~/.aiki/flows/aiki/quick-lint.yml
  - vendor/eslint                 # Vendor: ~/.aiki/flows/vendor/eslint.yml
  - @/.aiki/flows/custom.yml      # Project root: {project}/.aiki/flows/custom.yml
  - ./helpers/lint.yml            # Flow-relative: {current_flow_dir}/helpers/lint.yml
  - /abs/path/checks.yml          # Absolute path

PrePrompt:
  prompt:
    prepend:
      - @/docs/architecture.md    # Project root: {project}/docs/architecture.md
      - aiki/skills/rust          # Built-in: ~/.aiki/flows/aiki/skills/rust.yml
```

**Why two path types?**
- `@/` - When you want to reference project files (docs, configs) or flows from project root
- `./` - When you want to organize flows into subdirectories relative to each other

**Path resolution from nested flows:**
```yaml
# .aiki/flows/my-workflow.yml
before:
  - ./helpers/lint.yml            # Resolves to: .aiki/flows/helpers/lint.yml

# .aiki/flows/helpers/lint.yml
before:
  - ../shared/base.yml            # Resolves to: .aiki/flows/shared/base.yml
  - @/.aiki/flows/custom.yml      # Always resolves to: {project}/.aiki/flows/custom.yml
```

### 4. Circular Dependency Detection

Prevent infinite loops by tracking all flow invocations at runtime:

**Static cycles (before/after):**
```yaml
# flow-a.yml
before:
  - ./flow-b.yml

# flow-b.yml
before:
  - ./flow-a.yml  # ERROR: Circular dependency detected
```

**Runtime cycles (flow: action):**
```yaml
# flow-a.yml
PostResponse:
  - flow: ./flow-b.yml

# flow-b.yml
PostResponse:
  - flow: ./flow-a.yml  # ERROR: Circular dependency detected
```

**Self-invocation (not allowed):**
```yaml
# my-workflow.yml
PostResponse:
  - if: $counter < 10
    then:
      flow: ./my-workflow.yml  # ERROR: Circular dependency (self-invocation)
```

**Detection mechanism:**
- Track flow call stack during execution (runtime checking)
- Use canonical paths to detect cycles regardless of path format
- Error if any flow appears twice in the call stack
- Clear error message showing full cycle path

### 5. Atomic Flow Execution

Each flow is self-contained and runs its own before/after flows internally:

```yaml
# aiki/quick-lint.yml
before:
  - aiki/base-checks

PostResponse:
  - let: errors = self.lint()
  - if: $errors > 0
    then:
      autoreply: "Fix lint errors"
```

```yaml
# my-workflow.yml
before:
  - aiki/quick-lint      # Runs quick-lint atomically

PostResponse:
  - shell: echo "My logic"
```

**Execution order:**
1. `aiki/base-checks` (quick-lint's before)
2. `aiki/quick-lint` actions
3. `my-workflow` actions

The user doesn't need to know about `aiki/base-checks`—it's an implementation detail of `quick-lint`.

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
before:
  - aiki/quick-lint  # Runs before user's actions

PostResponse:
  - shell: echo "My custom validation"
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

### Use Case 3: Before and After Flows

```yaml
# User's workflow with cleanup
before:
  - aiki/quick-lint
  - aiki/security-scan

after:
  - aiki/cleanup          # Clean up temp files
  - aiki/metrics          # Report metrics

PostResponse:
  - shell: echo "Main logic here"
```

### Use Case 4: Project-Relative and Flow-Relative Paths

```yaml
# .aiki/flows/my-workflow.yml
PrePrompt:
  prompt:
    prepend:
      - @/docs/architecture.md      # Project root: {project}/docs/architecture.md
      - @/docs/coding-style.md      # Project root: {project}/docs/coding-style.md
      - aiki/skills/rust            # Built-in: ~/.aiki/flows/aiki/skills/rust.yml

before:
  - ./helpers/lint.yml              # Flow-relative: .aiki/flows/helpers/lint.yml
  - @/.aiki/flows/company/policy.yml # Project root: {project}/.aiki/flows/company/policy.yml

PostResponse:
  - shell: echo "Custom logic"

# .aiki/flows/helpers/lint.yml (referenced above)
before:
  - ../shared/base-checks.yml       # Parent dir: .aiki/flows/shared/base-checks.yml
  - @/scripts/custom-lint.sh        # Project root: {project}/scripts/custom-lint.sh
```

### Use Case 5: Multi-Layer Composition

```yaml
# aiki/default.yml
before:
  - aiki/quick-lint
  - aiki/build-check
  - aiki/test-runner

PostResponse:
  - shell: echo "Default checks complete"

# User's custom-workflow.yml
before:
  - aiki/default           # Runs default's before flows + actions
  - ./company-policies.yml

PostResponse:
  - shell: echo "Custom logic"
```

**Execution order:**
1. `aiki/quick-lint` (from aiki/default's before)
2. `aiki/build-check` (from aiki/default's before)
3. `aiki/test-runner` (from aiki/default's before)
4. `aiki/default` PostResponse actions
5. `./company-policies.yml` (atomic execution)
6. User's PostResponse actions

### Use Case 6: Vendor-Specific Workflows

```yaml
# vendor/github/pr-checks.yml
name: "GitHub PR Checks"

before:
  - aiki/quick-lint
  - aiki/test-runner

PostResponse:
  - shell: gh pr review --approve

# User includes vendor workflow
before:
  - vendor/github/pr-checks
```

---

## Implementation Tasks

### Core Parser

- [ ] Add `before:` and `after:` directives to flow schema
- [ ] Parse `before` and `after` lists in `cli/src/flows/parser.rs`
- [ ] Add `flow:` action to flow DSL
- [ ] Parse `flow:` action in `cli/src/flows/parser.rs`

### Flow Loader

- [ ] Implement `cli/src/flows/loader.rs`
  - [ ] Load flow from path
  - [ ] Resolve flow paths (aiki/*, vendor/*, local)
  - [ ] Recursive loading for before/after flows
  - [ ] Circular dependency detection
  - [ ] Flow caching (avoid reloading same flow)
  - [ ] Atomic flow execution (each flow runs its own before/after)

### Flow Resolver

- [ ] Implement `cli/src/flows/resolver.rs`
  - [ ] Resolve `aiki/*` to `~/.aiki/flows/aiki/`
  - [ ] Resolve `vendor/*` to `~/.aiki/flows/vendor/`
  - [ ] Resolve `@/` paths (project-relative)
  - [ ] Resolve `./` and `../` paths (flow-relative)
  - [ ] Resolve absolute paths
  - [ ] Track current_flow_dir for flow-relative resolution
  - [ ] Track project_root for project-relative resolution
  - [ ] Error handling (flow not found)

### Flow Executor

- [ ] Implement flow execution logic
  - [ ] Execute before flows in order (each atomically)
  - [ ] Execute this flow's actions
  - [ ] Execute after flows in order (each atomically)
  - [ ] Pass event context through all flows
  - [ ] Handle errors in before/after flows gracefully

### Engine Integration

- [ ] Add `flow:` action executor to `cli/src/flows/engine.rs`
- [ ] Load and execute referenced flow
- [ ] Pass current event context to invoked flow
- [ ] Return control after flow completes

### Testing

- [ ] Unit tests: Flow path resolution
  - [ ] Test `aiki/*` resolution
  - [ ] Test `vendor/*` resolution
  - [ ] Test `@/` project-relative resolution
  - [ ] Test `./` flow-relative resolution
  - [ ] Test `../` parent directory resolution
  - [ ] Test absolute path resolution
- [ ] Unit tests: Circular dependency detection
  - [ ] Test static cycles (before/after)
  - [ ] Test runtime cycles (flow: action)
  - [ ] Test self-invocation
- [ ] Unit tests: Atomic flow execution
- [ ] Unit tests: Before/after execution order
- [ ] Integration tests: `before:` and `after:` directives
- [ ] Integration tests: `flow:` action (inline invocation)
- [ ] Integration tests: Multi-level composition (nested before/after)
- [ ] Integration tests: Same event execution (PostResponse → PostResponse)
- [ ] Integration tests: Mix of path types (aiki/*, @/, ./)
- [ ] E2E tests: Real flows with before/after

### Documentation

- [ ] Tutorial: "Composing Flows"
- [ ] Cookbook: Common patterns (reusable checks, vendor workflows)
- [ ] Reference: Flow composition syntax
- [ ] Examples: Real-world composed flows

---

## Success Criteria

✅ Can include flows via `before:` and `after:` directives  
✅ Can invoke flows inline via `flow:` action  
✅ Flow paths resolve correctly (aiki/*, vendor/*, local)  
✅ Circular dependencies are detected at runtime (before/after + flow: action)  
✅ Self-invocation is detected and rejected  
✅ Before flows execute before this flow's actions  
✅ After flows execute after this flow's actions  
✅ Flow: action executes at the correct point in action list  
✅ Each flow executes atomically (runs its own before/after)  
✅ Flow caching prevents redundant loads  
✅ Clear error messages for missing flows and cycles  
✅ Multi-level composition works (nested before/after)  

---

## Technical Design

### Flow Structure

```rust
pub struct Flow {
    pub name: String,
    pub version: String,
    pub before: Vec<String>,             // Flows to run before this flow
    pub after: Vec<String>,              // Flows to run after this flow
    pub events: HashMap<EventType, Vec<Action>>,
}
```

### Flow Loader

```rust
pub struct FlowLoader {
    cache: HashMap<PathBuf, Flow>,       // Loaded flows cache
}

impl FlowLoader {
    pub fn load(&mut self, path: &str) -> Result<Flow> {
        let resolved_path = FlowResolver::resolve(path)?;
        
        // Check cache
        if let Some(flow) = self.cache.get(&resolved_path) {
            return Ok(flow.clone());
        }
        
        // Load and parse flow file
        let flow = self.load_from_file(&resolved_path)?;
        
        // Cache and return
        self.cache.insert(resolved_path, flow.clone());
        Ok(flow)
    }
    
    fn load_from_file(&self, path: &Path) -> Result<Flow> {
        let contents = std::fs::read_to_string(path)?;
        let flow: Flow = serde_yaml::from_str(&contents)?;
        Ok(flow)
    }
}
```

### Flow Resolver

```rust
pub struct FlowResolver;

impl FlowResolver {
    /// Resolve a flow path to an absolute PathBuf
    /// 
    /// # Arguments
    /// * `path` - The path to resolve (e.g., "aiki/quick-lint", "@/docs/arch.md", "./helpers/lint.yml")
    /// * `current_flow_dir` - Directory containing the current flow file (for ./ paths)
    /// * `project_root` - Project root directory where .aiki/ is located (for @/ paths)
    pub fn resolve(
        path: &str,
        current_flow_dir: &Path,
        project_root: &Path,
    ) -> Result<PathBuf> {
        if path.starts_with("aiki/") {
            // Built-in flows: aiki/* → ~/.aiki/flows/aiki/
            Ok(home_dir()?.join(".aiki/flows").join(path).with_extension("yml"))
        } else if path.starts_with("vendor/") {
            // Vendor flows: vendor/* → ~/.aiki/flows/vendor/
            Ok(home_dir()?.join(".aiki/flows").join(path).with_extension("yml"))
        } else if path.starts_with("@/") {
            // Project-relative: @/ → {project_root}/
            Ok(project_root.join(&path[2..]))  // Strip @/ prefix
        } else if path.starts_with("./") || path.starts_with("../") {
            // Flow-relative: ./ or ../ → relative to current flow directory
            Ok(current_flow_dir.join(path))
        } else {
            // Absolute path
            Ok(PathBuf::from(path))
        }
    }
}
```

**Examples of resolution:**
```rust
// Built-in flow
FlowResolver::resolve(
    "aiki/quick-lint",
    Path::new(".aiki/flows"),
    Path::new("/project"),
) // → ~/.aiki/flows/aiki/quick-lint.yml

// Project-relative
FlowResolver::resolve(
    "@/docs/architecture.md",
    Path::new(".aiki/flows"),
    Path::new("/project"),
) // → /project/docs/architecture.md

// Flow-relative
FlowResolver::resolve(
    "./helpers/lint.yml",
    Path::new("/project/.aiki/flows"),
    Path::new("/project"),
) // → /project/.aiki/flows/helpers/lint.yml

// Flow-relative with parent directory
FlowResolver::resolve(
    "../shared/base.yml",
    Path::new("/project/.aiki/flows/helpers"),
    Path::new("/project"),
) // → /project/.aiki/flows/shared/base.yml
```

### Flow Executor

```rust
pub struct FlowExecutor<'a> {
    loader: &'a mut FlowLoader,
    call_stack: Vec<PathBuf>,  // Runtime call stack for cycle detection
}

impl<'a> FlowExecutor<'a> {
    /// Execute a flow atomically (before → this flow → after)
    pub fn execute(&mut self, flow_path: &str, event: &mut dyn Event) -> Result<()> {
        // Load the flow
        let flow = self.loader.load(flow_path)?;
        let canonical_path = PathBuf::from(flow_path).canonicalize()?;
        
        // Check for circular dependency (including self-invocation)
        if self.call_stack.contains(&canonical_path) {
            return Err(AikiError::CircularDependency {
                path: flow_path.to_string(),
                stack: self.call_stack.iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            });
        }
        
        // Push onto call stack
        self.call_stack.push(canonical_path.clone());
        
        // 1. Execute before flows (each atomically)
        for before_path in &flow.before {
            self.execute(before_path, event)?;  // Recursive, atomic
        }
        
        // 2. Execute this flow's actions
        let actions = flow.events.get(&event.event_type())
            .ok_or(AikiError::NoActionsForEvent)?;
        
        for action in actions {
            self.execute_action(action, event)?;
        }
        
        // 3. Execute after flows (each atomically)
        for after_path in &flow.after {
            self.execute(after_path, event)?;  // Recursive, atomic
        }
        
        // Pop from call stack
        self.call_stack.pop();
        Ok(())
    }
    
    fn execute_action(&mut self, action: &Action, event: &mut dyn Event) -> Result<()> {
        match action {
            Action::Flow { path } => {
                // Inline flow invocation (checked by call stack)
                self.execute(path, event)?;  // Execute atomically
            }
            // ... other action types ...
        }
        Ok(())
    }
}
```

---

## Example Execution

Given these flows:

```yaml
# aiki/base-checks.yml
PostResponse:
  - shell: echo "Running base checks"

# aiki/quick-lint.yml
before:
  - aiki/base-checks

PostResponse:
  - let: lint_errors = self.count_lint_errors
  - if: $lint_errors > 0
    then:
      autoreply: "Fix linting"

# my-workflow.yml
before:
  - aiki/quick-lint

after:
  - aiki/cleanup

PostResponse:
  - shell: echo "Custom check"
```

**Execution order:**

1. Execute `my-workflow`'s before flows:
   - Execute `aiki/quick-lint` atomically:
     - Execute `aiki/base-checks` (quick-lint's before)
     - Execute `aiki/quick-lint` PostResponse actions
2. Execute `my-workflow`'s PostResponse actions
3. Execute `my-workflow`'s after flows:
   - Execute `aiki/cleanup` atomically

**Output:**
```
Running base checks          ← aiki/base-checks
[lint check runs]            ← aiki/quick-lint
Custom check                 ← my-workflow
[cleanup runs]               ← aiki/cleanup
```

**Key insights:**
- User doesn't need to know `aiki/quick-lint` depends on `aiki/base-checks`
- Each flow is self-contained and atomic
- Event context (PostResponse) is shared across all flows

---

## Error Handling

### Flow Not Found

**Example 1: Built-in flow not found**
```
Error: Flow not found: 'aiki/missing-flow'

Searched locations:
  - ~/.aiki/flows/aiki/missing-flow.yml
  - ~/.aiki/flows/aiki/missing-flow.yaml

Available aiki/* flows:
  - aiki/quick-lint
  - aiki/build-check
  - aiki/test-runner
```

**Example 2: Project-relative path not found**
```
Error: Flow not found: '@/.aiki/flows/custom.yml'

Searched location:
  - /Users/you/project/.aiki/flows/custom.yml

Path type: project-relative (@ means project root)
```

**Example 3: Flow-relative path not found**
```
Error: Flow not found: './helpers/lint.yml'

Searched location:
  - /Users/you/project/.aiki/flows/helpers/lint.yml

Path type: flow-relative (. means relative to current flow directory)
Current flow: .aiki/flows/my-workflow.yml
```

### Circular Dependency

```
Error: Circular dependency detected

Flow execution chain:
  my-workflow.yml
  → aiki/shared.yml (before)
  → vendor/checks.yml (before)
  → aiki/shared.yml  ← Circular!

Remove the circular dependency to fix this.
```

### Before/After Flow Execution Errors

If a before flow fails:
```
Error: Before flow failed: aiki/quick-lint

  Caused by: Lint errors detected

Aborting execution of my-workflow.yml
```

If an after flow fails:
```
Warning: After flow failed: aiki/cleanup

  Caused by: Failed to remove temp files

Main workflow completed successfully, but cleanup failed.
```

**Error handling strategy:**
- Before flow errors → abort entire workflow (fail fast)
- After flow errors → log warning but don't fail workflow (best effort cleanup)

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
