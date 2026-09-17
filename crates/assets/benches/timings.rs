use aoe_assets::palette;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn benchmarks(c: &mut Criterion) {
    let mut source = b"JASC-PAL\n0100\n256\n".to_vec();
    for index in 0..256 {
        source.extend_from_slice(format!("{index} {} {}\n", 255 - index, index / 2).as_bytes());
    }
    assert!(palette::parse(&source).is_ok());
    c.bench_function("palette_decode_256", |bench| {
        bench.iter(|| black_box(palette::parse(black_box(&source))));
    });
}

criterion_group!(timings, benchmarks);
criterion_main!(timings);
