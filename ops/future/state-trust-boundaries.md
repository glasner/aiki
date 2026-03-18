# User-Controlled Trust Boundaries

**Status:** Future work. Out of scope for v1.

## Use Case

Consultants, contractors, or users with mixed personal/work repositories may want to isolate aiki state by project or client.

## Example Scenarios

- Contractor working on ClientA and ClientB projects - don't want conversation history mixed
- Enterprise requirement to keep work sessions separate from personal projects
- Developer with multiple identities (personal GitHub, work GitLab, client Bitbucket)

## Design Considerations

### The Problem with `.aiki-boundary` Marker Files

The original idea of `.aiki-boundary` marker files has a conflict: per-repo `.aiki/` folders already exist for project-level state (flows, task templates, tasks JJ). Using `.aiki/` for both per-repo state AND boundary-level state creates ambiguity, especially for the `.jj/` subdirectory which would serve different purposes.

### Alternative Approaches to Explore

#### 1. Workspaces Configuration
**Prereq:** [user-settings](user-settings.md) — provides `~/.aiki/config.yaml` infrastructure.

Centralized config in `~/.aiki/config.yaml` defining workspace boundaries by path patterns (similar to Git's `includeIf`):

```yaml
workspaces:
  - name: personal
    paths: ["/Users/me/personal/**"]
    home: ~/.aiki/workspaces/personal

  - name: client-a
    paths: ["/Users/me/work/client-a/**"]
    home: ~/.aiki/workspaces/client-a

  - name: client-b
    paths: ["/Users/me/work/client-b/**"]
    home: ~/.aiki/workspaces/client-b
```

**Pros:**
- Clear, centralized configuration
- Easy to manage and review boundaries
- Can support overlapping paths with priority rules

**Cons:**
- Requires manual configuration
- Not portable across machines without config sync

#### 2. Separate Folder Names
- `.aiki/` for project state (flows, tasks, per-repo JJ)
- `.aiki-home/` for boundary-level aiki home (sessions, conversations)

**Pros:**
- Clear separation of concerns
- No ambiguity about folder purpose
- Can still use marker file approach

**Cons:**
- Two different conventions to remember
- More complex mental model

#### 3. Nested Structure
- `.aiki/home/` subdirectory presence indicates boundary
- If present, use `{boundary_dir}/.aiki/home/` instead of `~/.aiki/`

**Pros:**
- Keeps everything under `.aiki/`
- Clear hierarchy

**Cons:**
- Nested structure might be confusing
- Still has ambiguity about which `.aiki/` level

### Original Marker-Based Idea (Problematic)

Allow users to place `.aiki-boundary` marker file in a directory:
- Walk up from cwd looking for marker (like `.git`)
- Use `{boundary_dir}/.aiki/` instead of `~/.aiki/`
- Provide UX indicator showing which boundary is active (like `direnv` or Python venv)

**Problem:** Conflicts with per-repo `.aiki/` usage.

## Prior Art

- **direnv** - Per-directory environment variables, shows active environment in prompt
- **Git's `includeIf`** - Conditional config based on path patterns
- **Firefox containers** - Isolate browsing context by domain/project
- **Python venv** - Shows active virtual environment in shell prompt

## Requirements

Any solution should provide:

1. **Visible indicator of active boundary** - Status line, shell prompt integration showing which workspace is active
2. **Clear documentation on security model** - What's isolated, what's not
3. **Tool to list/switch boundaries** - `aiki workspace list`, `aiki workspace show`
4. **Default to `~/.aiki/` when no boundary defined** - Zero-config for simple use cases
5. **Portability** - Either config sync or marker-based approach

## Security Model

What gets isolated:
- Session state (`sessions/`)
- Conversation history (`.jj/` on `aiki/conversations` branch)

What stays per-repo:
- Task management (`{repo}/.aiki/.jj/` on `aiki/tasks` branch)
- Flows and task templates (`{repo}/.aiki/flows/`, `{repo}/.aiki/task-templates/`)
- Git hooks (`{repo}/.git/hooks/`)

## Open Questions

1. Should workspace switching be explicit (`aiki workspace use <name>`) or implicit (based on cwd)?
2. How to handle sessions that span multiple workspaces?
3. Should there be a "global" workspace for cross-boundary work?
4. What happens if a repo matches multiple workspace patterns?

## Next Steps (When Implemented)

1. Prototype one approach (recommend: workspaces config)
2. Add `aiki workspace` commands
3. Add shell prompt integration (`aiki prompt`)
4. Document security boundaries clearly
5. Add tests for boundary violations
