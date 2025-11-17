use aiki::events::{AikiEvent, AikiEventType};
use aiki::provenance::AgentType;
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

/// Benchmark: Flow parsing
fn bench_flow_parsing(c: &mut Criterion) {
    let flow_yaml = r#"
name: "Test Flow"
version: "1"

PostChange:
  - let: description = self.build_description
    on_failure: stop
  - jj: describe -m "$description"
  - jj: new
  - log: "Recorded change"
"#;

    c.bench_function("flow_parsing", |b| {
        b.iter(|| {
            use aiki::flows::FlowParser;
            let flow = FlowParser::parse_str(black_box(flow_yaml)).unwrap();
            black_box(flow);
        });
    });
}

/// Benchmark: Variable interpolation
fn bench_variable_interpolation(c: &mut Criterion) {
    use aiki::flows::VariableResolver;
    use std::collections::HashMap;

    let mut resolver = VariableResolver::new();
    let mut event_vars = HashMap::new();
    event_vars.insert("file_path".to_string(), "/path/to/file.rs".to_string());
    event_vars.insert("agent".to_string(), "ClaudeCode".to_string());
    event_vars.insert("session_id".to_string(), "test-session-123".to_string());

    for (key, value) in &event_vars {
        resolver.add_var(format!("event.{}", key), value.clone());
    }

    resolver.add_var("cwd", "/home/user/project".to_string());

    c.bench_function("variable_interpolation", |b| {
        b.iter(|| {
            let input = "File $event.file_path modified by $event.agent in $cwd (session: $event.session_id)";
            let result = resolver.resolve(black_box(input));
            black_box(result);
        });
    });
}

/// Benchmark: Let action execution (variable aliasing only)
fn bench_let_action_execution(c: &mut Criterion) {
    use aiki::flows::{Action, AikiState, FailureMode, FlowExecutor, LetAction};

    let actions = vec![
        Action::Let(LetAction {
            let_: "file = $event.file_path".to_string(),
            on_failure: FailureMode::Continue,
        }),
        Action::Let(LetAction {
            let_: "copy = $file".to_string(),
            on_failure: FailureMode::Continue,
        }),
    ];

    c.bench_function("let_action_execution", |b| {
        b.iter(|| {
            let event = AikiEvent::new(AikiEventType::PostChange, AgentType::ClaudeCode, "/tmp")
                .with_metadata("file_path", "test.rs");
            let mut context = AikiState::new(event);

            let results = FlowExecutor::execute_actions(black_box(&actions), &mut context).unwrap();
            black_box(results);
        });
    });
}

/// Benchmark: Full provenance flow with let syntax (including JJ commands)
fn bench_provenance_flow_with_let(c: &mut Criterion) {
    let temp_dir = init_jj_workspace();

    // Create a test file to track
    std::fs::write(temp_dir.path().join("test.rs"), "fn main() {}").unwrap();

    // Add it to JJ
    Command::new("jj")
        .arg("file")
        .arg("track")
        .arg("test.rs")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to track file");

    use aiki::flows::{Action, AikiState, FailureMode, FlowExecutor, JjAction, LetAction};

    let actions = vec![
        // Let action to call build_description function
        Action::Let(LetAction {
            let_: "description = aiki/provenance.build_description".to_string(),
            on_failure: FailureMode::Stop,
        }),
        // JJ action to update change description
        Action::Jj(JjAction {
            jj: "describe -m \"$description\"".to_string(),
            timeout: None,
            on_failure: FailureMode::Stop,
            alias: None,
        }),
    ];

    c.bench_function("provenance_flow_with_let", |b| {
        b.iter(|| {
            let event = AikiEvent::new(
                AikiEventType::PostChange,
                AgentType::ClaudeCode,
                temp_dir.path(),
            )
            .with_session_id("test-session-123")
            .with_metadata("agent", "claude-code")
            .with_metadata("session_id", "test-session-123")
            .with_metadata("tool_name", "Edit")
            .with_metadata("file_path", "test.rs");
            let mut context = AikiState::new(event);

            let results = FlowExecutor::execute_actions(black_box(&actions), &mut context);
            black_box(results);
        });
    });
}

criterion_group!(
    benches,
    bench_current_provenance_recording,
    bench_event_dispatch,
    bench_flow_parsing,
    bench_variable_interpolation,
    bench_let_action_execution,
    bench_provenance_flow_with_let
);
criterion_main!(benches);
