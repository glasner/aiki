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
/// Results are saved to benchmarks/default/YYYY-MM-DD_HH-MM-SS/
pub fn run(num_edits: usize) -> Result<()> {
    let flow_name = "default"; // Future: make this a parameter

    println!("======================================");
    println!("  Aiki End-to-End Benchmark");
    println!("======================================");
    println!();
    println!("Configuration:");
    println!("  Flow: {}", flow_name);
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

    // Prepare results directory and load previous metrics
    let benchmark_dir = PathBuf::from(".aiki/benchmarks").join(flow_name);
    fs::create_dir_all(&benchmark_dir)?;
    let previous_metrics = load_previous_benchmark(&benchmark_dir)?;

    println!("Phase Timing:");
    for (phase, time) in &phase_times {
        println!("  {}: {:.3}s", phase, time);
    }
    println!("  Hot path average: {:.3}s", avg_hook);
    println!();
    println!("Total: {:.3}s", total_time);

    // Show comparison if previous benchmark exists
    if let Some(prev) = &previous_metrics {
        println!();
        println!("======================================");
        println!("Comparison to Previous Run");
        println!("======================================");
        println!();
        print_comparison(prev, total_time, avg_hook);
    }

    println!();

    // Save results to .aiki/benchmarks
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

    // Add comparison if previous benchmark exists
    if let Some(prev) = &previous_metrics {
        writeln!(results_file)?;
        writeln!(results_file, "Comparison to Previous Run:")?;
        writeln!(results_file, "=====================================")?;
        write_comparison(&mut results_file, prev, total_time, avg_hook)?;
    }

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

#[derive(serde::Deserialize)]
struct PreviousMetrics {
    total: f64,
    phases: PreviousPhases,
}

#[derive(serde::Deserialize)]
struct PreviousPhases {
    hot_path: HotPathMetrics,
}

#[derive(serde::Deserialize)]
struct HotPathMetrics {
    average: f64,
}

fn load_previous_benchmark(benchmark_dir: &PathBuf) -> Result<Option<PreviousMetrics>> {
    let latest_link = benchmark_dir.join("latest");

    if !latest_link.exists() {
        return Ok(None);
    }

    let metrics_file = latest_link.join("metrics.json");
    if !metrics_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&metrics_file)?;
    let metrics: PreviousMetrics = serde_json::from_str(&content).map_err(|e| {
        crate::error::AikiError::Other(anyhow::anyhow!("Failed to parse previous metrics: {}", e))
    })?;

    Ok(Some(metrics))
}

fn print_comparison(prev: &PreviousMetrics, current_total: f64, current_avg_hook: f64) {
    let total_diff = current_total - prev.total;
    let total_pct = (total_diff / prev.total) * 100.0;
    let total_symbol = if total_diff > 0.0 { "+" } else { "" };
    let total_indicator = if total_diff > 0.0 {
        "🔴"
    } else if total_diff < 0.0 {
        "🟢"
    } else {
        "⚪"
    };

    let hook_diff = current_avg_hook - prev.phases.hot_path.average;
    let hook_pct = (hook_diff / prev.phases.hot_path.average) * 100.0;
    let hook_symbol = if hook_diff > 0.0 { "+" } else { "" };
    let hook_indicator = if hook_diff > 0.0 {
        "🔴"
    } else if hook_diff < 0.0 {
        "🟢"
    } else {
        "⚪"
    };

    println!("  Total time:");
    println!("    Previous: {:.3}s", prev.total);
    println!("    Current:  {:.3}s", current_total);
    println!(
        "    Change:   {}{:.3}s ({}{:.1}%) {}",
        total_symbol, total_diff, total_symbol, total_pct, total_indicator
    );
    println!();
    println!("  Hook execution (avg):");
    println!("    Previous: {:.3}s", prev.phases.hot_path.average);
    println!("    Current:  {:.3}s", current_avg_hook);
    println!(
        "    Change:   {}{:.3}s ({}{:.1}%) {}",
        hook_symbol, hook_diff, hook_symbol, hook_pct, hook_indicator
    );
}

fn write_comparison(
    file: &mut fs::File,
    prev: &PreviousMetrics,
    current_total: f64,
    current_avg_hook: f64,
) -> Result<()> {
    let total_diff = current_total - prev.total;
    let total_pct = (total_diff / prev.total) * 100.0;
    let total_symbol = if total_diff > 0.0 { "+" } else { "" };

    let hook_diff = current_avg_hook - prev.phases.hot_path.average;
    let hook_pct = (hook_diff / prev.phases.hot_path.average) * 100.0;
    let hook_symbol = if hook_diff > 0.0 { "+" } else { "" };

    writeln!(file, "  Total time:")?;
    writeln!(file, "    Previous: {:.3}s", prev.total)?;
    writeln!(file, "    Current:  {:.3}s", current_total)?;
    writeln!(
        file,
        "    Change:   {}{:.3}s ({}{:.1}%)",
        total_symbol, total_diff, total_symbol, total_pct
    )?;
    writeln!(file)?;
    writeln!(file, "  Hook execution (avg):")?;
    writeln!(file, "    Previous: {:.3}s", prev.phases.hot_path.average)?;
    writeln!(file, "    Current:  {:.3}s", current_avg_hook)?;
    writeln!(
        file,
        "    Change:   {}{:.3}s ({}{:.1}%)",
        hook_symbol, hook_diff, hook_symbol, hook_pct
    )?;

    Ok(())
}
