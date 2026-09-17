use aoe_assets::palette;
use gungraun::Dhat;
use gungraun::prelude::*;
use std::hint::black_box;

fn setup() -> Vec<u8> {
    let mut source = b"JASC-PAL\n0100\n256\n".to_vec();
    for index in 0..256 {
        source.extend_from_slice(format!("{index} {} {}\n", 255 - index, index / 2).as_bytes());
    }
    source
}

#[library_benchmark(setup = setup)]
fn palette_decode(source: Vec<u8>) -> usize {
    black_box(
        palette::parse(black_box(&source))
            .map(|palette| palette.0.len())
            .unwrap_or_default(),
    )
}

library_benchmark_group!(name = assets, benchmarks = [palette_decode]);

main!(
    config = LibraryBenchmarkConfig::default().tool(Dhat::default()),
    library_benchmark_groups = assets
);
