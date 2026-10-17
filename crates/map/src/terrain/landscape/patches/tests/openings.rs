//! Glades and paths belong to large forests only, and keep them crossable.
use super::*;

#[test]
fn forests_start_at_the_documented_wildness_and_grow_into_large_ones() {
    for seed in SEEDS {
        let p = patches(seed);
        let (mut present, mut large) = ([0usize; 3], [0usize; 3]);
        for cy in -6..6 {
            for cx in -6..6 {
                assert_eq!(p.large_forest(cx, cy, FOREST_MIN_WILDNESS - 1), None);
                for (index, wildness) in [500, 700, 1000].into_iter().enumerate() {
                    let Some(forest) = p.large_forest(cx, cy, wildness) else {
                        continue;
                    };
                    present[index] += 1;
                    large[index] += usize::from(forest.large);
                    assert!((26..=70).contains(&forest.radius));
                    assert_eq!(forest.large, forest.radius >= LARGE_FOREST_MIN_RADIUS);
                    let node = p.node(cx, cy);
                    // The trail node, and so the hub glade, lies inside the main lobe.
                    let offset =
                        (node.0 - forest.center.0).pow(2) + (node.1 - forest.center.1).pow(2);
                    assert!(offset < 29 * 29 && 29 < LARGE_FOREST_MIN_RADIUS);
                    assert!((node.0 - (cx * 192 + 96)).abs() <= 40);
                    assert!((node.1 - (cy * 192 + 96)).abs() <= 40);
                }
            }
        }
        assert!(present[0] > 10 && present[0] < present[1] && present[2] == 144);
        assert_eq!(large[0], 0, "wildness 500 has no large forest");
        assert!(large[1] > 0 && large[1] < large[2] && large[2] == 144);
    }
}

#[test]
fn groves_and_forests_below_the_large_radius_are_solid() {
    groves_and_forests_below_the_large_radius_are_solid_on(&SEEDS);
}

#[test]
fn groves_and_forests_below_the_large_radius_are_solid_on_two_seeds() {
    groves_and_forests_below_the_large_radius_are_solid_on(&QUICK_SEEDS);
}

fn groves_and_forests_below_the_large_radius_are_solid_on(seeds: &[u64]) {
    // Up to wildness 500 no forest reaches the large radius: no glade, no
    // trail, and a caller corridor clears nothing either.
    for &seed in seeds {
        let p = patches(seed);
        for wildness in [FOREST_MIN_WILDNESS - 1, 500] {
            let f = fixture(p, 768, |_, _| FOREST, |_, _| wildness);
            assert!(f.metrics.area > 20_000, "fixture must contain stands");
            assert_eq!((f.metrics.glades, f.metrics.trails), (0, 0));
            let forests = f.samples.iter().any(|s| s.stand == Stand::Forest);
            assert_eq!(forests, wildness == 500, "seed={seed} wildness={wildness}");
            let corridor = Input {
                corridor: true,
                ..FOREST
            };
            let crossed = fixture(p, 768, |_, _| corridor, |_, _| wildness);
            assert!(crossed.samples == f.samples);
            // Solid: open ground enclosed by canopy is an accident of two
            // overlapping lobes, never a cleared interior.
            let canopy: Vec<bool> = f.samples.iter().map(|s| s.zone != Zone::Exterior).collect();
            let (open, _) = components(&canopy, f.size, false, false);
            let enclosed: usize = open.iter().sum::<usize>() - open.iter().max().unwrap();
            assert!(
                enclosed * 200 <= f.metrics.area,
                "seed={seed} enclosed={enclosed}"
            );
        }
    }
}

#[test]
fn every_glade_and_path_lies_inside_a_large_forest_domain() {
    for seed in SEEDS {
        let p = patches(seed);
        let wildness = |cx: i64, cy: i64| ((cx * 3 + cy).rem_euclid(4) * 333) as u16;
        let f = fixture(p, 768, |_, _| FOREST, wildness);
        assert!(
            f.metrics.glades > 300 && f.metrics.trails > 1_000,
            "{:?}",
            f.metrics
        );
        for (index, sample) in f.samples.iter().enumerate() {
            if sample.opening == Opening::None {
                continue;
            }
            assert_eq!((sample.zone, sample.tree), (Zone::Exterior, false));
            let (x, y) = ((index % f.size) as i64 - 128, (index / f.size) as i64 - 128);
            let (cx, cy) = (x.div_euclid(192), y.div_euclid(192));
            // Lobes reach at most 1.55 radii from the centre; apron 8, and the
            // warp moves a tile by at most 15 per axis.
            let inside = (cy - 1..=cy + 1).any(|my| {
                (cx - 1..=cx + 1).any(|mx| {
                    p.large_forest(mx, my, wildness(mx, my))
                        .is_some_and(|forest| {
                            let reach = forest.radius * 155 / 100 + 8 + 22;
                            forest.large
                                && (x - forest.center.0).pow(2) + (y - forest.center.1).pow(2)
                                    <= reach * reach
                        })
                })
            });
            assert!(
                inside,
                "seed={seed} opening at ({x},{y}) outside every large forest"
            );
        }
    }
}

#[test]
fn most_forested_country_stays_connected_open_ground_on_all16_seeds() {
    most_forested_country_stays_connected_open_ground(&SEEDS);
}

#[test]
fn most_forested_country_stays_connected_open_ground_on_two_seeds() {
    most_forested_country_stays_connected_open_ground(&QUICK_SEEDS);
}

fn most_forested_country_stays_connected_open_ground(seeds: &[u64]) {
    // Five cells a side; connectivity is judged on the inner three, because a
    // pocket at the fixture border may be joined to the rest outside it.
    let (size, margin) = (960usize, 192usize);
    for &seed in seeds {
        let p = patches(seed);
        let f = fixture(p, size, |_, _| FOREST, |_, _| 1000);
        assert!(f.metrics.open * 100 >= 40 * size * size, "seed={seed}");
        let canopy: Vec<bool> = f.samples.iter().map(|s| s.zone != Zone::Exterior).collect();
        let (sizes, label) = components(&canopy, size, false, false);
        let main = (0..sizes.len()).max_by_key(|i| sizes[*i]).unwrap();
        let (mut open, mut joined) = (0usize, 0usize);
        for y in margin..size - margin {
            for x in margin..size - margin {
                open += usize::from(!canopy[y * size + x]);
                joined += usize::from(label[y * size + x] == main);
            }
        }
        // Trails, hub glades and glade links join the pockets between merged
        // forests into one four-connected region of open ground; the rest is
        // small natural pockets sealed between overlapping lobes.
        assert!(
            joined * 100 >= open * 98,
            "seed={seed} open={open} joined={joined}"
        );
        // Every large forest is crossed: its hub is open ground of that region.
        for cy in 1..=2 {
            for cx in 1..=2 {
                let node = p.node(cx, cy);
                // The tile whose warped coordinates are nearest the node.
                let tile = p.node_tile(cx, cy);
                let warped = p.warped(tile.0, tile.1);
                assert!((warped.0 - node.0).abs() <= 2 && (warped.1 - node.1).abs() <= 2);
                let index = (tile.1 + 128) as usize * size + (tile.0 + 128) as usize;
                assert_ne!(
                    f.samples[index].opening,
                    Opening::None,
                    "seed={seed} hub {cx},{cy}"
                );
                assert_eq!(label[index], main, "seed={seed} hub {cx},{cy}");
            }
        }
    }
}
