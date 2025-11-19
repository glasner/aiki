use crate::error::Result;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

/// Run an end-to-end performance benchmark
///
/// This benchmarks the complete Aiki workflow:
/// 1. Repository initialization
/// 2. Simulated AI edits (hook execution)
/// 3. Query operations (blame, authors)
/// 4. Commit with co-authors
///
/// Results are saved to benchmarks/YYYY-MM-DD_HH-MM-SS/
pub fn run(num_edits: usize) -> Result<()> {
    println!("======================================");
    println!("  Aiki End-to-End Benchmark");
    println!("======================================");
    println!();
    println!("Configuration:");
    println!("  Number of edits: {}", num_edits);
    println!();

    // Create temporary repository
    let tmp_base = std::env::temp_dir().join(format!("aiki-benchmark-{}", std::process::id()));
    fs::create_dir_all(&tmp_base)?;
    let repo_path = tmp_base.join("benchmark-repo");
    fs::create_dir_all(&repo_path)?;

    // Track timing for each phase
    let mut phase_times = Vec::new();

    // Phase 1: Initialize repository
    println!("======================================");
    println!("Phase 1: Repository Initialization");
    println!("======================================");
    println!();

    let start = Instant::now();

    // Git init
    run_command(&repo_path, "git", &["init"])?;
    run_command(&repo_path, "git", &["config", "user.name", "Benchmark"])?;
    run_command(
        &repo_path,
        "git",
        &["config", "user.email", "bench@test.com"],
    )?;

    // Aiki init
    let aiki_exe = std::env::current_exe()?;
    run_command(&repo_path, aiki_exe.to_str().unwrap(), &["init", "--quiet"])?;

    let init_time = start.elapsed();
    println!("  ✓ Initialization: {:.3}s", init_time.as_secs_f64());
    println!();
    phase_times.push(("Initialization", init_time.as_secs_f64()));

    // Phase 2: Initial commit
    println!("======================================");
    println!("Phase 2: Initial Commit");
    println!("======================================");
    println!();

    let start = Instant::now();

    // Create test files
    let src_dir = repo_path.join("src");
    fs::create_dir_all(&src_dir)?;
    for i in 1..=num_edits {
        let file_path = src_dir.join(format!("file_{}.rs", i));
        fs::write(&file_path, format!("fn function_{}() {{}}\n", i))?;
    }

    run_command(&repo_path, "git", &["add", "."])?;
    run_command(&repo_path, "git", &["commit", "-m", "Initial commit"])?;

    let commit_time = start.elapsed();
    println!("  ✓ Initial commit: {:.3}s", commit_time.as_secs_f64());
    println!();
    phase_times.push(("Initial commit", commit_time.as_secs_f64()));

    // Phase 3: Hot path - simulate edits
    println!("======================================");
    println!("Phase 3: Hot Path - {} Edits", num_edits);
    println!("======================================");
    println!();

    let mut hook_times = Vec::new();
    let total_hook_start = Instant::now();

    for i in 1..=num_edits {
        let file_path = src_dir.join(format!("file_{}.rs", i));

        // Append a line to the file
        let mut file = fs::OpenOptions::new().append(true).open(&file_path)?;
        writeln!(file, "    println!(\"Edit {}\");", i)?;

        // Create hook payload
        let payload = serde_json::json!({
            "session_id": format!("benchmark-session-{}", i),
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": file_path.to_str().unwrap(),
                "cwd": repo_path.to_str().unwrap()
            },
            "cwd": repo_path.to_str().unwrap()
        });

        println!("  Edit {}/{}", i, num_edits);

        let start = Instant::now();

        // Run hook
        let mut child = Command::new(aiki_exe.to_str().unwrap())
            .current_dir(&repo_path)
            .args(&[
                "hooks",
                "handle",
                "--agent",
                "claude-code",
                "--event",
                "PostToolUse",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Write payload to stdin
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(serde_json::to_string(&payload).unwrap().as_bytes());
        }

        // Wait for hook to complete
        let _ = child.wait()?;

        let elapsed = start.elapsed();
        hook_times.push(elapsed.as_secs_f64());
        println!("    Hook execution: {:.3}s", elapsed.as_secs_f64());
    }

    let total_hook_time = total_hook_start.elapsed();

    // Calculate hook statistics
    let min_hook = hook_times
        .iter()
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(&0.0);
    let max_hook = hook_times
        .iter()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(&0.0);
    let avg_hook = hook_times.iter().sum::<f64>() / hook_times.len() as f64;

    println!();
    println!("Hook Statistics:");
    println!("  Total: {:.3}s", total_hook_time.as_secs_f64());
    println!("  Average: {:.3}s", avg_hook);
    println!("  Min: {:.3}s", min_hook);
    println!("  Max: {:.3}s", max_hook);
    println!();
    phase_times.push(("Hot path (total)", total_hook_time.as_secs_f64()));

    // Phase 4: Query operations
    println!("======================================");
    println!("Phase 4: Query Operations");
    println!("======================================");
    println!();

    let start = Instant::now();
    let blame_result = run_command(
        &repo_path,
        aiki_exe.to_str().unwrap(),
        &["blame", "src/file_1.rs"],
    );
    let blame_time = start.elapsed();

    match blame_result {
        Ok(_) => println!("  ✓ Blame query: {:.3}s", blame_time.as_secs_f64()),
        Err(e) => println!("  ⚠ Blame query skipped: {}", e),
    }

    let start = Instant::now();
    let authors_result = run_command(&repo_path, aiki_exe.to_str().unwrap(), &["authors"]);
    let authors_time = start.elapsed();

    match authors_result {
        Ok(_) => println!("  ✓ Authors query: {:.3}s", authors_time.as_secs_f64()),
        Err(e) => println!("  ⚠ Authors query skipped: {}", e),
    }
    println!();

    phase_times.push(("Blame query", blame_time.as_secs_f64()));
    phase_times.push(("Authors query", authors_time.as_secs_f64()));

    // Phase 5: Commit with co-authors
    println!("======================================");
    println!("Phase 5: Commit with Co-Authors");
    println!("======================================");
    println!();

    let start = Instant::now();
    run_command(&repo_path, "git", &["add", "."])?;
    run_command(
        &repo_path,
        "git",
        &[
            "commit",
            "-m",
            &format!("Add {} edits from Claude", num_edits),
        ],
    )?;
    let final_commit_time = start.elapsed();
    println!("  ✓ Commit: {:.3}s", final_commit_time.as_secs_f64());
    println!();
    phase_times.push(("Final commit", final_commit_time.as_secs_f64()));

    // Summary
    println!("======================================");
    println!("Summary");
    println!("======================================");
    println!();

    let total_time: f64 = phase_times.iter().map(|(_, t)| t).sum();

    println!("Phase Timing:");
    for (phase, time) in &phase_times {
        println!("  {}: {:.3}s", phase, time);
    }
    println!("  Hot path average: {:.3}s", avg_hook);
    println!();
    println!("Total: {:.3}s", total_time);
    println!();

    // Save results
    let benchmark_dir = PathBuf::from("benchmarks");
    fs::create_dir_all(&benchmark_dir)?;

    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let results_dir = benchmark_dir.join(timestamp.to_string());
    fs::create_dir_all(&results_dir)?;

    // Write results.txt
    let mut results_file = fs::File::create(results_dir.join("results.txt"))?;
    writeln!(results_file, "Aiki End-to-End Benchmark Results")?;
    writeln!(results_file, "Date: {}", chrono::Local::now())?;
    writeln!(results_file, "=====================================")?;
    writeln!(results_file)?;
    writeln!(results_file, "Configuration:")?;
    writeln!(results_file, "  Number of edits: {}", num_edits)?;
    writeln!(results_file)?;
    writeln!(results_file, "Phase Timing:")?;
    for (phase, time) in &phase_times {
        writeln!(results_file, "  {}: {:.3}s", phase, time)?;
    }
    writeln!(results_file, "  Hot path average: {:.3}s", avg_hook)?;
    writeln!(results_file, "  Hot path min: {:.3}s", min_hook)?;
    writeln!(results_file, "  Hot path max: {:.3}s", max_hook)?;
    writeln!(results_file)?;
    writeln!(results_file, "Total: {:.3}s", total_time)?;

    // Write metrics.json
    let metrics = serde_json::json!({
        "timestamp": timestamp.to_string(),
        "date": chrono::Local::now().to_rfc3339(),
        "config": {
            "num_edits": num_edits
        },
        "phases": {
            "initialization": phase_times[0].1,
            "initial_commit": phase_times[1].1,
            "hot_path": {
                "total": phase_times[2].1,
                "average": avg_hook,
                "min": min_hook,
                "max": max_hook
            },
            "blame_query": phase_times[3].1,
            "authors_query": phase_times[4].1,
            "final_commit": phase_times[5].1
        },
        "total": total_time
    });

    let metrics_json = serde_json::to_string_pretty(&metrics).map_err(|e| {
        crate::error::AikiError::Other(anyhow::anyhow!("Failed to serialize metrics: {}", e))
    })?;
    fs::write(results_dir.join("metrics.json"), metrics_json)?;

    // Create latest symlink
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let latest_link = benchmark_dir.join("latest");
        let _ = fs::remove_file(&latest_link); // Ignore error if doesn't exist
        symlink(timestamp.to_string(), latest_link)?;
    }

    println!("✓ Benchmark complete!");
    println!();
    println!("Results saved to:");
    println!("  {}/results.txt", results_dir.display());
    println!("  {}/metrics.json", results_dir.display());
    println!();

    // Clean up temporary directory
    let _ = fs::remove_dir_all(&tmp_base);

    Ok(())
}

fn run_command(cwd: &PathBuf, program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program).current_dir(cwd).args(args).output()?;

    if !output.status.success() {
        return Err(crate::error::AikiError::Other(anyhow::anyhow!(
            "Command failed: {} {}\nStderr: {}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(())
}
