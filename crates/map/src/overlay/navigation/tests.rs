use super::*;
use crate::{Depletion, ObjectKind, ResourceKind};
use aoe_core::TileRect;
use std::sync::Arc;

#[test]
fn bridge_and_foundation_reservations_share_the_fixed_collision_capacity() {
    let mut overlay = ResourceOverlay::default();
    // Exercise admission at the logical cap without allocating sixteen
    // million tree entries. No runtime or physical-memory claim is made.
    overlay.navigation_state.snapshot.bridge_tiles = super::super::MAX_OBSTACLE_TILES as usize;
    assert_eq!(
        overlay.add_footprint(TileRect::from_xywh(1, 1, 1, 1)),
        Err(super::super::ResourceOverlayError::Capacity)
    );
    assert_eq!(
        overlay.add_bridge(TileRect::from_xywh(1, 1, 3, 1)),
        Err(super::super::ResourceOverlayError::Capacity)
    );
    assert_eq!(
        overlay.bridge_tile_count() as u64,
        super::super::MAX_OBSTACLE_TILES
    );
}

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
fn nonblocking_fish_remain_available_until_exhausted() {
    for object in [ObjectKind::ShoreFish, ObjectKind::DeepFish] {
        let mut overlay = ResourceOverlay::default();
        let node = ResourceNode {
            kind: ResourceKind::Food,
            object,
            ..tree(TileCoord::new(12, 12))
        };
        assert!(!overlay.blocks_node(node));
        assert!(overlay.resource_available(node));
        exhaust(&mut overlay, node);
        assert!(!overlay.blocks_node(node));
        assert!(!overlay.resource_available(node));
    }
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

#[test]
fn bridge_completion_requires_the_reserved_span_and_keeps_failed_attempts_atomic() {
    let rect = aoe_core::TileRect::from_xywh(7, 9, 6, 1);
    let gap = aoe_core::TileRect::from_xywh(9, 9, 1, 1);
    let mut overlay = super::super::ResourceOverlay::default();
    overlay.add_footprint(rect).unwrap();
    overlay.remove_footprint(gap);
    let before = overlay.clone();
    assert!(!overlay.complete_bridge(rect));
    assert_eq!(overlay, before);
    overlay.add_footprint(gap).unwrap();
    assert!(overlay.complete_bridge(rect));
    for x in 7..13 {
        let tile = aoe_core::TileCoord::new(x, 9);
        assert!(!overlay.blocks_tile(tile));
        assert_eq!(
            overlay.bridge_axis(tile),
            (8..12).contains(&x).then_some(true)
        );
    }
}
