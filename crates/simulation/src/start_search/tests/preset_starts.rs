use super::*;

#[test]
fn every_menu_landscape_starts_and_prefers_the_largest_valley() {
    let config = WorldConfig::new(512, 512, Seed(1)).unwrap();
    let terrain = Terrain::Map {
        generator: flat_temperate_generator(),
        overlay: ResourceOverlay::default(),
    };
    for preset in ["paris", "alps", "brittany", "finland"] {
        let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
        for cy in 0..16 {
            for cx in 0..16 {
                let tiles = (0..1024)
                    .map(|i| {
                        let (x, y) = (cx * 32 + i % 32, cy * 32 + i / 32);
                        match preset {
                            "paris" => true,
                            // Disconnected mountain bowls: the larger lowland is far
                            // outside the old central 64-chunk survey.
                            "alps" => {
                                (20..80).contains(&x) && (20..65).contains(&y)
                                    || (240..252).contains(&x) && (240..252).contains(&y)
                            }
                            "brittany" => x < 45,
                            _ => {
                                (420..490).contains(&x) && (30..90).contains(&y)
                                    || (240..254).contains(&x) && (240..254).contains(&y)
                            }
                        }
                    })
                    .collect();
                cache.entries.insert((cx, cy), tiles);
            }
        }
        let result = robust::search(&mut cache).unwrap();
        let StartSearchResult::Found(tile) = result else {
            panic!("{preset}: {result:?}");
        };
        if preset == "alps" {
            assert!(tile.x < 80 && tile.y < 65, "largest valley: {tile:?}");
        }
        if preset == "finland" {
            assert!(tile.x >= 420, "mainland: {tile:?}");
        }
        assert!(robust::footprint(&mut cache, tile, 4).unwrap());
        assert_eq!(robust::search(&mut cache).unwrap(), result, "{preset}");
    }
}

#[test]
fn tiny_off_grid_clearing_is_found_by_exhaustive_fallback() {
    let terrain = Terrain::uniform(1);
    let config = WorldConfig::new(64, 64, Seed(1)).unwrap();
    let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
    for cy in 0..2 {
        for cx in 0..2 {
            cache.entries.insert(
                (cx, cy),
                (0..1024)
                    .map(|i| {
                        let (x, y) = (cx * 32 + i % 32, cy * 32 + i / 32);
                        (1..7).contains(&x) && (52..58).contains(&y)
                    })
                    .collect(),
            );
        }
    }
    assert!(matches!(
        robust::search(&mut cache).unwrap(),
        StartSearchResult::Found(_)
    ));
}

#[test]
fn a_small_passable_map_needs_only_the_footprint_and_villager_room() {
    let terrain = Terrain::uniform(1);
    for width in 1..=5 {
        let config = WorldConfig::new(width, width, Seed(1)).unwrap();
        let result = terrain.search_start_checked(config, 64, || false).unwrap();
        assert_eq!(matches!(result, StartSearchResult::Found(_)), width == 5);
    }
}

#[test]
fn menu_presets_complete_village_initialization_on_characteristic_source_fields() {
    use crate::GameWorld;
    use aoe_core::PlayerId;
    let config = WorldConfig::new(512, 512, Seed(1)).unwrap();
    for preset in ["paris", "alps", "brittany", "finland"] {
        let water = (0..1024)
            .map(|i| {
                let (x, y) = (i % 32, i / 32);
                match preset {
                    "brittany" if x > 15 => 100,
                    "finland" if (12..=19).contains(&x) && (10..=22).contains(&y) => 100,
                    _ => 0,
                }
            })
            .collect();
        let generator = landscape_fixture(
            32,
            water,
            |x, y| {
                if preset != "alps" || ((2..=6).contains(&x) && (2..=6).contains(&y)) {
                    30_000
                } else if (15..=28).contains(&x) && (15..=28).contains(&y) {
                    250_000
                } else {
                    200_000 + i32::from(x % 5 + y % 5) * 40_000
                }
            },
            if preset == "finland" { 9 } else { 28 },
        );
        let mut world = GameWorld::new(config).unwrap();
        world.terrain = Terrain::Map {
            generator,
            overlay: ResourceOverlay::default(),
        };
        let start = world
            .terrain
            .starting_tile(config)
            .unwrap_or_else(|| panic!("{preset} must start"));
        world
            .initialize_village(PlayerId(0), start)
            .unwrap_or_else(|e| panic!("{preset}: {e:?}"));
        assert_eq!(world.unit_count(), 4, "{preset}");
        assert_eq!(world.buildings().count(), 1, "{preset}");
        let tc = world.buildings().next().unwrap();
        if preset == "alps" {
            assert!(
                tc.footprint.min.x < 112 && tc.footprint.min.y < 112,
                "prefer low valley over a larger high plateau: {:?}",
                tc.footprint
            );
        }
        let center = TileCoord::new(tc.footprint.min.x + 2, tc.footprint.min.y + 2);
        for kind in [
            aoe_map::ResourceKind::Food,
            aoe_map::ResourceKind::Wood,
            aoe_map::ResourceKind::Gold,
            aoe_map::ResourceKind::Stone,
        ] {
            let found = world.starting_resources().iter().any(|n| {
                n.kind == kind
                    && n.tile.x.abs_diff(center.x).pow(2) + n.tile.y.abs_diff(center.y).pow(2)
                        <= 225
            }) || (-15_i32..=15).any(|dy| {
                (-15_i32..=15).any(|dx| {
                    dx * dx + dy * dy <= 225
                        && match &world.terrain {
                            Terrain::Map { generator, .. } => generator
                                .object_at(TileCoord::new(center.x + dx, center.y + dy))
                                .is_some_and(|n| n.kind == kind),
                            _ => false,
                        }
                })
            });
            assert!(found, "{preset}: {kind:?}");
        }
    }
}

#[test]
fn global_survey_preserves_cancellation_during_a_provider_query() {
    let config = WorldConfig::new(512, 512, Seed(1)).unwrap();
    let terrain = Terrain::Map {
        generator: flat_temperate_generator(),
        overlay: ResourceOverlay::default(),
    };
    let calls = std::cell::Cell::new(0);
    let result = terrain.search_start_checked(config, 64, || {
        calls.set(calls.get() + 1);
        calls.get() > 100
    });
    assert_eq!(result, Ok(StartSearchResult::Cancelled));
    assert!(calls.get() <= 102, "cancellation must bound further work");
}

#[test]
fn an_existing_starting_kit_does_not_move_the_geographic_start_on_reopen() {
    let config = WorldConfig::new(512, 512, Seed(1)).unwrap();
    let mut world = crate::GameWorld::new(config).unwrap();
    world.terrain = Terrain::Map {
        generator: landscape_fixture(1, vec![0], |_, _| 0, 28),
        overlay: ResourceOverlay::default(),
    };
    let before = world.terrain.starting_tile(config).unwrap();
    world
        .initialize_village(aoe_core::PlayerId(0), before)
        .unwrap();
    assert_eq!(world.terrain.starting_tile(config), Some(before));
}

#[test]
fn a_tiny_land_clearing_starts_even_when_full_resource_clusters_do_not_fit() {
    let config = WorldConfig::new(64, 64, Seed(1)).unwrap();
    let water = (0..1024)
        .map(|i| {
            if (10..=13).contains(&(i % 32)) && (10..=13).contains(&(i / 32)) {
                0
            } else {
                100
            }
        })
        .collect();
    let mut world = crate::GameWorld::new(config).unwrap();
    world.terrain = Terrain::Map {
        generator: landscape_fixture_width(64, 32, water, |_, _| 0, 28),
        overlay: ResourceOverlay::default(),
    };
    for budget in [1, 4, 64] {
        assert!(
            matches!(
                world
                    .terrain
                    .search_start_checked(config, budget, || false)
                    .unwrap(),
                StartSearchResult::Found(_)
            ),
            "positive survey budget {budget} must relax optional certificates"
        );
    }
    let start = world
        .terrain
        .starting_tile(config)
        .expect("clear footprint and villager room");
    world
        .initialize_village(aoe_core::PlayerId(0), start)
        .expect("kit space is optional on tiny islands");
    assert_eq!(world.unit_count(), 4);
    assert_eq!(world.buildings().count(), 1);
}

#[test]
fn a_large_forest_finds_its_off_lattice_opening_without_a_map_area_scan() {
    let config = WorldConfig::new(50_000, 50_000, Seed(1)).unwrap();
    let terrain = Terrain::Map {
        generator: landscape_fixture_width(50_000, 1, vec![0], |_, _| 0, 9),
        overlay: ResourceOverlay::default(),
    };
    let work = std::cell::Cell::new(0_u32);
    let result = terrain
        .search_start_checked(config, 64, || {
            work.set(work.get() + 1);
            work.get() > 10_000_000
        })
        .unwrap();
    assert!(
        matches!(result, StartSearchResult::Found(_)),
        "{result:?} after {} checks",
        work.get()
    );
}
