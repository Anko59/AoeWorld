//! Mountain and cold-country fixtures: synthetic elevation fields at the
//! France package scale (30:1, 60 m tiles, 48 tiles per elevation sample).
use super::*;
use crate::biome::PreparedBiome;
use crate::water::PreparedWater;
use std::collections::VecDeque;
use std::sync::Arc;

mod cliffs;
mod review;
mod rocks;
mod snow;

pub(super) const SIDE: i32 = 768;
const SAMPLES: u16 = 16;

/// Heights in metres per elevation sample; one potential-vegetation class.
pub(super) fn relief(seed: u64, class: u8, height: impl Fn(i32, i32) -> i32) -> MapChunkGenerator {
    let mut generator = MapChunkGenerator::new([41; 32], seed, SIDE);
    let side = i32::from(SAMPLES);
    generator.elevation = Some(Arc::new(PreparedElevation {
        samples_per_axis: SAMPLES,
        compression: Ratio::new(30, 1).expect("ratio"),
        pages: [(
            (0, 0),
            ElevationPage {
                level: 0,
                x: 0,
                y: 0,
                width: SAMPLES as u8,
                height: SAMPLES as u8,
                geographic_height_centimeters: (0..side * side)
                    .map(|index| height(index % side, index / side) * 100)
                    .collect(),
            },
        )]
        .into(),
    }));
    generator.biome = Some(Arc::new(PreparedBiome::new(
        1,
        [(
            (0, 0),
            PotentialBiomePage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                potential_biome_class: vec![class],
            },
        )]
        .into(),
    )));
    generator.water = Some(Arc::new(PreparedWater::new(
        1,
        [(
            (0, 0),
            WaterPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                ocean_coverage_percent: vec![0],
                inland_coverage_percent: vec![0],
            },
        )]
        .into(),
    )));
    generator
}

/// Ridges and valleys between 0 and 3,600 m with grades up to about 22%.
pub(super) fn alpine(seed: u64) -> MapChunkGenerator {
    const WAVE: [i32; 8] = [0, 1, 2, 1, 0, -1, -2, -1];
    relief(seed, 13, |x, y| {
        1_800 + 450 * WAVE[(x % 8) as usize] + 450 * WAVE[((y + 2) % 8) as usize]
    })
}

/// A plane rising `x`/`y` metres per sample (2,880 m of ground).
pub(super) fn plane(x: i32, y: i32) -> MapChunkGenerator {
    relief(1, 13, move |i, j| 3_000 + (i - 8) * x + (j - 8) * y)
}

pub(super) fn is_cliff(tile: Tile) -> bool {
    tile.surface.kind == SurfaceKind::Cliff && !tile.passable
}

/// Sizes of the four-connected regions of `open`, largest first, and whether
/// the largest touches the west, east, north and south map edges.
pub(super) fn regions(open: &[bool]) -> (Vec<usize>, [bool; 4]) {
    let side = SIDE as usize;
    let mut label = vec![0_u32; open.len()];
    let mut sizes = Vec::new();
    let mut edges = Vec::new();
    for start in 0..open.len() {
        if !open[start] || label[start] != 0 {
            continue;
        }
        let id = sizes.len() as u32 + 1;
        let (mut size, mut edge) = (0, [false; 4]);
        let mut queue = VecDeque::from([start]);
        label[start] = id;
        while let Some(index) = queue.pop_front() {
            size += 1;
            let (x, y) = (index % side, index / side);
            edge[0] |= x == 0;
            edge[1] |= x == side - 1;
            edge[2] |= y == 0;
            edge[3] |= y == side - 1;
            let mut visit = |next: usize| {
                if open[next] && label[next] == 0 {
                    label[next] = id;
                    queue.push_back(next);
                }
            };
            if x > 0 {
                visit(index - 1);
            }
            if x + 1 < side {
                visit(index + 1);
            }
            if y > 0 {
                visit(index - side);
            }
            if y + 1 < side {
                visit(index + side);
            }
        }
        sizes.push(size);
        edges.push(edge);
    }
    let largest = (0..sizes.len()).max_by_key(|&index| sizes[index]);
    let edge = largest.map_or([false; 4], |index| edges[index]);
    sizes.sort_unstable_by(|left, right| right.cmp(left));
    (sizes, edge)
}

pub(super) fn base_tiles(generator: &MapChunkGenerator) -> Vec<Tile> {
    (0..SIDE * SIDE)
        .map(|index| generator.sample_base_tile(TileCoord::new(index % SIDE, index / SIDE)))
        .collect()
}
