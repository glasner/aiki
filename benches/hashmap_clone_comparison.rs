use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;

/// Compare HashMap.extend(hashmap.clone()) vs HashMap.extend(hashmap.iter().map(...))
fn bench_hashmap_extend_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashmap_extend");

    for size in [5, 10, 20, 50, 100] {
        let mut source = HashMap::new();
        for i in 0..size {
            source.insert(format!("key_{}", i), format!("value_{}", i));
        }

        // Pattern 1: Clone entire HashMap
        group.bench_with_input(
            BenchmarkId::new("clone_hashmap", size),
            &source,
            |b, source| {
                b.iter(|| {
                    let mut target = HashMap::new();
                    target.extend(black_box(source.clone()));
                    black_box(target);
                });
            },
        );

        // Pattern 2: Iterate and clone individual entries
        group.bench_with_input(
            BenchmarkId::new("iter_map_clone", size),
            &source,
            |b, source| {
                b.iter(|| {
                    let mut target = HashMap::new();
                    target.extend(
                        black_box(source)
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone())),
                    );
                    black_box(target);
                });
            },
        );

        // Pattern 3: For loop (baseline reference)
        group.bench_with_input(BenchmarkId::new("for_loop", size), &source, |b, source| {
            b.iter(|| {
                let mut target = HashMap::new();
                for (k, v) in black_box(source) {
                    target.insert(k.clone(), v.clone());
                }
                black_box(target);
            });
        });
    }

    group.finish();
}

/// Benchmark allocation patterns
fn bench_hashmap_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashmap_allocation");

    let size = 20;
    let mut source = HashMap::new();
    for i in 0..size {
        source.insert(format!("key_{}", i), format!("value_{}", i));
    }

    // Clone entire HashMap (allocates HashMap structure + all entries)
    group.bench_function("clone_entire_hashmap", |b| {
        b.iter(|| {
            let cloned = black_box(&source).clone();
            black_box(cloned);
        });
    });

    // Clone just the keys
    group.bench_function("clone_keys_only", |b| {
        b.iter(|| {
            let keys: Vec<_> = black_box(&source).keys().cloned().collect();
            black_box(keys);
        });
    });

    // Clone just the values
    group.bench_function("clone_values_only", |b| {
        b.iter(|| {
            let values: Vec<_> = black_box(&source).values().cloned().collect();
            black_box(values);
        });
    });

    // Iterator overhead (no cloning)
    group.bench_function("iter_no_clone", |b| {
        b.iter(|| {
            let count = black_box(&source).iter().count();
            black_box(count);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_hashmap_extend_patterns,
    bench_hashmap_allocation
);
criterion_main!(benches);
