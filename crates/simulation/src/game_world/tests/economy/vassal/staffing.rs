use super::*;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn vassal_focus_is_owned_and_survives_a_save() {
    let (mut w, vassal, _) = setup();
    let owner = PlayerId(0);
    let focus = crate::VassalFocus::Roads;
    assert_eq!(
        w.issue_vassal_focus(PlayerId(1), vassal, focus),
        Err(EconomyError::NotOwner)
    );
    w.issue_vassal_focus(owner, vassal, focus).unwrap();
    let (restored, _) = crate::game_world::tests::snapshot_roundtrip::roundtrip(&w, world());
    assert_eq!(restored.vassal_status(vassal).unwrap().focus, focus);
}

#[test]
fn vassal_road_focus_reorders_the_paused_work_plan() {
    let (mut w, vassal, _) = setup();
    w.set_fog_disabled(true);
    w.issue_vassal_development(PlayerId(0), vassal, false)
        .unwrap();
    assert!(!w.vassal_network(vassal).is_empty());
    advance(&mut w, 20);
    assert_eq!(
        w.vassal_status(vassal).unwrap().planned.recent[0]
            .unwrap()
            .kind,
        BuildingKind::WatchTower,
    );
    w.issue_vassal_focus(PlayerId(0), vassal, crate::VassalFocus::Roads)
        .unwrap();
    advance(&mut w, 20);
    let report = w.vassal_status(vassal).unwrap();
    assert!(matches!(
        report.planned.recent[0].unwrap().kind,
        BuildingKind::Road | BuildingKind::Bridge
    ));
    assert_eq!(report.current.count, 0);
}

#[test]
fn vassal_foundations_share_villagers_with_the_economy() {
    let (mut w, vassal, _) = setup();
    let player = PlayerId(0);
    w.set_fog_disabled(true);
    let town = w
        .player_buildings_of(player, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let steward = w.spawn_kind(player, UnitKind::Steward, at(63, 64)).unwrap();
    w.issue_garrison(player, steward, town).unwrap();
    for i in 0..24 {
        w.spawn_villager(player, at(80 + i % 6, 78 + i / 6))
            .unwrap();
    }
    let existing: BTreeSet<_> = w.buildings().map(|b| b.id).collect();
    let mut idle = 0;
    let mut total = 0;
    let mut controlled = 0;
    let seconds = 180;
    let tick_hz = w.config.tick_hz;
    for _ in 0..seconds {
        advance(&mut w, tick_hz);
        for u in w
            .units()
            .filter(|u| u.player == player && u.kind == UnitKind::Villager && u.hp > 0)
        {
            total += 1;
            idle += usize::from(u.task == UnitTask::Idle && !u.moving && !u.planning);
            controlled += usize::from(w.vassal_controller(u.id).is_some());
        }
    }
    let newly_complete = w
        .buildings()
        .filter(|b| b.player == player && b.is_complete() && !existing.contains(&b.id))
        .count();
    eprintln!("{newly_complete} new own foundations complete in {seconds}s");
    let finished = w.vassal_status(vassal).unwrap().finished.count;
    eprintln!(
        "{finished} foundations in {seconds}s; idle {idle}/{total} villager-seconds; Vassal-controlled {controlled}/{total}"
    );
    assert_eq!(
        controlled, 0,
        "Vassals plan buildings and never own villagers"
    );
    // The same seeded scenario with Vassal rosters completed 68 foundations
    // and recorded 37 idle samples out of 4,860.
    assert_eq!(total, 4_860);
    assert!(finished >= 68, "foundation throughput fell below baseline");
    assert!(idle <= 37, "idle share exceeded baseline: {idle}/{total}");
}

#[test]
fn shared_foundations_respect_the_construction_share_and_restore_the_queue() {
    let (mut w, vassal, _) = setup();
    let player = PlayerId(0);
    w.set_fog_disabled(true);
    w.issue_vassal_development(player, vassal, false).unwrap();
    let villagers: Vec<_> = (0..20)
        .map(|i| {
            w.spawn_villager(player, at(102 + i % 5, 105 + i / 5))
                .unwrap()
        })
        .collect();
    let enemy = w.spawn_villager(PlayerId(1), at(106, 110)).unwrap();
    let sites = w
        .issue_build_wall(
            player,
            &[],
            BuildingKind::Road,
            TileCoord::new(102, 110),
            TileCoord::new(116, 110),
            false,
        )
        .unwrap();
    assert!(w.economy.builders.is_empty());
    assert!(
        sites
            .iter()
            .all(|id| w.building(*id).unwrap().is_unstarted())
    );
    let (mut w, _) = crate::game_world::tests::snapshot_roundtrip::roundtrip(&w, world());
    assert!(
        sites
            .iter()
            .all(|id| w.economy.dispatch.sites.contains_key(id))
    );
    let cap = w.unit_census()[&player][UnitKind::Villager as usize] as usize / 2;
    let mut peak = 0;
    for _ in 0..600 {
        w.advance();
        let assigned = villagers
            .iter()
            .filter(|id| w.economy.builders.contains_key(id))
            .count();
        peak = peak.max(assigned);
        assert!(w.economy.builders.len() <= cap);
        assert!(!w.economy.builders.contains_key(&enemy));
    }
    assert!(peak >= 2, "parallel work while slots are free");
    assert!(
        sites
            .iter()
            .all(|id| w.building(*id).unwrap().is_complete()),
        "{:?}",
        sites
            .iter()
            .map(|id| {
                let b = w.building(*id).unwrap();
                (b.footprint.min, b.work_done, b.work_total)
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_paused_vassal_leaves_its_paid_foundations_with_the_shared_builders() {
    let (mut w, vassal, _) = setup();
    let player = PlayerId(0);
    w.set_fog_disabled(true);
    for i in 0..16 {
        w.spawn_villager(player, at(95 + i % 4, 91 + i / 4))
            .unwrap();
    }
    advance(&mut w, 20);
    let paid = w
        .economy
        .dispatch
        .sites
        .iter()
        .filter(|(_, owner)| **owner == Some(vassal))
        .map(|(&id, _)| id)
        .collect::<Vec<_>>();
    assert!(!paid.is_empty());
    w.issue_vassal_development(player, vassal, false).unwrap();
    advance(&mut w, 1800);
    assert!(paid.iter().any(|&id| w.building(id).unwrap().is_complete()));
    assert!(
        w.economy
            .dispatch
            .sites
            .iter()
            .all(|(id, owner)| *owner != Some(vassal) || paid.contains(id))
    );
    assert!(w.vassal_status(vassal).unwrap().finished.count > 0);
}

#[test]
fn a_borrowed_steward_villager_returns_to_its_farm() {
    let (mut w, vassal, _) = setup();
    let player = PlayerId(0);
    w.set_fog_disabled(true);
    w.issue_vassal_development(player, vassal, false).unwrap();
    let starting = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    w.issue_delete(player, &starting).unwrap();
    let town = w
        .player_buildings_of(player, BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let steward = w.spawn_kind(player, UnitKind::Steward, at(63, 64)).unwrap();
    w.issue_garrison(player, steward, town).unwrap();
    let worker = w.spawn_villager(player, at(70, 72)).unwrap();
    let farm = w
        .insert_building(
            player,
            BuildingKind::Farm,
            TileRect::from_xywh(72, 72, 3, 3),
            true,
        )
        .unwrap();
    advance(&mut w, 40);
    w.economy.stewards.automatic = true;
    w.issue_farm(player, worker, farm).unwrap();
    w.economy.stewards.automatic = false;
    assert_eq!(w.steward_controller(worker), Some(steward));

    let house = w
        .issue_build(
            player,
            &[],
            BuildingKind::House,
            TileCoord::new(76, 72),
            false,
        )
        .unwrap();
    let mut borrowed = false;
    for _ in 0..1_200 {
        w.advance();
        borrowed |= w.steward_has_held_job(worker);
        if borrowed
            && w.building(house).unwrap().is_complete()
            && w.unit(worker)
                .is_some_and(|unit| unit.task == UnitTask::Farming)
        {
            assert_eq!(w.steward_controller(worker), Some(steward));
            return;
        }
    }
    panic!(
        "borrowed {borrowed}; villager {:?}; house {:?}",
        w.unit(worker),
        w.building(house)
    );
}

#[test]
fn far_placed_foundations_borrow_manual_gatherers_and_return_them() {
    let (mut w, node, _) = resource_world(aoe_map::ResourceKind::Wood);
    let player = PlayerId(0);
    w.initialize_village(player, node.tile).unwrap();
    w.set_fog_disabled(true);
    w.economy.players.get_mut(&player).unwrap().stockpile = Stockpile {
        food: 10_000,
        wood: 10_000,
        gold: 10_000,
        stone: 10_000,
    };
    let starting = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    w.issue_delete(player, &starting).unwrap();
    let mut workers = Vec::new();
    'search: for dy in -12..=12 {
        for dx in -12..=12 {
            let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
            let Ok(position) = WorldPosition::from_tile_center(tile) else {
                continue;
            };
            let Ok(worker) = w.spawn_villager(player, position) else {
                continue;
            };
            if w.issue_gather(player, worker, node.id).is_ok() {
                workers.push(worker);
            } else {
                w.issue_delete(player, &[worker]).unwrap();
            }
            if workers.len() == 12 {
                break 'search;
            }
        }
    }
    assert_eq!(workers.len(), 12);
    advance(&mut w, 40);
    assert!(
        workers
            .iter()
            .all(|id| w.economy.gatherers.contains_key(id))
    );
    assert!(w.economy.stewards.controlled.is_empty());
    // A far-view placement names no builder: the shared queue recruits them.
    let house = w
        .issue_build(
            player,
            &[],
            BuildingKind::House,
            TileCoord::new(node.tile.x + 8, node.tile.y + 8),
            false,
        )
        .unwrap();
    let mut peak = 0;
    for _ in 0..3_000 {
        w.advance();
        peak = peak.max(
            workers
                .iter()
                .filter(|id| w.economy.builders.contains_key(id))
                .count(),
        );
        if w.building(house).unwrap().is_complete() {
            break;
        }
    }
    assert!(w.building(house).unwrap().is_complete());
    assert!(peak >= 1, "no manual gatherer was recruited");
    // At most a quarter of 12 villagers are lent.
    assert!(peak <= 3, "peak {peak}");
    advance(&mut w, 60);
    assert!(w.economy.dispatch.lent.is_empty());
    assert!(
        workers
            .iter()
            .all(|id| w.economy.gatherers.contains_key(id) && !w.economy.builders.contains_key(id))
    );
}
