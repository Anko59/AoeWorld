//! A large formation sent against a forest edge, where its packed files only
//! fit behind the goal, still comes to rest a body apart: members brush
//! through moving members only, so an arrival never settles on one at rest.
use super::*;
use aoe_core::Formation;

/// Pairs of members closer than half a militia's body width.
fn stacked_pairs(w: &GameWorld, ids: &[EntityId]) -> usize {
    let positions: Vec<_> = ids.iter().map(|id| w.unit(*id).unwrap().position).collect();
    let mut pairs = 0;
    for (n, a) in positions.iter().enumerate() {
        for b in &positions[n + 1..] {
            let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
            pairs += usize::from(dx * dx + dy * dy < 205 * 205);
        }
    }
    pairs
}

#[test]
fn a_hundred_members_sent_against_a_forest_come_to_rest_apart() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let origin = center(30, 50);
    let ids = crate::game_world::formation::layout::slots(Formation::Box, 100)
        .into_iter()
        .map(|offset| {
            let p = WorldPosition::new(origin.x + offset.x, origin.y + offset.y);
            w.spawn_kind(PlayerId(0), UnitKind::Militia, p).unwrap()
        })
        .collect::<Vec<_>>();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for x in 72..90 {
            for y in 30..70 {
                let node = aoe_map::ResourceNode::starting(
                    TileCoord::new(x, y),
                    aoe_map::ResourceKind::Wood,
                )
                .unwrap();
                overlay.insert_starting_resource(generator, node).unwrap();
            }
        }
    }
    w.prepare_navigation(&|| false).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Box,
        center(71, 50),
        false,
        false,
    )
    .unwrap();
    for _ in 0..3000 {
        w.advance_movement();
        w.refill_route_budgets();
    }
    let moving = ids.iter().filter(|id| w.unit(**id).unwrap().moving).count();
    let stacked = stacked_pairs(&w, &ids);
    assert_eq!(moving, 0);
    assert_eq!(stacked, 0, "members settled on members at rest");
}
