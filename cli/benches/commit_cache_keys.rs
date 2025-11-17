use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;

/// Simulates a commit ID (20 bytes for git SHA-1)
fn generate_commit_id(n: usize) -> Vec<u8> {
    let mut id = vec![0u8; 20];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = ((n + i) % 256) as u8;
    }
    id
}

/// Convert bytes to hex string (simulates commit_id.hex())
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Benchmark: Vec<u8> vs String as HashMap key
fn bench_hashmap_key_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("commit_cache_keys");

    // Generate test commit IDs
    let commit_count = 100;
    let commit_ids: Vec<Vec<u8>> = (0..commit_count).map(generate_commit_id).collect();
    let commit_id_hexes: Vec<String> = commit_ids.iter().map(|id| bytes_to_hex(id)).collect();

    // === Pattern 1: Vec<u8> as key (current implementation) ===
    group.bench_function("vec_u8_key_insert", |b| {
        b.iter(|| {
            let mut cache: HashMap<Vec<u8>, (String, Option<String>)> = HashMap::new();
            for (i, commit_id) in commit_ids.iter().enumerate() {
                // Allocates new Vec on every insert!
                cache.insert(
                    black_box(commit_id.to_vec()),
                    (format!("change_{}", i), Some(format!("prov_{}", i))),
                );
            }
            black_box(cache);
        });
    });

    group.bench_function("vec_u8_key_lookup", |b| {
        let mut cache: HashMap<Vec<u8>, (String, Option<String>)> = HashMap::new();
        for (i, commit_id) in commit_ids.iter().enumerate() {
            cache.insert(
                commit_id.clone(),
                (format!("change_{}", i), Some(format!("prov_{}", i))),
            );
        }

        b.iter(|| {
            for commit_id in &commit_ids {
                // Has to hash the Vec<u8> on every lookup
                let _ = black_box(cache.get(commit_id.as_slice()));
            }
        });
    });

    group.bench_function("vec_u8_key_lookup_with_alloc", |b| {
        let mut cache: HashMap<Vec<u8>, (String, Option<String>)> = HashMap::new();
        for (i, commit_id) in commit_ids.iter().enumerate() {
            cache.insert(
                commit_id.clone(),
                (format!("change_{}", i), Some(format!("prov_{}", i))),
            );
        }

        b.iter(|| {
            for commit_id in &commit_ids {
                // Simulates current code: allocates Vec on lookup!
                let key = black_box(commit_id).to_vec();
                let _ = black_box(cache.get(&key));
            }
        });
    });

    // === Pattern 2: String as key (optimized) ===
    group.bench_function("string_key_insert", |b| {
        b.iter(|| {
            let mut cache: HashMap<String, (String, Option<String>)> = HashMap::new();
            for (i, commit_id_hex) in commit_id_hexes.iter().enumerate() {
                // Clones String (already allocated once)
                cache.insert(
                    black_box(commit_id_hex.clone()),
                    (format!("change_{}", i), Some(format!("prov_{}", i))),
                );
            }
            black_box(cache);
        });
    });

    group.bench_function("string_key_lookup", |b| {
        let mut cache: HashMap<String, (String, Option<String>)> = HashMap::new();
        for (i, commit_id_hex) in commit_id_hexes.iter().enumerate() {
            cache.insert(
                commit_id_hex.clone(),
                (format!("change_{}", i), Some(format!("prov_{}", i))),
            );
        }

        b.iter(|| {
            for commit_id_hex in &commit_id_hexes {
                // Just hashes the String, no allocation
                let _ = black_box(cache.get(commit_id_hex));
            }
        });
    });

    group.bench_function("string_key_lookup_with_conversion", |b| {
        let mut cache: HashMap<String, (String, Option<String>)> = HashMap::new();
        for (i, commit_id_hex) in commit_id_hexes.iter().enumerate() {
            cache.insert(
                commit_id_hex.clone(),
                (format!("change_{}", i), Some(format!("prov_{}", i))),
            );
        }

        b.iter(|| {
            for commit_id in &commit_ids {
                // Simulates commit_id.hex() call
                let key = black_box(bytes_to_hex(commit_id));
                let _ = black_box(cache.get(&key));
            }
        });
    });

    group.finish();
}

/// Benchmark: Hash computation for Vec<u8> vs String
fn bench_hash_computation(c: &mut Criterion) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let commit_id = generate_commit_id(42);
    let commit_id_hex = bytes_to_hex(&commit_id);

    c.bench_function("hash_vec_u8", |b| {
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            black_box(&commit_id).hash(&mut hasher);
            black_box(hasher.finish());
        });
    });

    c.bench_function("hash_string", |b| {
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            black_box(&commit_id_hex).hash(&mut hasher);
            black_box(hasher.finish());
        });
    });
}

/// Benchmark: Memory overhead
fn bench_memory_overhead(c: &mut Criterion) {
    let commit_count = 1000;
    let commit_ids: Vec<Vec<u8>> = (0..commit_count).map(generate_commit_id).collect();
    let commit_id_hexes: Vec<String> = commit_ids.iter().map(|id| bytes_to_hex(id)).collect();

    c.bench_function("vec_u8_cache_creation", |b| {
        b.iter(|| {
            let mut cache: HashMap<Vec<u8>, (String, Option<String>)> =
                HashMap::with_capacity(commit_count);
            for (i, commit_id) in commit_ids.iter().enumerate() {
                cache.insert(
                    commit_id.clone(),
                    (format!("change_{}", i), Some(format!("prov_{}", i))),
                );
            }
            black_box(cache);
        });
    });

    c.bench_function("string_cache_creation", |b| {
        b.iter(|| {
            let mut cache: HashMap<String, (String, Option<String>)> =
                HashMap::with_capacity(commit_count);
            for (i, commit_id_hex) in commit_id_hexes.iter().enumerate() {
                cache.insert(
                    commit_id_hex.clone(),
                    (format!("change_{}", i), Some(format!("prov_{}", i))),
                );
            }
            black_box(cache);
        });
    });
}

criterion_group!(
    benches,
    bench_hashmap_key_types,
    bench_hash_computation,
    bench_memory_overhead
);
criterion_main!(benches);
