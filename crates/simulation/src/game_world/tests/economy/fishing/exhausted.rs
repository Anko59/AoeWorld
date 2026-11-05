use super::steward::{boat_near, bounded, domain, fish_near};
use super::*;
use crate::StewardSettings;

/// With every fish gone, Food demand falls back to Farms and no idle boat
/// stays managed. Measured before and after this change: 5 Farms either way.
#[test]
fn exhausted_fishing_releases_boats_and_food_demand_falls_back_to_farms() {
    let mut w = coast();
    let shore = site(&w);
    let (town_center, king) = domain(&mut w, shore);
    let footprint = w.building(town_center).unwrap().footprint;
    let mut villagers = 0;
    'ring: for r in 2..8 {
        for dy in -r..=r {
            for dx in -r..=r {
                let tile = TileCoord::new(footprint.min.x + 2 + dx, footprint.min.y + 2 + dy);
                if dx.abs().max(dy.abs()) == r
                    && w.tile_free(tile)
                    && w.spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
                        .is_ok()
                {
                    villagers += 1;
                    if villagers == 12 {
                        break 'ring;
                    }
                }
            }
        }
    }
    let crate::Terrain::Map { generator, .. } = w.terrain() else {
        panic!()
    };
    let fish: Vec<_> = (0..5)
        .flat_map(|y| (0..8).flat_map(move |x| generator.chunk(x, y).unwrap().resources))
        .filter(|n| matches!(n.object, ObjectKind::DeepFish | ObjectKind::ShoreFish))
        .collect();
    let home = footprint.min;
    let mut near: Vec<_> = fish
        .iter()
        .filter(|n| (n.tile.x - home.x).pow(2) + (n.tile.y - home.y).pow(2) < 70 * 70)
        .copied()
        .collect();
    near.sort_by_key(|n| n.id);
    let boats: Vec<_> = near
        .iter()
        .take(12)
        .map(|n| boat_near(&mut w, *n, PlayerId(0)))
        .collect();
    assert_eq!(boats.len(), 12, "need a dozen boats");
    for node in fish {
        let _ = w.deplete_resource(node.id, node.initial_amount);
    }
    let mut searches = 0;
    for _ in 0..20 * 120 {
        w.advance();
        bounded(&w);
        searches += w.steward_work().resource_searches;
    }
    let farms = w
        .player_buildings_of(PlayerId(0), BuildingKind::Farm)
        .count();
    let held = boats
        .iter()
        .filter(|b| w.steward_controller(**b) == Some(king))
        .count();
    let farmers = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager && u.task == UnitTask::Farming)
        .count();
    eprintln!("fishing: farms {farms} farmers {farmers} boats held {held}/12 searches {searches}");
    assert!(farms >= 1 && farmers >= 1, "Food must fall back to Farms");
    assert_eq!(held, 0, "exhausted boats must be released");
}

/// Docks keep adding ships while every earlier ship sits frozen "Moving" beside
/// the Dock (the owner's snapshots: 18 such ships, 14 within three tiles of a
/// Dock). A boat that cannot fish is not food supply, so none may be added.
#[test]
fn a_frozen_boat_stops_the_dock_from_training_more_ships() {
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
    let index = w.lookup[&boat];
    w.units[index].state.task = UnitTask::Moving;
    assert!(!w.unit(boat).unwrap().moving && !w.unit(boat).unwrap().planning);
    w.issue_steward_settings(
        PlayerId(0),
        king,
        StewardSettings {
            training: true,
            weights: [100, 1, 1, 1],
            ..StewardSettings::default()
        },
    )
    .unwrap();
    let mut queued = 0;
    for _ in 0..20 * 30 {
        w.advance();
        queued = queued.max(w.building(dock).unwrap().training_queue.len());
        // Keep the boat frozen: this is the situation under test.
        let index = w.lookup[&boat];
        w.units[index].state.task = UnitTask::Moving;
        w.economy.gatherers.remove(&boat);
    }
    let ships = w
        .units()
        .filter(|u| u.kind == UnitKind::FishingShip)
        .count();
    assert_eq!(
        (queued, ships),
        (0, 1),
        "a frozen boat must not buy more boats"
    );
}
