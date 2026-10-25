use super::*;
use crate::{Depletion, ObjectKind, ResourceKind};
use aoe_core::TileRect;
use std::sync::Arc;

fn tree(tile: TileCoord) -> ResourceNode {
    ResourceNode {
        id: ((tile.y as u64) << 19) | ((tile.x as u64) << 1),
        tile,
        kind: ResourceKind::Wood,
        object: ObjectKind::Tree,
        initial_amount: 100,
        visual_variant: 0,
    }
}

fn exhaust(overlay: &mut ResourceOverlay, node: ResourceNode) {
    overlay.apply_depletion(&Depletion {
        id: node.id,
        removed: node.initial_amount,
        remaining: 0,
        became_nonblocking: true,
        revision: overlay.revision() + 1,
    });
}

#[test]
fn navigation_snapshots_share_unchanged_collision_chunks_without_resource_history() {
    let mut overlay = ResourceOverlay::default();
    // Model 180,000 exhausted historical trees without generating terrain.
    for index in 0..180_000 {
        let node = tree(TileCoord::new(index % 600, index / 600));
        overlay.remaining.insert(node.id, 0);
    }
    overlay.rebuild_navigation();
    let before = overlay.navigation_snapshot();
    assert!(Arc::ptr_eq(
        &before.navigation_state.snapshot.shards,
        &overlay.navigation_state.snapshot.shards
    ));
    assert_eq!(overlay.changed_count(), 180_000);
    assert!(before.remaining.is_empty());
    assert!(before.navigation_state.obstacles.is_empty());

    let node = tree(TileCoord::new(1_000, 1_000));
    assert!(before.blocks_node(node));
    exhaust(&mut overlay, node);
    let after = overlay.navigation_snapshot();
    assert_eq!(overlay.changed_count(), 180_001);
    assert!(after.remaining.is_empty());
    assert!(after.navigation_state.obstacles.is_empty());
    assert!(
        before.blocks_node(node),
        "retained searches pin old collision"
    );
    assert!(!after.blocks_node(node));
    assert!(!before.blocks_node(tree(TileCoord::new(10, 10))));
    assert!(!after.blocks_node(tree(TileCoord::new(10, 10))));

    let old = &before.navigation_state.snapshot;
    let new = &after.navigation_state.snapshot;
    assert!(Arc::ptr_eq(
        old.chunk((0, 0)).unwrap(),
        new.chunk((0, 0)).unwrap()
    ));
    assert_eq!(old.shards.len(), 64);
    assert_eq!(
        old.shards
            .iter()
            .zip(new.shards.iter())
            .filter(|(old, new)| Arc::ptr_eq(old, new))
            .count(),
        63,
        "a one-chunk refresh must retain every other shard"
    );
}

#[test]
fn navigation_snapshots_pin_building_and_starting_resource_collision() {
    let mut overlay = ResourceOverlay::default();
    let empty = overlay.navigation_snapshot();
    let building = TileCoord::new(40, 40);
    let footprint = TileRect::from_xywh(building.x, building.y, 1, 1);
    let starting = ResourceNode::starting(TileCoord::new(80, 80), ResourceKind::Wood).unwrap();
    overlay.add_footprint(footprint).unwrap();
    // Starting insertion normally validates terrain; this test isolates the
    // collision snapshot without generating an unrelated terrain fixture.
    overlay
        .remaining
        .insert(starting.id, starting.initial_amount);
    overlay.navigation_toggle(starting.tile, 3, starting.id);
    let occupied = overlay.navigation_snapshot();
    assert!(!empty.blocks_tile(building));
    assert!(!empty.blocks_tile(starting.tile));
    assert!(occupied.blocks_tile(building));
    assert!(occupied.blocks_tile(starting.tile));
    assert!(occupied.blocks_node(starting));
    assert!(occupied.remaining.is_empty());
    assert!(occupied.navigation_state.obstacles.is_empty());

    overlay.remove_footprint(footprint);
    exhaust(&mut overlay, starting);
    let cleared = overlay.navigation_snapshot();
    assert!(occupied.blocks_tile(building));
    assert!(occupied.blocks_tile(starting.tile));
    assert!(occupied.blocks_node(starting));
    assert!(!cleared.blocks_tile(building));
    assert!(!cleared.blocks_tile(starting.tile));
    assert!(!cleared.blocks_node(starting));
    assert!(cleared.remaining.is_empty());
}
