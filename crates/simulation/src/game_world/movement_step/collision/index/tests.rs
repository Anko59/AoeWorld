use super::*;

#[test]
fn cyclic_collision_samples_keep_order_and_the_actual_work_bound() {
    use crate::game_world::route_scheduler::GroupWork;
    let tile = TileCoord::new(2, 3);
    for size in [1, 7, 31, 32, 33, 65] {
        let mut index = CollisionIndex::default();
        for id in 0..size {
            index.insert(
                EntityId(id),
                WorldPosition::from_tile_center(tile).unwrap(),
                false,
                id as usize,
            );
        }
        for salt in [0, 1, 31, 32, u64::MAX] {
            let mut actual = Vec::new();
            let work = GroupWork::default();
            assert!(!index.blocked(
                (WorldPosition::from_tile_center(tile).unwrap(), 0),
                false,
                salt,
                &work,
                |id| {
                    actual.push(id);
                    false
                },
                |_| false
            ));
            let expected = (0..size.min(32))
                .map(|n| {
                    EntityId(((salt % u64::from(size) + u64::from(n)) % u64::from(size)) as u32)
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
            let mut metrics = crate::game_world::route_scheduler::RouteMetrics::default();
            work.snapshot(&mut metrics);
            assert_eq!(metrics.group_collision_work, 1 + u64::from(size.min(32)));
        }
    }
}

#[test]
fn a_window_rejects_outside_centres_without_a_body_test() {
    use crate::game_world::route_scheduler::GroupWork;
    let tile = TileCoord::new(2, 3);
    let centre = WorldPosition::from_tile_center(tile).unwrap();
    let mut index = CollisionIndex::default();
    index.insert(EntityId(0), centre, false, 0);
    // A step inside the tile moves the indexed centre out of the window.
    let moved = WorldPosition::new(centre.x + 100, centre.y);
    index.relocate(EntityId(0), tile, moved, false);
    assert_eq!(index.body_at(EntityId(0), tile, false), Some(moved));
    let tested = |window| {
        let mut tested = false;
        index.blocked_in(
            (centre, 0),
            window,
            false,
            0,
            &GroupWork::default(),
            |_| {
                tested = true;
                false
            },
            |_| false,
        );
        tested
    };
    assert!(!tested((centre.x, centre.x + 99, centre.y, centre.y)));
    assert!(tested((centre.x, centre.x + 100, centre.y, centre.y)));
}

#[test]
fn far_corner_units_keep_fine_and_coarse_collision_queries() {
    use crate::game_world::route_scheduler::GroupWork;
    let far = TileCoord::new(16_383, 16_383);
    let centre = WorldPosition::from_tile_center(far).unwrap();
    let mut index = CollisionIndex::default();
    for id in 0..32 {
        index.insert(EntityId(id), centre, id % 2 != 0, id as usize);
    }
    for naval in [false, true] {
        let mut seen = Vec::new();
        assert!(!index.blocked(
            (centre, 0),
            naval,
            0,
            &GroupWork::default(),
            |id| {
                seen.push(id);
                false
            },
            |_| false
        ));
        assert_eq!(
            seen,
            (0..32)
                .filter(|id| (id % 2 != 0) == naval)
                .map(EntityId)
                .collect::<Vec<_>>()
        );
    }
    assert!(index.contact_may_be_crowded(centre, 16, &GroupWork::default()));
    for id in (0..32).step_by(2) {
        index.remove(EntityId(id), far, false);
    }
    assert!(!index.contact_may_be_crowded(centre, 2, &GroupWork::default()));
    assert_eq!(index.cells[1].get(far).unwrap().len(), 16);
}

#[test]
fn empty_sweep_cells_exclude_only_the_actor_and_follow_relocations() {
    let mut grid = CollisionIndex::default();
    let actor = EntityId(1);
    let at = WorldPosition::new(2048, 3072);
    let min = TileCoord::new(1, 2);
    let max = TileCoord::new(3, 4);
    grid.insert(actor, at, false, 0);
    assert!(grid.cells_empty_except(min, max, false, actor));
    // An opposite movement layer cannot collide.
    grid.insert(EntityId(2), at, true, 1);
    assert!(grid.cells_empty_except(min, max, false, actor));
    // Even a centre outside the precise sweep box vetoes this cell proof.
    grid.insert(EntityId(3), WorldPosition::new(4095, 5119), false, 2);
    assert!(!grid.cells_empty_except(min, max, false, actor));
    grid.relocate(EntityId(3), max, WorldPosition::new(4096, 5120), false);
    assert!(grid.cells_empty_except(min, max, false, actor));
    grid.relocate(EntityId(3), TileCoord::new(4, 5), at, false);
    assert!(!grid.cells_empty_except(min, max, false, actor));
    grid.remove(EntityId(3), at.tile_floor(), false);
    assert!(grid.cells_empty_except(min, max, false, actor));
}
