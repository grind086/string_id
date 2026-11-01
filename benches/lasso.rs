use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

mod utils;

use foldhash::fast::FixedState;
use lasso::{Spur, ThreadedRodeo};
use utils::{bench_sizes, rand_ascii};

fn init_rodeo() -> ThreadedRodeo<Spur, FixedState> {
    ThreadedRodeo::<Spur, FixedState>::with_hasher(FixedState::with_seed(0))
}

fn intern(c: &mut Criterion) {
    bench_sizes(
        c,
        "lasso: intern",
        0x019bbb3ba326703a,
        init_rodeo,
        |_, rng, bytes| rand_ascii(rng, bytes),
        |interner, s| {
            interner.get_or_intern(black_box(s));
        },
    );
}

fn intern_existing(c: &mut Criterion) {
    bench_sizes(
        c,
        "lasso: intern existing",
        0x9446128204f8fea8,
        init_rodeo,
        |interner, rng, bytes| {
            let s = rand_ascii(rng, bytes);
            interner.get_or_intern(&s);
            s
        },
        |interner, s| {
            interner.get_or_intern(black_box(s));
        },
    );
}

fn resolve(c: &mut Criterion) {
    bench_sizes(
        c,
        "lasso: resolve",
        0x85caa879d75e9db2,
        init_rodeo,
        |interner, rng, bytes| {
            let s = rand_ascii(rng, bytes);
            interner.get_or_intern(&s)
        },
        |interner, id| {
            black_box(interner.resolve(black_box(&id)));
        },
    );
}

criterion_group!(benches, intern, intern_existing, resolve);
criterion_main!(benches);
