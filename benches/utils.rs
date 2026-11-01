use std::cell::UnsafeCell;

use criterion::{BatchSize, Criterion};
use rand::{Rng, RngExt, SeedableRng, distr::Alphanumeric};
use rand_chacha::ChaCha8Rng;

pub fn bench_sizes<S, I>(
    c: &mut Criterion,
    group: &str,
    seed: u64,
    init_state: impl Fn() -> S,
    init_input: impl Fn(&mut S, &mut ChaCha8Rng, usize) -> I,
    bench_fn: impl Fn(&mut S, I),
) {
    let sizes = [
        ("16b", 16),
        ("256b", 256),
        ("1kb", 1024),
        ("150kb", 150 * 1024),
    ];

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut g = c.benchmark_group(group);
    for &(name, bytes) in &sizes {
        g.bench_function(name, |b| {
            let state = UnsafeCell::new(init_state());
            b.iter_batched(
                || {
                    let state = unsafe { &mut *state.get() };
                    init_input(&mut *state, &mut rng, bytes)
                },
                |s| {
                    let state = unsafe { &mut *state.get() };
                    bench_fn(&mut *state, s)
                },
                BatchSize::SmallInput,
            );
        });
    }

    g.bench_function("mixed 16b - 1kb", |b| {
        let state = UnsafeCell::new(init_state());
        b.iter_batched(
            || {
                let n = rng.random_range(16..=1024);
                let state = unsafe { &mut *state.get() };
                init_input(&mut *state, &mut rng, n)
            },
            |s| {
                let state = unsafe { &mut *state.get() };
                bench_fn(&mut *state, s)
            },
            BatchSize::SmallInput,
        );
    });
}

pub fn rand_ascii(rng: &mut impl Rng, len: usize) -> String {
    core::iter::repeat_with(|| rng.sample(Alphanumeric) as char)
        .take(len)
        .collect()
}
