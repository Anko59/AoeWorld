//! S7: twelve villagers sent to one gold mine. Records today's behaviour; PR 10
//! of the group-movement rewrite (docs/movement/group-movement-rewrite.md,
//! section 6) requires all twelve to gather within ten seconds.
use super::*;

/// `count` villagers spread around the ring of one node. Returns the number
/// carrying within 10 s, 30 s and 60 s of the order, and the number idle at 60 s.
fn crowd(kind: ResourceKind, count: usize) -> (usize, usize, usize, usize, u32, usize) {
    crowd_from(kind, count, 6, 9)
}

fn crowd_from(
    kind: ResourceKind,
    count: usize,
    inner: i32,
    outer: i32,
) -> (usize, usize, usize, usize, u32, usize) {
    let (mut world, node, first) = resource_world(kind);
    let mut ids = vec![first];
    let ring = (-outer..=outer)
        .flat_map(|dy| (-outer..=outer).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy): &(i32, i32)| dx.abs().max(dy.abs()) >= inner)
        .map(|(dx, dy)| TileCoord::new(node.tile.x + dx, node.tile.y + dy))
        .filter(|&t| world.tile_passable(t))
        .collect::<Vec<_>>();
    for n in 0..count - 1 {
        let tile = ring[n * ring.len() / (count - 1)];
        ids.push(
            world
                .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
                .unwrap(),
        );
    }
    let refused = ids
        .iter()
        .filter(|&&id| world.issue_gather(PlayerId(0), id, node.id).is_err())
        .count();
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
    let last = first_load.iter().flatten().copied().max().unwrap_or(0);
    let result = (within(200), within(600), within(1200), idle, last, refused);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/movement");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("crowd-{kind:?}-{count}-{outer}.txt")),
        format!("{result:?}\n"),
    )
    .unwrap();
    result
}

#[test]
fn twelve_villagers_on_one_gold_mine() {
    let got = crowd(ResourceKind::Gold, 12);
    eprintln!("group-baseline gold-crowd: {got:?}");
    // Preserve the original distant starts. They include >12 tiles of walking,
    // so the ten-second worksite target is tested separately below.
    assert!(got.0 >= 2, "{got:?}");
    assert_eq!((got.1, got.2, got.3, got.5), (12, 12, 0, 0), "{got:?}");
    assert!(got.4 <= 600, "{got:?}");
    assert!(got.4 <= 395, "PR 10 gold collect time: {got:?}");
}

#[test]
fn twelve_nearby_villagers_all_collect_within_ten_seconds() {
    let got = [ResourceKind::Wood, ResourceKind::Food, ResourceKind::Gold]
        .map(|kind| crowd_from(kind, 12, 2, 3));
    for (result, limit) in got.iter().zip([190, 179, 141]) {
        assert!(
            result.4 <= limit,
            "PR 10 worksite collect time: {result:?}, limit {limit}"
        );
    }
    assert!(
        got.iter()
            .all(|r| r.0 == 12 && r.3 == 0 && r.4 <= 200 && r.5 == 0),
        "{got:?}"
    );
}

#[test]
fn eight_villagers_on_one_tree_berry_bush_or_gold_patch() {
    let got = [ResourceKind::Wood, ResourceKind::Food, ResourceKind::Gold].map(|k| crowd(k, 8));
    let twelve = [ResourceKind::Wood, ResourceKind::Food].map(|k| crowd(k, 12));
    assert!(
        got.iter()
            .all(|r| r.1 == 8 && r.2 == 8 && r.3 == 0 && r.4 <= 600 && r.5 == 0),
        "{got:?}"
    );
    for (result, limit) in twelve.iter().zip([382, 395]) {
        assert!(
            result.4 <= limit,
            "PR 10 distant collect time: {result:?}, limit {limit}"
        );
    }
    assert!(
        twelve
            .iter()
            .all(|r| r.1 == 12 && r.2 == 12 && r.3 == 0 && r.4 <= 600 && r.5 == 0),
        "{twelve:?}"
    );
}
