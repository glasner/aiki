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

Flows are resolved using different path prefixes:

1. **Built-in flows:** `aiki/*` → Search project first, then user
   - Try `{project}/.aiki/flows/aiki/` first
   - If not found, try `~/.aiki/flows/aiki/`
2. **Vendor flows:** `vendor/*` → Search project first, then user
   - Try `{project}/.aiki/flows/vendor/` first
   - If not found, try `~/.aiki/flows/vendor/`
3. **Project root:** `@/` → `{project}/` (for docs, configs, or flows outside .aiki/flows/)
4. **Flow-relative:** `./` or `../` → Relative to current flow directory
5. **Absolute paths:** `/path/to/file` → As-is

**Why project-first for aiki/* and vendor/*?**
- Projects can override built-in flows (e.g., custom `aiki/quick-lint.yml`)
- Team-specific configurations in version control
- User flows provide defaults/fallbacks
- Standard precedence (like `.gitignore` - project overrides user)

**Examples:**
```yaml
before:
  - aiki/quick-lint               # Searches: 1) {project}/.aiki/flows/aiki/quick-lint.yml
                                  #           2) ~/.aiki/flows/aiki/quick-lint.yml
  - vendor/eslint                 # Searches: 1) {project}/.aiki/flows/vendor/eslint.yml
                                  #           2) ~/.aiki/flows/vendor/eslint.yml
  - ./helpers/lint.yml            # Flow-relative: {current_flow_dir}/helpers/lint.yml
  - /abs/path/checks.yml          # Absolute path

PrePrompt:
  prompt:
    prepend:
      - @/docs/architecture.md    # Project root: {project}/docs/architecture.md
      - @/README.md               # Project root: {project}/README.md
      - aiki/skills/rust          # Searches project, then user
```

**When to use which path type?**

| Path Type | Use Case | Example |
|-----------|----------|---------|
| `aiki/*` | Built-in Aiki flows | `aiki/quick-lint` |
| `vendor/*` | Third-party flows | `vendor/github/pr-checks` |
| `./` or `../` | Flows in subdirectories | `./helpers/lint.yml` |
| `@/` | Project files or flows outside .aiki/flows/ | `@/docs/arch.md` |

**Path resolution from nested flows:**
```yaml
# .aiki/flows/my-workflow.yml
before:
  - ./helpers/lint.yml            # Flow-relative: .aiki/flows/helpers/lint.yml
  - ./shared/base.yml             # Flow-relative: .aiki/flows/shared/base.yml

# .aiki/flows/helpers/lint.yml
before:
  - ../shared/base.yml            # Flow-relative: .aiki/flows/shared/base.yml
  - @/scripts/custom-lint.sh      # Project root: {project}/scripts/custom-lint.sh
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
- **Use canonical paths** to detect cycles regardless of path format
- **Canonicalization happens in FlowResolver** - all path variants (./, ../, @/, symlinks) resolve to same canonical path
- Error if any flow appears twice in the call stack
- Clear error message showing full cycle path

**Why canonical paths are critical:**
```yaml
# flow-a.yml in .aiki/flows/
before:
  - ./flow-b.yml          # Relative path

# flow-b.yml in .aiki/flows/
before:
  - @/.aiki/flows/flow-a.yml    # Project root path

# Without canonicalization: Looks like different files (different strings)
# With canonicalization: Both resolve to /project/.aiki/flows/flow-a.yml → Cycle detected!
```

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

### Use Case 4: Multiple Path Types

```yaml
# .aiki/flows/my-workflow.yml
PrePrompt:
  prompt:
    prepend:
      - @/docs/architecture.md        # Project root: {project}/docs/architecture.md
      - @/docs/coding-style.md        # Project root: {project}/docs/coding-style.md
      - aiki/skills/rust              # Built-in: ~/.aiki/flows/aiki/skills/rust.yml

before:
  - ./helpers/lint.yml                # Flow-relative: .aiki/flows/helpers/lint.yml
  - ./company/policy.yml              # Flow-relative: .aiki/flows/company/policy.yml

PostResponse:
  - shell: echo "Custom logic"

# .aiki/flows/helpers/lint.yml (referenced above)
before:
  - ../shared/base-checks.yml         # Parent dir: .aiki/flows/shared/base-checks.yml
  - ../shared/rust-checks.yml         # Parent dir: .aiki/flows/shared/rust-checks.yml
  - @/scripts/custom-lint.sh          # Project root: {project}/scripts/custom-lint.sh
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

### Use Case 7: Shared Event State Across Composed Flows

**Example: PrePrompt with shared prompt_assembler**

```yaml
# aiki/rust-skills.yml
PrePrompt:
  prompt:
    prepend: ~/.aiki/skills/rust.md

# my-workflow.yml
before:
  - aiki/rust-skills

PrePrompt:
  prompt:
    prepend: @/docs/architecture.md
    append: "Remember to run tests."
```

**Final prompt:**
```
[rust.md content]           ← from before flow (aiki/rust-skills)
[architecture.md content]   ← from main flow
[original user prompt]
Remember to run tests.      ← from main flow
```

**Key insight:** All flows share the same `prompt_assembler`, so content accumulates in execution order.

**Example: PrepareCommitMessage with shared body_assembler and trailers_assembler**

```yaml
# aiki/co-author.yml
PrepareCommitMessage:
  - commit_message:
      trailers:
        append: "Co-authored-by: AI Assistant <ai@example.com>"

# my-workflow.yml
before:
  - aiki/co-author

PrepareCommitMessage:
  - commit_message:
      body:
        append: "Implements authentication with JWT validation"
      trailers:
        append: "Ticket: AUTH-123"
```

**Final commit message:**
```
feat: add authentication

Implements authentication with JWT validation

Co-authored-by: AI Assistant <ai@example.com>
Ticket: AUTH-123
```

**Key insight:** 
- All flows share `body_assembler` and `trailers_assembler`
- Trailers accumulate in execution order: before flow's Co-authored-by appears first
- Body and trailers are separate assemblers, but both shared across flows

**See also:** [Use Case 5 in milestone-1.2](./milestone-1.2-post-response.md#use-case-5-composed-flows-with-shared-autoreply-accumulation) for how PostResponse `autoreply:` actions accumulate across composed flows without short-circuiting.

---

## Implementation Tasks

### Core Parser

- [ ] Add `before:` and `after:` directives to flow schema
- [ ] Parse `before` and `after` lists in `cli/src/flows/parser.rs`
- [ ] Add `flow:` action to flow DSL
- [ ] Parse `flow:` action in `cli/src/flows/parser.rs`

### Flow Loader

- [ ] Implement `cli/src/flows/loader.rs`
  - [ ] Use FlowResolver for path resolution
  - [ ] Load flow from path (parse YAML)
  - [ ] Flow caching (avoid reloading same flow)
  - [ ] Pass current_flow_dir to resolver for relative paths

### Path Resolver

- [ ] Implement `cli/src/flows/path_resolver.rs`
  - [ ] Implement `find_project_root()` - search upward for `.aiki/` directory
  - [ ] Cache project_root and home_dir in PathResolver struct
  - [ ] Resolve `@/` paths (project root)
  - [ ] Resolve `./` and `../` paths (relative to current directory)
  - [ ] Resolve absolute paths
  - [ ] Validate empty path after `@/`
  - [ ] Error handling (not in Aiki project, invalid prefix)

### Flow Resolver

- [ ] Implement `cli/src/flows/flow_resolver.rs`
  - [ ] Use PathResolver internally
  - [ ] Resolve `aiki/*` - try project first, then user (adds .yml)
  - [ ] Resolve `vendor/*` - try project first, then user (adds .yml)
  - [ ] Delegate `@/`, `./`, `../`, `/` to PathResolver
  - [ ] Error handling (flow not found)

### Flow Composer

- [ ] Implement `cli/src/flows/composer.rs`
  - [ ] Create FlowComposer with loader, executor, and call_stack
  - [ ] Implement `compose()` - orchestrate before/this flow/after
  - [ ] Implement `compose_action()` - dispatch to executor or self
  - [ ] Runtime cycle detection with call stack
  - [ ] Pass event context through all flows
  - [ ] Handle errors in before/after flows gracefully

### Executor Integration

- [ ] Update `cli/src/flows/executor.rs`
  - [ ] Add `Action::Flow { path }` variant to action enum
  - [ ] FlowExecutor remains unchanged (no flow: handling)
  - [ ] FlowComposer intercepts flow: actions before delegating to executor

### Testing

- [ ] Unit tests: Flow path resolution
  - [ ] Test `aiki/*` resolution (project first, then user)
  - [ ] Test `vendor/*` resolution (project first, then user)
  - [ ] Test project override of built-in flows
  - [ ] Test `@/` resolution (project root)
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

### Event Trait

```rust
/// Trait for all event types that can trigger flows
/// 
/// This allows FlowComposer to work with any event type polymorphically
pub trait Event {
    /// Get the type of this event (PrePrompt, PostResponse, etc.)
    fn event_type(&self) -> EventType;
}

/// Event type enum for routing to correct handler
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    PrePrompt,
    PostResponse,
    PrepareCommitMessage,
    PostToolUse,
    // Future events...
}

// Example implementations
impl Event for PrePromptEvent {
    fn event_type(&self) -> EventType {
        EventType::PrePrompt
    }
}

impl Event for PostResponseEvent {
    fn event_type(&self) -> EventType {
        EventType::PostResponse
    }
}
```

**Why this trait:**
- **Polymorphism**: FlowComposer accepts `&mut dyn Event` instead of concrete types
- **Extensibility**: Add new events without changing FlowComposer
- **Type-safe routing**: Flow system uses `event.event_type()` to find correct actions in Flow struct

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
    resolver: FlowResolver,              // Path resolution with cached project root (returns canonical paths)
    cache: HashMap<PathBuf, Flow>,       // Loaded flows cache (keyed by canonical path)
}

impl FlowLoader {
    pub fn new() -> Result<Self> {
        Ok(Self {
            resolver: FlowResolver::new()?,  // Discovers project root once
            cache: HashMap::new(),
        })
    }
    
    /// Load a flow and return both the flow and its canonical path
    /// 
    /// The canonical path is used by FlowComposer for cycle detection.
    /// Caching is done by canonical path to avoid loading the same file multiple times.
    pub fn load(&mut self, path: &str, current_flow_dir: &Path) -> Result<(Flow, PathBuf)> {
        // Resolve to canonical path (handles ./, ../, @/, symlinks)
        let canonical_path = self.resolver.resolve(path, current_flow_dir)?;
        
        // Check cache (by canonical path)
        if let Some(flow) = self.cache.get(&canonical_path) {
            return Ok((flow.clone(), canonical_path));
        }
        
        // Load and parse flow file
        let flow = self.load_from_file(&canonical_path)?;
        
        // Cache by canonical path and return both flow and path
        self.cache.insert(canonical_path.clone(), flow.clone());
        Ok((flow, canonical_path))
    }
    
    fn load_from_file(&self, path: &Path) -> Result<Flow> {
        let contents = std::fs::read_to_string(path)?;
        let flow: Flow = serde_yaml::from_str(&contents)?;
        Ok(flow)
    }
}
```

### Path Resolver

```rust
/// Low-level path resolver for all file types (flows, docs, scripts, etc.)
pub struct PathResolver {
    project_root: PathBuf,  // Discovered once, cached
    home_dir: PathBuf,      // Cached for performance
}

impl PathResolver {
    /// Create a new PathResolver by discovering project root
    pub fn new() -> Result<Self> {
        Ok(Self {
            project_root: Self::find_project_root()?,
            home_dir: home_dir()?,
        })
    }
    
    /// Find project root by searching upward for .aiki/ directory
    fn find_project_root() -> Result<PathBuf> {
        let mut current = env::current_dir()?;
        
        loop {
            if current.join(".aiki").is_dir() {
                return Ok(current);
            }
            
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => {
                    return Err(AikiError::NotInAikiProject {
                        searched_from: env::current_dir()?,
                    });
                }
            }
        }
    }
    
    /// Resolve a generic path (does NOT add .yml extension or search flow directories)
    /// Used for docs, configs, scripts, or any non-flow file
    pub fn resolve(&self, path: &str, current_dir: &Path) -> Result<PathBuf> {
        if path.is_empty() {
            return Err(AikiError::InvalidPath {
                path: path.to_string(),
                reason: "Path cannot be empty".to_string(),
            });
        }
        
        let resolved = if let Some(rest) = path.strip_prefix("@/") {
            // Project root
            if rest.is_empty() {
                return Err(AikiError::InvalidPath {
                    path: path.to_string(),
                    reason: "Path after @/ cannot be empty".to_string(),
                });
            }
            self.project_root.join(rest)
        } else if path.starts_with("./") || path.starts_with("../") {
            // Relative to current directory
            current_dir.join(path)
        } else if path.starts_with('/') {
            // Absolute path
            PathBuf::from(path)
        } else {
            return Err(AikiError::InvalidPath {
                path: path.to_string(),
                reason: "Path must start with @/, ./, ../, or /".to_string(),
            });
        };
        
        Ok(resolved)
    }
}
```

### Flow Resolver

```rust
/// High-level flow resolver (uses PathResolver + flow-specific logic)
pub struct FlowResolver {
    path_resolver: PathResolver,
}

impl FlowResolver {
    pub fn new() -> Result<Self> {
        Ok(Self {
            path_resolver: PathResolver::new()?,
        })
    }
    
    /// Resolve a flow path to an absolute, canonical PathBuf
    /// Adds .yml extension and searches aiki/vendor directories
    /// 
    /// **IMPORTANT**: Returns canonicalized path for reliable cycle detection.
    /// All path variants (./, ../, @/, symlinks) are resolved to the same canonical path.
    /// 
    /// # Arguments
    /// * `path` - The path to resolve (e.g., "aiki/quick-lint", "@/docs/arch.md", "./helpers/lint.yml")
    /// * `current_flow_dir` - Directory containing the current flow file (for ./ paths)
    pub fn resolve(
        &self,
        path: &str,
        current_flow_dir: &Path,
    ) -> Result<PathBuf> {
        if path.is_empty() {
            return Err(AikiError::InvalidFlowPath {
                path: path.to_string(),
                reason: "Path cannot be empty".to_string(),
            });
        }
        
        let resolved = if let Some(rest) = path.strip_prefix("aiki/") {
            // Built-in flows: try project first, then user
            let project_path = self.path_resolver.project_root
                .join(".aiki/flows/aiki")
                .join(rest)
                .with_extension("yml");
            
            if project_path.exists() {
                project_path
            } else {
                self.path_resolver.home_dir
                    .join(".aiki/flows/aiki")
                    .join(rest)
                    .with_extension("yml")
            }
        } else if let Some(rest) = path.strip_prefix("vendor/") {
            // Vendor flows: try project first, then user
            let project_path = self.path_resolver.project_root
                .join(".aiki/flows/vendor")
                .join(rest)
                .with_extension("yml");
            
            if project_path.exists() {
                project_path
            } else {
                self.path_resolver.home_dir
                    .join(".aiki/flows/vendor")
                    .join(rest)
                    .with_extension("yml")
            }
        } else {
            // For generic paths (@/, ./, ../, /), delegate to PathResolver
            self.path_resolver.resolve(path, current_flow_dir)?
        };
        
        // CRITICAL: Canonicalize path for reliable cycle detection
        // This ensures ./foo.yml, ../flows/foo.yml, and @/flows/foo.yml
        // all resolve to the same canonical path if they reference the same file
        resolved.canonicalize().map_err(|e| AikiError::FlowNotFound {
            path: path.to_string(),
            resolved_path: resolved.display().to_string(),
            source: e,
        })
    }
}
```

**Examples of resolution:**
```rust
// Create resolvers (discover project root automatically)
let path_resolver = PathResolver::new()?;
let flow_resolver = FlowResolver::new()?;

// Built-in flow (searches project first, then user) - FlowResolver
flow_resolver.resolve(
    "aiki/quick-lint",
    Path::new(".aiki/flows"),
) // → Checks: 1) {project}/.aiki/flows/aiki/quick-lint.yml
  //          2) ~/.aiki/flows/aiki/quick-lint.yml

// Project root (docs) - PathResolver for non-flow files
path_resolver.resolve(
    "@/docs/architecture.md",
    Path::new(".aiki/flows"),
) // → /project/docs/architecture.md

// Project root (script) - PathResolver
path_resolver.resolve(
    "@/scripts/lint.sh",
    Path::new(".aiki/flows"),
) // → /project/scripts/lint.sh

// Flow-relative - FlowResolver (adds .yml)
flow_resolver.resolve(
    "./helpers/lint.yml",
    Path::new("/project/.aiki/flows"),
) // → /project/.aiki/flows/helpers/lint.yml

// Flow-relative with parent directory - FlowResolver
flow_resolver.resolve(
    "../shared/base.yml",
    Path::new("/project/.aiki/flows/helpers"),
) // → /project/.aiki/flows/shared/base.yml
```

### Flow Composer

FlowComposer orchestrates flow composition (before/after, cycle detection) and delegates
action execution to FlowExecutor.

```rust
/// Orchestrates flow composition and delegates action execution to FlowExecutor
pub struct FlowComposer<'a> {
    loader: &'a mut FlowLoader,
    executor: &'a mut FlowExecutor,    // Executes individual actions
    call_stack: Vec<PathBuf>,          // Runtime call stack for cycle detection
}

impl<'a> FlowComposer<'a> {
    pub fn new(loader: &'a mut FlowLoader, executor: &'a mut FlowExecutor) -> Self {
        Self {
            loader,
            executor,
            call_stack: Vec::new(),
        }
    }
    
    /// Compose and execute a flow atomically (before → this flow → after)
    /// 
    /// This is the orchestration layer that handles:
    /// - Flow composition (before/after)
    /// - Cycle detection
    /// - Recursive flow invocation
    pub fn compose_flow(&mut self, flow_path: &str, event: &mut dyn Event) -> Result<()> {
        // Load the flow (FlowLoader uses FlowResolver which returns canonical paths)
        let (flow, canonical_path) = self.loader.load(flow_path, current_flow_dir)?;
        
        // Check for circular dependency (including self-invocation)
        // canonical_path is already canonicalized by FlowResolver, so this comparison is reliable
        if self.call_stack.contains(&canonical_path) {
            return Err(AikiError::CircularFlowDependency {
                path: flow_path.to_string(),
                canonical_path: canonical_path.display().to_string(),
                stack: self.call_stack.iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            });
        }
        
        // Push canonical path onto call stack for cycle detection
        self.call_stack.push(canonical_path.clone());
        
        // 1. Execute before flows (each atomically)
        for before_path in &flow.before {
            self.compose_flow(before_path, event)?;  // Recursive, atomic
        }
        
        // 2. Execute this flow's actions (if any for this event)
        if let Some(actions) = flow.events.get(&event.event_type()) {
            for action in actions {
                self.compose_action(action, event)?;
            }
        }
        
        // 3. Execute after flows (each atomically)
        for after_path in &flow.after {
            self.compose_flow(after_path, event)?;  // Recursive, atomic
        }
        
        // Pop from call stack
        self.call_stack.pop();
        Ok(())
    }
    
    /// Compose a single action (delegates to executor or recurses for flow: actions)
    fn compose_action(&mut self, action: &Action, event: &mut dyn Event) -> Result<()> {
        match action {
            Action::Flow { path } => {
                // Inline flow invocation - delegate to compose_flow() for composition
                self.compose_flow(path, event)?;
            }
            _ => {
                // All other actions - delegate to FlowExecutor
                self.executor.execute_action(action, event)?;
            }
        }
        Ok(())
    }
}
```

### Relationship with FlowExecutor

**FlowExecutor** (already exists from Phase 5):
- Executes lists of actions: `shell`, `let`, `if`, `autoreply`, etc.
- Handles failure modes (`continue`, `stop`, `block`)
- Stores action results in context (via `alias`)
- Returns `FlowResult` with timing

**FlowComposer** (new for Milestone 1.3):
- Orchestrates flow composition: `before:`, `after:`, `flow:` action
- Manages call stack for cycle detection
- Provides variable isolation (each flow gets fresh variable context)
- Shares event state across all flows (e.g., PrePromptEvent's MessageAssembler)
- Splits action lists around `flow:` actions
- Delegates action chunks to FlowExecutor

**Key insight on isolation:**
- **Variables are isolated** - Each flow gets fresh variable context
- **Event state is shared** - All flows modify the same event object
  - Example: PrePromptEvent's MessageAssembler accumulates chunks from all flows
  - Example: PostResponseEvent's response builder accumulates from all flows
  - This allows composed flows to contribute to the same output

**Architecture:**
```
User triggers event (e.g., PostResponse)
    ↓
FlowComposer.compose_flow("my-workflow.yml", &mut event)
    ↓
    Loads flow via FlowLoader
    Checks call stack for cycles
    ↓
    Executes before flows (each gets fresh variable context, shares event state)
    ↓
    For actions in this flow (fresh variable context, shares event state):
        - Accumulate non-flow actions in a chunk
        - When flow: action encountered:
            1. Execute chunk via FlowExecutor
            2. Execute flow: recursively via FlowComposer.compose_flow()
        - Continue accumulating
        - Execute remaining chunk via FlowExecutor
    ↓
    Executes after flows (each gets fresh variable context, shares event state)
    ↓
    Returns ActionResult (like shell/jj actions)
```

**Key insights:** 
- FlowComposer handles *orchestration* (what flows run when, isolation)
- FlowExecutor handles *execution* (action chunks with shared context)
- FlowExecutor already has the loop and failure handling
- FlowComposer splits around `flow:` actions for chunking
- Event object (&mut event) passed through entire composition tree
- Each flow gets fresh variables, but all modify the same event state

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

**Example 2: Project root file not found**
```
Error: Flow not found: '@/docs/architecture.md'

Searched location:
  - /Users/you/project/docs/architecture.md

Path type: project root (@/ means project root directory)
```

**Example 4: Flow-relative path not found**
```
Error: Flow not found: './helpers/lint.yml'

Searched location:
  - /Users/you/project/.aiki/flows/helpers/lint.yml

Path type: flow-relative (./ means relative to current flow directory)
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

Pass parameters to composed flows:

```yaml
before:
  - flow: aiki/lint-check
    with:
      max_warnings: 10
      auto_fix: true
```

### 2. Conditional Composition

Conditionally compose flows based on runtime conditions:

```yaml
before:
  - if: $project.language == "typescript"
    then:
      - aiki/typescript-check
  - if: $project.language == "rust"
    then:
      - aiki/rust-check
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
