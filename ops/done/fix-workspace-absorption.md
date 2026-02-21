 I found the root cause. There are actually two bugs working together:

  Bug 1: Wrong repo root in workspace_absorb_all

  In functions.rs:1239, workspace_absorb_all calls isolation::find_jj_root(&session_ws_dir) to find the repo root. But find_jj_root walks up looking for .jj/
  — and the isolated workspace itself has a .jj/ directory. So it returns the workspace path (e.g., ~/.aiki/workspaces/REPOID/SESSION/) instead of the actual
  repo root (/Users/glasner/code/aiki/).

  This means the rebase in absorb_workspace runs from the workspace directory, where @ resolves to the workspace's own working copy — making the rebase a
  no-op. Then cleanup_workspace deletes the directory, and the changes are lost.

  Bug 2: find_repo_root_from_workspace only handles symlinks

  The function that SHOULD find the real repo root (find_repo_root_from_workspace in isolation.rs:618) uses fs::read_link() — but modern JJ (0.38+) stores
  .jj/repo as a plain text file containing the path, not a symlink. So this function always returns None.

  The content of .jj/repo is literally: /Users/glasner/code/aiki/.jj/repo

  The net effect

  1. Agent writes files in isolated workspace
  2. Turn ends, workspace_absorb_all runs
  3. find_jj_root returns workspace path (wrong!) as repo_root
  4. Rebase runs jj rebase -b @ -d <ws_head> from workspace dir → rebases the workspace's own @ onto its parent (no-op)
  5. cleanup_workspace deletes the workspace directory — changes are lost forever
  6. Next turn, new workspace is created. Agent doesn't see its previous files.