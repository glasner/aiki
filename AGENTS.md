# Implementation Planning and TDD

When creating implementation plans, **Test-Driven Development (TDD) is critical**.

### Why TDD Matters

- **Clear requirements** - Writing tests first forces you to define what "done" looks like
- **Faster feedback** - Catch bugs immediately rather than during manual testing
- **Confident refactoring** - Tests provide a safety net for future changes
- **Reduced debugging time** - Isolated test failures pinpoint exact issues

### TDD Workflow

1. **Write the test first** - Define expected behavior before implementation
2. **Run the test and watch it fail** - Confirm the test is valid
3. **Write minimal code to pass** - Implement just enough to satisfy the test
4. **Refactor** - Clean up code while keeping tests green
5. **Repeat** - Continue with next functionality

### Implementation Plans Must Include

- **Test cases** - What tests will be written (unit, integration, etc.)
- **Test-first ordering** - Write tests before implementation in each step
- **Failure conditions** - What errors/edge cases need testing
- **Success criteria** - All tests pass

### Example Plan

```markdown
## Task: Implement user authentication

### Step 1: Write authentication tests
- Test valid credentials → success
- Test invalid credentials → error
- Test missing credentials → error

### Step 2: Implement authentication logic
- Write minimal code to pass tests

### Step 3: Refactor
- Clean up while keeping tests green
```

### Anti-Patterns to Avoid

❌ **Writing implementation first, tests later** - Leads to tests that confirm what code does, not what it should do
❌ **Skipping tests for "simple" changes** - Even small changes have unexpected side effects
❌ **Treating tests as optional** - Tests are as important as implementation

**Always start with tests, then implement.** This saves time, catches bugs early, and produces better code.

# Code Reviews

When performing code reviews, track each issue with structured data:

```bash
aiki review issue add <review-id> "Description" --severity high --file src/auth.rs:42
```

**Severity** (pick one per issue):
- `--high` — Must fix: incorrect behavior, bug, or contract violation
- (default) — Should fix: suboptimal, missing, or inconsistent (no flag needed)
- `--low` — Could fix: style, naming, cosmetic

**Location** (`--file`, repeatable):
- `--file src/auth.rs` — file only
- `--file src/auth.rs:42` — file and line
- `--file src/auth.rs:42-50` — file and line range
- `--file src/a.rs:10 --file src/b.rs:20` — multiple files

# Repo Documentation

Public docs for the cli live in ./docs

Before closing any task, make sure to update docs to keep them up to date.
