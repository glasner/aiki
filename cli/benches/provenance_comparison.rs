use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Initialize a JJ workspace with a tracked file
fn setup_workspace() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();

    // Initialize JJ workspace
    Command::new("jj")
        .arg("git")
        .arg("init")
        .arg("--colocate")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to initialize JJ workspace");

    // Create and track a test file
    std::fs::write(temp_dir.path().join("test.rs"), "fn main() {}").unwrap();
    Command::new("jj")
        .arg("file")
        .arg("track")
        .arg("test.rs")
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to track file");

    temp_dir
}

/// Benchmark: OLD approach using AikiAction
fn bench_old_aiki_action(c: &mut Criterion) {
    use aiki::flows::{Action, AikiAction, ExecutionContext, FailureMode, FlowExecutor};
    use std::collections::HashMap;

    let temp_dir = setup_workspace();

    let mut args = HashMap::new();
    args.insert("agent".to_string(), "$event.agent".to_string());
    args.insert("session_id".to_string(), "$event.session_id".to_string());
    args.insert("tool_name".to_string(), "$event.tool_name".to_string());
    args.insert("file_path".to_string(), "$event.file_path".to_string());

    let actions = vec![Action::Aiki(AikiAction {
        aiki: "build_provenance_description".to_string(),
        args,
        on_failure: FailureMode::Fail,
    })];

    c.bench_function("old_aiki_action", |b| {
        b.iter(|| {
            let mut context = ExecutionContext::new(PathBuf::from(temp_dir.path()));
            context
                .event_vars
                .insert("agent".to_string(), "claude-code".to_string());
            context
                .event_vars
                .insert("session_id".to_string(), "test-session".to_string());
            context
                .event_vars
                .insert("tool_name".to_string(), "Edit".to_string());
            context
                .event_vars
                .insert("file_path".to_string(), "test.rs".to_string());

            let results = FlowExecutor::execute_actions(black_box(&actions), &mut context).unwrap();
            black_box(results);
        });
    });
}

/// Benchmark: NEW approach using LetAction
fn bench_new_let_action(c: &mut Criterion) {
    use aiki::flows::{Action, ExecutionContext, FailureMode, FlowExecutor, LetAction};

    let temp_dir = setup_workspace();

    let actions = vec![Action::Let(LetAction {
        let_: "description = aiki/provenance.build_description".to_string(),
        on_failure: FailureMode::Fail,
    })];

    c.bench_function("new_let_action", |b| {
        b.iter(|| {
            let mut context = ExecutionContext::new(PathBuf::from(temp_dir.path()));
            context
                .event_vars
                .insert("agent".to_string(), "claude-code".to_string());
            context
                .event_vars
                .insert("session_id".to_string(), "test-session".to_string());
            context
                .event_vars
                .insert("tool_name".to_string(), "Edit".to_string());
            context
                .event_vars
                .insert("file_path".to_string(), "test.rs".to_string());

            let results = FlowExecutor::execute_actions(black_box(&actions), &mut context).unwrap();
            black_box(results);
        });
    });
}

/// Benchmark: OLD full provenance workflow (with JJ commands)
fn bench_old_full_workflow(c: &mut Criterion) {
    use aiki::flows::{Action, AikiAction, ExecutionContext, FailureMode, FlowExecutor, JjAction};
    use std::collections::HashMap;

    let temp_dir = setup_workspace();

    let mut args = HashMap::new();
    args.insert("agent".to_string(), "$event.agent".to_string());
    args.insert("session_id".to_string(), "$event.session_id".to_string());
    args.insert("tool_name".to_string(), "$event.tool_name".to_string());
    args.insert("file_path".to_string(), "$event.file_path".to_string());

    let actions = vec![
        Action::Aiki(AikiAction {
            aiki: "build_provenance_description".to_string(),
            args,
            on_failure: FailureMode::Fail,
        }),
        Action::Jj(JjAction {
            jj: "describe -m \"$description\"".to_string(),
            timeout: None,
            on_failure: FailureMode::Fail,
            alias: None,
        }),
    ];

    c.bench_function("old_full_workflow", |b| {
        b.iter(|| {
            let mut context = ExecutionContext::new(PathBuf::from(temp_dir.path()));
            context
                .event_vars
                .insert("agent".to_string(), "claude-code".to_string());
            context
                .event_vars
                .insert("session_id".to_string(), "test-session".to_string());
            context
                .event_vars
                .insert("tool_name".to_string(), "Edit".to_string());
            context
                .event_vars
                .insert("file_path".to_string(), "test.rs".to_string());

            let results = FlowExecutor::execute_actions(black_box(&actions), &mut context).unwrap();
            black_box(results);
        });
    });
}

/// Benchmark: NEW full provenance workflow (with JJ commands)
fn bench_new_full_workflow(c: &mut Criterion) {
    use aiki::flows::{Action, ExecutionContext, FailureMode, FlowExecutor, JjAction, LetAction};

    let temp_dir = setup_workspace();

    let actions = vec![
        Action::Let(LetAction {
            let_: "description = aiki/provenance.build_description".to_string(),
            on_failure: FailureMode::Fail,
        }),
        Action::Jj(JjAction {
            jj: "describe -m \"$description\"".to_string(),
            timeout: None,
            on_failure: FailureMode::Fail,
            alias: None,
        }),
    ];

    c.bench_function("new_full_workflow", |b| {
        b.iter(|| {
            let mut context = ExecutionContext::new(PathBuf::from(temp_dir.path()));
            context
                .event_vars
                .insert("agent".to_string(), "claude-code".to_string());
            context
                .event_vars
                .insert("session_id".to_string(), "test-session".to_string());
            context
                .event_vars
                .insert("tool_name".to_string(), "Edit".to_string());
            context
                .event_vars
                .insert("file_path".to_string(), "test.rs".to_string());

            let results = FlowExecutor::execute_actions(black_box(&actions), &mut context).unwrap();
            black_box(results);
        });
    });
}

criterion_group!(
    benches,
    bench_old_aiki_action,
    bench_new_let_action,
    bench_old_full_workflow,
    bench_new_full_workflow
);
criterion_main!(benches);
