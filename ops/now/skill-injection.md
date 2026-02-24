---
status: draft
---

# Skill Injection for Workflow Customization

## Problem

Currently, customizing workflows (like code review criteria) requires editing template files in `.aiki/templates/`. This creates friction:

1. **Template complexity**: Users must understand template structure, frontmatter, and subtask composition
2. **Not portable**: Customizations are tied to the repo, not easily shared across projects
3. **Rigid structure**: Adding custom review criteria requires creating new subtask template files

Example: To customize code review criteria, users must edit `.aiki/templates/aiki/review/criteria/code.md`.

## Proposed Solution

Allow tasks to reference **skills** that get auto-injected when agents start the task. Skills provide contextual instructions without requiring template modifications.

### Key Benefits

1. **Simpler customization**: Edit a single skill file instead of navigating template structure
2. **Portable**: Skills can be project-specific (`.aiki/skills/`) or personal (`~/.aiki/skills/`)
3. **Composable**: Templates provide structure, skills provide detailed context/instructions
4. **Backward compatible**: Existing templates continue to work without skills
5. **Standards-based**: Implements the [Agent Skills specification](https://agentskills.io/specification) — an open format maintained by Anthropic, compatible with Claude Code, Cursor, Gemini CLI, GitHub Copilot, and other adopters

## Agent Skills Specification Summary

The [Agent Skills spec](https://agentskills.io/specification) defines a simple, open format for giving agents new capabilities. Key points relevant to aiki:

### Spec-Defined Frontmatter Fields

| Field | Required | Constraints |
|-------|----------|-------------|
| `name` | Yes | Max 64 chars. Lowercase letters, numbers, hyphens only. No start/end hyphen. No consecutive hyphens. **Must match parent directory name.** |
| `description` | Yes | Max 1024 chars. Non-empty. Describes what the skill does AND when to use it. Write in third person. |
| `license` | No | License name or reference to bundled license file. |
| `compatibility` | No | Max 500 chars. Environment requirements (intended product, system packages, network access). |
| `metadata` | No | Arbitrary key-value map (`string → string`). For client-specific properties. |
| `allowed-tools` | No | **Space-delimited** string of pre-approved tools. Experimental. Example: `Bash(git:*) Read Grep` |

### Claude Code Extensions (Beyond the Spec)

Claude Code extends the Agent Skills spec with additional frontmatter fields and features. Aiki should support these to maintain compatibility with Claude Code skills.

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `user-invocable` | `bool` | `true` | Set `false` to hide from `/` menu. Use for background knowledge that shouldn't be invoked as a command. |
| `disable-model-invocation` | `bool` | `false` | Set `true` to prevent agent from auto-loading. Use for workflows with side effects (deploy, send-message). |
| `argument-hint` | `string` | — | Autocomplete hint shown during `/` invocation. Example: `[issue-number]` or `[filename] [format]`. |
| `context` | `string` | — | Set to `fork` to run in an isolated subagent context. |
| `agent` | `string` | `general-purpose` | Which subagent type when `context: fork`. Options: `Explore`, `Plan`, `general-purpose`, or custom agent name. |
| `model` | `string` | — | Model to use when skill is active. |
| `hooks` | `object` | — | Lifecycle hooks scoped to this skill's execution. |

**Invocation control matrix** (Claude Code):

| Frontmatter | User can invoke | Agent can invoke | Description in context? |
|-------------|----------------|-----------------|------------------------|
| (default) | Yes | Yes | Yes |
| `disable-model-invocation: true` | Yes | No | No |
| `user-invocable: false` | No | Yes | Yes |

**String substitutions** (Claude Code extension):
- `$ARGUMENTS` — all arguments passed when invoking the skill
- `$ARGUMENTS[N]` / `$N` — specific argument by 0-based index
- `${CLAUDE_SESSION_ID}` — current session ID

**Dynamic content** (Claude Code extension):
- `!`command`` syntax — runs shell commands before sending content to agent, output replaces the placeholder
- This is preprocessing, not agent-executed. Agent sees only the final rendered content.

**Behavioral differences from spec:**
- Claude Code makes `name` optional (falls back to directory name)
- Claude Code makes `description` recommended but not required (falls back to first paragraph of markdown)
- The spec requires both `name` and `description`

### How Aiki Should Handle Extensions

**Strategy**: Parse and store all fields (spec + Claude Code extensions) but only _require_ spec-defined fields. Claude Code extension fields are supported but optional. Unknown fields outside both sets produce warnings.

This means:
1. Aiki skills validate against the Agent Skills spec (name, description required)
2. Claude Code-authored skills (with `user-invocable`, `context`, etc.) are accepted without warnings
3. Aiki-specific properties go in `metadata` to remain portable
4. Skills authored for aiki work in Claude Code and vice versa

### Progressive Disclosure (Three Tiers)

The spec defines a three-tier loading model to manage context efficiently:

1. **Metadata** (~100 tokens per skill): `name` + `description` loaded at startup for ALL discovered skills
2. **Instructions** (<5000 tokens recommended): Full `SKILL.md` body loaded when skill is activated
3. **Resources** (as needed): Files in `scripts/`, `references/`, `assets/` loaded only when required during execution

### Discovery & Prompt Injection

The spec recommends injecting skill metadata as XML in the system prompt:

```xml
<available_skills>
<skill>
<name>aiki-review</name>
<description>Code review criteria and methodology for aiki workflows.</description>
<location>/path/to/.aiki/skills/aiki-review/SKILL.md</location>
</skill>
</available_skills>
```

For filesystem-based agents (like aiki), include `<location>` with the absolute path so the agent can `cat` the file when activated.

### Spec-Defined Directory Conventions

```
skill-name/
├── SKILL.md          # Required (or skill.md)
├── scripts/          # Optional: executable code
├── references/       # Optional: documentation (REFERENCE.md, domain files)
└── assets/           # Optional: templates, images, data files, schemas
```

File references should be **one level deep** from SKILL.md — avoid deeply nested reference chains.

### Validation

The spec provides a reference library ([skills-ref](https://github.com/agentskills/agentskills/tree/main/skills-ref)) for validation:
- Name format validation (charset, length, directory match, no consecutive hyphens)
- Required fields check (`name`, `description`)
- Unknown field detection (only spec fields allowed in frontmatter)
- `<available_skills>` XML prompt generation

## Design

### 1. Skill Association

Tasks can be associated with skills in two ways:

**A. Convention-based (built-in workflows)**
- `aiki review` → auto-associates `aiki-review` skill
- `aiki build` → auto-associates `aiki-build` skill
- Pattern: command name maps to skill name (with `aiki-` prefix for built-ins)

**B. Explicit declaration (custom templates)**

Via template frontmatter:
```yaml
---
version: 1.0.0
type: custom-workflow
skill: security-audit
---
```

Via CLI flag:
```bash
aiki task add "Custom analysis" --template mytemplate --skill deep-dive
```

### 2. Skill Storage

Follow the Agent Skills spec conventions (with `.aiki/` instead of `.claude/`):

**Project-level** (committed to repo):
```
.aiki/skills/
├── aiki-review/
│   └── SKILL.md         # Built-in review skill (ships with aiki)
├── security-audit/
│   └── SKILL.md
└── performance-review/
    └── SKILL.md
```

**Personal-level** (user's home directory):
```
~/.aiki/skills/
├── my-review-style/
│   └── SKILL.md
```

**Precedence order**: Project overrides personal. Rationale: repo-specific workflow requirements (committed by the team) should trump personal preferences. Users who want personal customization can fork the project skill.

1. Project skills (`.aiki/skills/`) — highest priority
2. Personal skills (`~/.aiki/skills/`) — fallback

This is the opposite of Claude Code's order (personal > project) but makes more sense for aiki's team-workflow model.

**Discovery**: Skills are auto-discovered by scanning `.aiki/skills/` directories for folders containing `SKILL.md`. Supports nested discovery for monorepo structures.

### 3. Skill Format

Use standard `SKILL.md` format per the Agent Skills spec:

```markdown
---
name: aiki-review
description: Evaluates code changes against plan coverage, code quality, security, and architecture alignment. Used automatically during aiki review workflows.
---

# Code Review Skill

When reviewing code, evaluate against these categories:

## Plan Coverage
- All requirements from the plan exist in the codebase
- No missing features or unimplemented sections
- No scope creep beyond what the plan describes

## Code Quality
- Logic errors, incorrect assumptions, edge cases
- Error handling and resource management
- Code clarity and maintainability

## Security
- Injection vulnerabilities (command, SQL, XSS)
- Authentication and authorization issues
- Data exposure or crypto misuse

## Plan Alignment
- UX matches plan design (commands, flags, output format)
- Architecture follows plan's prescribed approach
- Acceptance criteria from plan are met
```

**Spec compliance notes:**
- `name` must match directory name (`aiki-review/` → `name: aiki-review`)
- `description` should be specific, third-person, include keywords for when to use
- Only use spec-defined frontmatter fields; put aiki-specific properties in `metadata`
- Keep `SKILL.md` body under 500 lines / ~5000 tokens
- `allowed-tools` is space-delimited if used: `allowed-tools: Read Grep Bash`

### 4. Injection Mechanism

Skills are injected when an agent starts a task via `aiki task start`. Aiki follows the spec's two-phase loading model:

**Phase A — Startup (all skills, metadata only):**
1. Scan `.aiki/skills/` and `~/.aiki/skills/` for valid skill directories
2. Parse only frontmatter (`name` + `description`) from each `SKILL.md`
3. Inject `<available_skills>` XML block into agent's system prompt (~100 tokens per skill)

**Phase B — Activation (task-associated skill, full content):**
1. When `aiki task start <task-id>` runs, check if task has an associated skill
2. If skill exists, load full `SKILL.md` content (frontmatter + body)
3. Prepend skill instructions to the task's main instructions
4. Agent receives: skill context + template instructions + subtask list

**Why auto-inject instead of model-invoked?** Unlike Claude Code where users manually invoke skills (`/skill-name`) or models decide based on description matching, aiki skills are **deterministically injected** based on task metadata. This is more reliable for workflow automation — the right context is always present when agents start work via `aiki task run`.

**Respecting Claude Code extension fields during injection:**
- `disable-model-invocation: true` → exclude from `<available_skills>` XML (agents can't discover it)
- `user-invocable: false` → exclude from `aiki skill list` output (users can't invoke it)
- `context: fork` → when this skill is activated, run it in a subagent rather than inline
- `allowed-tools` → pass to agent as pre-approved tools when skill is active

**Hybrid approach (future):** Phase A metadata injection means agents could also discover and activate skills on-demand beyond what's auto-injected. This enables both deterministic injection (for built-in workflows) and model-invoked activation (for user-installed skills).

### 5. Review Workflow Migration

**Before** (current state):
```
.aiki/templates/aiki/review.md           # Main template
.aiki/templates/aiki/review/criteria/
├── code.md                              # Code review criteria
└── plan.md                              # Plan review criteria
```

The template uses conditional subtasks:
```liquid
{% subtask aiki/review/criteria/plan if data.scope.kind == "plan" %}
{% subtask aiki/review/criteria/code if data.scope.kind != "plan" %}
```

**After** (with skills):
```
.aiki/templates/aiki/review.md           # Main template (simplified)
.aiki/skills/aiki-review/
├── SKILL.md                             # Review criteria (all in one)
└── references/
    └── review-examples.md               # Example reviews (loaded on demand)
```

The skill contains all review criteria (both plan and code), and the template references the skill:
```yaml
---
version: 2.0.0
type: review
skill: aiki-review
---
```

**Key change**: Eliminate `review/criteria/*` subtask templates. Move that content into `aiki-review` skill.

### 6. Template Frontmatter Extension

Add `skill` field to `TemplateFrontmatter`:

```rust
pub struct TemplateFrontmatter {
    pub slug: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub task_type: Option<String>,
    pub assignee: Option<String>,
    pub priority: Option<String>,
    pub data: HashMap<String, serde_json::Value>,
    pub spawns: Vec<SpawnEntry>,
    pub loop_config: Option<LoopConfig>,

    // NEW: Associated skill (name must match a discovered skill)
    pub skill: Option<String>,
}
```

Store skill reference in task metadata so it's available when the agent starts.

### 7. Supporting Files

Skills can include supplementary files per the spec's directory conventions:

```
.aiki/skills/aiki-review/
├── SKILL.md              # Main skill file (under 500 lines)
├── scripts/
│   └── check-coverage.sh # Executable utility scripts
├── references/
│   ├── good-review.md    # Example of a thorough review
│   └── common-mistakes.md # Common review mistakes to avoid
└── assets/
    └── review-report.md  # Template for review output format
```

Reference these from `SKILL.md` (one level deep only):

```markdown
For examples, see:
- [Good review example](references/good-review.md)
- [Common mistakes](references/common-mistakes.md)

Use the [review report template](assets/review-report.md) for your output.
```

### 8. Skill Parsing & Validation

Implement parsing/validation aligned with the spec's [reference SDK](https://github.com/agentskills/agentskills/tree/main/skills-ref):

**Parsing:**
- Find `SKILL.md` (or `skill.md`) in skill directory
- Extract YAML frontmatter between `---` delimiters
- Parse into `SkillProperties` struct with `name`, `description`, `license`, `compatibility`, `allowed_tools`, `metadata`
- Return markdown body separately for injection

**Validation:**
- `name`: 1-64 chars, lowercase alphanumeric + hyphens, no start/end hyphen, no `--`, must match directory name
- `description`: 1-1024 chars, non-empty
- `compatibility`: max 500 chars if present
- Spec-defined fields: accepted without warning
- Claude Code extension fields: accepted without warning (known extensions)
- Unknown fields: produce a warning

```rust
/// Agent Skills spec fields (required for compliance)
pub struct SkillProperties {
    pub name: String,                           // required (spec)
    pub description: String,                    // required (spec)
    pub license: Option<String>,                // optional (spec)
    pub compatibility: Option<String>,          // optional (spec)
    pub allowed_tools: Option<String>,          // optional (spec), space-delimited
    pub metadata: HashMap<String, String>,      // optional (spec), arbitrary k-v
}

/// Claude Code extension fields (optional, for compatibility)
pub struct SkillExtensions {
    pub user_invocable: Option<bool>,           // default: true
    pub disable_model_invocation: Option<bool>, // default: false
    pub argument_hint: Option<String>,
    pub context: Option<String>,                // "fork" for subagent
    pub agent: Option<String>,                  // subagent type
    pub model: Option<String>,
    pub hooks: Option<serde_json::Value>,       // opaque hook config
}

/// Combined skill definition
pub struct Skill {
    pub properties: SkillProperties,            // spec-compliant fields
    pub extensions: SkillExtensions,            // Claude Code extensions
    pub body: String,                           // markdown content
    pub path: PathBuf,                          // filesystem location
}
```

### 9. Claude Code Extensions Support

Aiki will parse and store all Claude Code extension fields but implement their behavior incrementally:

**Phase 1 (parse only):** Parse `user-invocable`, `disable-model-invocation`, `argument-hint`, `context`, `agent`, `model`, `hooks` from frontmatter. Store in `SkillExtensions`. No behavioral effect yet.

**Phase 2 (invocation control):** Respect `disable-model-invocation` (exclude from `<available_skills>` XML) and `user-invocable` (exclude from `aiki skill list`).

**Phase 5 (tooling):** Use `argument-hint` in `aiki skill list` output.

**Phase 6 (advanced):** Implement `context: fork` + `agent` (subagent execution), `$ARGUMENTS`/`$N` substitutions, and `!`command`` preprocessing.

**String substitutions** (`$ARGUMENTS`, `$N`, `${CLAUDE_SESSION_ID}`): These are a Claude Code extension. Aiki already has template variables — we'll support the Claude Code syntax as an alias so skills are portable. Deferred to Phase 6.

**Dynamic content** (`!`command``): Also a Claude Code extension. Preprocessing shell commands before injection is powerful but complex. Deferred to Phase 6. Aiki's existing template variable system covers most use cases.

## Implementation Plan

### Phase 1: Skill Parsing & Discovery
1. Add `SkillProperties` + `SkillExtensions` + `Skill` structs
2. Implement `SKILL.md` frontmatter parser (YAML extraction, spec fields + Claude Code extensions)
3. Implement validation: name format, directory-name match, required fields, known-field allowlist (spec + extensions), warn on unknown
4. Implement skill directory discovery (scan `.aiki/skills/`, `~/.aiki/skills/`)
5. Add `skill: Option<String>` to `TemplateFrontmatter`
6. Add `--skill <name>` flag to `aiki task add`
7. Store skill reference in task data

### Phase 2: Injection Mechanism
1. On `aiki task start`, check if task has associated skill
2. Load full `SKILL.md` content and prepend to agent context
3. Generate `<available_skills>` XML from all discovered skills (metadata only)
4. Inject XML into system prompt at startup (for future model-invoked activation)
5. Test with simple skill examples

### Phase 3: Convention Support & Built-in Skills
1. Add convention mapping (`aiki review` → `aiki-review` skill)
2. Update `aiki review` command to auto-associate skill
3. Create `.aiki/skills/aiki-review/SKILL.md` with combined review criteria
4. Test convention-based injection end-to-end

### Phase 4: Review Workflow Migration
1. Update `.aiki/templates/aiki/review.md` to reference `aiki-review` skill
2. Remove `.aiki/templates/aiki/review/criteria/*` subtask templates
3. Add `references/` directory to `aiki-review` skill for supplementary content
4. Test review workflow with skill injection (both `aiki review` and `aiki task run`)

### Phase 5: Documentation & Tooling
1. `aiki skill list` — show discovered skills (name, description, location)
2. `aiki skill show <name>` — display full `SKILL.md` content
3. `aiki skill create <name>` — scaffold new skill directory with valid `SKILL.md`
4. `aiki skill validate <name>` — run spec validation on a skill
5. Document skill format, conventions, and customization workflow

### Phase 6: Advanced Features (Optional)
1. Support `allowed-tools` frontmatter (pre-approve tool usage for delegated agents)
2. Model-invoked skill activation (agents choose skills from `<available_skills>` XML)
3. Multiple skills per task (`skills: [aiki-review, security-audit]`)
4. Skill composition (one skill referencing another)
5. Support `context: fork` + `agent` (run skill in subagent via `aiki task run`)
6. Support `$ARGUMENTS` / `$N` string substitutions for parameterized skills
7. Support `!`command`` dynamic content preprocessing (Claude Code extension)

## Design Decisions

### Resolved

1. **Naming convention**: Use flat kebab-case names (`aiki-review`, not `aiki/review`). Matches Agent Skills spec requirement and is consistent with Claude Code. Directory name must match.

2. **Skill precedence**: Project overrides personal. Unlike Claude Code (personal > project), aiki prioritizes team workflow consistency. Repo-committed skills represent the team's chosen review criteria; personal skills are fallbacks.

3. **Dynamic content**: Deferred. The `!`command`` syntax is a Claude Code extension, not part of the spec. Use aiki's existing template variables if dynamic content is needed in skills.

4. **Frontmatter compliance**: Accept both spec-defined fields AND Claude Code extension fields. Require spec fields (`name`, `description`). Store aiki-specific properties in `metadata` (e.g., `metadata: { aiki-auto-inject: "true" }`). Warn on truly unknown fields.

5. **Supporting directories**: Use spec-standard names: `scripts/`, `references/`, `assets/` (not `examples/`, `templates/`).

6. **Claude Code compatibility**: Full round-trip support. Skills authored in Claude Code (with `user-invocable`, `context: fork`, `argument-hint`, etc.) work in aiki. Skills authored in aiki work in Claude Code. Extension fields are parsed, stored, and respected where applicable.

### Open

1. **Multiple skills per task**: Support `skills: [aiki-review, security-audit]` or single skill only?
   - The spec is skill-per-invocation, but aiki's workflow model may benefit from composition
   - Start with single skill, add multi-skill if needed

2. **Skill versioning**: Track via `metadata.version` for documentation, not as a functional feature. The spec's `metadata` field is the right place for this.

3. **Conditional skills**: Should skill injection depend on task data (e.g., only inject security skill if `data.security_critical = true`)? Defer — start with unconditional injection.

## Success Criteria

1. Users can customize review criteria by editing `.aiki/skills/aiki-review/SKILL.md`
2. Skills work with `aiki task run` (delegated agents receive skill context)
3. Skills are portable (can copy `.aiki/skills/` across projects)
4. No breaking changes to existing templates
5. Clear migration path from subtask templates to skills
6. **Fully compliant with the [Agent Skills specification](https://agentskills.io/specification)**
7. Skill frontmatter validates against spec rules (name format, required fields, field allowlist)
8. Skills under 500 lines with `references/` and `assets/` for supplementary material
9. `<available_skills>` XML generated for agent prompt injection
10. `aiki skill validate` passes the same checks as the spec's `skills-ref validate`

## References

- [Agent Skills Specification](https://agentskills.io/specification) — the normative spec
- [Agent Skills Integration Guide](https://agentskills.io/integrate-skills) — how to build a compatible client
- [Agent Skills Reference SDK](https://github.com/agentskills/agentskills/tree/main/skills-ref) — Python reference implementation (parser, validator, prompt generator)
- [Example Skills](https://github.com/anthropics/skills) — official example skills from Anthropic
- [Skill Authoring Best Practices](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices) — writing effective skills
- [Claude Code Skills Documentation](https://code.claude.com/docs/en/skills) — Claude Code-specific extensions
