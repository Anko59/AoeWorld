//! An army ordered across a river with no crossing at all. The carrier's
//! search proves the far bank unreachable and walks to the nearest bank tile;
//! the members used to keep steering at their own slots across the water,
//! scattering along the bank on detours of their own. The group now forms at
//! the reachable tile nearest the destination and stands there.
use super::*;
use crate::game_world::terrain_fixtures::build;

const KINDS: [UnitKind; 4] = [
    UnitKind::Champion,
    UnitKind::Crossbowman,
    UnitKind::Paladin,
    UnitKind::SiegeRam,
];

fn tile(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn an_army_sent_across_a_sealed_river_stops_together_on_the_bank() {
    let mut f = build("army-river-sealed");
    let ids = (0..16)
        .map(|i| {
            let at = tile(100 + (i % 4) * 2, 126 + (i / 4) * 2);
            f.world
                .spawn_kind(PlayerId(0), KINDS[i as usize % KINDS.len()], at)
                .unwrap()
        })
        .collect::<Vec<_>>();
    f.world
        .issue_formation_move(
            PlayerId(0),
            &ids,
            aoe_core::Formation::Line,
            tile(170, 126),
            false,
            false,
        )
        .unwrap();
    for _ in 0..3_000 {
        f.world.advance();
    }
    let a = &f.world.route_metrics.attribution;
    let at = ids
        .iter()
        .map(|&id| {
            let u = f.world.unit(id).unwrap();
            (u.position.tile_floor(), u.moving || u.planning)
        })
        .collect::<Vec<_>>();
    let report = format!(
        "failed {:?} stopped {} short {} expired {} at {at:?}",
        a.group_failures, a.group_carriers_stopped, a.group_stopped_short, a.group_orders_expired
    );
    assert_eq!(a.group_stopped_short, 1, "{report}");
    // One body on the near bank, every member at rest, none in the water.
    let (xs, ys): (Vec<_>, Vec<_>) = at.iter().map(|(t, _)| (t.x, t.y)).unzip();
    let spread = |v: &[i32]| v.iter().max().unwrap() - v.iter().min().unwrap();
    assert!(spread(&xs) <= 10 && spread(&ys) <= 14, "{report}");
    assert!(xs.iter().all(|&x| (135..150).contains(&x)), "{report}");
    assert!(at.iter().all(|(_, busy)| !busy), "{report}");
}
