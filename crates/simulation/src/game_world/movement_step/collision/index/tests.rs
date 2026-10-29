use super::*;

#[test]
fn cyclic_collision_samples_keep_order_and_the_actual_work_bound() {
    use crate::game_world::route_scheduler::GroupWork;
    let tile = TileCoord::new(2, 3);
    for size in [1, 7, 31, 32, 33, 65] {
        let mut index = CollisionIndex::default();
        for id in 0..size {
            index.insert(EntityId(id), tile, false, id as usize);
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
