//! A Steward never sends a walker anywhere its own territory cannot reach.
use super::*;
use crate::StewardSettings;

const TC: TileCoord = TileCoord::new(45, 200);

/// River at x 85..91 with its only gap near y 82..94, which lies beyond the
/// 100-tile circle of a Town Center at (45, 200). All natural resources are
/// removed so the scene holds exactly what the test places.
fn river_realm(villagers: usize) -> (GameWorld, EntityId, Vec<EntityId>) {
    let mut w = crate::game_world::terrain_fixtures::build("river").world;
    let old: Vec<_> = w.units().map(|u| u.id).collect();
    for id in old {
        w.kill_unit(w.lookup[&id]);
        w.remove_unit(id);
    }
    w.initialize_village(PlayerId(0), TileCoord::new(40, 40))
        .unwrap();
    let old: Vec<_> = w.units().map(|u| u.id).collect();
    for id in old {
        w.kill_unit(w.lookup[&id]);
        w.remove_unit(id);
    }
    let old: Vec<_> = w.buildings().map(|b| b.id).collect();
    for id in old {
        w.remove_building(id);
    }
    w.set_fog_disabled(true);
    let mut natural = Vec::new();
    if let Terrain::Map { generator, .. } = &w.terrain {
        for cy in 0..16 {
            for cx in 0..8 {
                if let Ok(chunk) = generator.chunk(cx, cy) {
                    natural.extend(chunk.resources);
                }
            }
        }
    }
    for node in natural {
        let _ = w.deplete_resource(node.id, node.initial_amount);
    }
    w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Castle;
    reserves(&mut w, 50_000, 0, 50_000, 50_000);
    let tc = place_near(&mut w, BuildingKind::TownCenter, TC);
    for i in 0..6 {
        place_near(&mut w, BuildingKind::House, TileCoord::new(20 + 4 * i, 215));
    }
    place_near(&mut w, BuildingKind::LumberCamp, TileCoord::new(96, 200));
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for y in 196..204 {
            for x in 100..108 {
                overlay
                    .insert_starting_resource(
                        generator,
                        ResourceNode::starting(TileCoord::new(x, y), ResourceKind::Wood).unwrap(),
                    )
                    .unwrap();
            }
        }
    }
    w.resource_collision_changed();
    let king = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Steward,
            position(TileCoord::new(TC.x - 1, TC.y)),
        )
        .unwrap();
    w.issue_garrison(PlayerId(0), king, tc).unwrap();
    let workers = workers_near(&mut w, TileCoord::new(TC.x + 6, TC.y), villagers);
    // Only Wood is wanted: a Wood job is the sole thing the river tempts.
    let steward = StewardSettings {
        weights: [0, 100, 0, 0],
        ..StewardSettings::default()
    };
    advance(&mut w, 2);
    w.issue_steward_settings(PlayerId(0), king, steward)
        .unwrap();
    (w, king, workers)
}

#[test]
fn a_steward_never_sends_a_villager_to_a_bank_its_territory_cannot_walk_to() {
    let (mut w, king, workers) = river_realm(6);
    assert!(w.steward_town_center(king).is_some());
    let mut across = 0;
    for _ in 0..20 * 90 {
        w.advance();
        for &id in &workers {
            across += usize::from(w.gather_tile(id).is_some_and(|t| t.x > 92));
        }
    }
    assert_eq!(across, 0, "a villager was given a job across the river");
    for &id in &workers {
        let u = w.unit(id).unwrap();
        assert!(
            !u.moving && !u.planning && u.task == UnitTask::Idle,
            "{id:?} must wait idle, found {:?} moving={} planning={}",
            u.task,
            u.moving,
            u.planning
        );
        assert!(u.position.tile_floor().x < 85, "{id:?} crossed the river");
    }
}
