---
version: 2.0.0
type: orchestrator
spawns:
  - when: "data.options.review or data.options.fix"
    task:
      template: aiki/review
      data:
        scope.kind: '"code"'
        scope.id: data.plan
        scope.name: '"Code (" + data.plan + ")"'
        options.fix: data.options.fix
---

# Implement: {{data.target}}

You are orchestrating the implementation of task {{data.target}}.

## Step 1: Understand the work

    aiki task show {{data.target}}
    aiki task lane {{data.target}} --all

## Step 2: Execute

Loop until all lanes are complete:

1. Get ready lanes via `aiki task lane {{data.target}}`
2. For each ready lane, start it with `--next-session --lane <lane-id> --async`
3. Collect the last task IDs from started sessions
4. Wait for any to finish with `aiki task wait <id1> <id2> ... --any`
5. Loop back — finished session may have unblocked new lanes or the next session in a lane

```bash
while true; do
  # Get ready lanes
  ready=$(aiki task lane {{data.target}})
  [ -z "$ready" ] && break

  # Start ready lanes, collect last task IDs for waiting
  wait_ids=()
  for lane in $ready; do
    last_id=$(aiki task run {{data.target}} --next-session --lane $lane --async)
    wait_ids+=("$last_id")
  done

  # If nothing was started (all lanes already running or blocked), wait on existing
  [ ${#wait_ids[@]} -eq 0 ] && break

  # Wait for any session to finish
  aiki task wait "${wait_ids[@]}" --any
done
```

**How it works:**

1. Get ready lanes (may be empty if all lanes are blocked or running)
2. Start sessions for ready lanes
3. Wait for any running session to finish
4. Loop back - finished session may have unblocked new lanes
5. Exit when no ready lanes remain

## Failure handling

If a session fails, its lane cannot proceed. Dependent lanes
are also blocked. Independent lanes continue.

    aiki task lane {{data.target}} --all

If unrecoverable:

    aiki task stop {{id}} --reason "Failed: <reason>"

## Completion

When all lanes are complete:

    aiki task close {{id}} --summary "All lanes completed"
