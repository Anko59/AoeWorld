use super::*;
use crate::AnimalKind;
use aoe_map::{ObjectKind, ResourceKind, ResourceNode};

fn advance(w: &mut GameWorld, seconds: u32) {
    for _ in 0..seconds * w.config().tick_hz {
        w.advance();
    }
}

fn tree_line() -> (GameWorld, [ResourceNode; 3]) {
    let (mut w, _) = super::snapshot_roundtrip::flat();
    let Terrain::Map { generator, overlay } = &mut w.terrain else {
        unreachable!()
    };
    for y in 10..100 {
        for x in 10..100 {
            let tiles = [0, 1, 2, 3].map(|dx| TileCoord::new(x + dx, y));
            // Also keep the open side and the perimeter away from natural woods.
            if !(y - 1..=y + 1).all(|ny| {
                (x - 1..=x + 3).all(|nx| {
                    generator
                        .tile_and_node_with_cancel(TileCoord::new(nx, ny), &|| false)
                        .unwrap()
                        .is_some_and(|(ground, node)| ground.passable && node.is_none())
                })
            }) {
                continue;
            }
            let trees = [tiles[0], tiles[1], tiles[2]]
                .map(|tile| ResourceNode::starting(tile, ResourceKind::Wood).unwrap());
            for tree in trees {
                overlay.insert_starting_resource(generator, tree).unwrap();
            }
            return (w, trees);
        }
    }
    panic!("fixture needs a clear forest edge");
}

#[test]
fn a_forest_regrows_along_its_original_edge_and_recloses_ground() {
    let (mut w, [_, edge, far]) = tree_line();
    for tree in [edge, far] {
        w.deplete_resource(tree.id, tree.initial_amount).unwrap();
        assert!(w.tile_passable(tree.tile));
    }
    advance(&mut w, 119);
    assert_eq!(w.resource_remaining(edge.id), Some(0));
    advance(&mut w, 1);
    assert_eq!(w.resource_remaining(edge.id), Some(edge.initial_amount));
    assert!(!w.tile_passable(edge.tile));
    assert_eq!(w.resource_remaining(far.id), Some(0));
    advance(&mut w, 120);
    assert_eq!(w.resource_remaining(far.id), Some(far.initial_amount));
    assert!(w.tile_passable(TileCoord::new(far.tile.x + 1, far.tile.y)));
}

#[test]
fn a_fully_cleared_patch_never_regrows() {
    let (mut w, trees) = tree_line();
    for tree in trees {
        w.deplete_resource(tree.id, tree.initial_amount).unwrap();
    }
    assert!(w.resource_recovery_deadline().is_none());
    advance(&mut w, 360);
    for tree in trees {
        assert_eq!(w.resource_remaining(tree.id), Some(0));
    }
}

#[test]
fn occupied_ground_defers_regrowth_and_pending_growth_survives_a_save() {
    let (mut w, [_, edge, _]) = tree_line();
    w.deplete_resource(edge.id, edge.initial_amount).unwrap();
    let at = WorldPosition::from_tile_center(edge.tile).unwrap();
    let villager = w.spawn_villager(PlayerId(0), at).unwrap();
    advance(&mut w, 119);
    let snapshot = w.snapshot([0; 32]).unwrap();
    let mut restored = super::snapshot_roundtrip::flat().0;
    restored.set_tick_hz(10);
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    advance(&mut restored, 1);
    assert_eq!(restored.resource_remaining(edge.id), Some(0));
    restored.kill_unit(restored.lookup[&villager]);
    advance(&mut restored, 120);
    assert_eq!(
        restored.resource_remaining(edge.id),
        Some(edge.initial_amount)
    );
}

#[test]
fn a_shoal_recovers_only_while_fish_remain() {
    let mut w = super::navy::coast();
    let Terrain::Map { generator, .. } = &w.terrain else {
        unreachable!()
    };
    let fish: Vec<_> = (0..12)
        .flat_map(|x| (0..5).map(move |y| (x, y)))
        .flat_map(|(x, y)| generator.chunk(x, y).unwrap().resources)
        .filter(|node| matches!(node.object, ObjectKind::ShoreFish | ObjectKind::DeepFish))
        .take(2)
        .collect();
    let [surviving, extinct] = fish.try_into().unwrap();
    w.deplete_resource(surviving.id, 13).unwrap();
    w.deplete_resource(extinct.id, 10).unwrap();
    w.deplete_resource(extinct.id, extinct.initial_amount)
        .unwrap();
    advance(&mut w, 179);
    assert_eq!(
        w.resource_remaining(surviving.id),
        Some(surviving.initial_amount - 13)
    );
    advance(&mut w, 1);
    assert_eq!(
        w.resource_remaining(surviving.id),
        Some(surviving.initial_amount - 3)
    );
    advance(&mut w, 180);
    assert_eq!(
        w.resource_remaining(surviving.id),
        Some(surviving.initial_amount)
    );
    assert_eq!(w.resource_remaining(extinct.id), Some(0));
}

fn herd(w: &mut GameWorld, x: i32) -> [EntityId; 2] {
    let members = [x, x + 1].map(|x| {
        let at = WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap();
        w.spawn_animal(AnimalKind::Deer, at).unwrap()
    });
    w.register_herd(AnimalKind::Deer, &members);
    members
}

#[test]
fn a_herd_recovers_near_its_own_survivor_and_an_extinct_herd_stays_gone() {
    let mut w = world();
    let [first, survivor] = herd(&mut w, 100);
    let extinct = herd(&mut w, 104);
    for id in [first, extinct[0], extinct[1]] {
        assert!(w.strike_animal(id, EntityId(u32::MAX), u16::MAX));
    }
    advance(&mut w, 239);
    let snapshot = w.snapshot([0; 32]).unwrap();
    let mut w = world();
    w.restore_snapshot(&snapshot, [0; 32], &|| false).unwrap();
    assert_eq!(w.animals().filter(|a| a.hp > 0).count(), 1);
    advance(&mut w, 1);
    let born = w.animals().find(|a| a.hp > 0 && a.id != survivor).unwrap();
    let position = born.position.tile_floor();
    assert_eq!(born.kind, AnimalKind::Deer);
    assert_ne!(position, TileCoord::new(101, 100));
    assert!(position.x.abs_diff(101).max(position.y.abs_diff(100)) <= 2);
    advance(&mut w, 480);
    assert_eq!(w.animals().filter(|a| a.hp > 0).count(), 2);
}

#[test]
fn a_refused_growth_transaction_retries_without_publishing_stock() {
    let (mut w, [_, edge, _]) = tree_line();
    w.deplete_resource(edge.id, edge.initial_amount).unwrap();
    let revision = w.resource_revision().unwrap();
    advance(&mut w, 119);
    for _ in 0..w.config().tick_hz {
        w.advance_with_resource_sink(&mut |growth| {
            assert_eq!(growth.previous_revision(), revision);
            assert_eq!(growth.snapshot([0; 32]).revision, revision + 1);
            false
        });
    }
    assert!(w.take_resource_changes().is_empty());
    assert_eq!(w.resource_revision(), Some(revision));
    assert_eq!(w.resource_remaining(edge.id), Some(0));
    advance(&mut w, 120);
    assert_eq!(w.resource_remaining(edge.id), Some(edge.initial_amount));
    let changes = w.take_resource_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].id, edge.id);
    assert_eq!(changes[0].added, edge.initial_amount);
    assert_eq!(changes[0].revision, revision + 1);
}

#[test]
fn recovery_replays_at_one_and_four_workers() {
    let play = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let (mut w, [_, edge, far]) = tree_line();
                w.set_tick_hz(5);
                for tree in [edge, far] {
                    w.deplete_resource(tree.id, tree.initial_amount).unwrap();
                }
                let [first, _] = herd(&mut w, 100);
                w.strike_animal(first, EntityId(u32::MAX), u16::MAX);
                [0, 120, 120].map(|seconds| {
                    advance(&mut w, seconds);
                    w.canonical_hash()
                })
            })
    };
    assert_eq!(play(1), play(4));
}

#[test]
fn simultaneous_herd_deadlines_are_served_on_the_same_game_tick() {
    let mut w = world();
    w.set_tick_hz(5);
    for x in (40..120).step_by(4) {
        let [first, _] = herd(&mut w, x);
        w.strike_animal(first, EntityId(u32::MAX), u16::MAX);
    }
    advance(&mut w, 239);
    assert_eq!(w.animals().filter(|a| a.hp > 0).count(), 20);
    advance(&mut w, 1);
    assert_eq!(w.animals().filter(|a| a.hp > 0).count(), 40);
}

#[test]
fn a_prepared_cut_persists_the_recovery_deadline_before_commit() {
    let (mut world, [_, edge, _]) = tree_line();
    let cut = world
        .prepare_resource_depletion(edge.id, edge.initial_amount)
        .unwrap();
    let stored = cut.snapshot([0; 32]);
    cut.commit();
    assert_eq!(world.resource_snapshot([0; 32]).unwrap(), stored);

    let (mut restored, _) = tree_line();
    let before = restored.canonical_hash();
    let mut malformed = stored.clone();
    malformed.recovery.push(malformed.recovery[0]);
    assert!(
        restored
            .restore_resources(&malformed, [0; 32], &|| false)
            .is_err()
    );
    assert_eq!(restored.canonical_hash(), before);
    restored
        .restore_resources(&stored, [0; 32], &|| false)
        .unwrap();
    assert_eq!(restored.canonical_hash(), world.canonical_hash());
    advance(&mut world, 40);
    let mut restored = tree_line().0;
    restored.set_tick_hz(10);
    restored
        .restore_resources(&world.resource_snapshot([0; 32]).unwrap(), [0; 32], &|| {
            false
        })
        .unwrap();
    advance(&mut restored, 79);
    assert_eq!(restored.resource_remaining(edge.id), Some(0));
    advance(&mut restored, 1);
    assert_eq!(
        restored.resource_remaining(edge.id),
        Some(edge.initial_amount)
    );
}

#[test]
fn a_tree_waits_while_a_unit_overlaps_from_the_adjacent_tile() {
    let (mut world, [_, edge, _]) = tree_line();
    world
        .deplete_resource(edge.id, edge.initial_amount)
        .unwrap();
    let center = WorldPosition::from_tile_center(edge.tile).unwrap();
    let at = WorldPosition::new(center.x, center.y + 520);
    assert_ne!(at.tile_floor(), edge.tile);
    let unit = world.spawn_villager(PlayerId(0), at).unwrap();
    advance(&mut world, 120);
    let unit = world.unit(unit).unwrap();
    assert!(world.body_position_clear(unit.kind, unit.position));
    assert_eq!(world.resource_remaining(edge.id), Some(0));
}

#[test]
fn a_herd_waits_for_a_walkable_neighbor_without_a_unit_or_animal() {
    let mut world = world();
    let [lost, _] = herd(&mut world, 100);
    assert!(world.strike_animal(lost, EntityId(u32::MAX), u16::MAX));
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    world.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    world.spawn_animal(AnimalKind::Sheep, at(101, 99)).unwrap();
    let animal = world.spawn_animal(AnimalKind::Sheep, at(102, 100)).unwrap();
    world.spawn_animal(AnimalKind::Sheep, at(101, 101)).unwrap();
    let interval = 240 * u64::from(world.config().tick_hz);
    world.tick.0 = interval;
    world.advance_herd_recovery();
    let deer = |world: &GameWorld| {
        world
            .animals()
            .filter(|a| a.kind == AnimalKind::Deer && a.hp > 0)
            .count()
    };
    assert_eq!(deer(&world), 1);

    assert!(world.strike_animal(animal, EntityId(u32::MAX), u16::MAX));
    world.tick.0 += interval;
    world.advance_herd_recovery();
    assert_eq!(deer(&world), 2);
    assert!(
        world
            .animals()
            .any(|a| { a.kind == AnimalKind::Deer && a.hp > 0 && a.position == at(102, 100) })
    );
}

#[test]
fn a_tree_waits_while_a_unit_has_reserved_the_tile_for_arrival() {
    let (mut world, [_, edge, _]) = tree_line();
    world
        .deplete_resource(edge.id, edge.initial_amount)
        .unwrap();
    let approach = TileCoord::new(edge.tile.x, edge.tile.y + 1);
    let unit = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(approach).unwrap(),
        )
        .unwrap();
    world
        .issue_move(unit, WorldPosition::from_tile_center(edge.tile).unwrap())
        .unwrap();
    assert_eq!(world.unit(unit).unwrap().position.tile_floor(), approach);
    world.tick.0 = 120 * u64::from(world.config().tick_hz);
    world.advance_resource_recovery(&mut |_| true);
    assert_eq!(world.resource_remaining(edge.id), Some(0));
}

#[test]
fn herd_recruitment_keeps_regular_ids_and_the_recruit_cursor_across_a_save() {
    let mut control = world();
    let mut simulation = world();
    herd(&mut control, 100);
    let [lost, founder] = herd(&mut simulation, 100);
    assert!(simulation.strike_animal(lost, EntityId(u32::MAX), u16::MAX));
    let interval = 240 * u64::from(simulation.config().tick_hz);
    simulation.tick.0 = interval;
    simulation.advance_herd_recovery();
    let recruit = simulation
        .animals()
        .find(|a| a.kind == AnimalKind::Deer && a.hp > 0 && a.id != founder)
        .unwrap()
        .id;
    let at = WorldPosition::from_tile_center(TileCoord::new(110, 110)).unwrap();
    let next = control.spawn_villager(PlayerId(0), at).unwrap();

    *simulation.animal_hp_quarters_mut(recruit).unwrap() = 3;
    let save = simulation.snapshot([0; 32]).unwrap();
    let mut restored = world();
    restored
        .restore_snapshot(&save, [0; 32], &|| false)
        .unwrap();
    assert!(restored.animal(recruit).is_some());
    assert_eq!(restored.animal_hp_quarters_mut(recruit).copied(), Some(3));
    assert_eq!(restored.spawn_villager(PlayerId(0), at).unwrap(), next);
    assert!(restored.strike_animal(recruit, EntityId(u32::MAX), u16::MAX));
    restored.tick.0 += interval;
    restored.advance_herd_recovery();
    assert_eq!(restored.animals().filter(|a| a.hp > 0).count(), 2);
    assert!(restored.animals().any(|a| a.hp > 0 && a.id > recruit));
}

#[test]
fn a_newborn_sheep_preserves_its_owners_sight_when_it_is_the_last_survivor() {
    let mut world = world();
    world.set_fog_disabled(false);
    let player = PlayerId(0);
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let lost = world.spawn_animal(AnimalKind::Sheep, at(100, 100)).unwrap();
    let parent = world.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
    world.register_herd(AnimalKind::Sheep, &[lost, parent]);
    let scout = world.spawn_unit(player, at(101, 101)).unwrap();
    world.advance();
    assert_eq!(world.animal(parent).unwrap().owner, Some(player));
    assert!(world.strike_animal(lost, scout, u16::MAX));
    world.tick.0 += 300 * u64::from(world.config().tick_hz);
    world.advance_herd_recovery();
    let newborn = *world
        .animals()
        .find(|a| a.id != parent && a.hp > 0)
        .unwrap();
    assert_eq!(newborn.owner, Some(player));

    assert!(world.strike_animal(parent, scout, u16::MAX));
    world.kill_unit(world.lookup[&scout]);
    let visible = crate::TileVisibility::Visible;
    assert_eq!(
        world.visibility(player, newborn.position.tile_floor()),
        visible
    );
    let save = world.snapshot([0; 32]).unwrap();
    let mut restored = super::world();
    restored
        .restore_snapshot(&save, [0; 32], &|| false)
        .unwrap();
    assert_eq!(
        restored.visibility(player, newborn.position.tile_floor()),
        visible
    );
}
