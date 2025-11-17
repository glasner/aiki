use aiki::flows::VariableResolver;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;

/// Benchmark variable resolution across different scenarios
fn bench_variable_resolution_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_resolution");

    // Test case 1: No variables in input (should be fast path)
    let no_vars_input = "This is a plain string with no variables at all";

    // Test case 2: One simple variable
    let one_var_input = "Processing file $event.file_path";

    // Test case 3: Many variables (realistic workflow)
    let many_vars_input = "Agent $event.agent in $cwd with session $event.session_id and tool $event.tool_name modifying $event.file_path";

    // Test case 4: Overlapping variable names (edge case)
    let overlapping_input = "File: $event.file and path: $event.file_path";

    // Test case 5: Same variable repeated multiple times
    let repeated_input = "$event.file_path modified $event.file_path saved $event.file_path";

    // Test case 6: Mix of defined and undefined variables
    let mixed_input = "$event.file_path in $cwd with $undefined_var and $another_missing";

    // Test case 7: Very long string with many replacements
    let long_input = format!(
        "{}",
        (0..100)
            .map(|i| format!("Item {}: $event.file_path in $cwd", i))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Setup resolver with typical variables
    let mut resolver = VariableResolver::new();
    resolver.add_var("event.file_path", "/path/to/file.rs");
    resolver.add_var("event.file", "file.rs");
    resolver.add_var("event.agent", "claude-code");
    resolver.add_var("event.session_id", "session-123");
    resolver.add_var("event.tool_name", "Edit");
    resolver.add_var("cwd", "/home/user/project");

    // Additional environment variables (realistic scenario)
    let mut env_vars = HashMap::new();
    env_vars.insert("HOME".to_string(), "/home/user".to_string());
    env_vars.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
    env_vars.insert("USER".to_string(), "testuser".to_string());
    resolver.add_env_vars(&env_vars);

    // Benchmark each scenario
    group.bench_with_input(
        BenchmarkId::new("no_vars", "plain_text"),
        &no_vars_input,
        |b, input| {
            b.iter(|| {
                let result = resolver.resolve(black_box(input));
                black_box(result);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("one_var", "simple"),
        &one_var_input,
        |b, input| {
            b.iter(|| {
                let result = resolver.resolve(black_box(input));
                black_box(result);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("many_vars", "realistic"),
        &many_vars_input,
        |b, input| {
            b.iter(|| {
                let result = resolver.resolve(black_box(input));
                black_box(result);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("overlapping", "edge_case"),
        &overlapping_input,
        |b, input| {
            b.iter(|| {
                let result = resolver.resolve(black_box(input));
                black_box(result);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("repeated", "same_var_multiple"),
        &repeated_input,
        |b, input| {
            b.iter(|| {
                let result = resolver.resolve(black_box(input));
                black_box(result);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("mixed_defined_undefined", "missing_vars"),
        &mixed_input,
        |b, input| {
            b.iter(|| {
                let result = resolver.resolve(black_box(input));
                black_box(result);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("long_string", "100_items"),
        &long_input,
        |b, input| {
            b.iter(|| {
                let result = resolver.resolve(black_box(input));
                black_box(result);
            });
        },
    );

    group.finish();
}

/// Benchmark the overhead of creating a resolver
fn bench_resolver_creation(c: &mut Criterion) {
    let mut event_vars = HashMap::new();
    event_vars.insert("file_path".to_string(), "/path/to/file.rs".to_string());
    event_vars.insert("agent".to_string(), "claude-code".to_string());
    event_vars.insert("session_id".to_string(), "session-123".to_string());
    event_vars.insert("tool_name".to_string(), "Edit".to_string());

    let mut env_vars = HashMap::new();
    env_vars.insert("HOME".to_string(), "/home/user".to_string());
    env_vars.insert("PATH".to_string(), "/usr/bin:/bin".to_string());

    c.bench_function("resolver_creation", |b| {
        b.iter(|| {
            let mut resolver = VariableResolver::new();

            for (key, value) in &event_vars {
                resolver.add_var(format!("event.{}", key), value.clone());
            }

            resolver.add_var("cwd", "/home/user/project");
            resolver.add_env_vars(black_box(&env_vars));

            black_box(resolver);
        });
    });
}

/// Benchmark variable resolution with different numbers of variables
fn bench_variable_count_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_count_scaling");

    for var_count in [5, 10, 20, 50, 100] {
        let mut resolver = VariableResolver::new();

        // Add N variables
        for i in 0..var_count {
            resolver.add_var(format!("var{}", i), format!("value{}", i));
        }

        // Test string that uses some of the variables
        let test_input = "$var0 and $var1 and $var2";

        group.bench_with_input(
            BenchmarkId::from_parameter(var_count),
            &test_input,
            |b, input| {
                b.iter(|| {
                    let result = resolver.resolve(black_box(input));
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark the sorting overhead specifically
fn bench_sorting_overhead(c: &mut Criterion) {
    use std::cmp::Reverse;

    let mut group = c.benchmark_group("sorting_overhead");

    // Create a realistic HashMap of variables
    let mut variables = HashMap::new();
    variables.insert(
        "event.file_path".to_string(),
        "/path/to/file.rs".to_string(),
    );
    variables.insert("event.file".to_string(), "file.rs".to_string());
    variables.insert("event.agent".to_string(), "claude-code".to_string());
    variables.insert("event.session_id".to_string(), "session-123".to_string());
    variables.insert("event.tool_name".to_string(), "Edit".to_string());
    variables.insert("cwd".to_string(), "/home/user/project".to_string());
    variables.insert("HOME".to_string(), "/home/user".to_string());
    variables.insert("PATH".to_string(), "/usr/bin".to_string());

    group.bench_function("sort_variables_by_length", |b| {
        b.iter(|| {
            let mut vars: Vec<_> = variables.iter().collect();
            vars.sort_by_key(|(k, _)| Reverse(k.len()));
            black_box(vars);
        });
    });

    group.finish();
}

/// Benchmark HashMap cloning in add_env_vars
fn bench_add_env_vars_cloning(c: &mut Criterion) {
    let mut group = c.benchmark_group("add_env_vars");

    // Create realistic environment variable sets of different sizes
    for env_count in [5, 10, 20, 50] {
        let mut env_vars = HashMap::new();
        for i in 0..env_count {
            env_vars.insert(format!("ENV_VAR_{}", i), format!("value_{}", i));
        }

        group.bench_with_input(
            BenchmarkId::new("add_env_vars", env_count),
            &env_vars,
            |b, env_vars| {
                b.iter(|| {
                    let mut resolver = VariableResolver::new();
                    // Uses extend with iterator (current implementation)
                    resolver.add_env_vars(black_box(env_vars));
                    black_box(resolver);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark the full resolver creation pipeline
fn bench_resolver_creation_with_env_vars(c: &mut Criterion) {
    let mut event_vars = HashMap::new();
    event_vars.insert("file_path".to_string(), "/path/to/file.rs".to_string());
    event_vars.insert("agent".to_string(), "claude-code".to_string());
    event_vars.insert("session_id".to_string(), "session-123".to_string());

    let mut env_vars = HashMap::new();
    for i in 0..20 {
        env_vars.insert(format!("ENV_{}", i), format!("val_{}", i));
    }

    c.bench_function("full_resolver_creation", |b| {
        b.iter(|| {
            let mut resolver = VariableResolver::new();
            for (key, value) in &event_vars {
                resolver.add_var(format!("event.{}", key), value.clone());
            }
            resolver.add_var("cwd", "/home/user/project");
            resolver.add_env_vars(black_box(&env_vars));
            black_box(resolver);
        });
    });
}

criterion_group!(
    benches,
    bench_variable_resolution_scenarios,
    bench_resolver_creation,
    bench_variable_count_scaling,
    bench_sorting_overhead,
    bench_add_env_vars_cloning,
    bench_resolver_creation_with_env_vars
);
criterion_main!(benches);
