use super::*;
const SEEDS: [u64; 16] = [0, 1, 2, 3, 7, 11, 17, 18, 23, 29, 31, 47, 63, 71, 97, 255];
const FOREST: Input = Input {
    mode: Mode::Forest,
    fitness: Fitness::FULL,
    eligible: true,
    corridor: false,
};
fn patches(seed: u64) -> Patches {
    Patches::new([17; 32], seed)
}
#[derive(Default, Debug)]
struct Metrics {
    area: usize,
    core: usize,
    core_trees: usize,
    exterior_trees: usize,
    trees: usize,
    singletons: usize,
    glades: usize,
    trails: usize,
    largest_stand: usize,
    largest_forest: usize,
    open: usize,
    largest_open: usize,
}
#[path = "tests/metrics.rs"]
mod metrics;
#[path = "tests/openings.rs"]
mod openings;
use metrics::{components, fixture};

/// Cover and largest stand of a uniform-wildness 768-square fixture.
fn uniform(seed: u64, wildness: u16) -> Metrics {
    fixture(patches(seed), 768, |_, _| FOREST, |_, _| wildness).metrics
}

#[test]
fn wilder_regions_have_more_forest_and_larger_stands_on_all16_seeds() {
    // Per-thousand canopy cover bounds for open, typical, forested and most
    // forested country, and the size of the largest forest (canopy with its
    // own glades and paths) each must stay under or reach.
    let bounds = [
        (60, (0, 30), (0, 900)),
        (450, (70, 220), (0, usize::MAX)),
        (700, (220, 420), (6_000, usize::MAX)),
        (1000, (400, 600), (20_000, usize::MAX)),
    ];
    for seed in SEEDS {
        let mut previous = 0;
        for (wildness, cover, stand) in bounds {
            let m = uniform(seed, wildness);
            let per_thousand = m.area * 1000 / (768 * 768);
            eprintln!("seed={seed} wildness={wildness} cover={per_thousand} {m:?}");
            assert!(
                (cover.0..=cover.1).contains(&per_thousand),
                "seed={seed} wildness={wildness} cover={per_thousand}"
            );
            assert!(
                (stand.0..=stand.1).contains(&m.largest_forest),
                "seed={seed} wildness={wildness} largest={}",
                m.largest_forest
            );
            assert!(m.area > previous, "cover must rise with wildness");
            previous = m.area;
            assert!(m.core_trees * 100 >= 85 * m.core && m.core_trees * 100 <= 95 * m.core);
            assert_eq!(m.exterior_trees, 0);
            assert!(m.singletons * 100 < m.trees.max(1));
        }
    }
}

#[test]
fn cover_follows_a_wildness_gradient_without_a_step_at_cell_borders() {
    // Wildness rises by 125 per cell from west to east. Every 96-tile column
    // band holds at least as much forest as the band four cells further west.
    let (mut bands, size) = ([0usize; 16], 1536usize);
    for seed in SEEDS {
        let p = patches(seed);
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                let s = p.sample(
                    x,
                    y,
                    |_, _| FOREST,
                    |cx, _| (cx * 125).clamp(0, 1000) as u16,
                );
                bands[x as usize / 96] += usize::from(s.zone != Zone::Exterior);
            }
        }
    }
    eprintln!("bands={bands:?}");
    for band in 8..16 {
        assert!(bands[band] > bands[band - 8], "bands={bands:?}");
    }
    assert!(bands[15] > 8 * bands[1].max(1));
}

#[test]
fn stand_edges_do_not_align_to_chunk_cell_or_lattice_grids() {
    // Count canopy transitions by column and row residue. A boundary clipped
    // to a grid of that period would pile transitions into one residue.
    for period in [16usize, 32, 48, 64] {
        let mut bins = vec![0usize; 2 * period];
        for seed in SEEDS {
            let f = fixture(
                patches(seed),
                768,
                |_, _| FOREST,
                |cx, cy| (((cx * 7 + cy * 13).rem_euclid(5)) * 250) as u16,
            );
            let canopy = |x: usize, y: usize| f.samples[y * f.size + x].zone != Zone::Exterior;
            for y in 1..f.size {
                for x in 1..f.size {
                    bins[(x + 896) % period] += usize::from(canopy(x, y) != canopy(x - 1, y));
                    bins[period + (y + 896) % period] +=
                        usize::from(canopy(x, y) != canopy(x, y - 1));
                }
            }
        }
        let mean = bins.iter().sum::<usize>() / bins.len();
        let (low, high) = (bins.iter().min().unwrap(), bins.iter().max().unwrap());
        eprintln!("period={period} mean={mean} low={low} high={high}");
        assert!(mean > 200);
        assert!(
            *high * 10 <= mean * 15 && *low * 10 >= mean * 6,
            "period={period} {bins:?}"
        );
    }
}

#[test]
fn no_stand_edge_runs_straight_along_an_axis_for_a_chunk_side() {
    // A square hole or a grid-clipped stand has axis-aligned straight edges
    // of a chunk side (32 tiles) or more; warped analytic stands do not.
    // Glade and path borders are excluded: a path is meant to run straight.
    let mut longest = 0;
    for seed in SEEDS {
        let f = fixture(
            patches(seed),
            768,
            |_, _| FOREST,
            |cx, cy| (((cx * 7 + cy * 13).rem_euclid(5)) * 250) as u16,
        );
        let at = |x: usize, y: usize| f.samples[y * f.size + x];
        let edge = |a: DensitySample, b: DensitySample| {
            (a.zone != Zone::Exterior) != (b.zone != Zone::Exterior)
                && a.opening == Opening::None
                && b.opening == Opening::None
        };
        for a in 1..f.size {
            let (mut row, mut column) = (0, 0);
            for b in 0..f.size {
                row = if edge(at(b, a), at(b, a - 1)) {
                    row + 1
                } else {
                    0
                };
                column = if edge(at(a, b), at(a - 1, b)) {
                    column + 1
                } else {
                    0
                };
                longest = longest.max(row).max(column);
            }
        }
    }
    eprintln!("longest straight edge={longest}");
    assert!(
        longest < 32,
        "longest straight axis-aligned edge: {longest} tiles"
    );
}

#[test]
fn all16_seeds_seams_shuffled_chunks_reverse_and_duplicate_queries() {
    for seed in SEEDS {
        let p = patches(seed);
        let size = 256;
        let wildness = |cx: i64, cy: i64| ((cx + cy).rem_euclid(3) * 500) as u16;
        let at = |x: i32, y: i32| p.sample(x - 128, y - 128, |_, _| FOREST, wildness);
        let baseline: Vec<_> = (0..size)
            .flat_map(|y| (0..size).map(move |x| at(x, y)))
            .collect();
        let chunks = size / 32;
        for order in 0..chunks * chunks {
            let c = (order * 13 + 7) % (chunks * chunks);
            for ly in (0..32).rev() {
                for lx in (0..32).rev() {
                    let (x, y) = ((c % chunks) * 32 + lx, (c / chunks) * 32 + ly);
                    assert_eq!(baseline[(y * size + x) as usize], at(x, y));
                }
            }
        }
    }
}

#[test]
fn one_chunk_reads_a_bounded_block_of_wildness_cells() {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    for seed in SEEDS {
        let p = patches(seed);
        for (ox, oy) in [(0, 0), (160, 160), (-192, 352), (180, -40)] {
            let (cells, calls) = (RefCell::new(BTreeSet::new()), RefCell::new(0usize));
            for y in oy..oy + 32 {
                for x in ox..ox + 32 {
                    p.sample(
                        x,
                        y,
                        |_, _| FOREST,
                        |cx, cy| {
                            cells.borrow_mut().insert((cx, cy));
                            *calls.borrow_mut() += 1;
                            1000
                        },
                    );
                }
            }
            let cells = cells.into_inner();
            let span = |axis: fn(&(i64, i64)) -> i64| {
                cells.iter().map(axis).max().unwrap() - cells.iter().map(axis).min().unwrap()
            };
            assert!(cells.len() <= 16 && span(|c| c.0) <= 3 && span(|c| c.1) <= 3);
            // Nine descriptor evaluations of at most 9 + 9 * 4 lookups each.
            assert!(*calls.borrow() <= 1024 * 9 * 45);
        }
    }
}

#[test]
fn savanna_treeless_fitness_extremes_world_key_and_source_errors() {
    let p = patches(1);
    let (seed2, key2) = (patches(2), Patches::new([18; 32], 1));
    let (mut ds, mut dk, mut savanna_trees) = (0, 0, 0);
    let dense = |_: i64, _: i64| Ok::<u16, &str>(1000);
    for y in -128..128 {
        for x in -128..128 {
            let s = p.try_density_sample(x, y, FOREST, &dense).unwrap();
            assert_eq!(s.canopy_per_thousand, s.forest_floor_per_thousand);
            assert_eq!(
                s.density_per_thousand,
                match s.zone {
                    Zone::Core => 900,
                    Zone::Edge => 650,
                    Zone::Exterior => 0,
                }
            );
            ds += usize::from(Ok(s) != seed2.try_density_sample(x, y, FOREST, &dense));
            dk += usize::from(Ok(s) != key2.try_density_sample(x, y, FOREST, &dense));
            let savanna = Input {
                mode: Mode::SparseSavanna,
                ..FOREST
            };
            let sav = p.try_density_sample(x, y, savanna, &dense).unwrap();
            savanna_trees += usize::from(sav.tree);
            assert_eq!((sav.zone, sav.canopy_per_thousand), (Zone::Exterior, 0));
            for input in [
                Input {
                    mode: Mode::Treeless,
                    ..FOREST
                },
                Input {
                    eligible: false,
                    ..FOREST
                },
                Input {
                    fitness: Fitness {
                        suitable: false,
                        support_per_thousand: 1000,
                    },
                    ..FOREST
                },
                Input {
                    fitness: Fitness {
                        suitable: true,
                        support_per_thousand: 0,
                    },
                    ..FOREST
                },
            ] {
                assert_eq!(
                    p.try_density_sample(x, y, input, &dense),
                    Ok(DensitySample::OPEN)
                );
            }
        }
    }
    assert!(ds > 100 && dk > 100);
    assert!((4000..=6500).contains(&savanna_trees));
    for x in [i32::MIN, -193, -192, -1, 0, 191, 192, i32::MAX] {
        for y in [i32::MIN, -1, 0, 192, i32::MAX] {
            let once = p.sample(x, y, |_, _| FOREST, |_, _| 1000);
            assert_eq!(once, p.sample(x, y, |_, _| FOREST, |_, _| 1000));
        }
    }
    assert_eq!(
        p.try_sample(0, 0, |_, _| Err::<Input, _>("cancelled"), dense),
        Err("cancelled")
    );
    let failing = |_: i64, _: i64| Err::<u16, _>("page");
    assert!((0..192).any(|x| p.try_sample(x, 96, |_, _| Ok(FOREST), failing) == Err("page")));
}
