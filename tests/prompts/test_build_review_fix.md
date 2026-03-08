You are testing aiki's build, review, and fix pipeline commands. Run through ALL phases carefully and report results.

## Setup
- Run `aiki task start "Test build/review/fix pipeline: comprehensive command test" --source prompt`
- Note the task ID as PARENT

---

## Phase 1: `aiki build <plan.md>`

Test that `aiki build` creates an epic from a plan file, runs decompose to create subtasks, then runs the loop to execute them.

### 1.0 Create a plan file

Create `test-plan.md` with this content:

```markdown
---
title: Add greeting utilities
scope: test-greetings/
---

# Add greeting utilities

Create a small greeting utility module for testing purposes.

## Tasks

1. Create `test-greetings/hello.py` with a function `greet(name: str) -> str` that returns `"Hello, {name}!"`
2. Create `test-greetings/farewell.py` with a function `farewell(name: str) -> str` that returns `"Goodbye, {name}!"`
```

Commit it so the build command can see it:

    jj commit -m "Add test-plan.md for build/review/fix pipeline test"

### 1.1 Run the build

    aiki build test-plan.md

This should run synchronously. Wait for it to complete.

### 1.2 Verify build results

**Check epic was created:**
- `aiki task` — should show the epic task linked to `test-plan.md`
- The epic should have subtasks created by the decompose phase

**Check subtasks were executed:**
- `aiki task show <epic-id>` — all subtasks should be closed/completed
- The files described in the plan should exist:
  - `test-greetings/hello.py` — should contain a `greet()` function
  - `test-greetings/farewell.py` — should contain a `farewell()` function

**Check JJ history:**
- `jj log -r ..@` — should show changes from the build agents
- Each change should have `[aiki]` metadata in its description

### 1.3 Record phase 1 results

Note: PASS if build completed, epic was created with subtasks, subtasks were executed, and output files exist. FAIL otherwise with details.

---

## Phase 2: `aiki review <task-id>`

Test that `aiki review` creates a review task, runs the reviewer agent, and records issues.

### 2.0 Setup

Identify the epic task ID from phase 1. If phase 1 failed, create a simple task manually:

    aiki task start "Create a test file with a deliberate bug"

Then create `buggy.py` with intentionally reviewable code:

```python
def divide(a, b):
    return a / b  # No zero-division check

def parse_age(s):
    return int(s)  # No error handling for non-numeric input

def read_config(path):
    f = open(path)  # No context manager, no error handling
    data = f.read()
    return data
```

Close the task:

    aiki task close <task-id> --summary "Created buggy.py with deliberate issues"

### 2.1 Run the review

    aiki review <task-id>

This should run synchronously. Wait for it to complete.

### 2.2 Verify review results

**Check review task was created:**
- `aiki task` — should show a review task
- `aiki task show <review-task-id>` — should show the review task details

**Check issues were recorded:**
- `aiki review issue list <review-task-id>` — should list issues found
- Issues should reference specific files and lines
- Issues should have severity levels (high/medium/low)

**Check review task structure:**
- The review task should have the correct scope (task scope pointing to the reviewed task)
- The review task should be linked to the original task

### 2.3 Note the review task ID

Save the review task ID for phase 4 (fix) and phase 5 (fix --once). If no issues were found, the review passed cleanly — note this as a data point but the test still passes.

### 2.4 Record phase 2 results

Note: PASS if review task was created, ran to completion, and either found issues or approved. FAIL if the command errored or produced no output.

---

## Phase 3: `aiki review <task-id> --fix`

Test the combined review+fix flow: review runs first, then if issues are found, fix pipeline automatically kicks in.

### 3.0 Setup

Create a new task with code that will definitely produce review issues:

    aiki task start "Create code with security issues for review --fix test"

Create `insecure.py`:

```python
import subprocess
import sqlite3

def run_command(user_input):
    # Command injection vulnerability
    subprocess.call("echo " + user_input, shell=True)

def query_db(user_input):
    # SQL injection vulnerability
    conn = sqlite3.connect("test.db")
    conn.execute("SELECT * FROM users WHERE name = '" + user_input + "'")

def set_password(password):
    # Storing password in plaintext
    with open("passwords.txt", "a") as f:
        f.write(password + "\n")
```

Close the task:

    aiki task close <task-id> --summary "Created insecure.py with security vulnerabilities"

### 3.1 Run review --fix

    aiki review <task-id> --fix

This should:
1. Run the review phase (find issues)
2. Automatically trigger the fix pipeline
3. The fix pipeline creates a plan, decomposes it, runs the loop
4. A post-fix review checks the fixes

Wait for it to complete.

### 3.2 Verify review --fix results

**Check the review found issues:**
- `aiki review issue list <review-task-id>` — should list security issues

**Check fix pipeline ran:**
- `aiki task` — should show fix-related tasks (fix-parent, plan-fix, etc.)
- The fix tasks should be closed/completed

**Check code was actually fixed:**
- `cat insecure.py` — the security vulnerabilities should be remediated:
  - Command injection should use parameterized calls or shlex
  - SQL injection should use parameterized queries
  - Password storage should use hashing

**Check the quality loop:**
- There should be a post-fix review task
- The post-fix review should have fewer (or zero) issues

### 3.3 Record phase 3 results

Note: PASS if review found issues, fix pipeline ran, and code was improved. FAIL otherwise with details.

---

## Phase 4: `aiki fix <review-task-id>`

Test the fix command standalone — given a review task with issues, create a fix plan, decompose, and loop.

### 4.0 Setup

If phase 2 produced a review task with issues, use that review task ID. Otherwise, create a setup:

    aiki task start "Create code with bugs for fix test"

Create `broken.py`:

```python
def fibonacci(n):
    # Bug: doesn't handle n=0 or n=1 correctly
    if n == 1:
        return 1
    return fibonacci(n-1) + fibonacci(n-2)  # Stack overflow for n=0

def average(numbers):
    # Bug: crashes on empty list
    return sum(numbers) / len(numbers)

def find_max(lst):
    # Bug: returns None for single-element list
    max_val = lst[0]
    for i in range(1, len(lst)):
        if lst[i] > max_val:
            max_val = lst[i]
    # Missing return statement
```

Close the task:

    aiki task close <task-id> --summary "Created broken.py with bugs"

Run a review to get a review task with issues:

    aiki review <task-id>

Note the review task ID.

### 4.1 Run fix

    aiki fix <review-task-id>

This should:
1. Check for actionable issues in the review
2. Create a fix-parent task
3. Create a plan-fix task (generates a fix plan)
4. Run decompose on the plan
5. Run the loop to execute fix subtasks
6. Run a post-fix review
7. If issues remain, loop back (quality loop)
8. Continue until approved or MAX_QUALITY_ITERATIONS

Wait for it to complete.

### 4.2 Verify fix results

**Check the fix pipeline structure:**
- `aiki task` — should show fix-parent, plan-fix, and implementation subtasks
- All fix tasks should be closed/completed

**Check code was fixed:**
- `cat broken.py` — bugs should be remediated:
  - `fibonacci(0)` should work (return 0)
  - `average([])` should handle empty lists
  - `find_max` should have a return statement

**Check the quality loop ran:**
- There should be at least one post-fix review
- If the first fix attempt didn't fully resolve issues, there should be multiple iterations
- The final review should approve (or reach max iterations)

**Check the output:**
- The command should output "Approved" if all issues were resolved
- Or indicate that max iterations were reached

### 4.3 Record phase 4 results

Note: PASS if fix pipeline ran (plan → decompose → loop → review cycle), issues were addressed, and the quality loop completed. FAIL otherwise with details.

---

## Phase 5: `aiki fix <review-task-id> --once`

Test the single-pass fix mode — same as `aiki fix` but without the post-fix review loop.

### 5.0 Setup

Create a new task and review:

    aiki task start "Create code with style issues for fix --once test"

Create `messy.py`:

```python
def   calculateTotal(  items ,tax_rate):
    t = 0
    for i in items:
        t = t + i
    t = t + t * tax_rate
    return t

def    processData(   d  ):
    r = []
    for x in d:
        if x > 0:
            r.append(x * 2)
        else:
            r.append(0)
    return r
```

Close the task:

    aiki task close <task-id> --summary "Created messy.py with style issues"

Run a review:

    aiki review <task-id>

Note the review task ID.

### 5.1 Run fix --once

    aiki fix <review-task-id> --once

This should:
1. Check for actionable issues
2. Create fix-parent task
3. Create plan-fix task
4. Run decompose
5. Run loop to execute fixes
6. **STOP** — no post-fix review, no quality loop

### 5.2 Verify fix --once results

**Check it ran the fix pipeline:**
- `aiki task` — should show fix-parent and subtasks
- All fix tasks should be closed/completed

**Check code was fixed:**
- `cat messy.py` — style issues should be improved

**Critical: Verify no post-fix review ran:**
- There should be NO review task created after the fix
- The command should complete after the loop phase without entering a quality loop
- This is the key difference from `aiki fix` (phase 4)

### 5.3 Record phase 5 results

Note: PASS if fix ran a single pass (plan → decompose → loop) and stopped WITHOUT a post-fix review. FAIL if a post-fix review was created or the command errored.

---

## Phase 6: `aiki build <plan.md> --fix`

Test the full build+review+fix pipeline — build runs first, then review, then fix if issues found.

### 6.0 Create a plan that will produce reviewable output

Create `test-plan-fixable.md`:

```markdown
---
title: Create data processing module
scope: test-dataproc/
---

# Create data processing module

Build a small data processing module. Intentionally keep the implementation simple so the reviewer can find improvements.

## Tasks

1. Create `test-dataproc/parser.py` with a function `parse_csv(path: str) -> list[dict]` that reads a CSV file and returns a list of dictionaries. Use basic file I/O (not the csv module) to keep it simple.
2. Create `test-dataproc/transform.py` with a function `normalize(data: list[dict], field: str) -> list[dict]` that normalizes a numeric field to 0-1 range.
3. Create `test-dataproc/stats.py` with functions `mean(values)`, `median(values)`, and `std_dev(values)` for basic statistics.
```

Commit it:

    jj commit -m "Add test-plan-fixable.md for build --fix pipeline test"

### 6.1 Run build --fix

    aiki build test-plan-fixable.md --fix

This should run the full pipeline:
1. **Build phase:** decompose → loop (create the code)
2. **Review phase:** review the built code
3. **Fix phase:** if issues found, run fix pipeline (plan → decompose → loop → review loop)

Wait for it to complete.

### 6.2 Verify build --fix results

**Check build completed:**
- The epic task should exist and its subtasks should be closed
- The code files should exist in `test-dataproc/`

**Check review ran after build:**
- A review task should exist linked to the epic
- `aiki review issue list <review-task-id>` — should show issues found (or approved)

**Check fix ran after review (if issues found):**
- Fix-related tasks should exist
- The code should be improved compared to the initial build output

**Check the full pipeline chain:**
- `aiki task` — should show the epic, review, and fix tasks in a clear hierarchy
- The timeline should show: build → review → fix → post-fix review

### 6.3 Record phase 6 results

Note: PASS if the full build → review → fix pipeline completed end-to-end. FAIL otherwise with details.

---

## Cleanup

Remove all test artifacts:

    rm -rf test-greetings/ test-dataproc/
    rm -f test-plan.md test-plan-fixable.md buggy.py insecure.py broken.py messy.py

---

## Final Summary

Close the parent task with results:

    aiki task close <PARENT> --summary "Results: Phase 1 (build): PASS/FAIL. Phase 2 (review): PASS/FAIL. Phase 3 (review --fix): PASS/FAIL. Phase 4 (fix): PASS/FAIL. Phase 5 (fix --once): PASS/FAIL. Phase 6 (build --fix): PASS/FAIL. Details: ..."

**Report format:** For each phase and sub-check, state PASS or FAIL with details. Include any error output verbatim. Pay special attention to:
- Did `aiki build` correctly create an epic, decompose subtasks, and run the loop?
- Did `aiki review` create a review task and record issues?
- Did `aiki review --fix` chain review → fix automatically?
- Did `aiki fix` run the full quality loop (plan → decompose → loop → review, repeating)?
- Did `aiki fix --once` stop after a single pass (no post-fix review)?
- Did `aiki build --fix` chain build → review → fix end-to-end?
- Were all task links correct (epic, review scope, fix-parent relationships)?
- Did the agents produce actual code changes (not just empty tasks)?
