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
