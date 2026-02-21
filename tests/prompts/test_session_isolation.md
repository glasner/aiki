You are testing aiki's workspace isolation system with concurrent agents. Run through these checks carefully and report results.

  ## Setup
  - Run `aiki task start "Test workspace isolation with concurrent agents" --source prompt`
  - Note the task ID as PARENT

  ## 1. Launch parallel agents
  Create 3 tasks that will run concurrently:

      aiki task add "Agent A: create file agent-a.txt containing 'written by agent A at <timestamp>'"
      aiki task add "Agent B: create file agent-b.txt containing 'written by agent B at <timestamp>'"
      aiki task add "Agent C: create file agent-c.txt containing 'written by agent C at <timestamp>'"

  Launch all three with --async:

      aiki task run <id-a> --async
      aiki task run <id-b> --async
      aiki task run <id-c> --async

  ## 2. Monitor workspaces while agents run
  While agents are running (before they finish), check:

      ls ~/.aiki/workspaces/

  - Verify there are workspace directories for each agent session
  - Run `jj workspace list` from the repo root — you should see the agent workspaces listed
  - Run `ls` inside each workspace dir to confirm agents are working in isolated copies

  ## 3. Wait and verify absorption

      aiki task wait <id-a> <id-b> <id-c>

  After all three finish:

  - Run `jj log -r ..@` to see absorbed changes
  - Verify there are 3 separate changes, one per agent
  - For each change, run `jj show <change-id>` and confirm:
    - The file diff is correct (agent-a.txt, agent-b.txt, agent-c.txt)
    - The description has an `[aiki]` metadata block with author, session, tool, task fields
    - Each change references a DIFFERENT session ID (proves isolation)
    - Each change references the correct task ID

  ## 4. Verify cleanup
  - Run `ls ~/.aiki/workspaces/` — agent workspace dirs should be gone
  - Run `jj workspace list` — only `default` should remain
  - Run `ls ~/.aiki/sessions/by-repo/` — agent session sidecars should be cleaned up

  ## 5. Verify no cross-contamination
  - Confirm each agent's file only appears in its own change (not bundled together)
  - Run `jj diff -r <change-for-agent-a>` and verify it ONLY contains agent-a.txt
  - Same for B and C

  ## 6. Cleanup
  - Delete agent-a.txt, agent-b.txt, agent-c.txt
  - Close your task: `aiki task close <PARENT> --summary "Results: ..."` noting which checks passed/failed

  Report: for each numbered section, state PASS or FAIL with details. Include any error output verbatim.