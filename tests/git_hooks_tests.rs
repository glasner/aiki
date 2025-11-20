use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Helper to run aiki init in a directory
fn run_aiki_init(dir: &Path) -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .arg("init")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("aiki init failed: {}", stderr);
    }

    Ok(())
}

// ========================================
// Multi-Editor Test Scenarios
// ========================================

#[test]
fn test_git_hook_includes_multiple_editors() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Initialize JJ repo for provenance tracking
    Command::new("jj")
        .args(["git", "init", "--no-colocate"])
        .current_dir(temp_dir.path())
        .output()?;

    // Create file with Claude Code attribution
    let file1 = temp_dir.path().join("file1.txt");
    fs::write(&file1, "Line 1\n")?;
    Command::new("jj")
        .args([
            "describe",
            "-m",
            "[aiki]\nagent=claude-code\nsession=claude-session-1\ntool=Edit\nconfidence=High\nmethod=Hook\n[/aiki]",
        ])
        .current_dir(temp_dir.path())
        .output()?;

    // Create new change for Cursor
    Command::new("jj")
        .args(["new"])
        .current_dir(temp_dir.path())
        .output()?;

    // Modify same file with Cursor attribution
    fs::write(&file1, "Line 1\nLine 2 from Cursor\n")?;
    Command::new("jj")
        .args([
            "describe",
            "-m",
            "[aiki]\nagent=cursor\nsession=cursor-session-1\ntool=Edit\nconfidence=High\nmethod=Hook\n[/aiki]",
        ])
        .current_dir(temp_dir.path())
        .output()?;

    // Stage the file in Git
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    // Run aiki authors with git format
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .args(["authors", "--format=git", "--changes=staged"])
        .current_dir(temp_dir.path())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should include Cursor as co-author (the current change)
    assert!(
        stdout.contains("Co-Authored-By: Cursor <noreply@cursor.com>"),
        "Should include Cursor co-author: {}",
        stdout
    );

    Ok(())
}

#[test]
fn test_git_hook_deduplicates_same_editor() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Initialize JJ repo
    Command::new("jj")
        .args(["git", "init", "--no-colocate"])
        .current_dir(temp_dir.path())
        .output()?;

    // Create first file with Claude Code
    let file1 = temp_dir.path().join("file1.txt");
    fs::write(&file1, "First change\n")?;
    Command::new("jj")
        .args([
            "describe",
            "-m",
            "[aiki]\nagent=claude-code\nsession=session-1\ntool=Edit\nconfidence=High\nmethod=Hook\n[/aiki]",
        ])
        .current_dir(temp_dir.path())
        .output()?;

    // Create new change with same editor (different session)
    Command::new("jj")
        .args(["new"])
        .current_dir(temp_dir.path())
        .output()?;

    let file2 = temp_dir.path().join("file2.txt");
    fs::write(&file2, "Second change\n")?;
    Command::new("jj")
        .args([
            "describe",
            "-m",
            "[aiki]\nagent=claude-code\nsession=session-2\ntool=Edit\nconfidence=High\nmethod=Hook\n[/aiki]",
        ])
        .current_dir(temp_dir.path())
        .output()?;

    // Stage both files
    Command::new("git")
        .args(["add", "file1.txt", "file2.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    // Run aiki authors
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .args(["authors", "--format=git", "--changes=staged"])
        .current_dir(temp_dir.path())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should only have ONE co-author line (deduplicated by email)
    let coauthor_count = stdout.matches("Co-Authored-By: Claude").count();
    assert_eq!(
        coauthor_count, 1,
        "Should deduplicate same editor to one co-author: {}",
        stdout
    );

    Ok(())
}

#[test]
fn test_git_authors_json_format_multiple_editors() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Initialize JJ repo
    Command::new("jj")
        .args(["git", "init", "--no-colocate"])
        .current_dir(temp_dir.path())
        .output()?;

    // Create file with Claude Code
    let file1 = temp_dir.path().join("file1.txt");
    fs::write(&file1, "Line 1\n")?;
    Command::new("jj")
        .args([
            "describe",
            "-m",
            "[aiki]\nagent=claude-code\nsession=claude-1\ntool=Edit\nconfidence=High\nmethod=Hook\n[/aiki]",
        ])
        .current_dir(temp_dir.path())
        .output()?;

    // Create new change with Cursor
    Command::new("jj")
        .args(["new"])
        .current_dir(temp_dir.path())
        .output()?;

    fs::write(&file1, "Line 1\nLine 2 by Cursor\n")?;
    Command::new("jj")
        .args([
            "describe",
            "-m",
            "[aiki]\nagent=cursor\nsession=cursor-1\ntool=Edit\nconfidence=High\nmethod=Hook\n[/aiki]",
        ])
        .current_dir(temp_dir.path())
        .output()?;

    // Stage the file
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    // Run authors with JSON format
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .args(["authors", "--format=json", "--changes=staged"])
        .current_dir(temp_dir.path())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON with Cursor
    assert!(stdout.contains("Cursor"), "Should include Cursor");
    assert!(
        stdout.contains("noreply@cursor.com"),
        "Should include Cursor email"
    );

    // Verify it's valid JSON
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");

    Ok(())
}

#[test]
fn test_git_hook_only_shows_ai_contributors() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Initialize JJ repo
    Command::new("jj")
        .args(["git", "init", "--no-colocate"])
        .current_dir(temp_dir.path())
        .output()?;

    // Create file with AI attribution
    let ai_file = temp_dir.path().join("ai_file.txt");
    fs::write(&ai_file, "AI change\n")?;
    Command::new("jj")
        .args([
            "describe",
            "-m",
            "[aiki]\nagent=cursor\nsession=cursor-1\ntool=Edit\nconfidence=High\nmethod=Hook\n[/aiki]",
        ])
        .current_dir(temp_dir.path())
        .output()?;

    // Create new change without AI attribution (human change)
    Command::new("jj")
        .args(["new"])
        .current_dir(temp_dir.path())
        .output()?;

    let human_file = temp_dir.path().join("human_file.txt");
    fs::write(&human_file, "Human change\n")?;
    Command::new("jj")
        .args(["describe", "-m", "Manual change by human"])
        .current_dir(temp_dir.path())
        .output()?;

    // Stage only the human file (from working copy)
    Command::new("git")
        .args(["add", "human_file.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    // Run authors
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .args(["authors", "--format=git", "--changes=staged"])
        .current_dir(temp_dir.path())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be empty since human_file has no AI attribution
    // This test verifies that human changes (without [aiki] metadata) are filtered out
    assert!(
        !stdout.contains("Unknown"),
        "Should not include Unknown agent: {}",
        stdout
    );

    // Should have zero co-author lines (no AI contributors in staged changes)
    let coauthor_count = stdout.matches("Co-authored-by:").count();
    assert_eq!(
        coauthor_count, 0,
        "Should have zero AI co-authors for human changes: {}",
        stdout
    );

    Ok(())
}

/// Helper to initialize a git repository
fn init_git_repo(dir: &Path) -> Result<()> {
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()?;

    // Configure git user for commits
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir)
        .output()?;

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output()?;

    Ok(())
}

#[test]
fn test_git_hooks_installation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;

    // Run aiki init
    run_aiki_init(temp_dir.path())?;

    // Verify git config points to global hooks
    let output = Command::new("git")
        .args(["config", "core.hooksPath"])
        .current_dir(temp_dir.path())
        .output()?;

    let hooks_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        hooks_path.contains(".aiki/githooks"),
        "core.hooksPath should point to global .aiki/githooks, got: {}",
        hooks_path
    );

    // Verify global hook exists (hooks should be installed via `aiki hooks install`)
    let home_dir = dirs::home_dir().expect("home directory should exist");
    let global_hook_file = home_dir.join(".aiki/githooks/prepare-commit-msg");
    if global_hook_file.exists() {
        // Verify hook is executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&global_hook_file)?;
            let permissions = metadata.permissions();
            assert!(permissions.mode() & 0o111 != 0, "Hook should be executable");
        }
    }

    Ok(())
}

#[test]
fn test_previous_hooks_path_saved() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;

    // Set a custom hooks path before running aiki init
    Command::new("git")
        .args(["config", "core.hooksPath", ".custom-hooks"])
        .current_dir(temp_dir.path())
        .output()?;

    // Run aiki init
    run_aiki_init(temp_dir.path())?;

    // Verify previous hooks path was saved
    let previous_path_file = temp_dir.path().join(".aiki/.previous_hooks_path");
    assert!(
        previous_path_file.exists(),
        ".previous_hooks_path file should exist"
    );

    let saved_path = fs::read_to_string(&previous_path_file)?;
    assert_eq!(
        saved_path, ".custom-hooks",
        "Previous hooks path should be saved"
    );

    // In the new architecture, the global hook dynamically reads .aiki/.previous_hooks_path
    // So we verify the global hook exists and has the dynamic lookup
    let home_dir = dirs::home_dir().expect("home directory should exist");
    let hook_file = home_dir.join(".aiki/githooks/prepare-commit-msg");
    if hook_file.exists() {
        let hook_content = fs::read_to_string(&hook_file)?;
        assert!(
            hook_content.contains(".aiki/.previous_hooks_path"),
            "Hook should dynamically read previous hooks path from .aiki/.previous_hooks_path"
        );
    }

    Ok(())
}

#[test]
fn test_no_previous_hooks_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;

    // Don't set any hooks path
    // Run aiki init
    run_aiki_init(temp_dir.path())?;

    // In the new architecture, .aiki directory is only created if there's a previous hooks path
    // When there's no previous hooks path set, .aiki won't be created
    let previous_path_file = temp_dir.path().join(".aiki/.previous_hooks_path");
    assert!(
        !previous_path_file.exists(),
        ".previous_hooks_path file should not exist when there's no previous hooks"
    );

    // Verify git config points to global hooks
    let output = Command::new("git")
        .args(["config", "core.hooksPath"])
        .current_dir(temp_dir.path())
        .output()?;
    let hooks_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        hooks_path.contains(".aiki/githooks"),
        "Should point to global hooks"
    );

    Ok(())
}

#[test]
fn test_git_coauthors_command_with_no_staged_changes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Run aiki authors with no staged changes
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .args(["authors", "--format=git", "--changes=staged"])
        .current_dir(temp_dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "authors should succeed with no staged changes"
    );

    // Should output nothing when there are no staged changes
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "Should output nothing with no staged changes"
    );

    Ok(())
}

#[test]
fn test_git_coauthors_command_with_new_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Create and stage a new file
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "Hello, world!\n")?;

    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    // Run aiki authors
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .args(["authors", "--format=git", "--changes=staged"])
        .current_dir(temp_dir.path())
        .output()?;

    // New files won't have AI attribution, so output should be empty
    // (unless they were created by an AI agent through aiki record-change)
    // This is expected to be empty since we manually created the file
    // In a real workflow, AI-created files would have provenance
    assert!(output.status.success(), "authors should succeed");

    Ok(())
}

#[test]
fn test_hook_template_embedded() -> Result<()> {
    // Verify the global hook file contains expected content
    let home_dir = dirs::home_dir().expect("home directory should exist");
    let hook_file = home_dir.join(".aiki/githooks/prepare-commit-msg");

    if !hook_file.exists() {
        // Skip test if global hooks not installed
        eprintln!("Skipping test: global hooks not installed. Run `aiki hooks install` first.");
        return Ok(());
    }

    let hook_content = fs::read_to_string(&hook_file)?;

    // Check for key parts of the hook script
    assert!(
        hook_content.contains("#!/usr/bin/env bash"),
        "Should have bash shebang"
    );
    assert!(
        hook_content.contains("aiki authors"),
        "Should call aiki authors"
    );
    assert!(
        hook_content.contains("COMMIT_MSG_FILE"),
        "Should use COMMIT_MSG_FILE variable"
    );
    assert!(
        hook_content.contains("Co-authored-by"),
        "Should mention co-authors in comments"
    );

    Ok(())
}

#[test]
fn test_aiki_init_is_idempotent() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;

    // Run aiki init twice
    run_aiki_init(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Verify git config is still correct after second init
    let output = Command::new("git")
        .args(["config", "core.hooksPath"])
        .current_dir(temp_dir.path())
        .output()?;

    let hooks_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        hooks_path.contains(".aiki/githooks"),
        "core.hooksPath should still point to global hooks"
    );

    Ok(())
}

#[test]
fn test_hook_runs_for_normal_commits() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Create and stage a file
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "Test content\n")?;
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    // Commit with -m flag (this sets COMMIT_SOURCE=message)
    let output = Command::new("git")
        .args(["commit", "-m", "Test commit"])
        .current_dir(temp_dir.path())
        .output()?;

    // The hook should run (even though there are no co-authors in this case)
    assert!(
        output.status.success(),
        "Commit should succeed with hook running"
    );

    // Verify the commit was created
    let log_output = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(temp_dir.path())
        .output()?;

    let log = String::from_utf8_lossy(&log_output.stdout);
    assert!(log.contains("Test commit"), "Commit should be in history");

    Ok(())
}

#[test]
fn test_hook_skips_merge_commits() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Create initial commit
    let file1 = temp_dir.path().join("file1.txt");
    fs::write(&file1, "Content 1\n")?;
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(temp_dir.path())
        .output()?;
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp_dir.path())
        .output()?;

    // Create a branch
    Command::new("git")
        .args(["checkout", "-b", "feature"])
        .current_dir(temp_dir.path())
        .output()?;

    // Add commit on branch
    let file2 = temp_dir.path().join("file2.txt");
    fs::write(&file2, "Content 2\n")?;
    Command::new("git")
        .args(["add", "file2.txt"])
        .current_dir(temp_dir.path())
        .output()?;
    Command::new("git")
        .args(["commit", "-m", "Feature commit"])
        .current_dir(temp_dir.path())
        .output()?;

    // Go back to main and merge (this sets COMMIT_SOURCE=merge)
    Command::new("git")
        .args(["checkout", "master"])
        .current_dir(temp_dir.path())
        .output()?;

    let merge_output = Command::new("git")
        .args(["merge", "feature", "--no-ff", "-m", "Merge feature"])
        .current_dir(temp_dir.path())
        .output()?;

    // Merge should succeed and hook should skip
    assert!(merge_output.status.success(), "Merge commit should succeed");

    Ok(())
}

#[test]
fn test_hook_chains_to_existing_hook() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;

    // Create a pre-existing hook in .git/hooks
    let git_hooks_dir = temp_dir.path().join(".git/hooks");
    fs::create_dir_all(&git_hooks_dir)?;
    let existing_hook = git_hooks_dir.join("prepare-commit-msg");

    // Create a simple hook that adds a comment
    let hook_script = "#!/usr/bin/env bash\necho '# Existing hook was here' >> \"$1\"\n";
    fs::write(&existing_hook, hook_script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&existing_hook)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&existing_hook, perms)?;
    }

    // Set git to use .git/hooks initially
    Command::new("git")
        .args(["config", "core.hooksPath", ".git/hooks"])
        .current_dir(temp_dir.path())
        .output()?;

    // Now run aiki init (this should save the previous hooks path)
    run_aiki_init(temp_dir.path())?;

    // Verify .aiki/.previous_hooks_path was created since there was a previous hooks path
    let previous_path_file = temp_dir.path().join(".aiki/.previous_hooks_path");
    assert!(
        previous_path_file.exists(),
        ".previous_hooks_path should exist when there's a custom hooks path"
    );

    let saved_path = fs::read_to_string(&previous_path_file)?;
    assert_eq!(
        saved_path, ".git/hooks",
        "Should save the previous hooks path"
    );

    // Create and commit a file
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "Test\n")?;
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    let commit_output = Command::new("git")
        .args(["commit", "-m", "Test chaining"])
        .current_dir(temp_dir.path())
        .output()?;

    assert!(
        commit_output.status.success(),
        "Commit should succeed with chained hooks"
    );

    // Check the commit message includes the existing hook's output
    let show_output = Command::new("git")
        .args(["show", "--format=%B", "-s"])
        .current_dir(temp_dir.path())
        .output()?;

    let commit_msg = String::from_utf8_lossy(&show_output.stdout);
    assert!(
        commit_msg.contains("Existing hook was here"),
        "Existing hook should have been called: {}",
        commit_msg
    );

    Ok(())
}

#[test]
fn test_git_diff_handles_color_config() -> Result<()> {
    let temp_dir = TempDir::new()?;
    init_git_repo(temp_dir.path())?;
    run_aiki_init(temp_dir.path())?;

    // Set color.diff to always (which could break parsing without --no-color)
    Command::new("git")
        .args(["config", "color.diff", "always"])
        .current_dir(temp_dir.path())
        .output()?;

    // Create and stage a file
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "Hello, world!\n")?;
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(temp_dir.path())
        .output()?;

    // Run authors - should not fail due to color codes
    let output = Command::new(env!("CARGO_BIN_EXE_aiki"))
        .args(["authors", "--format=git", "--changes=staged"])
        .current_dir(temp_dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "authors should handle color.diff=always: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}
