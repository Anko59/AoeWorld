use super::super::*;
use aoe_core::{Seed, TileCoord, WorldConfig};

#[test]
fn timing_only_probe_does_not_run_the_geometric_census() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Debug, Default)]
    struct Probe {
        census: AtomicUsize,
        spans: AtomicUsize,
    }
    impl crate::PlanningProbe for Probe {
        fn begin(&self) {}
        fn end(&self) {}
        fn movement_count(&self, name: &'static str, count: usize) {
            if name == "applied_no_body" {
                self.census.fetch_add(count, Ordering::Relaxed);
            }
        }
        fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
            if name == "apply_loop" {
                self.spans.fetch_add(1, Ordering::Relaxed);
            }
            None
        }
    }
    let fixture = || {
        let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(0)).unwrap()).unwrap();
        let from = WorldPosition::from_tile_center(TileCoord::new(20, 20)).unwrap();
        let to = WorldPosition::from_tile_center(TileCoord::new(40, 20)).unwrap();
        let id = world.spawn_unit(PlayerId(0), from).unwrap();
        world.issue_move(id, to).unwrap();
        world
    };
    let mut plain = fixture();
    let mut observed = fixture();
    let probe = Arc::new(Probe::default());
    observed.set_planning_probe(probe.clone());
    for _ in 0..32 {
        assert_eq!(plain.advance(), observed.advance());
        assert_eq!(plain.canonical_hash(), observed.canonical_hash());
    }
    assert!(probe.spans.load(Ordering::Relaxed) > 0);
    assert_eq!(probe.census.load(Ordering::Relaxed), 0);
}

#[test]
fn empty_cell_sweeps_match_full_geometry_at_fractional_boundaries() {
    use crate::UnitKind;
    for kind in [UnitKind::Scout, UnitKind::Mangonel] {
        let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
        let from = WorldPosition::new(20 * 1024, 20 * 1024 + 1);
        let actor = world.spawn_kind(PlayerId(0), kind, from).unwrap();
        let other = world
            .spawn_kind(PlayerId(1), kind, WorldPosition::new(40000, 40000))
            .unwrap();
        let index = world.lookup[&actor];
        let mut proven = 0;
        for dx in [-2049, -1025, -1024, -513, -1, 0, 1, 513, 1024, 1025, 2049] {
            for dy in [-2049, -1025, -1024, -513, -1, 0, 1, 513, 1024, 1025, 2049] {
                let other_index = world.lookup[&other];
                let old = world.units[other_index].state.position;
                world.units[other_index].state.position =
                    WorldPosition::new(from.x + dx, from.y + dy);
                let state = world.units[other_index].state;
                world.track_unit_step(other, old.tile_floor(), &state);
                for to in [
                    WorldPosition::new(from.x + 100, from.y),
                    WorldPosition::new(from.x - 100, from.y + 99),
                ] {
                    let full = world.collision_sweep_full(index, from, to, false, false, None);
                    assert_eq!(world.collision_sweep(index, from, to, false, false), full);
                    if world.sweep_cells_clear(index, from, to, 0) {
                        proven += 1;
                        assert!(full, "{kind:?}, other offset ({dx}, {dy}), to {to:?}");
                    }
                }
            }
        }
        assert!(proven > 0);
    }
}

fn unchanged_step(world: &GameWorld, index: usize) -> FastStep {
    let unit = &world.units[index];
    FastStep {
        index,
        state: unit.state,
        order: unit.order,
        pops: 0,
        keeps_moving: true,
        bucket: unit.bucket,
        vision_stale: false,
        member: None,
    }
}

/// Called by every no-op hit in unit tests: execute the entire old apply and
/// prove its canonical state and output lists equal the skipped operation.
pub(super) fn assert_stationary_apply(world: &mut GameWorld, step: FastStep) {
    let stored = &world.units[step.index];
    let before = (
        stored.state,
        step.order,
        stored.bucket,
        stored.planning_since,
        stored.route.clone(),
    );
    let index = step.index;
    let id = step.state.id;
    let keeps_moving = step.keeps_moving;
    let mut still = Vec::new();
    let mut changed = Vec::new();
    assert!(world.apply_step_full(step, &mut still, &mut changed));
    let stored = &world.units[index];
    assert_eq!(
        (
            stored.state,
            stored.order,
            stored.bucket,
            stored.planning_since,
            stored.route.clone()
        ),
        before
    );
    assert!(stored.obstruction.is_none());
    assert_eq!(still, if keeps_moving { vec![id] } else { vec![] });
    assert!(changed.is_empty());
}

#[test]
fn unchanged_apply_skips_only_when_every_shared_write_is_inert() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let from = WorldPosition::new(20 * 1024, 20 * 1024);
    let id = world.spawn_unit(PlayerId(0), from).unwrap();
    world
        .issue_move(id, WorldPosition::new(from.x + 10000, from.y))
        .unwrap();
    let index = world.lookup[&id];
    assert!(world.apply_is_stationary(&unchanged_step(&world, index)));
    let before = world.canonical_hash();
    let mut still = Vec::new();
    let mut changed = Vec::new();
    assert!(world.apply_step(unchanged_step(&world, index), &mut still, &mut changed));
    assert_eq!(world.canonical_hash(), before);
    assert_eq!(still, vec![id]);
    assert!(changed.is_empty());
    let mut carry = unchanged_step(&world, index);
    carry.order.as_mut().unwrap().speed_carry += 1;
    assert!(world.apply_is_stationary(&carry));
    let expected = carry.order;
    assert!(world.apply_step(carry, &mut Vec::new(), &mut Vec::new()));
    assert_eq!(world.units[index].order, expected);
    let mut previous = unchanged_step(&world, index);
    previous.state.previous_position.x += 1;
    assert!(!world.apply_is_stationary(&previous));
    let mut route = unchanged_step(&world, index);
    route.pops = 1;
    assert!(!world.apply_is_stationary(&route));
    let mut vision = unchanged_step(&world, index);
    vision.vision_stale = true;
    assert!(!world.apply_is_stationary(&vision));
    let mut bucket = unchanged_step(&world, index);
    bucket.bucket = ChunkCoord::from_position(WorldPosition::new(100000, 100000));
    assert!(!world.apply_is_stationary(&bucket));
    world.record_unit_obstruction(index);
    assert!(!world.apply_is_stationary(&unchanged_step(&world, index)));
}

#[test]
fn group_cell_cache_is_rebuilt_after_static_changes() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let tile = TileCoord::new(20, 20);
    let from = WorldPosition::from_tile_center(tile).unwrap();
    let id = world.spawn_unit(PlayerId(0), from).unwrap();
    let index = world.lookup[&id];
    let mut tables = Tables::default();
    tables.cache_group_cells(&world, [index, index]);
    assert_eq!(tables.sealed_cells.len(), 1);
    assert!(!tables.group_cell_sealed(&world, crate::UnitKind::Scout, tile));
    for y in 19..=21 {
        for x in 19..=21 {
            if (x, y) != (20, 20) {
                world
                    .insert_building(
                        PlayerId(0),
                        crate::BuildingKind::PalisadeWall,
                        aoe_core::TileRect::from_xywh(x, y, 1, 1),
                        true,
                    )
                    .unwrap();
            }
        }
    }
    tables.cache_group_cells(&world, [index]);
    assert!(tables.group_cell_sealed(&world, crate::UnitKind::Scout, tile));
}

#[test]
fn the_geometric_census_counts_initial_overlap_and_excludes_tangency() {
    use super::swept_body_intersects;
    let from = WorldPosition::new(0, 0);
    let to = WorldPosition::new(100, 0);
    assert!(swept_body_intersects(
        from,
        to,
        WorldPosition::new(50, 9),
        10
    ));
    assert!(!swept_body_intersects(
        from,
        to,
        WorldPosition::new(50, 10),
        10
    ));
    assert!(swept_body_intersects(
        from,
        to,
        WorldPosition::new(-9, 0),
        10
    ));
    assert!(!swept_body_intersects(
        from,
        to,
        WorldPosition::new(-10, 0),
        10
    ));
    assert!(swept_body_intersects(from, from, from, 10));
    assert!(!swept_body_intersects(
        from,
        from,
        WorldPosition::new(10, 0),
        10
    ));
}

pub(super) fn assert_settling_skip(world: &mut GameWorld, index: usize) {
    let unit = &world.units[index];
    let before = (
        unit.state,
        unit.order,
        unit.bucket,
        unit.planning_since,
        unit.route.clone(),
    );
    assert!(!world.settle_obstructed_unit(index));
    let unit = &world.units[index];
    assert_eq!(
        (
            unit.state,
            unit.order,
            unit.bucket,
            unit.planning_since,
            unit.route.clone()
        ),
        before
    );
    assert!(unit.obstruction.is_none());
}
