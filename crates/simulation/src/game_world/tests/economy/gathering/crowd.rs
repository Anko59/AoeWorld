//! S7: twelve villagers sent to one gold mine. Records today's behaviour; PR 10
//! of the group-movement rewrite (docs/movement/group-movement-rewrite.md,
//! section 6) requires all twelve to gather within ten seconds.
use super::*;

#[test]
fn twelve_villagers_on_one_gold_mine() {
    let (mut world, node, first) = resource_world(ResourceKind::Gold);
    let mut ids = vec![first];
    let ring = (-9..=9)
        .flat_map(|dy| (-9..=9).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy): &(i32, i32)| dx.abs().max(dy.abs()) >= 6)
        .map(|(dx, dy)| TileCoord::new(node.tile.x + dx, node.tile.y + dy))
        .filter(|&t| world.tile_passable(t))
        .collect::<Vec<_>>();
    for n in 0..11 {
        let tile = ring[n * ring.len() / 11];
        ids.push(
            world
                .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
                .unwrap(),
        );
    }
    for &id in &ids {
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
    }
    let mut first_load = vec![None; ids.len()];
    for tick in 1..=1200_u32 {
        world.advance();
        for (n, &id) in ids.iter().enumerate() {
            if first_load[n].is_none() && world.unit(id).unwrap().carried.is_some() {
                first_load[n] = Some(tick);
            }
        }
    }
    let within = |ticks: u32| first_load.iter().flatten().filter(|&&t| t <= ticks).count();
    let idle = ids
        .iter()
        .filter(|&&id| world.unit(id).unwrap().task == UnitTask::Idle)
        .count();
    eprintln!(
        "group-baseline gold-crowd: within_10s={} within_30s={} within_60s={} idle={idle}",
        within(200),
        within(600),
        within(1200),
    );
    // S7: ten seconds after the order only 2 of the 12 carry gold, 9 after
    // thirty seconds and 10 after sixty; two stay idle. PR 10 flips this: all
    // twelve gather within ten seconds.
    assert_eq!(
        (within(200), within(600), within(1200), idle),
        (2, 9, 10, 2)
    );
}
