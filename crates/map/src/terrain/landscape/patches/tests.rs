use super::*;
const SEEDS: [u64; 16] = [0, 1, 2, 3, 7, 11, 17, 18, 23, 29, 31, 47, 63, 71, 97, 255];
const REGIONS: [Region; 4] = [
    Region::Sparse,
    Region::Moderate,
    Region::Heavy,
    Region::Exceptional,
];
fn input(region: Region) -> Input {
    Input {
        mode: Mode::Forest(region),
        fitness: Fitness::FULL,
        eligible: true,
    }
}
fn patches(seed: u64) -> Patches {
    Patches::new([17; 32], seed, Parameters::default()).unwrap()
}
#[derive(Default, Debug)]
struct Metrics {
    area: usize,
    core: usize,
    core_trees: usize,
    exterior: usize,
    exterior_trees: usize,
    trees: usize,
    singletons: usize,
    largest_tree: usize,
    largest_open: usize,
}
#[path = "tests/metrics.rs"]
mod metrics;
use metrics::fixture;
#[test]
fn all16_seeds_native256_512_area_core_exterior_singletons() {
    let bounds = [(10, 20), (25, 40), (45, 60), (65, 80)];
    for seed in SEEDS {
        for size in [256, 512] {
            for (j, region) in REGIONS.into_iter().enumerate() {
                let m = fixture(patches(seed), size, |_, _| input(region));
                let n = size * size;
                eprintln!("seed={seed} region={region:?} size={size} {m:?}");
                assert!(m.area * 100 >= bounds[j].0 * n && m.area * 100 <= bounds[j].1 * n);
                assert!(
                    m.core > 0
                        && m.core_trees * 100 >= 85 * m.core
                        && m.core_trees * 100 <= 95 * m.core
                );
                assert!(m.exterior_trees * 100 <= m.exterior);
                assert!(m.singletons * 100 < m.trees);
                assert!(m.largest_tree >= 100);
                assert!(m.largest_open > 0);
            }
        }
    }
}
#[test]
fn integer_mirror_counts_match_native_rust_execution() {
    let expected = [
        (256, Region::Sparse, (10951, 8182, 7369, 9115)),
        (512, Region::Sparse, (41477, 30886, 27736, 34562)),
        (256, Region::Moderate, (24578, 20201, 18109, 20927)),
        (512, Region::Moderate, (92599, 76305, 68546, 79105)),
        (256, Region::Heavy, (35500, 30495, 27317, 30573)),
        (512, Region::Heavy, (136083, 117112, 105263, 117593)),
        (256, Region::Exceptional, (50556, 46173, 41442, 44387)),
        (512, Region::Exceptional, (196037, 177683, 159892, 172027)),
    ];
    for (size, region, counts) in expected {
        let m = fixture(patches(1), size, |_, _| input(region));
        assert_eq!((m.area, m.core, m.core_trees, m.trees), counts);
    }
}
#[test]
fn all16_seeds_seams_shuffled_chunks_reverse_and_duplicate_queries() {
    for seed in SEEDS {
        let p = patches(seed);
        for size in [256, 512] {
            let region = Region::Heavy;
            let baseline: Vec<_> = (0..size)
                .flat_map(|y| {
                    (0..size).map(move |x| p.sample(x - 128, y - 128, |_, _| input(region)))
                })
                .collect();
            let chunks = size / 32;
            for order in 0..chunks * chunks {
                let c = (order * 13 + 7) % (chunks * chunks);
                for ly in (0..32).rev() {
                    for lx in (0..32).rev() {
                        let x = (c % chunks) * 32 + lx;
                        let y = (c / chunks) * 32 + ly;
                        let i = (y * size + x) as usize;
                        assert_eq!(
                            baseline[i],
                            p.sample(x - 128, y - 128, |_, _| input(region))
                        );
                        if lx == 0 || ly == 0 || lx == 31 || ly == 31 {
                            assert_eq!(
                                baseline[i],
                                p.sample(x - 128, y - 128, |_, _| input(region))
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn shared_mask_source_proportions_and_post_exclusion_metrics() {
    let p = patches(17);
    let source_mask = |x: i32, _: i32| Input {
        eligible: x.rem_euclid(100) >= 59,
        ..input(Region::Exceptional)
    };
    let mut excluded = 0;
    for y in 0..100 {
        for x in 0..100 {
            let s = p.sample(x, y, source_mask);
            if !source_mask(x, y).eligible {
                excluded += 1;
                assert_eq!(s, DensitySample::OPEN);
            }
        }
    }
    assert_eq!(excluded, 5900); // synthetic exact 42% crop + 17% grazing over land
    for seed in SEEDS {
        for size in [256, 512] {
            let m = fixture(patches(seed), size, |x, _| Input {
                eligible: x.rem_euclid(64) >= 16,
                ..input(Region::Heavy)
            });
            eprintln!(
                "post-exclusion seed={seed} size={size} original-suitable={} {m:?}",
                size * size
            );
            assert!(m.singletons * 100 < m.trees);
        }
    }
}
#[test]
fn explicit_density_zones_floor_canopy_and_separate_savanna() {
    let p = patches(1);
    let mut zones = [0; 3];
    let mut savanna_trees = 0;
    for y in -128..128 {
        for x in -128..128 {
            let s = p.sample(x, y, |_, _| input(Region::Heavy));
            assert_eq!(s.canopy_per_thousand, s.forest_floor_per_thousand);
            match s.zone {
                Zone::Core => {
                    zones[0] += 1;
                    assert_eq!(s.density_per_thousand, 900);
                }
                Zone::Edge => {
                    zones[1] += 1;
                    assert_eq!(s.density_per_thousand, 650);
                }
                Zone::Exterior => {
                    zones[2] += 1;
                    assert_eq!(s, DensitySample::OPEN);
                }
            }
            let sav = p.density_sample(
                x,
                y,
                Input {
                    mode: Mode::SparseSavanna,
                    ..input(Region::Sparse)
                },
            );
            savanna_trees += usize::from(sav.tree);
            assert_eq!(sav.zone, Zone::Exterior);
            assert_eq!(sav.canopy_per_thousand, 0);
            assert_eq!(sav.forest_floor_per_thousand, 0);
        }
    }
    assert!(zones.into_iter().all(|n| n > 0));
    assert!((4000..=6500).contains(&savanna_trees));
}
#[test]
fn world_key_seed_fitness_extremes_and_source_errors() {
    let p = patches(1);
    let seed2 = patches(2);
    let key2 = Patches::new([18; 32], 1, Parameters::default()).unwrap();
    let (mut ds, mut dk) = (0, 0);
    for y in 0..128 {
        for x in 0..128 {
            let s = p.density_sample(x, y, input(Region::Moderate));
            ds += usize::from(s != seed2.density_sample(x, y, input(Region::Moderate)));
            dk += usize::from(s != key2.density_sample(x, y, input(Region::Moderate)));
            for fitness in [
                Fitness {
                    suitable: false,
                    support_per_thousand: 1000,
                },
                Fitness {
                    suitable: true,
                    support_per_thousand: 0,
                },
            ] {
                assert_eq!(
                    p.density_sample(
                        x,
                        y,
                        Input {
                            fitness,
                            ..input(Region::Heavy)
                        }
                    ),
                    DensitySample::OPEN
                );
            }
        }
    }
    assert!(ds > 100 && dk > 100);
    for x in [
        i32::MIN,
        -65,
        -64,
        -33,
        -32,
        -1,
        0,
        31,
        32,
        63,
        64,
        i32::MAX,
    ] {
        for y in [i32::MIN, -65, -1, 0, 64, i32::MAX] {
            assert_eq!(
                p.sample(x, y, |_, _| input(Region::Heavy)),
                p.sample(x, y, |_, _| input(Region::Heavy))
            );
        }
    }
    assert_eq!(
        p.try_sample(0, 0, |_, _| Err::<Input, _>("cancelled")),
        Err("cancelled")
    );
}
#[test]
fn no_eight_by_eight_core_occupancy_parity_classes() {
    for seed in SEEDS {
        let p = patches(seed);
        let (mut core, mut occupied) = ([0usize; 64], [0usize; 64]);
        for y in -128i32..384 {
            for x in -128i32..384 {
                let s = p.sample(x, y, |_, _| input(Region::Heavy));
                if s.zone == Zone::Core {
                    let bin = (y.rem_euclid(8) * 8 + x.rem_euclid(8)) as usize;
                    core[bin] += 1;
                    occupied[bin] += usize::from(s.tree);
                }
            }
        }
        for bin in 0..64 {
            assert!(core[bin] > 100);
            assert!(
                occupied[bin] * 100 >= 85 * core[bin] && occupied[bin] * 100 <= 95 * core[bin],
                "seed={seed} parity-bin={bin} core={} occupied={}",
                core[bin],
                occupied[bin]
            );
        }
    }
}
#[test]
fn macro_cluster_activation_centres_radii_and_directions_are_not_uniform() {
    use std::collections::BTreeSet;
    for seed in SEEDS {
        let p = patches(seed);
        let (mut offsets, mut radii, mut directions) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        let (mut active, mut inactive) = (0, 0);
        for my in -8i64..8 {
            for mx in -8i64..8 {
                let h = p.noise(1, mx, my);
                offsets.insert((h % 29, ((h >> 8) % 29)));
                radii.insert((h >> 32) % 5);
                directions.insert((h >> 20) % 8);
                let bias = (p.noise(6, mx.div_euclid(3), my.div_euclid(3)) % 201) as i64 - 100;
                if (((h >> 48) % 1000) as i64) < (950 + bias).min(1000) {
                    active += 1;
                } else {
                    inactive += 1;
                }
            }
        }
        assert!(offsets.len() > 100 && radii.len() == 5 && directions.len() == 8);
        assert!(active > 100 && inactive > 0);
    }
}
#[test]
fn validated_parameter_bounds() {
    let mut params = Parameters::default();
    params.radii[3] = 37;
    assert!(matches!(
        Patches::new([0; 32], 0, params),
        Err(ParameterError::Radius)
    ));
    params = Parameters::default();
    params.edge_width = 0;
    assert!(matches!(
        Patches::new([0; 32], 0, params),
        Err(ParameterError::Edge)
    ));
    params = Parameters::default();
    params.core_density = 960;
    assert!(matches!(
        Patches::new([0; 32], 0, params),
        Err(ParameterError::Density)
    ));
}
