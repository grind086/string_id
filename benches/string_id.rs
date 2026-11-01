use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

mod utils;

use string_id::{buffer::StringBuffer, interner::Interner};
use utils::{bench_sizes, rand_ascii};

fn intern(c: &mut Criterion) {
    bench_sizes(
        c,
        "intern",
        0x019bbb3ba326703a,
        Interner::new,
        |_, rng, bytes| rand_ascii(rng, bytes),
        |interner, s| {
            interner.intern(&black_box(s));
        },
    );
}

fn intern_existing(c: &mut Criterion) {
    bench_sizes(
        c,
        "intern existing",
        0x9446128204f8fea8,
        Interner::new,
        |interner, rng, bytes| {
            let s = rand_ascii(rng, bytes);
            interner.intern(&s);
            s
        },
        |interner, s| {
            interner.intern(&black_box(s));
        },
    );
}

fn resolve(c: &mut Criterion) {
    bench_sizes(
        c,
        "resolve",
        0x85caa879d75e9db2,
        Interner::new,
        |interner, rng, bytes| {
            let s = rand_ascii(rng, bytes);
            interner.intern(&s)
        },
        |interner, id| {
            black_box(interner.resolve(black_box(id)));
        },
    );
}

fn write_buffer(c: &mut Criterion) {
    bench_sizes(
        c,
        "write buffer",
        0x8400899647554167,
        StringBuffer::new,
        |_, rng, bytes| rand_ascii(rng, bytes),
        |buf, s| {
            buf.write(black_box(s.as_str()));
        },
    );
}

criterion_group!(benches, intern, intern_existing, resolve, write_buffer);
criterion_main!(benches);
