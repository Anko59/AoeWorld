use super::*;
use crate::{Age, StewardSettings};

fn domain(w: &mut GameWorld, shore: TileCoord) -> (EntityId, EntityId) {
    w.place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    w.set_fog_disabled(true);
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.stockpile = Stockpile {
        food: 0,
        wood: 10_000,
        gold: 10_000,
        stone: 10_000,
    };
    let anchor = TileCoord::new(shore.x - 15, shore.y + 10);
    let mut at = None;
    'town_center: for r in 0_i32..24 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let candidate = TileCoord::new(anchor.x + dx, anchor.y + dy);
                if w.validate_building_placement(BuildingKind::TownCenter, candidate)
                    .is_ok()
                {
                    at = Some(candidate);
                    break 'town_center;
                }
            }
        }
    }
    let at = at.expect("coastal domain needs a legal land Town Center");
    let footprint = aoe_core::TileRect::from_xywh(at.x, at.y, 4, 4);
    let town_center = w
        .insert_building(PlayerId(0), BuildingKind::TownCenter, footprint, true)
        .unwrap();
    let exits: Vec<_> = (at.y - 1..=at.y + 4)
        .flat_map(|y| (at.x - 1..=at.x + 4).map(move |x| TileCoord::new(x, y)))
        .filter(|tile| !footprint.contains(*tile) && w.tile_free(*tile))
        .collect();
    assert!(exits.len() >= 2);
    let king = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Steward,
            WorldPosition::from_tile_center(exits[0]).unwrap(),
        )
        .unwrap();
    w.spawn_villager(
        PlayerId(0),
        WorldPosition::from_tile_center(exits[1]).unwrap(),
    )
    .unwrap();
    w.issue_garrison(PlayerId(0), king, town_center).unwrap();
    advance(w, 2);
    (town_center, king)
}

fn bounded(w: &GameWorld) {
    let a = w.steward_work();
    assert!(a.orders <= 64 && a.resource_searches <= 8 && a.site_checks <= 32);
    assert!(a.unit_visits <= 1_024 && a.building_visits <= 2_048);
    assert!(a.resource_blocks <= 25 * a.resource_searches);
    assert!(a.resource_nodes <= 1_608 * a.resource_searches);
}

#[test]
fn coastal_steward_builds_one_real_dock_and_trains_and_employs_a_fishing_ship() {
    let mut w = coast();
    let shore = site(&w);
    let (_, king) = domain(&mut w, shore);
    // Dock construction is useful even while worker training is switched off.
    for _ in 0..20 * 160 {
        w.advance();
        bounded(&w);
    }
    let docks: Vec<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::Dock)
        .collect();
    assert_eq!(
        docks.len(),
        1,
        "one coastal domain must build exactly one Dock"
    );
    assert!(
        docks[0].is_complete(),
        "ordinary builders must finish the Dock"
    );
    assert!(docks[0].training_queue.is_empty());
    assert!(!w.units().any(|u| u.kind == UnitKind::FishingShip));
    w.issue_steward_settings(
        PlayerId(0),
        king,
        StewardSettings {
            training: true,
            ..StewardSettings::default()
        },
    )
    .unwrap();
    for _ in 0..20 * 90 {
        w.advance();
        bounded(&w);
    }
    let boats: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::FishingShip)
        .collect();
    assert!(
        !boats.is_empty(),
        "Food priority with training on must train boats"
    );
    assert!(
        boats
            .iter()
            .any(|u| w.steward_controller(u.id) == Some(king)
                && w.gathering_kind(u.id) == Some(ResourceKind::Food))
    );
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Dock)
            .count(),
        1
    );
    w.issue_steward_settings(PlayerId(0), king, StewardSettings::default())
        .unwrap();
    let land_workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    for worker in land_workers {
        w.kill_unit(w.lookup[&worker]);
    }
    let food = w.player_economy(PlayerId(0)).unwrap().stockpile.food;
    for _ in 0..20 * 180 {
        w.advance();
        bounded(&w);
    }
    assert!(
        w.player_economy(PlayerId(0)).unwrap().stockpile.food > food,
        "only the fishing fleet can deliver Food after the land workers die"
    );
}

#[test]
fn inland_steward_never_builds_a_dock() {
    let (mut w, _) = GameWorld::steward_farming_demo(aoe_core::Seed(7)).unwrap();
    let king = w.units().find(|u| u.kind == UnitKind::Steward).unwrap().id;
    advance(&mut w, 2);
    w.issue_steward_settings(
        PlayerId(0),
        king,
        StewardSettings {
            training: true,
            ..StewardSettings::default()
        },
    )
    .unwrap();
    for _ in 0..60 * w.config().tick_hz {
        w.advance();
        bounded(&w);
    }
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Dock)
            .count(),
        0
    );
    assert!(!w.units().any(|u| u.kind == UnitKind::FishingShip));
}

fn fish_near(w: &GameWorld, town_center: EntityId) -> ResourceNode {
    let home = w.building(town_center).unwrap().footprint.min;
    let crate::Terrain::Map { generator, .. } = w.terrain() else {
        panic!()
    };
    (0..5)
        .flat_map(|y| (0..8).flat_map(move |x| generator.chunk(x, y).unwrap().resources))
        .filter(|n| {
            n.object == ObjectKind::DeepFish
                && (n.tile.x - home.x).pow(2) + (n.tile.y - home.y).pow(2) < 70 * 70
        })
        .min_by_key(|n| {
            (
                (n.tile.x - home.x).pow(2) + (n.tile.y - home.y).pow(2),
                n.id,
            )
        })
        .expect("coastal domain has known deep fish")
}

fn boat_near(w: &mut GameWorld, node: ResourceNode, player: PlayerId) -> EntityId {
    for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
        if let Ok(id) = w.spawn_kind(
            player,
            UnitKind::FishingShip,
            WorldPosition::from_tile_center(TileCoord::new(node.tile.x + dx, node.tile.y + dy))
                .unwrap(),
        ) {
            return id;
        }
    }
    panic!("fish must have a navigable neighbour");
}

#[test]
fn steward_adopts_own_busy_and_idle_fishing_ships_but_preserves_manual_and_foreign_orders() {
    let mut w = coast();
    let shore = site(&w);
    let (town_center, king) = domain(&mut w, shore);
    let dock = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Dock,
            aoe_core::TileRect::from_xywh(shore.x, shore.y, 3, 3),
            true,
        )
        .unwrap();
    let fish = fish_near(&w, town_center);
    let busy = boat_near(&mut w, fish, PlayerId(0));
    let idle = boat_near(&mut w, fish, PlayerId(0));
    let manual = boat_near(&mut w, fish, PlayerId(0));
    let foreign = boat_near(&mut w, fish, PlayerId(1));
    let outside = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::FishingShip,
            WorldPosition::from_tile_center(TileCoord::new(
                w.config().width_tiles - 3,
                w.config().height_tiles - 3,
            ))
            .unwrap(),
        )
        .unwrap();
    w.issue_gather(PlayerId(0), busy, fish.id).unwrap();
    let destination =
        WorldPosition::from_tile_center(TileCoord::new(fish.tile.x + 25, fish.tile.y)).unwrap();
    w.issue_move(manual, destination).unwrap();
    advance(&mut w, 40);
    assert_eq!(w.steward_controller(busy), Some(king));
    assert_eq!(w.steward_controller(idle), Some(king));
    assert_eq!(w.gathering_kind(idle), Some(ResourceKind::Food));
    assert_eq!(w.steward_controller(manual), None);
    assert_eq!(w.steward_controller(foreign), None);
    assert_eq!(w.steward_controller(outside), None);
    assert!(w.unit(manual).unwrap().moving || w.unit(manual).unwrap().planning);
    assert!(
        w.building(dock).unwrap().training_queue.is_empty(),
        "training defaults off"
    );
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Dock)
            .count(),
        1,
        "reuse the existing reachable Dock"
    );
    w.issue_move(idle, destination).unwrap();
    assert_eq!(
        w.steward_controller(idle),
        None,
        "manual override releases ships immediately"
    );
    w.issue_ungarrison(PlayerId(0), town_center, Some(king))
        .unwrap();
    assert_eq!(
        w.steward_controller(busy),
        None,
        "leaving releases the fleet too"
    );
}

#[test]
fn fishing_training_obeys_food_priority_and_naval_management_replays() {
    let make = || {
        let mut w = coast();
        let shore = site(&w);
        let (town_center, king) = domain(&mut w, shore);
        let dock = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Dock,
                aoe_core::TileRect::from_xywh(shore.x, shore.y, 3, 3),
                true,
            )
            .unwrap();
        let fish = fish_near(&w, town_center);
        let boat = boat_near(&mut w, fish, PlayerId(0));
        w.issue_steward_settings(
            PlayerId(0),
            king,
            StewardSettings {
                weights: [0, 100, 0, 0],
                training: true,
                ..Default::default()
            },
        )
        .unwrap();
        (w, king, dock, boat)
    };
    let (mut w, king, dock, boat) = make();
    let (mut replay, replay_king, replay_dock, replay_boat) = make();
    for _ in 0..40 {
        w.advance();
        replay.advance();
        assert_eq!(w.canonical_hash(), replay.canonical_hash());
        bounded(&w);
        bounded(&replay);
        assert!(
            w.building(dock).unwrap().training_queue.is_empty(),
            "Wood priority must not train fishing ships"
        );
        assert!(
            replay
                .building(replay_dock)
                .unwrap()
                .training_queue
                .is_empty()
        );
    }
    for (world, steward) in [(&mut w, king), (&mut replay, replay_king)] {
        world
            .issue_steward_settings(
                PlayerId(0),
                steward,
                StewardSettings {
                    training: true,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    for _ in 0..20 * 120 {
        w.advance();
        replay.advance();
        bounded(&w);
        bounded(&replay);
        assert_eq!(w.canonical_hash(), replay.canonical_hash());
    }
    assert_eq!(w.steward_controller(boat), Some(king));
    assert_eq!(replay.steward_controller(replay_boat), Some(replay_king));
    assert_eq!(w.gathering_kind(boat), Some(ResourceKind::Food));
}

#[test]
fn idle_ships_without_local_fish_do_not_starve_land_worker_orders() {
    let mut w = coast();
    let shore = site(&w);
    let (_, king) = domain(&mut w, shore);
    let worker = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
    let tile = (shore.y..shore.y + 60)
        .flat_map(|y| (shore.x + 5..shore.x + 60).map(move |x| TileCoord::new(x, y)))
        .find(|tile| {
            w.unit_tile_passable(UnitKind::FishingShip, *tile, &|| false)
                .unwrap()
                && w.economy.steward_homes[&king]
                    .owns(WorldPosition::from_tile_center(*tile).unwrap())
                && w.nearest_resource_matching(*tile, ResourceKind::Food, 10, None, |n| {
                    matches!(n.object, ObjectKind::DeepFish | ObjectKind::ShoreFish)
                })
                .is_none()
        })
        .expect("domain has navigable water without nearby fish");
    for _ in 0..100 {
        w.spawn_kind(
            PlayerId(0),
            UnitKind::FishingShip,
            WorldPosition::from_tile_center(tile).unwrap(),
        )
        .unwrap();
    }
    for _ in 0..40 {
        w.advance();
        bounded(&w);
    }
    assert_eq!(
        w.steward_controller(worker),
        Some(king),
        "unemployed boats must leave productive probes for land workers"
    );
    assert!(w.gathering_kind(worker).is_some() || w.economy.builders.contains_key(&worker));
}

#[test]
fn fishing_showcase_uses_real_coast_building_training_and_fleet_orders() {
    let (mut w, primary) = GameWorld::steward_fishing_demo(aoe_core::Seed(1)).unwrap();
    assert_eq!(
        w.config().tick_hz,
        1,
        "explicitly accelerated local recording clock"
    );
    assert_eq!(w.unit(primary).unwrap().kind, UnitKind::Scout);
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Dock)
            .count(),
        0
    );
    assert_eq!(
        w.units()
            .filter(|u| u.kind == UnitKind::FishingShip)
            .count(),
        1
    );
    for _ in 0..600 {
        w.advance();
        bounded(&w);
    }
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Dock)
            .any(|b| b.is_complete())
    );
    let king = w
        .buildings()
        .flat_map(|b| &b.garrison)
        .find(|g| g.unit.kind == UnitKind::Steward)
        .unwrap()
        .unit
        .id;
    assert!(
        w.units()
            .filter(|u| u.kind == UnitKind::FishingShip)
            .count()
            >= 2
    );
    assert!(w.units().any(|u| u.kind == UnitKind::FishingShip
        && w.steward_controller(u.id) == Some(king)
        && w.gathering_kind(u.id) == Some(ResourceKind::Food)));
    assert!(w.player_economy(PlayerId(0)).unwrap().stockpile.food > 0);
}
