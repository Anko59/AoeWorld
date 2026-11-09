use super::*;
use aoe_core::{Seed, WorldConfig};

fn idle_world() -> GameWorld {
    let mut world = GameWorld::new(WorldConfig::new(1024, 1024, Seed(7)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    for raw in 0..13_400 {
        let tile = aoe_core::TileCoord::new(10 + raw % 200 * 4, 10 + raw / 200 * 4);
        let at = aoe_core::WorldPosition::from_tile_center(tile).unwrap();
        let player = PlayerId((raw % 100) as u16);
        let id = world.spawn_unit(player, at).unwrap();
        world
            .issue_stance(player, &[id], aoe_core::CombatStance::NoAttack)
            .unwrap();
    }
    world.advance();
    world.advance();
    world
}

#[test]
fn idle_frame_measurement() {
    let mut world = idle_world();
    let mut previous = UnitFrame::build(&world, 1, 1, None).unwrap();
    let mut times = Vec::new();
    let mut visited = 0;
    for generation in 2..=301 {
        assert!(world.advance().is_empty(), "measurement must remain idle");
        assert!(
            world.unit_changes().is_empty(),
            "idle units were marked changed"
        );
        let started = std::time::Instant::now();
        let frame = UnitFrame::build(&world, 1, generation, Some(previous)).unwrap();
        times.push(started.elapsed().as_nanos());
        visited += frame.visited();
        previous = frame;
    }
    assert_eq!(visited, 0, "idle frames must not visit the population");
    times.sort_unstable();
    let report = format!(
        "units=13400 frames=300 visited={visited} p50_ns={} p95_ns={}\n",
        times[149], times[284]
    );
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/phase1-wire");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("frame-measurement.txt"), report).unwrap();
}

impl UnitFrame {
    pub(in crate::gameplay) fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Deliberately scans and converts independently of the incremental path.
    /// Every test fan-out checks this; the direct build benchmark omits it.
    pub(in crate::gameplay) fn assert_matches_world(&self, world: &GameWorld) {
        let mut owners = BTreeMap::<PlayerId, Vec<u64>>::new();
        let mut count = 0;
        for unit in world.units() {
            count += 1;
            let slot = self.index[&unit.id];
            assert_eq!(
                *self.state(slot),
                super::super::economy::unit_state(world, &unit),
                "unit {:?} at {:?}",
                unit.id,
                world.tick()
            );
            assert_eq!(
                self.rank[slot as usize],
                world.unit_storage_slot(unit.id).unwrap()
            );
            assert_eq!(self.owner[slot as usize], unit.player.0);
            if world.unit_visible_to(unit.player, unit.id) {
                let own = owners.entry(unit.player).or_default();
                if own.len() <= MAX_SUBSCRIBED_UNITS {
                    own.push(key(unit.id, slot));
                }
            }
        }
        assert_eq!(count, self.index.len());
        for (player, own) in &self.own {
            let mut expected = owners.remove(player).unwrap_or_default();
            expected.sort_unstable();
            assert_eq!(
                own.keys.iter().copied().collect::<Vec<_>>(),
                expected,
                "owner {player:?} at {:?}",
                world.tick()
            );
        }
        assert!(owners.is_empty());
    }
}

#[test]
fn one_mover_among_thousands_visits_only_changed_units_and_keeps_idle_lists() {
    let mut world = idle_world();
    let frame = UnitFrame::build(&world, 1, 1, None).unwrap();
    let idle_owner = PlayerId(99);
    let list = frame.own[&idle_owner].keys.first().unwrap() as *const u64;
    let unit = world.units().find(|u| u.player == PlayerId(0)).unwrap();
    let destination = aoe_core::WorldPosition {
        x: unit.position.x
            + if unit.position.tile_floor().x < 1023 {
                1024
            } else {
                -1024
            },
        y: unit.position.y,
    };
    assert!(world.issue_move(unit.id, destination).unwrap());
    world.advance();
    let frame = UnitFrame::build(&world, 1, 2, Some(frame)).unwrap();
    frame.assert_matches_world(&world);
    assert_eq!(frame.visited(), 1);
    assert_eq!(
        frame.own[&idle_owner].keys.first().unwrap() as *const u64,
        list,
        "idle owner list was rebuilt"
    );
}

#[test]
fn spawn_death_and_swap_removal_match_full_scan_each_tick() {
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap();
    let at =
        |x| aoe_core::WorldPosition::from_tile_center(aoe_core::TileCoord::new(x, 10)).unwrap();
    let first = world.spawn_unit(PlayerId(0), at(10)).unwrap();
    let last = world.spawn_unit(PlayerId(0), at(12)).unwrap();
    world.advance();
    let mut frame = UnitFrame::build(&world, 1, 1, None).unwrap();
    world.issue_delete(PlayerId(0), &[first]).unwrap();
    let ticks = u64::from(world.config().tick_hz) * (u64::from(aoe_simulation::CORPSE_SECONDS) + 4);
    for generation in 2..=ticks {
        if generation == 100 {
            world.spawn_unit(PlayerId(1), at(14)).unwrap();
        }
        world.advance();
        frame = UnitFrame::build(&world, 1, generation, Some(frame)).unwrap();
        frame.assert_matches_world(&world);
    }
    assert!(world.unit(first).is_none());
    assert!(world.unit(last).is_some());
}

#[test]
fn capped_owner_list_promotes_overflow_and_applies_storage_order_before_id_order() {
    let mut own = Owned::default();
    let cap = MAX_SUBSCRIBED_UNITS + 1;
    let mut truth = BTreeMap::new();
    for rank in 1..cap + 10 {
        let key = key(EntityId((cap + 10 - rank) as u32), rank as u32);
        own.insert(rank, key);
        truth.insert(rank, key);
    }
    let check = |own: &Owned, truth: &BTreeMap<usize, u64>| {
        let expected = truth.values().take(cap).copied().collect::<BTreeSet<_>>();
        assert_eq!(own.keys, expected);
    };
    check(&own, &truth);
    for rank in [1, cap + 8, cap / 2, cap + 1] {
        own.remove(rank);
        truth.remove(&rank);
        check(&own, &truth);
    }
    own.insert(0, key(EntityId(200_000), 0));
    truth.insert(0, key(EntityId(200_000), 0));
    check(&own, &truth);
}

#[test]
fn missing_tick_rebuilds_once_instead_of_losing_earlier_changes() {
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap();
    let at =
        |x| aoe_core::WorldPosition::from_tile_center(aoe_core::TileCoord::new(x, 10)).unwrap();
    let first = world.spawn_unit(PlayerId(0), at(10)).unwrap();
    world.advance();
    let frame = UnitFrame::build(&world, 1, 1, None).unwrap();
    world.issue_delete(PlayerId(0), &[first]).unwrap();
    world.advance();
    world.advance();
    let frame = UnitFrame::build(&world, 1, 2, Some(frame)).unwrap();
    frame.assert_matches_world(&world);
    assert_eq!(frame.visited(), world.unit_count());
    assert!(!frame.unchanged_since(Some(1), frame.index[&first]));
    world.advance();
    let frame = UnitFrame::build(&world, 1, 3, Some(frame)).unwrap();
    frame.assert_matches_world(&world);
    assert_eq!(frame.visited(), 0);
}
