//! Count the completion membership kernel, not shared-host wall time. This
//! synthetic job set is not a full 100-player movement/performance qualification.
use super::*;
use aoe_core::{Seed, WorldConfig};
use std::collections::BTreeMap;

#[test]
fn paid_foundation_clears_friendly_bystanders_while_every_builder_is_busy() {
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(1)).unwrap()).unwrap();
    let player = PlayerId(0);
    let active = world
        .insert_building(
            player,
            BuildingKind::House,
            TileRect::from_xywh(11, 10, 2, 2),
            false,
        )
        .unwrap();
    let worker = world
        .spawn_villager(player, position(TileCoord::new(10, 10)).unwrap())
        .unwrap();
    world.economy.builders.insert(
        worker,
        BuildTask {
            building: active,
            attempts: 0,
            queue: Vec::new(),
        },
    );
    let tile = TileCoord::new(20, 20);
    let at = position(tile).unwrap();
    let bystander = world.spawn_militia(player, at).unwrap();
    let queued = world
        .insert_building(
            player,
            BuildingKind::House,
            TileRect::from_xywh(tile.x, tile.y, 2, 2),
            false,
        )
        .unwrap();
    world.queue_foundations(&[queued], None);

    let mut changed = Vec::new();
    world.dispatch_construction(&mut changed);

    assert_ne!(world.unit(bystander).unwrap().position, at);
    assert_eq!(changed, [bystander]);
    assert_eq!(world.economy.builders[&worker].building, active);
    assert_eq!(world.economy.builders.assigned_count(queued), 0);
}

#[test]
fn completion_membership_visits_only_builders_of_the_completed_sites() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    let mut completed = BTreeMap::new();
    for player in 0..100_u16 {
        for slot in 0..200_u32 {
            let builder = EntityId(u32::from(player) * 200 + slot);
            let building = EntityId(20_000 + builder.0);
            world.economy.builders.insert(
                builder,
                BuildTask {
                    building,
                    attempts: 0,
                    queue: Vec::new(),
                },
            );
            completed.insert(
                building,
                (
                    PlayerId(player),
                    BuildingKind::Farm,
                    TileRect::from_xywh(100, 100, 3, 3),
                ),
            );
        }
    }
    let unrelated = EntityId(40_000);
    world.economy.builders.insert(
        unrelated,
        BuildTask {
            building: EntityId(40_001),
            attempts: 0,
            queue: Vec::new(),
        },
    );
    // No live actors are needed to exercise membership lookup and release.
    continuation::COMPLETION_VISITS.with(|visits| visits.set(0));
    let mut changed = Vec::new();
    world.finish_builders(&completed, &mut changed);
    assert_eq!(changed.len(), 20_000);
    assert_eq!(
        world.economy.builders.keys().copied().collect::<Vec<_>>(),
        [unrelated]
    );
    continuation::COMPLETION_VISITS.with(|visits| assert_eq!(visits.get(), 20_000));
}

#[test]
fn behaviour_recording_scenes_have_a_live_primary_and_deterministic_setup() {
    use crate::BehaviourScene::*;
    for scene in [TownCenter, Wolf, Deer, Hunt, Camps, Bridge] {
        let (a, primary) = GameWorld::behaviour_demo(Seed(7), scene)
            .unwrap_or_else(|e| panic!("{scene:?}: {e:?}"));
        let (b, _) = GameWorld::behaviour_demo(Seed(7), scene)
            .unwrap_or_else(|e| panic!("{scene:?}: {e:?}"));
        assert!(a.unit(primary).unwrap().hp > 0);
        for building in a.buildings() {
            let rules = building.kind.rules();
            assert_eq!(building.footprint.width(), rules.side);
            assert_eq!(building.footprint.height(), rules.depth);
        }
        assert_eq!(a.canonical_hash(), b.canonical_hash());
        if matches!(scene, Bridge) {
            let crate::Terrain::Map { generator, .. } = a.terrain() else {
                panic!("map")
            };
            let wet: Vec<_> = (80..170)
                .filter(|x| {
                    generator
                        .tile_at(TileCoord::new(*x, 127))
                        .is_some_and(|t| t.water != aoe_map::WaterKind::None)
                })
                .collect();
            let (from, to) = (
                TileCoord::new(wet[0] - 1, 127),
                TileCoord::new(wet[wet.len() - 1] + 1, 127),
            );
            assert!(
                a.bridge_site_valid(PlayerId(0), from, to),
                "{from:?} -> {to:?}"
            );
        }
    }
}

#[test]
fn regrowth_showcase_replays_real_growth_in_a_fixed_frame() {
    use crate::{AnimalKind, BehaviourScene};
    let (mut world, _) = GameWorld::behaviour_demo(Seed(7), BehaviourScene::Regrowth).unwrap();
    let living_deer = |world: &GameWorld| {
        world
            .animals()
            .filter(|a| a.kind == AnimalKind::Deer && a.hp > 0)
            .count()
    };
    let trees = world.starting_resources();
    let live_trees = |world: &GameWorld| {
        trees
            .iter()
            .filter(|tree| {
                world
                    .resource_remaining(tree.id)
                    .is_some_and(|left| left > 0)
            })
            .count()
    };
    assert_eq!((living_deer(&world), live_trees(&world)), (3, 8));
    for _ in 0..120 * world.config().tick_hz {
        world.advance();
    }
    assert_eq!((living_deer(&world), live_trees(&world)), (3, 12));
    for _ in 0..120 * world.config().tick_hz {
        world.advance();
    }
    assert_eq!((living_deer(&world), live_trees(&world)), (4, 16));
}
