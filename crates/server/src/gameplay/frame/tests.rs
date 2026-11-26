use super::*;
use aoe_core::{Seed, WorldConfig};

/// Ids per page of the slot index.
const INDEX_PAGE: u32 = 256;

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
        // A page of ids is held exactly while the world has a unit of it.
        let pages = world.units().map(|u| u.id.0 / INDEX_PAGE);
        assert_eq!(
            self.index.pages(),
            pages.collect::<BTreeSet<_>>().len(),
            "index pages at {:?}",
            world.tick()
        );
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
    // `index[&first]` would answer `ABSENT`, which is never unchanged either.
    let slot = frame.index.get(first).expect("the corpse keeps its slot");
    assert_eq!(frame.state(slot).id, first);
    assert!(!frame.unchanged_since(Some(1), slot));
    world.advance();
    let frame = UnitFrame::build(&world, 1, 3, Some(frame)).unwrap();
    frame.assert_matches_world(&world);
    assert_eq!(frame.visited(), 0);
}

#[path = "tests/slot_index.rs"]
mod slot_index;

#[test]
fn a_kept_frame_equals_a_rebuilt_one_through_spawns_moves_deaths_reentries_and_id_gaps() {
    use aoe_simulation::{AnimalKind, UnitKind};
    let at = |x: i32, y: i32| {
        aoe_core::WorldPosition::from_tile_center(aoe_core::TileCoord::new(x, y)).unwrap()
    };
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap();
    let hall = world
        .place_town_center(PlayerId(0), aoe_core::TileCoord::new(30, 30))
        .unwrap();
    world
        .place_town_center(PlayerId(1), aoe_core::TileCoord::new(80, 30))
        .unwrap();
    // Player 2 has no economy: its sessions see a region and a primary unit.
    let crowd = (0..600)
        .map(|n| {
            let place = at(5 + n % 40 * 3, 90 + n / 40 * 2);
            world.spawn_unit(PlayerId(2), place).unwrap()
        })
        .collect::<Vec<_>>();
    // At the hall's sides, and in the frame before they ever shelter: a unit
    // that enters on its first tick comes out as one the frame never had.
    let shelter = [(29, 30), (29, 32), (34, 30), (34, 32)]
        .map(|(x, y)| world.spawn_villager(PlayerId(0), at(x, y)).unwrap());
    world.advance();
    let mut frame = UnitFrame::build(&world, 4, 1, None).unwrap();
    let states = |frame: &UnitFrame, world: &GameWorld| {
        world
            .units()
            .map(|u| (u.id, *frame.state(frame.index[&u.id])))
            .collect::<BTreeMap<_, _>>()
    };
    let mut held = states(&frame, &world);
    let mut gone = BTreeSet::new();
    let (mut reentered, mut pooled) = (0, 0);
    let ticks = u64::from(world.config().tick_hz) * (u64::from(aoe_simulation::CORPSE_SECONDS) + 8);
    for generation in 2..=ticks {
        // A scripted start reaches every case; drawn orders follow it.
        match generation {
            // Early enough for the corpse to leave during the run.
            2 => world.issue_delete(PlayerId(2), &crowd[599..]).unwrap(),
            3 => {
                for id in shelter {
                    world.issue_garrison(PlayerId(0), id, hall).unwrap();
                }
            }
            // Others take two of the four slots before their units are back.
            4 => {
                for x in [60, 62] {
                    world.spawn_unit(PlayerId(1), at(x, 60)).unwrap();
                }
            }
            6 => world.issue_ungarrison(PlayerId(0), hall, None).unwrap(),
            _ => {}
        }
        let orders = if generation < 8 { 0 } else { 1 + next(2) };
        for _ in 0..orders {
            let player = PlayerId(next(3) as u16);
            let place = at(4 + next(120) as i32, 4 + next(120) as i32);
            let living = world
                .units()
                .filter(|u| u.hp > 0)
                .map(|u| (u.id, u.player))
                .collect::<Vec<_>>();
            let (id, owner) = living[next(living.len() as u64) as usize];
            // Refused orders are part of the sequence: they change nothing.
            match next(8) {
                0 | 1 => {
                    let kind = [UnitKind::Scout, UnitKind::Villager, UnitKind::Militia];
                    let _ = world.spawn_kind(player, kind[next(3) as usize], place);
                }
                2 | 3 => {
                    let _ = world.issue_move(id, place);
                }
                4 => {
                    let _ = world.issue_delete(owner, &[id]);
                }
                // Animals take ids no unit ever has.
                5 => {
                    let _ = world.spawn_animal(AnimalKind::Sheep, place);
                }
                // A sheltered unit leaves the world and comes back under its id.
                6 => {
                    let id = shelter[next(4) as usize];
                    let _ = world.issue_garrison(PlayerId(0), id, hall);
                }
                _ => {
                    let _ = world.issue_ungarrison(PlayerId(0), hall, None);
                }
            }
        }
        if generation == 30 {
            for id in &crowd {
                if let Some(unit) = world.unit(*id) {
                    let tile = unit.position.tile_floor();
                    let _ = world.issue_move(*id, at(tile.x + 1, tile.y));
                }
            }
        }
        world.advance();
        frame = UnitFrame::build(&world, 4, generation, Some(frame)).unwrap();
        frame.assert_matches_world(&world);
        pooled += usize::from(frame.work().0 >= 2 * CONVERT_TASK);
        let rebuilt = UnitFrame::build(&world, 1, 0, None).unwrap();
        for (viewer, region) in [
            (0, TileRect::from_xywh(16, 16, 48, 48)),
            (1, TileRect::from_xywh(0, 0, 128, 128)),
            (2, TileRect::from_xywh(64, 80, 40, 40)),
        ] {
            let lens = Lens {
                viewer: PlayerId(viewer),
                primary: crowd[0],
                region,
            };
            let seen = |frame: &UnitFrame| {
                frame
                    .current(&world, &lens, &mut Scratch::default())
                    .into_iter()
                    .map(|slot| *frame.state(slot))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                seen(&frame),
                seen(&rebuilt),
                "viewer {viewer} at {generation}"
            );
        }
        // A session is sent a unit it holds only when its slot is marked: the
        // marks must be exactly the units that are new or differ.
        let now = states(&frame, &world);
        for (id, state) in &now {
            assert_eq!(
                frame.changed.contains(frame.index[id]),
                held.get(id) != Some(state),
                "{id:?} at {generation}"
            );
        }
        if generation < 8 {
            let inside = (3..6).contains(&generation);
            for id in shelter {
                assert_eq!(now.contains_key(&id), !inside, "{id:?} at {generation}");
                assert_eq!(frame.index.get(id).is_none(), inside, "{id:?}");
            }
        }
        gone.extend(held.keys().copied().filter(|id| !now.contains_key(id)));
        reentered += now.keys().filter(|id| gone.remove(*id)).count();
        held = now;
    }
    assert!(
        reentered >= shelter.len() && pooled > 0 && !gone.is_empty(),
        "{reentered} reentries, {pooled} pooled ticks, {} left",
        gone.len()
    );
}
