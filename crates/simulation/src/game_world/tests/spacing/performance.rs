//! Count actual idle-separation visits, rather than asserting shared-host timings.
use super::*;

#[test]
fn tile_occupancy_updates_visit_only_eight_adjacent_cells() {
    let mut index = crate::game_world::spacing::IdleSpacing::default();
    for n in 0..2_000 {
        index.insert(EntityId(n), TileCoord::new(n as i32 * 4, 4));
    }
    let before = index.adjacent_visits;
    index.relocate(EntityId(0), TileCoord::new(0, 4), TileCoord::new(1, 4));
    assert_eq!(
        index.adjacent_visits - before,
        16,
        "one removal and one insertion"
    );
    for phase in 0..4 {
        assert_eq!(index.candidates(phase).count(), 0);
    }
}

#[test]
fn isolated_idle_population_does_no_separation_work() {
    let config = WorldConfig::new(1_024, 1_024, Seed(7)).unwrap();
    let mut w = GameWorld::new(config).unwrap();
    w.set_fog_disabled(true);
    for n in 0..8_000 {
        w.spawn_unit(PlayerId(0), at(4 + 4 * (n % 200), 4 + 4 * (n / 200)))
            .unwrap();
    }
    for _ in 0..8 {
        assert!(w.advance().is_empty());
        assert_eq!(w.idle_spacing.inspected_units, 0);
    }
}

#[test]
fn idle_separation_work_depends_on_the_local_crowd() {
    let mut w = world();
    w.set_fog_disabled(true);
    let a = w.spawn_villager(PlayerId(0), at(30, 30)).unwrap();
    let b = w.spawn_villager(PlayerId(1), at(30, 30)).unwrap();
    for n in 0..2_000 {
        w.spawn_unit(PlayerId(0), at(4 + 4 * (n % 50), 60 + 4 * (n / 50)))
            .unwrap();
    }
    let mut visits = 0;
    for _ in 0..4 {
        w.advance();
        visits += w.idle_spacing.inspected_units;
    }
    assert_eq!(visits, 2, "only the overlapping pair is inspected");
    assert_ne!(w.unit(a).unwrap().position, w.unit(b).unwrap().position);
}

fn indexed_crowd() -> GameWorld {
    let mut w = world();
    w.set_fog_disabled(true);
    for n in 0..96 {
        // Include exact stacks, adjacent tiles, eight-tile cell boundaries,
        // and distant singletons. Positions need not be tile centres.
        let position = if n >= 94 {
            at(32, 32) // removal moves an interacting unit ahead of the crowd
        } else if n < 48 {
            WorldPosition::new(32 * 1024 + (n % 4) * 400, 32 * 1024 + (n / 4) * 120)
        } else {
            at(80 + (n % 12) * 4, 80 + (n / 12) * 4)
        };
        let id = w
            .spawn_villager(PlayerId((n % 2) as u16), position)
            .unwrap();
        if n % 7 == 0 {
            w.issue_move(id, at(40, 33)).unwrap();
        }
    }
    w
}

#[test]
fn indexed_spacing_matches_population_scan_through_moves_stops_deaths_and_removal() {
    let (mut indexed, mut exhaustive) = (indexed_crowd(), indexed_crowd());
    for tick in 0..200 {
        for w in [&mut indexed, &mut exhaustive] {
            if tick == 11 {
                w.issue_stop(PlayerId(0), EntityId(0)).unwrap();
            }
            if tick == 23 {
                let index = w.lookup[&EntityId(1)];
                w.kill_unit(index);
            }
            if tick == 31 {
                w.remove_unit(EntityId(2)); // swaps the last unit into this slot
            }
            if tick == 43 {
                w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
                w.spawn_villager(PlayerId(1), at(100, 100)).unwrap();
            }
        }
        let mut actual = indexed.advance_movement();
        let mut expected = exhaustive.advance_movement();
        indexed.advance_spacing(&mut actual);
        let indices = (0..exhaustive.units.len()).collect::<Vec<_>>();
        exhaustive.advance_spacing_indices(&mut expected, indices);
        indexed.refill_route_budgets();
        exhaustive.refill_route_budgets();
        assert_eq!(actual, expected, "changed order at tick {tick}");
        assert_eq!(
            indexed.canonical_hash(),
            exhaustive.canonical_hash(),
            "tick {tick}"
        );
    }
}
