Here's the bug and fix:

  Bug: absorb_workspace rebases onto @- instead of @

  Location: cli/src/session/isolation.rs:170-285

  Root cause: Two issues compound to drop all file changes:

  Issue 1: Rebases onto wrong commit (the primary bug)

  Line 192:
  // Get the parent of the workspace's working copy (the last real commit)
  let parent_revset = format!("{}-", ws_change_id);

  This takes the workspace's @ (where changes live) and resolves its parent @-. Then line 256 rebases the target onto that parent:

  jj rebase -b @ -d <ws_head>   // ws_head = workspace's @-, NOT @

  In JJ, the working copy (@) IS a commit. When the agent writes files, JJ snapshots them into @. The parent @- is the clean base commit from before workspace
   creation. Rebasing onto @- means the file changes in @ are never picked up.

  Issue 2: No snapshot before absorption

  All JJ commands in absorb_workspace use --ignore-working-copy. This means files written to disk since the last JJ snapshot aren't captured into @. Even if
  Issue 1 were fixed, files written after the last implicit snapshot would still be lost.

  Fix

  Two changes to absorb_workspace:

  1. Trigger a snapshot before reading the workspace state — run a JJ command without --ignore-working-copy from the workspace directory (e.g., jj log -r @ -l
   1 or jj debug snapshot)
  2. Rebase onto @ directly instead of @- — replace format!("{}-", ws_change_id) with just ws_change_id. The workspace's @ contains the actual changes.

  // BEFORE (broken):
  let parent_revset = format!("{}-", ws_change_id);
  // ... resolves parent, rebases onto parent, skipping all changes in @

  // AFTER (fixed):
  // 1. Snapshot workspace working copy first
  jj_cmd().current_dir(&workspace.path).args(["debug", "snapshot"]).output()?;
  // 2. Use @ directly as rebase target
  let ws_head = ws_change_id;  // the working copy commit, which HAS the changes