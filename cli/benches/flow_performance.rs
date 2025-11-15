use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::process::Command;
use tempfile::TempDir;

/// Helper to initialize a JJ workspace for testing
fn init_jj_workspace() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();

    Command::new("jj")
        .arg("git")
        .arg("init")
        .arg("--colocate")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to initialize JJ workspace");

    temp_dir
}

/// Benchmark: Current provenance recording performance (baseline)
fn bench_current_provenance_recording(c: &mut Criterion) {
    let temp_dir = init_jj_workspace();
    let aiki_bin = env!("CARGO_BIN_EXE_aiki");

    c.bench_function("current_provenance_recording", |b| {
        b.iter(|| {
            // Simulate a PostToolUse hook call
            let payload = serde_json::json!({
                "session_id": "bench-session-123",
                "transcript_path": "/tmp/transcript.txt",
                "cwd": temp_dir.path().to_str().unwrap(),
                "hook_event_name": "PostToolUse",
                "tool_name": "Edit",
                "tool_input": {
                    "file_path": "test.rs",
                    "old_string": "old",
                    "new_string": "new"
                },
                "tool_output": "Success"
            });

            let mut child = Command::new(aiki_bin)
                .arg("hooks")
                .arg("handle")
                .arg("--agent")
                .arg("claude-code")
                .arg("--event")
                .arg("PostToolUse")
                .current_dir(temp_dir.path())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("Failed to spawn aiki");

            // Write JSON to stdin
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(payload.to_string().as_bytes()).unwrap();
            }

            let result = child.wait_with_output().unwrap();
            black_box(result);
        });
    });
}

/// Benchmark: Event bus dispatch overhead
fn bench_event_dispatch(c: &mut Criterion) {
    // This will be filled in after we implement the flow engine
    // For now, just a placeholder
    c.bench_function("event_dispatch_overhead", |b| {
        b.iter(|| {
            // Placeholder - will implement after flow engine is done
            black_box(1 + 1);
        });
    });
}

/// Benchmark: Flow parsing (will implement after parser is done)
fn bench_flow_parsing(c: &mut Criterion) {
    c.bench_function("flow_parsing", |b| {
        b.iter(|| {
            // Placeholder - will implement after parser is done
            black_box(1 + 1);
        });
    });
}

/// Benchmark: Variable interpolation (will implement after variable resolver is done)
fn bench_variable_interpolation(c: &mut Criterion) {
    c.bench_function("variable_interpolation", |b| {
        b.iter(|| {
            // Placeholder - will implement after variable resolver is done
            black_box(1 + 1);
        });
    });
}

criterion_group!(
    benches,
    bench_current_provenance_recording,
    bench_event_dispatch,
    bench_flow_parsing,
    bench_variable_interpolation
);
criterion_main!(benches);
