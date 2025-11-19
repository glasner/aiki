use crate::error::Result;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

/// Event-level timing information
#[derive(Debug, Clone)]
struct EventTiming {
    event_name: String,
    timings: Vec<f64>,
}

impl EventTiming {
    fn new(event_name: String) -> Self {
        Self {
            event_name,
            timings: Vec::new(),
        }
    }

    fn add_timing(&mut self, duration_secs: f64) {
        self.timings.push(duration_secs);
    }

    fn median(&self) -> f64 {
        if self.timings.is_empty() {
            return 0.0;
        }
        let mut sorted = self.timings.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[sorted.len() / 2]
    }

    fn min(&self) -> f64 {
        self.timings
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap_or(0.0)
    }

    fn max(&self) -> f64 {
        self.timings
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap_or(0.0)
    }

    fn count(&self) -> usize {
        self.timings.len()
    }
}

/// Run an end-to-end performance benchmark
///
/// This benchmarks the complete Aiki workflow:
/// 1. Repository initialization
/// 2. Simulated AI edits (hook execution)
/// 3. Query operations (blame, authors)
/// 4. Commit with co-authors
///
/// Results are saved to .aiki/benchmarks/{flow}/YYYY-MM-DD_HH-MM-SS/
pub fn run(flow_name: String, num_edits: usize) -> Result<()> {
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

    // Track timing for each phase and events
    let mut phase_times = Vec::new();
    let mut event_timings: HashMap<String, EventTiming> = HashMap::new();

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

    let git_init_time = start.elapsed();

    // Aiki init (includes SessionStart event)
    let aiki_exe = std::env::current_exe()?;
    let aiki_init_start = Instant::now();
    run_command(&repo_path, aiki_exe.to_str().unwrap(), &["init", "--quiet"])?;
    let aiki_init_time = aiki_init_start.elapsed();

    // Track SessionStart event timing
    event_timings
        .entry("SessionStart".to_string())
        .or_insert_with(|| EventTiming::new("SessionStart".to_string()))
        .add_timing(aiki_init_time.as_secs_f64());

    let init_time = start.elapsed();
    println!(
        "  ✓ Git init: {:.1}ms",
        git_init_time.as_secs_f64() * 1000.0
    );
    println!(
        "  ✓ SessionStart: {:.1}ms",
        aiki_init_time.as_secs_f64() * 1000.0
    );
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
    println!(
        "  ✓ Initial commit: {:.1}ms",
        commit_time.as_secs_f64() * 1000.0
    );
    println!();
    phase_times.push(("Initial commit", commit_time.as_secs_f64()));

    // Phase 3: Hot path - simulate edits
    println!("======================================");
    println!("Phase 3: Hot Path - {} Edits", num_edits);
    println!("======================================");
    println!();

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

        // Track timing by event type
        event_timings
            .entry("PostChange".to_string())
            .or_insert_with(|| EventTiming::new("PostChange".to_string()))
            .add_timing(elapsed.as_secs_f64());

        println!("    PostChange: {:.1}ms", elapsed.as_secs_f64() * 1000.0);
    }

    let total_hook_time = total_hook_start.elapsed();

    // Display event-level statistics
    println!();
    println!("Event Timing:");
    for (event_name, timing) in &event_timings {
        println!("  {} ({} occurrences):", event_name, timing.count());
        println!("    Median: {:.1}ms", timing.median() * 1000.0);
        println!(
            "    Range: {:.1}ms - {:.1}ms",
            timing.min() * 1000.0,
            timing.max() * 1000.0
        );
    }
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
        Ok(_) => println!(
            "  ✓ Blame query: {:.1}ms",
            blame_time.as_secs_f64() * 1000.0
        ),
        Err(e) => println!("  ⚠ Blame query skipped: {}", e),
    }

    let start = Instant::now();
    let authors_result = run_command(&repo_path, aiki_exe.to_str().unwrap(), &["authors"]);
    let authors_time = start.elapsed();

    match authors_result {
        Ok(_) => println!(
            "  ✓ Authors query: {:.1}ms",
            authors_time.as_secs_f64() * 1000.0
        ),
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
    println!(
        "  ✓ Commit: {:.1}ms",
        final_commit_time.as_secs_f64() * 1000.0
    );
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
    println!();

    println!("Event Timing Summary:");
    for (event_name, timing) in &event_timings {
        println!("  {} ({} occurrences):", event_name, timing.count());
        println!("    Median: {:.1}ms", timing.median() * 1000.0);
        println!(
            "    Range: {:.1}ms - {:.1}ms",
            timing.min() * 1000.0,
            timing.max() * 1000.0
        );
    }
    println!();
    println!("Total: {:.3}s", total_time);

    // Show comparison if previous benchmark exists
    if let Some(prev) = &previous_metrics {
        println!();
        println!("======================================");
        println!("Comparison to Previous Run");
        println!("======================================");
        println!();
        print_comparison(prev, total_time, &event_timings);
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
    writeln!(results_file)?;
    writeln!(results_file, "Event Timing:")?;
    for (event_name, timing) in &event_timings {
        writeln!(
            results_file,
            "  {} ({} occurrences):",
            event_name,
            timing.count()
        )?;
        writeln!(
            results_file,
            "    Median: {:.1}ms",
            timing.median() * 1000.0
        )?;
        writeln!(
            results_file,
            "    Range: {:.1}ms - {:.1}ms",
            timing.min() * 1000.0,
            timing.max() * 1000.0
        )?;
    }
    writeln!(results_file)?;
    writeln!(results_file, "Total: {:.3}s", total_time)?;

    // Add comparison if previous benchmark exists
    if let Some(prev) = &previous_metrics {
        writeln!(results_file)?;
        writeln!(results_file, "Comparison to Previous Run:")?;
        writeln!(results_file, "=====================================")?;
        write_comparison(&mut results_file, prev, total_time, &event_timings)?;
    }

    // Write metrics.json with event-level data
    let mut events_json = serde_json::Map::new();
    for (event_name, timing) in &event_timings {
        events_json.insert(
            event_name.clone(),
            serde_json::json!({
                "count": timing.count(),
                "median": timing.median(),
                "min": timing.min(),
                "max": timing.max()
            }),
        );
    }

    let metrics = serde_json::json!({
        "timestamp": timestamp.to_string(),
        "date": chrono::Local::now().to_rfc3339(),
        "config": {
            "num_edits": num_edits
        },
        "phases": {
            "initialization": phase_times[0].1,
            "initial_commit": phase_times[1].1,
            "hot_path": phase_times[2].1,
            "blame_query": phase_times[3].1,
            "authors_query": phase_times[4].1,
            "final_commit": phase_times[5].1
        },
        "events": events_json,
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
    #[serde(default)]
    events: HashMap<String, PreviousEventMetrics>,
}

#[derive(serde::Deserialize, Clone)]
struct PreviousEventMetrics {
    median: f64,
    #[serde(default)]
    count: usize,
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

fn print_comparison(
    prev: &PreviousMetrics,
    current_total: f64,
    current_events: &HashMap<String, EventTiming>,
) {
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

    println!("  Total time:");
    println!("    Previous: {:.3}s", prev.total);
    println!("    Current:  {:.3}s", current_total);
    println!(
        "    Change:   {}{:.3}s ({}{:.1}%) {}",
        total_symbol, total_diff, total_symbol, total_pct, total_indicator
    );
    println!();

    // Compare event-level timings
    println!("  Event-level comparison:");
    for (event_name, current_timing) in current_events {
        if let Some(prev_event) = prev.events.get(event_name) {
            let event_diff = current_timing.median() - prev_event.median;
            let event_pct = (event_diff / prev_event.median) * 100.0;
            let event_symbol = if event_diff > 0.0 { "+" } else { "" };
            let event_indicator = if event_diff > 0.0 {
                "🔴"
            } else if event_diff < 0.0 {
                "🟢"
            } else {
                "⚪"
            };

            println!("    {}:", event_name);
            println!(
                "      Previous: {:.1}ms (median)",
                prev_event.median * 1000.0
            );
            println!(
                "      Current:  {:.1}ms (median)",
                current_timing.median() * 1000.0
            );
            println!(
                "      Change:   {}{:.1}ms ({}{:.1}%) {}",
                event_symbol,
                event_diff * 1000.0,
                event_symbol,
                event_pct,
                event_indicator
            );
        } else {
            println!(
                "    {} (new): {:.1}ms (median)",
                event_name,
                current_timing.median() * 1000.0
            );
        }
    }
}

fn write_comparison(
    file: &mut fs::File,
    prev: &PreviousMetrics,
    current_total: f64,
    current_events: &HashMap<String, EventTiming>,
) -> Result<()> {
    let total_diff = current_total - prev.total;
    let total_pct = (total_diff / prev.total) * 100.0;
    let total_symbol = if total_diff > 0.0 { "+" } else { "" };

    writeln!(file, "  Total time:")?;
    writeln!(file, "    Previous: {:.3}s", prev.total)?;
    writeln!(file, "    Current:  {:.3}s", current_total)?;
    writeln!(
        file,
        "    Change:   {}{:.3}s ({}{:.1}%)",
        total_symbol, total_diff, total_symbol, total_pct
    )?;
    writeln!(file)?;

    writeln!(file, "  Event-level comparison:")?;
    for (event_name, current_timing) in current_events {
        if let Some(prev_event) = prev.events.get(event_name) {
            let event_diff = current_timing.median() - prev_event.median;
            let event_pct = (event_diff / prev_event.median) * 100.0;
            let event_symbol = if event_diff > 0.0 { "+" } else { "" };

            writeln!(file, "    {}:", event_name)?;
            writeln!(
                file,
                "      Previous: {:.1}ms (median)",
                prev_event.median * 1000.0
            )?;
            writeln!(
                file,
                "      Current:  {:.1}ms (median)",
                current_timing.median() * 1000.0
            )?;
            writeln!(
                file,
                "      Change:   {}{:.1}ms ({}{:.1}%)",
                event_symbol,
                event_diff * 1000.0,
                event_symbol,
                event_pct
            )?;
        } else {
            writeln!(
                file,
                "    {} (new): {:.1}ms (median)",
                event_name,
                current_timing.median() * 1000.0
            )?;
        }
    }

    Ok(())
}
